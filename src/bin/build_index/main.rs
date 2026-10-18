//! Builds the address index from a local Overture release directory (division, division_area,
//! segment, address and, when present, place parquet), plus optional OSM extracts, OpenAddresses
//! exports and GeoNames postcodes, without Nominatim. Only house numbers are spilled to disk.
mod digest;
mod fetch;
mod geo;
mod geonames;
mod job;
mod landmarks;
mod mem;
mod progress;
mod sources;
mod store;

use arrow_array::{
    Array, ArrayRef, RecordBatch, StringArray,
    cast::AsArray,
    types::{Float64Type, Int32Type},
};
use geo::{Assigner, KINDS, Poly, wkb_line, wkb_point, wkb_polygons};
use opengeocoding::index::{
    reverse::{self, StreetSpill, street_edges},
    schema::{
        self, Bucket, Num, NumberSpill, fold, fold_into, held_slot, pack, pc_norm, with_rank,
    },
};
use parquet::arrow::{
    ProjectionMask,
    arrow_reader::{ArrowReaderMetadata, ParquetRecordBatchReaderBuilder},
};
use rayon::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    hash::{BuildHasher, BuildHasherDefault, DefaultHasher, Hash},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU32, AtomicU64, Ordering::Relaxed},
    },
    time::Instant,
};
use tantivy::doc;

// Returns freed memory to the OS promptly, so the writer budget sees the real headroom.
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

const USAGE: &str = "usage: build_index <overture-release-dir> <out-dir> [--openaddresses DIR [--openaddresses-any-licence]] [--osm DIR] [--osm-landmarks DIR] [--geonames-postal FILE]... [--geonames-places FILE [--geonames-admin1 FILE]] [--bbox MINLON,MINLAT,MAXLON,MAXLAT]
       build_index job --store URL --index ID --builder-hash HASH --sources FILE --scratch DIR --models DIR [--models-version v1]
       build_index pull --store URL --index ID --to DIR [--models DIR] [--models-version v1]
       build_index digest DIR [--dump FILE]
       build_index hash";
const SHARDS: usize = 256;
const MAX_POSTCODES: usize = 20;
/// Postcodes a street holds while it is built: a house's slot is 6 bits, under its source rank.
/// Only the first `MAX_POSTCODES` by text are kept in the end; no street comes near this many.
const MAX_HELD_POSTCODES: usize = 63;
const MAX_CITIES: usize = 5;
const MAX_ALT_NAMES: usize = 64;
/// Must divide `SHARDS`: a house goes to the shard of its street's low byte. More shards mean smaller
/// sorts, so more of them fit in memory at once.
const NUMBER_SHARDS: usize = 64;
const MAX_WRITER_THREADS: usize = 16;
/// Segments left after the final merge. The writer leaves one per arena flush (319 for a world
/// index); each query looks its terms up in every segment, so queries cost about that many
/// times more. Merging in four parallel groups took 40 s for the world index; one segment took
/// 140 s and queried no faster.
const FINAL_SEGMENTS: usize = 4;
const NAME_SEP: char = '\u{1f}';
const GB: f64 = (1u64 << 30) as f64;

type Str = Box<str>;

struct Task {
    path: PathBuf,
    meta: ArrowReaderMetadata,
    rg: usize,
}

fn tasks(dir: &Path) -> Vec<Task> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "parquet"))
        .collect();
    files.sort();
    files
        .into_iter()
        .flat_map(|path| {
            let meta =
                ArrowReaderMetadata::load(&File::open(&path).unwrap(), Default::default()).unwrap();
            (0..meta.metadata().num_row_groups()).map(move |rg| Task {
                path: path.clone(),
                meta: meta.clone(),
                rg,
            })
        })
        .collect()
}

/// Reads one row group, projecting the leaves under the given column paths.
fn read(t: &Task, roots: &[&str]) -> impl Iterator<Item = RecordBatch> {
    let schema = t.meta.metadata().file_metadata().schema_descr();
    let leaves: Vec<usize> = (0..schema.num_columns())
        .filter(|&i| {
            let p = schema.column(i).path().string();
            roots
                .iter()
                .any(|r| p == *r || p.starts_with(&format!("{r}.")))
        })
        .collect();
    ParquetRecordBatchReaderBuilder::new_with_metadata(File::open(&t.path).unwrap(), t.meta.clone())
        .with_row_groups(vec![t.rg])
        .with_projection(ProjectionMask::leaves(schema, leaves))
        .with_batch_size(65536)
        .build()
        .unwrap()
        .map(|b| b.unwrap())
}

fn col<'a>(b: &'a RecordBatch, name: &str) -> &'a ArrayRef {
    b.column_by_name(name)
        .unwrap_or_else(|| panic!("missing column {name}"))
}

fn text(a: &StringArray, i: usize) -> Option<&str> {
    (!a.is_null(i))
        .then(|| a.value(i).trim())
        .filter(|s| !s.is_empty())
}

/// names.primary and the values of names.common, deduplicated.
fn names_at<'a>(names: &'a arrow_array::StructArray, i: usize, out: &mut Vec<&'a str>) {
    out.clear();
    if names.is_null(i) {
        return;
    }
    if let Some(p) = text(
        names.column_by_name("primary").unwrap().as_string::<i32>(),
        i,
    ) {
        out.push(p);
    }
    let common = names.column_by_name("common").unwrap().as_map();
    if common.is_null(i) {
        return;
    }
    let (o, values) = (common.value_offsets(), common.values().as_string::<i32>());
    for j in o[i] as usize..o[i + 1] as usize {
        if let Some(v) = text(values, j)
            && !out.contains(&v)
        {
            out.push(v);
        }
    }
}

fn shard<K: Hash + ?Sized>(k: &K) -> usize {
    BuildHasherDefault::<DefaultHasher>::default().hash_one(k) as usize % SHARDS
}

fn sharded<T: Default>() -> Vec<Mutex<T>> {
    (0..SHARDS).map(|_| Mutex::default()).collect()
}

fn country(s: Option<&str>) -> [u8; 2] {
    s.and_then(|c| c.as_bytes().try_into().ok())
        .unwrap_or([0; 2])
}

fn country_str(c: &[u8; 2]) -> &str {
    if *c == [0; 2] {
        ""
    } else {
        std::str::from_utf8(c).unwrap_or("")
    }
}

/// Adds `x` to a sorted list without duplicates. Merging such lists gives the same result in any
/// order, which a first-come list cut off at its limit does not; limits are applied once, when
/// every source has been read.
fn insert_sorted<T: Ord>(v: &mut Vec<T>, x: T) {
    if let Err(i) = v.binary_search(&x) {
        v.insert(i, x);
    }
}

/// Appends to a list whose positions are stored elsewhere (a house names its postcode by the
/// position in its street's list), so they must not shift while it is built.
fn push_unique<T: PartialEq>(v: &mut Vec<T>, x: T, max: usize) {
    if v.len() < max && !v.contains(&x) {
        v.push(x);
    }
}

fn insert_name(v: &mut Vec<Str>, s: &str) {
    if let Err(i) = v.binary_search_by(|a| (**a).cmp(s)) {
        v.insert(i, s.into());
    }
}

/// Coordinates are summed as integers of 2^-24 degree (6.6 mm): floating-point sums depend on the
/// order the addends arrive in, and an index built from the same sources must not.
const Q: f64 = 16_777_216.0;

fn quantize(v: f64) -> i64 {
    (v * Q).round() as i64
}

fn mean(sum: i64, n: u64) -> f64 {
    sum as f64 / n as f64 / Q
}

/// Whether `a` is the better of two spellings that fold to one street: not all capitals, then
/// the one that sorts first, whichever arrived first.
fn prefer(a: &str, b: &str) -> bool {
    let upper = |s: &str| s.chars().any(char::is_alphabetic) && s == s.to_uppercase();
    (upper(a), a) < (upper(b), b)
}

/// Deduplicated strings; ids are (position in shard) << 8 | shard.
struct Interner(Vec<Mutex<HashMap<Str, u32>>>);

impl Interner {
    fn new() -> Interner {
        Interner(sharded())
    }

    fn id(&self, s: &str) -> u32 {
        let sh = shard(s);
        let mut m = self.0[sh].lock().unwrap();
        if let Some(&i) = m.get(s) {
            return i;
        }
        let i = ((m.len() as u32) << 8) | sh as u32;
        m.insert(s.into(), i);
        i
    }

    /// Task-local cache in front of the shared, locked table.
    fn cached(&self, cache: &mut HashMap<Str, u32>, s: &str) -> u32 {
        if let Some(&i) = cache.get(s) {
            return i;
        }
        let i = self.id(s);
        cache.insert(s.into(), i);
        i
    }

    fn table(self) -> Strings {
        Strings(
            self.0
                .into_iter()
                .map(|m| {
                    let m = m.into_inner().unwrap();
                    let mut v = vec![Str::default(); m.len()];
                    for (s, i) in m {
                        v[(i >> 8) as usize] = s;
                    }
                    v
                })
                .collect(),
        )
    }
}

struct Strings(Vec<Vec<Str>>);

impl Strings {
    fn get(&self, id: u32) -> &str {
        &self.0[id as usize & 0xff][(id >> 8) as usize]
    }
}

const SUBTYPES: [&str; 10] = [
    "country",
    "dependency",
    "region",
    "county",
    "localadmin",
    "locality",
    "macrohood",
    "neighborhood",
    "microhood",
    "other",
];

struct Place {
    id: Str,
    subtype: &'static str,
    country: [u8; 2],
    /// ISO 3166-2 code of the region, or empty.
    region: Str,
    /// All names, primary first, joined by NAME_SEP.
    names: Str,
    population: u64,
    xy: Option<(f64, f64)>,
    parents: Vec<Str>,
    anc: Box<[u32]>,
    parent: Option<Str>,
    /// The `places.tsv` columns after the row's place id, with the parent left out.
    row: Str,
}

/// Tab-separated text with tabs, newlines and NAME_SEP in values turned into spaces.
fn cell(s: &str) -> std::borrow::Cow<'_, str> {
    if s.contains(['\t', '\n', '\r', NAME_SEP]) {
        s.replace(['\t', '\n', '\r', NAME_SEP], " ").into()
    } else {
        s.into()
    }
}

/// Also writes `places.tsv`, one row per place id: subtype, class, country, ISO 3166-2 region,
/// admin level, population, x, y, bbox (xmin, ymin, xmax, ymax), parent place id (or empty),
/// primary name, `language=name` pairs joined by NAME_SEP, and the Wikidata item (or empty).
fn load_places(dir: &Path, out: &str) -> (Vec<Place>, HashMap<Str, u32>) {
    let mut places: Vec<Place> = tasks(dir)
        .par_iter()
        .flat_map_iter(|t| {
            let mut out = vec![];
            for b in read(
                t,
                &[
                    "id",
                    "subtype",
                    "country",
                    "names.primary",
                    "names.common",
                    "names.rules",
                    "population",
                    "hierarchies",
                    "geometry",
                    "class",
                    "region",
                    "admin_level",
                    "parent_division_id",
                    "bbox",
                    "wikidata",
                ],
            ) {
                let (class, region, parent, wikidata) = (
                    col(&b, "class").as_string::<i32>(),
                    col(&b, "region").as_string::<i32>(),
                    col(&b, "parent_division_id").as_string::<i32>(),
                    col(&b, "wikidata").as_string::<i32>(),
                );
                let level = col(&b, "admin_level").as_primitive::<Int32Type>();
                let bbox = col(&b, "bbox").as_struct();
                let bb = |n: &str| {
                    bbox.column_by_name(n)
                        .unwrap()
                        .as_primitive::<Float64Type>()
                        .clone()
                };
                let (x0, x1, y0, y1) = (bb("xmin"), bb("xmax"), bb("ymin"), bb("ymax"));
                let (id, sub, cc) = (
                    col(&b, "id").as_string::<i32>(),
                    col(&b, "subtype").as_string::<i32>(),
                    col(&b, "country").as_string::<i32>(),
                );
                let (pop, geom, nm) = (
                    col(&b, "population").as_primitive::<Int32Type>(),
                    col(&b, "geometry").as_binary::<i32>(),
                    col(&b, "names").as_struct(),
                );
                let hier = col(&b, "hierarchies").as_list::<i32>();
                let mut names = vec![];
                let rules = nm
                    .column_by_name("rules")
                    .map(|r| r.as_list::<i32>().clone());
                for i in 0..b.num_rows() {
                    names_at(nm, i, &mut names);
                    // alternate, short and official names, as (Nominatim namedetails key, name)
                    let mut extra: Vec<(String, &str)> = vec![];
                    if let Some(rules) = rules.as_ref().filter(|r| !r.is_null(i)) {
                        let (o, v) = (rules.value_offsets(), rules.values().as_struct());
                        let (variant, lang, value) = (
                            v.column_by_name("variant").unwrap().as_string::<i32>(),
                            v.column_by_name("language").unwrap().as_string::<i32>(),
                            v.column_by_name("value").unwrap().as_string::<i32>(),
                        );
                        for j in o[i] as usize..o[i + 1] as usize {
                            let Some(name) = text(value, j) else { continue };
                            let key = match text(variant, j) {
                                Some("official") => "official_name",
                                Some("short") => "short_name",
                                _ => "alt_name",
                            };
                            let key = match text(lang, j) {
                                Some(l) => format!("{key}:{l}"),
                                None => key.to_string(),
                            };
                            if !names.contains(&name) {
                                names.push(name);
                            }
                            extra.push((key, name));
                        }
                    }
                    let mut parents = vec![];
                    if !hier.is_null(i) {
                        let outer = hier.value(i);
                        let outer = outer.as_list::<i32>();
                        for h in 0..outer.len() {
                            let inner = outer.value(h);
                            let ids = inner
                                .as_struct()
                                .column_by_name("division_id")
                                .unwrap()
                                .as_string::<i32>();
                            parents
                                .extend((0..ids.len()).filter_map(|k| text(ids, k)).map(Str::from));
                        }
                    }
                    let xy = (!geom.is_null(i))
                        .then(|| wkb_point(geom.value(i)))
                        .flatten();
                    let mut row = String::new();
                    {
                        use std::fmt::Write;
                        let (x, y) = xy.unwrap_or((f64::NAN, f64::NAN));
                        let num = |a: &arrow_array::Float64Array| {
                            if bbox.is_null(i) || a.is_null(i) {
                                f64::NAN
                            } else {
                                a.value(i)
                            }
                        };
                        let _ = write!(
                            row,
                            "{}\t{}\t{}\t{}\t{}\t{}\t{x}\t{y}\t{}\t{}\t{}\t{}\t{}\t",
                            sub.value(i),
                            text(class, i).unwrap_or(""),
                            text(cc, i).unwrap_or(""),
                            text(region, i).unwrap_or(""),
                            if level.is_null(i) { 0 } else { level.value(i) },
                            if pop.is_null(i) {
                                0
                            } else {
                                pop.value(i).max(0)
                            },
                            num(&x0),
                            num(&y0),
                            num(&x1),
                            num(&y1),
                            cell(names.first().copied().unwrap_or("")),
                        );
                        let common = nm.column_by_name("common").unwrap().as_map();
                        if !common.is_null(i) {
                            let (o, k, v) = (
                                common.value_offsets(),
                                common.keys().as_string::<i32>(),
                                common.values().as_string::<i32>(),
                            );
                            for j in o[i] as usize..o[i + 1] as usize {
                                if let Some(name) = text(v, j) {
                                    if !row.ends_with('\t') {
                                        row.push(NAME_SEP);
                                    }
                                    let _ = write!(row, "{}={}", cell(k.value(j)), cell(name));
                                }
                            }
                        }
                        for (k, name) in &extra {
                            if !row.ends_with('\t') {
                                row.push(NAME_SEP);
                            }
                            let _ = write!(row, "{}={}", cell(k), cell(name));
                        }
                        row.push('\t');
                        row.push_str(&cell(text(wikidata, i).unwrap_or("")));
                    }
                    out.push(Place {
                        parent: text(parent, i).map(Str::from),
                        row: row.into(),
                        id: id.value(i).into(),
                        subtype: SUBTYPES
                            .iter()
                            .find(|s| **s == sub.value(i))
                            .unwrap_or(&"other"),
                        country: country(text(cc, i)),
                        region: text(region, i).unwrap_or("").into(),
                        names: names.join(&NAME_SEP.to_string()).into(),
                        population: if pop.is_null(i) {
                            0
                        } else {
                            pop.value(i).max(0) as u64
                        },
                        xy,
                        parents,
                        anc: Box::default(),
                    });
                }
            }
            out
        })
        .collect();
    places.par_sort_unstable_by(|a, b| a.id.cmp(&b.id));
    let idx: HashMap<Str, u32> = places
        .iter_mut()
        .enumerate()
        .map(|(i, p)| (std::mem::take(&mut p.id), i as u32))
        .collect();
    places.par_iter_mut().enumerate().for_each(|(i, p)| {
        let mut anc: Vec<u32> = p
            .parents
            .iter()
            .filter_map(|d| idx.get(d).copied())
            .filter(|&a| a != i as u32)
            .collect();
        anc.sort_unstable();
        anc.dedup();
        p.anc = anc.into();
        p.parents = vec![];
    });
    let mut w = std::io::BufWriter::new(File::create(Path::new(out).join("places.tsv")).unwrap());
    for p in &mut places {
        use std::io::Write;
        let parent = p.parent.take().and_then(|d| idx.get(&d).copied());
        let row = std::mem::take(&mut p.row);
        let (head, tail) = row.split_at(
            row.match_indices('\t')
                .nth(11)
                .map_or(row.len(), |(k, _)| k + 1),
        );
        let parent = parent.map_or(String::new(), |v| v.to_string());
        writeln!(w, "{head}{parent}\t{tail}").unwrap();
    }
    std::io::Write::flush(&mut w).unwrap();
    (places, idx)
}

fn load_assigner(dir: &Path, places: &[Place], idx: &HashMap<Str, u32>) -> Assigner {
    let polys: Vec<Poly> = tasks(dir)
        .par_iter()
        .flat_map_iter(|t| {
            let mut out = vec![];
            for b in read(t, &["division_id", "subtype", "is_land", "geometry"]) {
                let (id, sub, land, geom) = (
                    col(&b, "division_id").as_string::<i32>(),
                    col(&b, "subtype").as_string::<i32>(),
                    col(&b, "is_land").as_boolean(),
                    col(&b, "geometry").as_binary::<i32>(),
                );
                for i in 0..b.num_rows() {
                    let Some(kind) = KINDS.iter().position(|k| *k == sub.value(i)) else {
                        continue;
                    };
                    // is_land picks the coastline-clipped polygon, exactly one per division
                    if land.is_null(i) || !land.value(i) || geom.is_null(i) {
                        continue;
                    }
                    let Some(&place) = idx.get(id.value(i)) else {
                        continue;
                    };
                    if let Some((xy, rings)) = wkb_polygons(geom.value(i)) {
                        out.extend(Poly::new(place, kind as u8, xy, &rings));
                    }
                }
            }
            out
        })
        .collect();
    let towns = places
        .iter()
        .enumerate()
        .filter(|(_, p)| p.subtype == "locality")
        .filter_map(|(i, p)| p.xy.map(|(x, y)| ([x, y], i as u32)))
        .collect();
    println!(
        "  polygons {} vertices {}",
        polys.len(),
        polys.iter().map(Poly::vertices).sum::<usize>()
    );
    Assigner::new(polys, towns)
}

/// The territorial polygons of `area::KINDS`, simplified, for `ResolveRegion`; those wholly outside
/// `bbox` are left out. Returns the polygons, vertices and bytes written.
fn write_areas(
    dir: &Path,
    idx: &HashMap<Str, u32>,
    bbox: Option<[f64; 4]>,
    out: &Path,
) -> (usize, usize, u64) {
    use opengeocoding::index::area;
    let areas: Vec<area::Area> = tasks(dir)
        .par_iter()
        .flat_map_iter(|t| {
            let mut out = vec![];
            for b in read(t, &["division_id", "subtype", "is_territorial", "geometry"]) {
                let (id, sub, sea, geom) = (
                    col(&b, "division_id").as_string::<i32>(),
                    col(&b, "subtype").as_string::<i32>(),
                    col(&b, "is_territorial").as_boolean(),
                    col(&b, "geometry").as_binary::<i32>(),
                );
                for i in 0..b.num_rows() {
                    let Some(kind) = area::KINDS.iter().position(|k| *k == sub.value(i)) else {
                        continue;
                    };
                    // is_territorial picks the polygon that takes in the territorial sea, so a
                    // point just offshore is placed; there is one per division
                    if sea.is_null(i) || !sea.value(i) || geom.is_null(i) {
                        continue;
                    }
                    let Some(&place) = idx.get(id.value(i)) else {
                        continue;
                    };
                    let Some((xy, rings)) = wkb_polygons(geom.value(i)) else {
                        continue;
                    };
                    let outside = bbox.is_some_and(|[x0, y0, x1, y1]| {
                        let (lo, hi) =
                            xy.iter()
                                .fold(([f64::MAX; 2], [f64::MIN; 2]), |(lo, hi), p| {
                                    (
                                        [lo[0].min(p[0]), lo[1].min(p[1])],
                                        [hi[0].max(p[0]), hi[1].max(p[1])],
                                    )
                                });
                        hi[0] < x0 || lo[0] > x1 || hi[1] < y0 || lo[1] > y1
                    });
                    if outside {
                        continue;
                    }
                    let rings: Vec<_> = rings
                        .iter()
                        .filter_map(|&(s, e)| {
                            area::simplify(&xy[s as usize..e as usize], area::TOLERANCE)
                        })
                        .collect();
                    if !rings.is_empty() {
                        out.push(area::Area {
                            place,
                            kind: kind as u8,
                            rings,
                        });
                    }
                }
            }
            out
        })
        .collect();
    let vertices = areas.iter().flat_map(|a| &a.rings).map(Vec::len).sum();
    let polygons = areas.len();
    let bytes = area::write(&out.join(area::FILE), areas).unwrap();
    (polygons, vertices, bytes)
}

#[derive(Default)]
struct SegAgg {
    /// Its id in the spilled street edges: position in its shard, then shard.
    id: u32,
    name: Str,
    /// Sorted, without duplicates.
    alt: Vec<Str>,
    sx: i64,
    sy: i64,
    n: u64,
}

impl SegAgg {
    fn add(&mut self, other: SegAgg) {
        if self.n == 0 || prefer(&other.name, &self.name) {
            self.name = other.name;
        }
        for a in other.alt {
            insert_sorted(&mut self.alt, a);
        }
        (self.sx, self.sy, self.n) = (self.sx + other.sx, self.sy + other.sy, self.n + other.n);
    }
}

type SegKey = (u32, Str);
/// A segment's edges as `street_edges` cuts them.
type StreetEdges = Vec<(u64, (i8, i8))>;

/// Segment classes left out of `streets.bin`: a point beside a path is not on a street.
const NOT_STREETS: [&str; 6] = ["footway", "steps", "path", "cycleway", "bridleway", "track"];

fn pass_segments(
    dir: &Path,
    a: &Assigner,
    spill: &StreetSpill,
) -> Vec<Mutex<HashMap<SegKey, SegAgg>>> {
    let shards = sharded::<HashMap<SegKey, SegAgg>>();
    let rows = AtomicU64::new(0);
    tasks(dir).par_iter().for_each(|t| {
        // place -> folded name, so lookups borrow the key instead of allocating one per row
        let mut local: HashMap<u32, HashMap<Str, (SegAgg, StreetEdges)>> = HashMap::new();
        let (mut key, mut last, mut cell_key) = (String::new(), String::new(), String::new());
        let mut line = vec![];
        for b in read(
            t,
            &[
                "subtype",
                "class",
                "names.primary",
                "names.common",
                "bbox",
                "geometry",
            ],
        ) {
            let (sub, class, nm, bbox, geom) = (
                col(&b, "subtype").as_string::<i32>(),
                col(&b, "class").as_string::<i32>(),
                col(&b, "names").as_struct(),
                col(&b, "bbox").as_struct(),
                col(&b, "geometry").as_binary::<i32>(),
            );
            let c = |n: &str| {
                bbox.column_by_name(n)
                    .unwrap()
                    .as_primitive::<Float64Type>()
                    .clone()
            };
            let (x0, x1, y0, y1) = (c("xmin"), c("xmax"), c("ymin"), c("ymax"));
            let mut names = vec![];
            rows.fetch_add(b.num_rows() as u64, Relaxed);
            for i in 0..b.num_rows() {
                if sub.is_null(i) || sub.value(i) != "road" || bbox.is_null(i) {
                    continue;
                }
                names_at(nm, i, &mut names);
                let Some(&primary) = names.first() else {
                    continue;
                };
                let (x, y) = (
                    (x0.value(i) + x1.value(i)) / 2.0,
                    (y0.value(i) + y1.value(i)) / 2.0,
                );
                let Some(place) = a.place(x, y) else { continue };
                if primary != last {
                    fold_into(primary, &mut key);
                    last.clear();
                    last.push_str(primary);
                }
                if key.is_empty() {
                    continue;
                }
                let m = local.entry(place).or_default();
                let celled = with_cell(&key, x, y, &mut cell_key);
                let (e, edges) = match m.get_mut(celled) {
                    Some(e) => e,
                    None => m.entry(celled.into()).or_insert_with(|| {
                        let agg = SegAgg {
                            name: primary.into(),
                            ..Default::default()
                        };
                        (agg, vec![])
                    }),
                };
                if prefer(primary, &e.name) {
                    e.name = primary.into();
                }
                for alt in &names[1..] {
                    insert_name(&mut e.alt, alt);
                }
                (e.sx, e.sy, e.n) = (e.sx + quantize(x), e.sy + quantize(y), e.n + 1);
                let street = class.is_null(i) || !NOT_STREETS.contains(&class.value(i));
                if street && !geom.is_null(i) && wkb_line(geom.value(i), &mut line).is_some() {
                    street_edges(&line, edges);
                }
            }
        }
        let mut edges = vec![];
        for (place, m) in local {
            for (k, (v, mine)) in m {
                let k = (place, k);
                let s = shard(&k);
                let mut g = shards[s].lock().unwrap();
                assert!(g.len() < 1 << 24, "segment ids overflow");
                let id = (g.len() as u32) << 8 | s as u32;
                let e = g.entry(k).or_insert_with(|| SegAgg {
                    id,
                    ..Default::default()
                });
                e.add(v);
                edges.extend(mine.into_iter().map(|(key, d)| (key, d, e.id)));
            }
        }
        spill.add(&mut edges);
    });
    println!("  {} segment rows", rows.into_inner());
    shards
}

#[derive(Default)]
struct AddrAgg {
    name: Str,
    /// Interned normalized postcodes, in arrival order (houses refer to positions), and postal city
    /// names, sorted by id. Once every source is read both are sorted by text and cut to
    /// `MAX_POSTCODES` and `MAX_CITIES`.
    pcs: Vec<u32>,
    cities: Vec<u32>,
    postal_towns: Vec<(u32, u32, u32, u32)>,
    sx: i64,
    sy: i64,
    n: u64,
}

impl AddrAgg {
    fn add(&mut self, other: AddrAgg) {
        if prefer(&other.name, &self.name) {
            self.name = other.name;
        }
        for p in other.pcs {
            push_unique(&mut self.pcs, p, MAX_HELD_POSTCODES);
        }
        for c in other.cities {
            insert_sorted(&mut self.cities, c);
        }
        if self.postal_towns.is_empty() {
            self.postal_towns = other.postal_towns;
        } else {
            for town in other.postal_towns {
                insert_sorted(&mut self.postal_towns, town);
            }
        }
        (self.sx, self.sy, self.n) = (self.sx + other.sx, self.sy + other.sy, self.n + other.n);
    }
}

type AddrKey = ([u8; 2], u32, Str);
type PcKey = ([u8; 2], Str);
const NO_PLACE: u32 = u32::MAX;

#[derive(Default)]
struct AddrShard {
    ids: HashMap<AddrKey, u32>,
    streets: Vec<AddrAgg>,
}

#[derive(Default)]
struct PcAgg {
    display: Str,
    sx: i64,
    sy: i64,
    n: u64,
}

impl PcAgg {
    fn add(&mut self, display: &str, x: i64, y: i64, n: u64) {
        if self.n == 0 || display < &*self.display {
            self.display = display.into();
        }
        (self.sx, self.sy, self.n) = (self.sx + x, self.sy + y, self.n + n);
    }
}

/// Street, postcode and house-number tables shared by every address source. Street ids are
/// (position in shard) << 8 | shard, so they're known when a task flushes.
struct Shared<'a> {
    assigner: &'a Assigner,
    /// Country of each place, for sources without one.
    countries: Vec<[u8; 2]>,
    numbers: &'a NumberSpill,
    streets: Vec<Mutex<AddrShard>>,
    postcodes: Vec<Mutex<HashMap<PcKey, PcAgg>>>,
    strings: Interner,
    rows: AtomicU64,
    /// Overture's address rows with a street, per country of the containing place.
    covered: Mutex<HashMap<[u8; 2], u64>>,
}

impl<'a> Shared<'a> {
    fn new(assigner: &'a Assigner, places: &[Place], numbers: &'a NumberSpill) -> Shared<'a> {
        Shared {
            assigner,
            countries: places.iter().map(|p| p.country).collect(),
            numbers,
            streets: sharded(),
            postcodes: sharded(),
            strings: Interner::new(),
            rows: AtomicU64::new(0),
            covered: Mutex::default(),
        }
    }
}

/// A street's merge key: its folded name and a 0.1° cell, so same-named streets in one large
/// locality (Port Chalmers' and central Dunedin's Beach Street) stay apart instead of sharing
/// house numbers. A street crossing a cell edge becomes two documents; lookups try both.
fn with_cell<'k>(key: &str, x: f64, y: f64, out: &'k mut String) -> &'k str {
    use std::fmt::Write;
    out.clear();
    let _ = write!(
        out,
        "{key}\u{1}{}:{}",
        (x * 10.0).floor() as i32,
        (y * 10.0).floor() as i32
    );
    out
}

/// The source rank of Overture's places. Their houses neither move a street's point nor add to
/// its weight: a road with one listed business keeps its segments' middle and importance.
const PLACES_RANK: u8 = 3;

/// Rows a sink aggregates before flushing, to bound task-local memory on large files.
const SINK_ROWS: u64 = 2_000_000;

/// One task's address rows, aggregated locally and merged into `Shared` on flush or drop.
struct Sink<'s, 'a> {
    shared: &'s Shared<'a>,
    local: HashMap<([u8; 2], u32), HashMap<Str, u32>>,
    aggs: Vec<AddrAgg>,
    pcs: HashMap<[u8; 2], HashMap<Str, PcAgg>>,
    cache: HashMap<Str, u32>,
    /// sid holds the index into `aggs` until the flush remaps it.
    nums: Bucket,
    rows: u64,
    key: String,
    last: String,
    lower: String,
    cell_key: String,
    pcn: String,
    /// Which source these rows come from: see `schema::with_rank`.
    rank: u8,
    covered: HashMap<[u8; 2], u64>,
}

fn nonempty(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

impl<'s, 'a> Sink<'s, 'a> {
    fn new(shared: &'s Shared<'a>) -> Self {
        Sink {
            shared,
            local: HashMap::new(),
            aggs: vec![],
            pcs: HashMap::new(),
            cache: HashMap::new(),
            nums: Bucket::default(),
            rows: 0,
            key: String::new(),
            last: String::new(),
            lower: String::new(),
            cell_key: String::new(),
            pcn: String::new(),
            rank: 0,
            covered: HashMap::new(),
        }
    }

    fn ranked(mut self, rank: u8) -> Self {
        self.rank = rank;
        self
    }

    /// One address. Without a country, the containing place's is used. `number` may list
    /// several numbers separated by ';', as OSM does.
    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        cc: Option<[u8; 2]>,
        x: f64,
        y: f64,
        street: Option<&str>,
        number: Option<&str>,
        postcode: Option<&str>,
        city: Option<(&str, &'static str)>,
    ) {
        self.rows += 1;
        if self.rows >= SINK_ROWS {
            self.flush();
        }
        let a = self.shared.assigner;
        if !a.within(x, y) {
            return;
        }
        let mut place = None;
        let c = match cc {
            Some(c) => c,
            None => {
                let p = a.place(x, y);
                place = Some(p);
                match p {
                    Some(p) => self.shared.countries[p as usize],
                    None => return,
                }
            }
        };
        if c == [0; 2] || !x.is_finite() || !y.is_finite() {
            return;
        }
        let pc = nonempty(postcode);
        if let Some(pc) = pc {
            self.pcn.clear();
            self.pcn.extend(
                pc.chars()
                    .filter(|c| c.is_alphanumeric())
                    .flat_map(char::to_uppercase),
            );
            let m = self.pcs.entry(c).or_default();
            match m.get_mut(self.pcn.as_str()) {
                Some(e) => e.add(pc, quantize(x), quantize(y), 1),
                None => m.entry(self.pcn.as_str().into()).or_default().add(
                    pc,
                    quantize(x),
                    quantize(y),
                    1,
                ),
            }
        }
        // Village addresses without streets ("Libá 156", OSM addr:place) number within the
        // village, so its name stands in for the street, as in Nominatim.
        let Some(st) =
            nonempty(street).or_else(|| nonempty(number).and(nonempty(city.map(|(name, _)| name))))
        else {
            return;
        };
        if st != self.last {
            fold_into(st, &mut self.key);
            self.last.clear();
            self.last.push_str(st);
        }
        if self.key.is_empty() {
            return;
        }
        let place = place.unwrap_or_else(|| a.place(x, y)).unwrap_or(NO_PLACE);
        if self.rank == 0 && place != NO_PLACE {
            *self
                .covered
                .entry(self.shared.countries[place as usize])
                .or_default() += 1;
        }
        let m = self.local.entry((c, place)).or_default();
        let celled = with_cell(&self.key, x, y, &mut self.cell_key);
        let li = match m.get(celled) {
            Some(&li) => li,
            None => {
                self.aggs.push(AddrAgg {
                    name: st.into(),
                    ..Default::default()
                });
                m.insert(celled.into(), self.aggs.len() as u32 - 1);
                self.aggs.len() as u32 - 1
            }
        };
        let e = &mut self.aggs[li as usize];
        if st != &*e.name && prefer(st, &e.name) {
            e.name = st.into();
        }
        let pc_id = (pc.is_some() && !self.pcn.is_empty())
            .then(|| self.shared.strings.cached(&mut self.cache, &self.pcn));
        if let Some(id) = pc_id {
            push_unique(&mut e.pcs, id, MAX_HELD_POSTCODES);
        }
        if let Some((city, source)) = city.filter(|(name, _)| !name.trim().is_empty()) {
            let strings = &self.shared.strings;
            let id = strings.cached(&mut self.cache, city.trim());
            let source = strings.cached(&mut self.cache, source);
            insert_sorted(&mut e.cities, id);
            let pc = pc_id.unwrap_or_else(|| strings.cached(&mut self.cache, ""));
            let numbers = if pc_id.is_some() {
                vec![String::new()]
            } else {
                number
                    .into_iter()
                    .flat_map(|n| n.split(';'))
                    .map(schema::number_key)
                    .filter(|n| !n.is_empty())
                    .collect()
            };
            for n in numbers {
                let number = strings.cached(&mut self.cache, &n);
                let town = (pc, number, id, source);
                if e.postal_towns.is_empty() {
                    e.postal_towns = vec![town];
                } else {
                    insert_sorted(&mut e.postal_towns, town);
                }
            }
        }
        if self.rank != PLACES_RANK {
            (e.sx, e.sy, e.n) = (e.sx + quantize(x), e.sy + quantize(y), e.n + 1);
        }
        for n in number
            .into_iter()
            .flat_map(|n| n.split(';'))
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            self.lower.clear();
            self.lower.push_str(&schema::number_key(n));
            // slot = 1-based position in this task's list for the street; remapped on flush
            let slot = pc_id
                .and_then(|id| e.pcs.iter().position(|&p| p == id))
                .map_or(0, |i| i as u8 + 1);
            let (px, py) = pack(x, y, slot);
            match Num::new(li, &self.lower, with_rank(px, self.rank), py) {
                Ok(v) => self.nums.short.push(v),
                Err(v) => self.nums.long.push(v),
            }
        }
    }

    fn flush(&mut self) {
        let sh = self.shared;
        sh.rows.fetch_add(std::mem::take(&mut self.rows), Relaxed);
        let mut covered = sh.covered.lock().unwrap();
        for (c, n) in self.covered.drain() {
            *covered.entry(c).or_default() += n;
        }
        drop(covered);
        let mut remap = vec![0u32; self.aggs.len()];
        // per local street: its local postcode slots -> slots in the merged street's list
        let mut slots: Vec<Vec<u8>> = vec![vec![]; self.aggs.len()];
        for ((c, place), m) in self.local.drain() {
            for (k, li) in m {
                let key: AddrKey = (c, place, k);
                let agg = std::mem::take(&mut self.aggs[li as usize]);
                let local_pcs = agg.pcs.clone();
                let shard_ix = shard(&key);
                let mut s = sh.streets[shard_ix].lock().unwrap();
                let pos = match s.ids.get(&key) {
                    Some(&p) => {
                        s.streets[p as usize].add(agg);
                        p
                    }
                    None => {
                        let p = s.streets.len() as u32;
                        s.ids.insert(key, p);
                        s.streets.push(agg);
                        p
                    }
                };
                assert!(pos < 1 << 24, "too many streets in one shard");
                let merged = &s.streets[pos as usize].pcs;
                slots[li as usize] = local_pcs
                    .iter()
                    .map(|id| {
                        merged
                            .iter()
                            .position(|m| m == id)
                            .map_or(0, |i| i as u8 + 1)
                    })
                    .collect();
                remap[li as usize] = (pos << 8) | shard_ix as u32;
            }
        }
        self.aggs.clear();
        for (c, m) in self.pcs.drain() {
            for (k, v) in m {
                let k = (c, k);
                let mut s = sh.postcodes[shard(&k)].lock().unwrap();
                s.entry(k).or_default().add(&v.display, v.sx, v.sy, v.n);
            }
        }
        let shards = sh.numbers.shards();
        let mut out: Vec<Bucket> = (0..shards).map(|_| Bucket::default()).collect();
        let nums = std::mem::take(&mut self.nums);
        let slot = |li: u32, x: u32| match held_slot(x) {
            0 => x,
            k => {
                let merged = slots[li as usize].get(k as usize - 1).copied().unwrap_or(0);
                (x & 0xc0ff_ffff) | (merged as u32) << 24
            }
        };
        for mut n in nums.short {
            n.x = slot(n.sid, n.x);
            n.sid = remap[n.sid as usize];
            out[n.sid as usize % shards].short.push(n);
        }
        for mut n in nums.long {
            n.x = slot(n.sid, n.x);
            n.sid = remap[n.sid as usize];
            out[n.sid as usize % shards].long.push(n);
        }
        sh.numbers.add(out);
    }
}

impl Drop for Sink<'_, '_> {
    fn drop(&mut self) {
        self.flush();
    }
}

fn pass_addresses(dir: &Path, shared: &Shared) {
    let tasks = tasks(dir);
    let done = progress::Progress::items(
        "overture addresses".into(),
        tasks.len() as u64,
        "row groups",
    );
    tasks.par_iter().for_each(|t| {
        let mut sink = Sink::new(shared);
        for b in read(
            t,
            &[
                "country",
                "street",
                "number",
                "postcode",
                "postal_city",
                "address_levels",
                "geometry",
            ],
        ) {
            let s = |n: &str| col(&b, n).as_string::<i32>().clone();
            let (cc, street, number, postcode, postal) = (
                s("country"),
                s("street"),
                s("number"),
                s("postcode"),
                s("postal_city"),
            );
            let (levels, geom) = (
                col(&b, "address_levels").as_list::<i32>(),
                col(&b, "geometry").as_binary::<i32>(),
            );
            for i in 0..b.num_rows() {
                let Some((x, y)) = (!geom.is_null(i))
                    .then(|| wkb_point(geom.value(i)))
                    .flatten()
                else {
                    continue;
                };
                let city = text(&postal, i)
                    .map(|name| (name, "postal_city"))
                    .or_else(|| {
                        let (o, v) = (
                            levels.value_offsets(),
                            levels.values().as_struct().column(0).as_string::<i32>(),
                        );
                        (!levels.is_null(i) && o[i + 1] > o[i])
                            .then(|| text(v, o[i + 1] as usize - 1))
                            .flatten()
                            .map(|name| (name, "address_levels"))
                    });
                sink.add(
                    Some(country(text(&cc, i))),
                    x,
                    y,
                    text(&street, i),
                    text(&number, i),
                    text(&postcode, i),
                    city,
                );
            }
        }
        done.add(1);
    });
}

// ponytail: only the number-first shape; "Jl. Sudirman No. 5" and "Avenida Corrientes 1234" are
// skipped, because "Calle 10" names a street, not house 10. A per-country address parser
// (libpostal at build time) is the upgrade path.
fn freeform_house(freeform: &str) -> Option<(&str, &str)> {
    let (number, street) = freeform.trim().split_once(' ')?;
    let street = street.trim();
    let digits = number.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    (!digits.is_empty()
        && number.len() - digits.len() <= 1
        && digits.bytes().all(|b| b.is_ascii_digit())
        && !street.contains(',')
        && street.chars().next().is_some_and(char::is_alphabetic))
    .then_some((number, street))
}

/// Use the containing place's country, not the listing's: BAN covers Réunion, which its rows
/// file under France. Require a known road; listings alone give unreliable street centroids.
fn pass_places(dir: &Path, shared: &Shared, segs: &[Mutex<HashMap<SegKey, SegAgg>>]) {
    // ponytail: a fixed floor, so the few addresses a registry files across a border do not
    // cover the neighbour; French Polynesia's 2,000 rows fall under it. A share of the country's
    // expected addresses would be the better test.
    let covered: HashSet<[u8; 2]> = (shared.covered.lock().unwrap().iter())
        .filter(|(_, n)| **n >= 10_000)
        .map(|(c, _)| *c)
        .collect();
    let mut listed: Vec<&str> = covered.iter().map(country_str).collect();
    listed.sort_unstable();
    println!(
        "  overture places: skipping {}, which Overture's addresses cover",
        listed.join(" ")
    );
    let tasks = tasks(dir);
    let done =
        progress::Progress::items("overture places".into(), tasks.len() as u64, "row groups");
    tasks.par_iter().for_each(|t| {
        let mut sink = Sink::new(shared).ranked(PLACES_RANK);
        let (mut key, mut cell_key) = (String::new(), String::new());
        for b in read(t, &["addresses", "geometry"]) {
            let (addresses, geom) = (
                col(&b, "addresses").as_list::<i32>(),
                col(&b, "geometry").as_binary::<i32>(),
            );
            let freeform = addresses.values().as_struct().column_by_name("freeform");
            let freeform = freeform.unwrap().as_string::<i32>();
            let o = addresses.value_offsets();
            for i in 0..b.num_rows() {
                if addresses.is_null(i) || geom.is_null(i) {
                    continue;
                }
                let Some((x, y)) = wkb_point(geom.value(i)) else {
                    continue;
                };
                let houses = (o[i] as usize..o[i + 1] as usize)
                    .filter_map(|j| text(freeform, j).and_then(freeform_house));
                let mut houses = houses.peekable();
                if houses.peek().is_none()
                    || !(-180.0..=180.0).contains(&x)
                    || !(-90.0..=90.0).contains(&y)
                    || !shared.assigner.within(x, y)
                {
                    continue;
                }
                let Some(p) = shared.assigner.place(x, y) else {
                    continue;
                };
                let c = shared.countries[p as usize];
                // ponytail: ship the two largest measured gains first; each further country
                // needs a full-index gold comparison, including reverse lookups.
                if ![*b"GB", *b"VN"].contains(&c) || covered.contains(&c) {
                    continue;
                }
                for (number, street) in houses {
                    fold_into(street, &mut key);
                    let k: SegKey = (p, with_cell(&key, x, y, &mut cell_key).into());
                    if segs[shard(&k)].lock().unwrap().contains_key(&k) {
                        sink.add(Some(c), x, y, Some(street), Some(number), None, None);
                    }
                }
            }
        }
        done.add(1);
    });
}

fn place_weight(subtype: &str) -> u64 {
    match subtype {
        "country" => 2_000_000_000,
        "dependency" => 1_000_000_000,
        "region" => 500_000_000,
        "county" => 1_000_000,
        "localadmin" => 200_000,
        "locality" => 10_000,
        "macrohood" => 5_000,
        "neighborhood" => 2_000,
        _ => 1_000,
    }
}

/// A shard's streets in key order.
type Streets = Vec<(AddrKey, AddrAgg)>;

/// The streets in their final form.
struct Canon {
    /// Per shard, in key order, so a street's id depends on the street and not on when it arrived.
    /// Postcodes and cities are sorted by text and cut to their limits.
    streets: Vec<Streets>,
    /// Per shard, each street's final position by its first one.
    renumber: Vec<Vec<u32>>,
    /// Per shard, for each street (by first position) with two or more postcodes, where each of
    /// its first postcode slots went (0: cut). A street with fewer keeps its slots.
    slots: Vec<HashMap<u32, Box<[u8]>>>,
}

fn canonical_streets(shards: Vec<Mutex<AddrShard>>, strings: &Strings) -> Canon {
    let by_text = |ids: &mut Vec<u32>, max: usize| {
        ids.sort_unstable_by(|&a, &b| strings.get(a).cmp(strings.get(b)));
        ids.truncate(max);
    };
    let per_shard: Vec<_> = shards
        .into_par_iter()
        .map(|m| {
            let AddrShard { ids, mut streets } = m.into_inner().unwrap();
            let mut keys: Vec<(AddrKey, u32)> = ids.into_iter().collect();
            keys.sort_unstable();
            let mut renumber = vec![0; keys.len()];
            let mut slots = HashMap::new();
            let sorted = keys
                .into_iter()
                .enumerate()
                .map(|(new, (key, old))| {
                    renumber[old as usize] = new as u32;
                    let mut agg = std::mem::take(&mut streets[old as usize]);
                    if agg.pcs.len() > 1 {
                        let first = agg.pcs.clone();
                        by_text(&mut agg.pcs, MAX_POSTCODES);
                        let moved = first
                            .iter()
                            .map(|id| {
                                agg.pcs
                                    .iter()
                                    .position(|p| p == id)
                                    .map_or(0, |i| i as u8 + 1)
                            })
                            .collect();
                        slots.insert(old, moved);
                    }
                    by_text(&mut agg.cities, MAX_CITIES);
                    (key, agg)
                })
                .collect();
            (sorted, renumber, slots)
        })
        .collect();
    let mut canon = Canon {
        streets: vec![],
        renumber: vec![],
        slots: vec![],
    };
    for (streets, renumber, slots) in per_shard {
        canon.streets.push(streets);
        canon.renumber.push(renumber);
        canon.slots.push(slots);
    }
    canon
}

/// GeoNames postal-code dump rows, averaged per (country, normalized code).
fn geonames_postcodes(path: &str) -> HashMap<PcKey, PcAgg> {
    let mut agg: HashMap<PcKey, PcAgg> = HashMap::new();
    for l in std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .lines()
    {
        let c: Vec<&str> = l.split('\t').collect();
        let (Some(cc), Some(pc)) = (c.first().filter(|s| s.len() == 2), c.get(1)) else {
            continue;
        };
        let (Some(y), Some(x)) = (
            c.get(9).and_then(|v| v.parse::<f64>().ok()),
            c.get(10).and_then(|v| v.parse::<f64>().ok()),
        ) else {
            continue;
        };
        let cc = country(Some(cc));
        agg.entry((cc, pc_norm(pc).into()))
            .or_default()
            .add(pc, quantize(x), quantize(y), 1);
        // "EC1A 1BB" also places its outward code "EC1A" (GB), "1012 JS" its PC4 (NL) and
        // "K1A 0B1" its FSA (CA) at the mean of their full postcodes.
        if let Some((area, _)) = pc.split_once(' ') {
            agg.entry((cc, pc_norm(area).into())).or_default().add(
                area,
                quantize(x),
                quantize(y),
                1,
            );
        }
    }
    agg
}

/// `items` (weight, value), heaviest first, dealt into `n` groups of near-equal total weight.
fn balanced<T>(items: Vec<(u64, T)>, n: usize) -> Vec<Vec<T>> {
    let mut groups: Vec<(u64, Vec<T>)> = (0..n).map(|_| (0, vec![])).collect();
    for (weight, item) in items {
        let lightest = groups.iter_mut().min_by_key(|g| g.0).expect("n > 0");
        lightest.0 += weight;
        lightest.1.push(item);
    }
    groups.into_iter().map(|g| g.1).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let run = match args.get(1).map(String::as_str) {
        Some("hash") => {
            println!("{}", job::BUILDER_HASH);
            return;
        }
        Some("job") => job::job(args),
        Some("pull") => job::pull(args),
        Some("digest") => digest::digest(&args),
        _ => {
            build(&args);
            return;
        }
    };
    if let Err(e) = run {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

/// Datasets that Overture's address rows cite: mostly OpenAddresses source names.
fn overture_datasets(dir: &Path) -> HashSet<String> {
    tasks(dir)
        .par_iter()
        .map(|t| {
            let mut found = HashSet::new();
            fetch::carried_datasets(read(t, &["sources.list.element.dataset"]), &mut found);
            found
        })
        .reduce(HashSet::new, |mut a, b| {
            a.extend(b);
            a
        })
}

fn build(args: &[String]) {
    let (Some(release), Some(out)) = (args.get(1), args.get(2)) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    let opt = |flag: &str| -> Vec<String> {
        args.windows(2)
            .filter(|w| w[0] == flag)
            .map(|w| w[1].clone())
            .collect()
    };
    // Earlier files win for a postcode that several define.
    let geonames = opt("--geonames-postal");
    let (oa_dirs, osm_dirs) = (opt("--openaddresses"), opt("--osm"));
    let dir = Path::new(release);
    let _ = std::fs::remove_dir_all(out);
    std::fs::create_dir_all(out).unwrap();
    let available = mem::available();
    let rss0 = mem::rss().unwrap_or(0);
    match available {
        Some(a) => println!("memory available at start: {:.1} GB", a as f64 / GB),
        None => println!("memory available at start: unknown"),
    }
    // Memory available at start minus what this process has grown by since.
    let headroom = || {
        available.map_or(u64::MAX, |a| {
            a.saturating_sub(mem::rss().unwrap_or(0).saturating_sub(rss0))
        })
    };
    let (t0, mut t) = (Instant::now(), Instant::now());
    let mut lap = |what: &str| {
        let rss = mem::rss().map_or(String::new(), |r| format!(", rss {:.1} GB", r as f64 / GB));
        println!("{what}: {:.1}s{rss}", t.elapsed().as_secs_f64());
        progress::touch();
        t = Instant::now();
    };

    progress::phase("Parse places and boundaries");
    let (mut places, idx) = load_places(&dir.join("division"), out);
    lap(&format!("places ({})", places.len()));
    let mut assigner = load_assigner(&dir.join("division_area"), &places, &idx);
    assigner.bbox = opt("--bbox").last().map(|b| {
        let v: Vec<f64> = b.split(',').filter_map(|v| v.trim().parse().ok()).collect();
        v.try_into().unwrap_or_else(|_| {
            eprintln!("--bbox takes min lon,min lat,max lon,max lat");
            std::process::exit(2)
        })
    });
    lap("polygons");
    let (polygons, vertices, bytes) = write_areas(
        &dir.join("division_area"),
        &idx,
        assigner.bbox,
        Path::new(out),
    );
    drop(idx);
    lap(&format!(
        "areas ({polygons} polygons, {vertices} vertices, {:.1} MB)",
        bytes as f64 / 1e6
    ));
    if let Some(cities) = opt("--geonames-places").last() {
        let admin1 = opt("--geonames-admin1");
        let added = geonames::add_places(
            &mut places,
            &assigner,
            Path::new(cities),
            admin1.last().map(Path::new),
            Path::new(out),
        );
        lap(&format!("geonames places ({added} added)"));
    }
    let street_spill = StreetSpill::new(&Path::new(out).join(reverse::STREETS_SPILL));
    let segs = pass_segments(&dir.join("segment"), &assigner, &street_spill);
    street_spill.finish();
    lap("segments");
    let spill = NumberSpill::new(Path::new(out).join(".numbers-spill"), NUMBER_SHARDS);
    let shared = Shared::new(&assigner, &places, &spill);
    progress::phase("Parse addresses");
    pass_addresses(&dir.join("address"), &shared);
    lap(&format!(
        "overture addresses ({} rows)",
        shared.rows.swap(0, Relaxed)
    ));
    if dir.join("place").is_dir() {
        std::fs::write(
            Path::new(out).join("PLACES-LICENSES.txt"),
            concat!(
                "Overture Maps Foundation places, https://docs.overturemaps.org/attribution/\n",
                "Includes Foursquare Open Source Places.\n",
                "Copyright 2024 Foursquare Labs, Inc. All rights reserved.\n",
                "Includes AllThePlaces (CC0-1.0).\n",
                "Modified: addresses filtered, split, normalized and merged into street/house tables.\n\n",
                include_str!("../../../LICENSES/CDLA-Permissive-2.0.txt"),
                "\n",
                include_str!("../../../LICENSES/Apache-2.0.txt"),
            ),
        )
        .unwrap();
        pass_places(&dir.join("place"), &shared, &segs);
        lap(&format!(
            "overture places ({} rows)",
            shared.rows.swap(0, Relaxed)
        ));
    }
    for d in &oa_dirs {
        let files = sources::openaddresses(
            Path::new(d),
            Path::new(out),
            args.iter().any(|a| a == "--openaddresses-any-licence"),
            &shared,
        );
        lap(&format!(
            "openaddresses {d} ({files} files, {} rows)",
            shared.rows.swap(0, Relaxed)
        ));
    }
    for d in &osm_dirs {
        let files = sources::osm(Path::new(d), &shared);
        lap(&format!(
            "osm {d} ({files} files, {} rows)",
            shared.rows.swap(0, Relaxed)
        ));
    }
    // `--osm-landmarks` extracts are read for landmarks only: their addresses would change answers
    // in countries whose addresses come from Overture.
    let landmark_dirs: Vec<PathBuf> = osm_dirs
        .iter()
        .chain(&opt("--osm-landmarks"))
        .map(PathBuf::from)
        .collect();
    if !landmark_dirs.is_empty() {
        let found = landmarks::read(&landmark_dirs);
        let (counts, placeless) = landmarks::write(Path::new(out), &found, &assigner);
        opengeocoding::index::landmark::build(Path::new(out), places.len()).unwrap();
        let counts: Vec<String> = landmarks::CATEGORIES
            .iter()
            .zip(counts)
            .map(|(c, n)| format!("{c} {n}"))
            .collect();
        lap(&format!(
            "landmarks ({}; {placeless} in no place)",
            counts.join(", ")
        ));
    }
    progress::phase("Merge streets, postcodes and house numbers");
    let Shared {
        streets,
        postcodes,
        strings,
        ..
    } = shared;
    let strings = strings.table();
    let Canon {
        streets,
        renumber,
        slots,
    } = canonical_streets(streets, &strings);
    let mut postcodes: HashMap<PcKey, PcAgg> = postcodes
        .into_iter()
        .flat_map(|m| m.into_inner().unwrap())
        .collect();
    let before = postcodes.len();
    // GeoNames' points are official centroids; a key seen on only a few addresses (often an
    // outward code typed alone, like "EC1A") takes GeoNames' instead. Earlier files win.
    let mut from_geonames = std::collections::HashSet::new();
    for path in &geonames {
        for (k, v) in geonames_postcodes(path) {
            if !from_geonames.contains(&k) && postcodes.get(&k).is_none_or(|e| e.n < 10) {
                from_geonames.insert(k.clone());
                postcodes.insert(k, v);
            }
        }
    }
    println!(
        "  {} postcodes from address data, {} from GeoNames ({} new)",
        before,
        from_geonames.len(),
        postcodes.len() - before
    );
    let postcodes: Vec<(PcKey, PcAgg, u32)> = postcodes
        .into_par_iter()
        .map(|(k, v)| {
            let place = assigner
                .place(mean(v.sx, v.n), mean(v.sy, v.n))
                .unwrap_or(NO_PLACE);
            (k, v, place)
        })
        .collect();
    drop(assigner);

    // Each shard sorts in memory; allow as many at once as half the headroom fits.
    let shard_bytes = spill.bytes() / NUMBER_SHARDS as u64;
    let parallel = (headroom() / 2 / shard_bytes.max(1)).clamp(1, NUMBER_SHARDS as u64) as usize;
    println!("  sorting {parallel} of {NUMBER_SHARDS} house-number shards at a time");
    // A house names its street and postcode slot as first numbered; the street's final number and
    // postcode list are known only now.
    let resolve = |sid: u32, slot: u8| -> (u32, u8) {
        let shard = (sid & 0xff) as usize;
        let old = sid >> 8;
        let moved = match slots[shard].get(&old) {
            Some(moved) if slot > 0 => moved.get(slot as usize - 1).copied().unwrap_or(0),
            _ => slot,
        };
        (((renumber[shard][old as usize]) << 8) | shard as u32, moved)
    };
    let n = spill.write_fsts(out, parallel, &resolve);
    drop((renumber, slots));
    lap(&format!("house numbers ({n})"));

    progress::phase("Build search index");
    let headroom = headroom();
    if headroom < 2 * mem::MIN_TOTAL {
        progress::warn(format!(
            "{:.1} GB headroom left; the writer takes {:.1} GB anyway",
            headroom as f64 / GB,
            mem::MIN_TOTAL as f64 / GB
        ));
    }
    let (threads, budget) = mem::writer_budget(
        headroom,
        MAX_WRITER_THREADS.min(rayon::current_num_threads()),
    );
    println!(
        "  writer: {threads} threads, {:.2} GB arena (headroom {:.1} GB)",
        budget as f64 / GB,
        headroom.min(1 << 50) as f64 / GB
    );
    let (index, writer, f) = schema::create(out, threads, budget);
    places
        .par_iter()
        .enumerate()
        .filter(|(_, p)| !p.names.is_empty())
        .for_each(|(i, p)| {
            let mut d = doc!(
                f.kind => "p",
                f.names => p.names.replace(NAME_SEP, " "),
                f.country => country_str(&p.country),
                f.ids => i.to_string(),
                f.name => p.names.split(NAME_SEP).next().unwrap_or(""),
                f.id => i as u64,
                f.importance => p.population + place_weight(p.subtype),
            );
            if let Some((x, y)) = p.xy {
                d.add_f64(f.x, x);
                d.add_f64(f.y, y);
            }
            writer.add_document(d).unwrap();
        });
    lap("place docs");

    let street_doc = |sid: u32,
                      cc: &str,
                      place: u32,
                      names: &[&str],
                      pcs: &[u32],
                      cities: &[u32],
                      postal_towns: &[(u32, u32, u32, u32)],
                      (x, y): (f64, f64),
                      weight: u64| {
        let cities: Vec<&str> = cities.iter().map(|&c| strings.get(c)).collect();
        let mut d = doc!(
            f.kind => "s",
            f.names => names.join(" "),
            f.city => cities.join(" "),
            f.country => cc,
            f.id => sid as u64,
            f.place => place as u64,
            f.importance => weight,
            f.x => x,
            f.y => y,
        );
        for n in names {
            d.add_text(f.name, n);
        }
        for &pc in pcs {
            d.add_text(f.pcode, strings.get(pc));
        }
        let mut towns: Vec<_> = postal_towns
            .iter()
            .map(|&(pc, number, city, source)| {
                (
                    strings.get(pc),
                    strings.get(number),
                    strings.get(city),
                    strings.get(source),
                )
            })
            .collect();
        towns.sort_unstable();
        for town in towns {
            d.add_text(f.postal_town, serde_json::to_string(&town).unwrap());
        }
        if place != NO_PLACE {
            d.add_text(f.ids, place.to_string());
            for a in places[place as usize].anc.iter() {
                d.add_text(f.ids, a.to_string());
            }
            let mut toks: Vec<String> = names
                .iter()
                .flat_map(|n| fold(n).split(' ').map(str::to_string).collect::<Vec<_>>())
                .collect();
            toks.sort_unstable();
            toks.dedup();
            for tok in toks.iter().filter(|t| !t.is_empty()) {
                d.add_text(f.ctok, format!("{place}|{tok}"));
            }
        }
        writer.add_document(d).unwrap();
    };

    // A segment's spill id -> its street's sid, by position: shard base plus place in shard.
    let seg_base: Vec<u32> = segs
        .iter()
        .scan(0, |n, m| {
            let base = *n;
            *n += m.lock().unwrap().len() as u32;
            Some(base)
        })
        .collect();
    let seg_count = seg_base.last().copied().unwrap_or(0)
        + segs.last().map_or(0, |m| m.lock().unwrap().len() as u32);
    let seg_sid: Vec<AtomicU32> = (0..seg_count).map(|_| AtomicU32::new(u32::MAX)).collect();
    let seg_at = |id: u32| seg_base[(id & 0xff) as usize] as usize + (id >> 8) as usize;
    let (merged, max_sid) = (AtomicU64::new(0), AtomicU32::new(0));
    streets.par_iter().enumerate().for_each(|(sh, s)| {
        for (pos, ((cc, place, key), st)) in s.iter().enumerate() {
            let sid = ((pos as u32) << 8) | sh as u32;
            max_sid.fetch_max(sid, Relaxed);
            // A segment belongs to its place's country: two address streets that differ only in
            // country would otherwise race for it.
            let seg = (*place != NO_PLACE && places[*place as usize].country == *cc)
                .then(|| {
                    let k = (*place, key.clone());
                    segs[shard(&k)].lock().unwrap().remove(&k)
                })
                .flatten();
            if let Some(g) = &seg {
                seg_sid[seg_at(g.id)].store(sid, Relaxed);
            }
            let primary = seg.as_ref().map_or(&*st.name, |g| &*g.name);
            let mut names = vec![primary];
            if let Some(g) = &seg {
                merged.fetch_add(1, Relaxed);
                names.extend(
                    g.alt
                        .iter()
                        .take(MAX_ALT_NAMES)
                        .map(|a| &**a)
                        .filter(|a| *a != primary),
                );
            }
            let n = st.n + seg.as_ref().map_or(0, |g| g.n);
            // Places contribute no centroid or weight and are admitted only on a known segment.
            let point = match &seg {
                Some(g) if st.n == 0 => (mean(g.sx, g.n), mean(g.sy, g.n)),
                _ => (mean(st.sx, st.n), mean(st.sy, st.n)),
            };
            street_doc(
                sid,
                country_str(cc),
                *place,
                &names,
                &st.pcs,
                &st.cities,
                &st.postal_towns,
                point,
                n,
            );
        }
    });
    let addr_streets: u64 = streets.iter().map(|s| s.len() as u64).sum();
    drop(streets);
    // Streets only the segments know, numbered in key order after the others.
    let mut only: Vec<(SegKey, SegAgg)> = segs
        .into_iter()
        .flat_map(|m| m.into_inner().unwrap())
        .collect();
    only.par_sort_unstable_by(|a, b| a.0.cmp(&b.0));
    let first = max_sid.load(Relaxed) + 1;
    only.par_iter()
        .enumerate()
        .for_each(|(i, ((place, _), g))| {
            seg_sid[seg_at(g.id)].store(first + i as u32, Relaxed);
            let names: Vec<&str> = std::iter::once(&*g.name)
                .chain(
                    g.alt
                        .iter()
                        .take(MAX_ALT_NAMES)
                        .map(|a| &**a)
                        .filter(|a| *a != &*g.name),
                )
                .collect();
            street_doc(
                first + i as u32,
                country_str(&places[*place as usize].country),
                *place,
                &names,
                &[],
                &[],
                &[],
                (mean(g.sx, g.n), mean(g.sy, g.n)),
                g.n,
            );
        });
    let seg_only = only.len() as u64;
    drop(only);
    let merged = merged.load(Relaxed);
    lap(&format!(
        "street docs ({} streets: {merged} merged, {} address only, {seg_only} segment only)",
        addr_streets + seg_only,
        addr_streets - merged
    ));

    postcodes.par_iter().for_each(|((cc, pc), v, place)| {
        writer
            .add_document(doc!(
                f.kind => "z",
                f.pcode => &**pc,
                f.country => country_str(cc),
                f.name => &*v.display,
                f.importance => v.n,
                f.place => *place as u64,
                f.x => mean(v.sx, v.n),
                f.y => mean(v.sy, v.n),
            ))
            .unwrap();
    });
    lap(&format!("postcode docs ({})", postcodes.len()));

    let mut writer = writer;
    writer.commit().unwrap();
    lap(&format!(
        "index commit ({} segments)",
        index.searchable_segment_ids().unwrap().len()
    ));
    let mut segments: Vec<_> = index
        .searchable_segment_metas()
        .unwrap()
        .iter()
        .map(|m| (m.max_doc() as u64, m.id()))
        .collect();
    segments.sort_by_key(|s| std::cmp::Reverse(s.0));
    let merges: Vec<_> = balanced(segments, FINAL_SEGMENTS)
        .iter()
        .filter(|ids| ids.len() > 1)
        .map(|ids| writer.merge(ids))
        .collect();
    for merge in merges {
        merge.wait().unwrap();
    }
    // the merged-away files, or the sealed index would carry both copies
    writer.garbage_collect_files().wait().unwrap();
    lap(&format!(
        "segment merge ({} segments)",
        index.searchable_segment_ids().unwrap().len()
    ));
    progress::phase("Build reverse lookup");
    let (houses, streets) = reverse::build(Path::new(out), &index, f.kind, |id| {
        Some(seg_sid[seg_at(id)].load(Relaxed)).filter(|&sid| sid != u32::MAX)
    })
    .unwrap();
    lap(&format!(
        "reverse lookup ({houses} house records, {streets} street records)"
    ));
    println!("total: {:.1}s", t0.elapsed().as_secs_f64());
    if !progress::in_job() {
        progress::summary();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postal_towns_stay_compact_when_streets_merge() {
        let town = (0, 1, 2, 3);
        let mut merged = AddrAgg::default();
        for _ in 0..2 {
            merged.add(AddrAgg {
                postal_towns: vec![town],
                ..Default::default()
            });
        }
        assert_eq!(merged.postal_towns, [town]);
        assert_eq!(std::mem::size_of_val(&merged.postal_towns[0]), 16);
        assert_eq!(merged.postal_towns.capacity(), 1);
        merged.add(AddrAgg {
            postal_towns: vec![(0, 1, 4, 3)],
            ..Default::default()
        });
        assert_eq!(merged.postal_towns, [town, (0, 1, 4, 3)]);
    }

    #[test]
    fn postcodes_sort_by_text_and_slots_follow() {
        let interner = Interner::new();
        let ids: Vec<u32> = (0..25)
            .rev()
            .map(|i| interner.id(&format!("{i:02}")))
            .collect();
        let strings = interner.table();
        let mut shard = AddrShard::default();
        // street 0 holds its postcodes in arrival order "24", "23", ... "00"; street 1 holds one
        for pcs in [ids.clone(), vec![ids[3]]] {
            let key: AddrKey = (*b"us", 0, format!("s{}", shard.streets.len()).into());
            shard.ids.insert(key, shard.streets.len() as u32);
            shard.streets.push(AddrAgg {
                pcs,
                ..Default::default()
            });
        }
        let canon = canonical_streets(vec![Mutex::new(shard)], &strings);
        let (street, _) = (&canon.streets[0][0].1, 0);
        let text: Vec<&str> = street.pcs.iter().map(|&p| strings.get(p)).collect();
        assert_eq!(text.len(), MAX_POSTCODES);
        assert_eq!((text[0], text[19]), ("00", "19"));
        // "24" was first in arrival order and is cut; "00" was last (slot 25) and is now first
        let moved = &canon.slots[0][&0];
        assert_eq!((moved[0], moved[24]), (0, 1));
        assert_eq!(moved[24 - 19], 20);
        assert!(
            !canon.slots[0].contains_key(&1),
            "one postcode keeps its slot"
        );
    }

    #[test]
    fn only_a_number_then_a_street_is_a_house() {
        assert_eq!(
            freeform_house("553 Sư Vạn Hạnh"),
            Some(("553", "Sư Vạn Hạnh"))
        );
        assert_eq!(
            freeform_house("12B High Street"),
            Some(("12B", "High Street"))
        );
        assert_eq!(freeform_house("991 พระราม 1"), Some(("991", "พระราม 1")));
        for no in [
            "315, 2, Kaggadasapura Main Rd",
            "Puan market, 41 Libby Road",
            "Calle 10",
            "12 3rd Street",
            "12AB Road",
            "12",
            "Nandinagar",
            "548/11 Rajajipuram",
        ] {
            assert_eq!(freeform_house(no), None, "{no}");
        }
    }

    use super::balanced;

    #[test]
    fn balanced_groups_split_the_weight_evenly() {
        let items = [9, 7, 5, 4, 3, 2, 1].map(|w| (w, w)).to_vec();
        let weights: Vec<u64> = balanced(items, 3).iter().map(|g| g.iter().sum()).collect();
        assert_eq!(weights.iter().sum::<u64>(), 31);
        assert!(weights.iter().all(|w| (10..=11).contains(w)), "{weights:?}");
        assert_eq!(balanced(vec![(1, 'a')], 4).iter().flatten().count(), 1);
    }
}
