//! Builds the address index from a local Overture release directory (division, division_area,
//! segment, address and, when present, place parquet), plus optional OSM extracts, OpenAddresses
//! exports and GeoNames postcodes, without Nominatim. Only house numbers are spilled to disk.
mod custom_names;
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
use geo::{Assigner, KINDS, Poly, PolyFile, wkb_line, wkb_point, wkb_polygons};
use hashbrown::HashTable;
use opengeocoding::index::{
    reverse::{self, StreetSpill, street_edges},
    schema::{
        self, Bucket, Num, NumberSpill, TOWN_CITY, fold, fold_into, held_slot, pack, pc_norm,
        with_rank,
    },
};
use parquet::arrow::{
    ProjectionMask,
    arrow_reader::{ArrowReaderMetadata, ParquetRecordBatchReaderBuilder},
};
use rayon::prelude::*;
use std::{
    borrow::Cow,
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

const USAGE: &str = "usage: build_index <overture-release-dir> <out-dir> [--openaddresses DIR [--openaddresses-any-licence]] [--osm DIR] [--osm-landmarks DIR] [--korea-shops ZIP] [--geonames-postal FILE]... [--geonames-places FILE [--geonames-admin1 FILE]] [--bbox MINLON,MINLAT,MAXLON,MAXLAT]
       build_index job --store URL --index ID --builder-hash HASH --sources FILE --scratch DIR --models DIR [--models-version v1] [--sources-mirror URL]
       build_index fetch --store URL --sources FILE --scratch DIR [--sources-mirror URL]
       build_index pull --store URL --index ID --to DIR [--models DIR] [--models-version v1]
       build_index digest DIR [--dump FILE]
       build_index hash";
const SHARDS: usize = 256;
const MAX_POSTCODES: usize = 20;
/// Postcodes a street holds while it is built: a house's slot is 6 bits, under its source rank.
/// A house whose postcode came too late for one carries the postcode itself (in the long list),
/// and its street keeps it in `AddrShard::more`, so the first `MAX_POSTCODES` by text, which are
/// kept in the end, do not depend on arrival order.
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

fn hash<K: Hash + ?Sized>(k: &K) -> u64 {
    BuildHasherDefault::<DefaultHasher>::default().hash_one(k)
}

fn shard<K: Hash + ?Sized>(k: &K) -> usize {
    (hash(k) % SHARDS as u64) as usize
}

/// A key's hash within its shard's table. Every key of a shard shares the hash's low byte,
/// which the table would otherwise use to pick buckets.
fn mix(h: u64) -> u64 {
    (h >> 8).wrapping_mul(0x9e37_79b9_7f4a_7c15)
}

/// Makes room for `extra` more, growing by an eighth rather than doubling: these tables are most
/// of the build's memory, and doubling left up to half of one unused.
fn grow<T>(v: &mut Vec<T>, extra: usize) {
    if v.capacity() - v.len() < extra {
        v.reserve_exact(extra.max(v.len() / 8 + 1024));
    }
}

/// Strings back to back, each after its length as a LEB128 varint, named by offset. Tens of
/// millions of boxed strings cost a 16-byte pointer each and the allocator's rounding.
#[derive(Default)]
struct Arena(Vec<u8>);

impl Arena {
    fn push(&mut self, s: &str) -> u32 {
        let at = u32::try_from(self.0.len()).expect("string arena over 4 GiB");
        grow(&mut self.0, s.len() + 5);
        let mut n = s.len();
        while n >= 0x80 {
            self.0.push(n as u8 | 0x80);
            n >>= 7;
        }
        self.0.push(n as u8);
        self.0.extend_from_slice(s.as_bytes());
        at
    }

    fn get(&self, at: u32) -> &str {
        let (mut i, mut n, mut shift) = (at as usize, 0, 0);
        loop {
            let b = self.0[i];
            i += 1;
            n |= usize::from(b & 0x7f) << shift;
            if b < 0x80 {
                break;
            }
            shift += 7;
        }
        std::str::from_utf8(&self.0[i..i + n]).expect("arena holds whole strings")
    }
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
    subtype: &'static str,
    country: [u8; 2],
    /// ISO 3166-2 code of the region, or empty.
    region: Str,
    /// All names, primary first, joined by NAME_SEP.
    names: Str,
    population: u64,
    xy: Option<(f64, f64)>,
    anc: Box<[u32]>,
}

/// What `places.tsv` takes of a place beyond `Place`, held only until it is written.
struct PlaceRow {
    id: Str,
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

fn load_places(dir: &Path) -> (Vec<Place>, Vec<PlaceRow>, HashMap<Str, u32>) {
    let mut places: Vec<(Place, PlaceRow, Vec<Str>)> = tasks(dir)
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
                    for &(code, language, name) in custom_names::REGION_ALIASES {
                        if sub.value(i) == "region" && text(region, i) == Some(code) {
                            if !names.contains(&name) {
                                names.push(name);
                            }
                            extra.push((format!("alt_name:{language}"), name));
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
                    let place = Place {
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
                        anc: Box::default(),
                    };
                    let row = PlaceRow {
                        id: id.value(i).into(),
                        parent: text(parent, i).map(Str::from),
                        row: row.into(),
                    };
                    out.push((place, row, parents));
                }
            }
            out
        })
        .collect();
    places.par_sort_unstable_by(|a, b| a.1.id.cmp(&b.1.id));
    let idx: HashMap<Str, u32> = places
        .iter()
        .enumerate()
        .map(|(i, p)| (p.1.id.clone(), i as u32))
        .collect();
    places
        .par_iter_mut()
        .enumerate()
        .for_each(|(i, (p, _, parents))| {
            let mut anc: Vec<u32> = parents
                .iter()
                .filter_map(|d| idx.get(d).copied())
                .filter(|&a| a != i as u32)
                .collect();
            anc.sort_unstable();
            anc.dedup();
            p.anc = anc.into();
        });
    let (places, rows) = places.into_iter().map(|(p, r, _)| (p, r)).unzip();
    (places, rows, idx)
}

/// `places.tsv`: subtype, class, country, ISO region, admin level, population, x, y,
/// bbox (xmin, ymin, xmax, ymax), parent id, primary name, `language=name` pairs, Wikidata item.
fn write_places(rows: Vec<PlaceRow>, idx: &HashMap<Str, u32>, out: &str) {
    let mut w = std::io::BufWriter::new(File::create(Path::new(out).join("places.tsv")).unwrap());
    for PlaceRow { parent, row, .. } in rows {
        use std::io::Write;
        let parent = parent.and_then(|d| idx.get(&d).copied());
        let (head, tail) = row.split_at(
            row.match_indices('\t')
                .nth(11)
                .map_or(row.len(), |(k, _)| k + 1),
        );
        let parent = parent.map_or(String::new(), |v| v.to_string());
        writeln!(w, "{head}{parent}\t{tail}").unwrap();
    }
    std::io::Write::flush(&mut w).unwrap();
}

fn reconcile_parents(
    places: &mut [Place],
    rows: &mut [PlaceRow],
    idx: &HashMap<Str, u32>,
    assigner: &Assigner,
) {
    let changes: HashMap<u32, (u32, u32)> = places
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let old = *idx.get(rows[i].parent.as_ref()?)?;
            if p.subtype != "locality"
                || places[old as usize].subtype != "region"
                || places[old as usize].country != p.country
            {
                return None;
            }
            let (x, y) = p.xy?;
            let new = assigner.region(x, y, p.country)?;
            (old != new).then_some((i as u32, (old, new)))
        })
        .collect();
    let ancestry: Vec<_> = places
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let &(old, new) = std::iter::once(i as u32)
                .chain(p.anc.iter().copied())
                .find_map(|id| changes.get(&id))?;
            let mut anc = p.anc.to_vec();
            anc.retain(|a| *a != old && !places[old as usize].anc.contains(a));
            anc.push(new);
            anc.extend_from_slice(&places[new as usize].anc);
            anc.sort_unstable();
            anc.dedup();
            Some((
                i,
                anc.into_boxed_slice(),
                places[new as usize].region.clone(),
            ))
        })
        .collect();
    for (i, &(old, new)) in &changes {
        rows[*i as usize].parent = Some(rows[new as usize].id.clone());
        println!(
            "  region parent {}: {} -> {}",
            rows[*i as usize].id, rows[old as usize].id, rows[new as usize].id
        );
    }
    for (i, anc, region) in ancestry {
        let (p, row) = (&mut places[i], &mut rows[i]);
        let mut fields: Vec<_> = row.row.split('\t').collect();
        fields[3] = &region;
        row.row = fields.join("\t").into();
        p.region = region;
        p.anc = anc;
    }
}

fn load_assigner(
    dir: &Path,
    places: &[Place],
    idx: &HashMap<Str, u32>,
    file: PolyFile,
) -> Assigner {
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
                        out.extend(Poly::new(place, kind as u8, xy, &rings, &file));
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
    let countries = places
        .iter()
        .map(|p| {
            let mut ancestors = p
                .anc
                .iter()
                .map(|&i| &places[i as usize])
                .filter(|a| a.subtype == "country")
                .map(|a| a.country);
            let first = ancestors.next().unwrap_or(p.country);
            let sovereign = if ancestors.all(|cc| cc == first) {
                first
            } else {
                p.country
            };
            (p.country, sovereign)
        })
        .collect();
    Assigner::new(polys, file, towns, countries)
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

/// A task's aggregate of one segment street.
#[derive(Default)]
struct SegAgg {
    name: Str,
    /// Sorted, without duplicates.
    alt: Vec<Str>,
    sx: i64,
    sy: i64,
    /// Segment rows; all of a world release's are a tenth of `u32::MAX`.
    n: u32,
}

/// A segment street as the shards hold it; its id in the spilled street edges is its position
/// in its shard, then the shard.
#[derive(Clone, Copy)]
struct Seg {
    sx: i64,
    sy: i64,
    place: u32,
    /// `text` offsets of its key and of its name.
    key: u32,
    name: u32,
    n: u32,
}

#[derive(Default)]
struct SegShard {
    table: HashTable<u32>,
    segs: Vec<Seg>,
    text: Arena,
    /// Other names, sorted, of the few segments that have them.
    alts: HashMap<u32, Box<[Str]>>,
}

impl SegShard {
    fn key(&self, pos: u32) -> (u32, &str) {
        let s = &self.segs[pos as usize];
        (s.place, self.text.get(s.key))
    }

    fn find(&self, h: u64, place: u32, key: &str) -> Option<u32> {
        let found = self.table.find(mix(h), |&i| self.key(i) == (place, key));
        found.copied()
    }

    /// Merges a task's segment street into the shard's, which it creates if new; returns its
    /// position.
    fn add(&mut self, h: u64, place: u32, key: &str, agg: SegAgg) -> u32 {
        let pos = match self.find(h, place, key) {
            Some(pos) => pos,
            None => {
                let pos = self.segs.len() as u32;
                assert!(pos < 1 << 24, "segment ids overflow");
                let (key, name) = (self.text.push(key), self.text.push(&agg.name));
                grow(&mut self.segs, 1);
                self.segs.push(Seg {
                    sx: 0,
                    sy: 0,
                    place,
                    key,
                    name,
                    n: 0,
                });
                let SegShard {
                    table, segs, text, ..
                } = self;
                table.insert_unique(mix(h), pos, |&i| {
                    let s = &segs[i as usize];
                    mix(hash(&(s.place, text.get(s.key))))
                });
                pos
            }
        };
        let s = &mut self.segs[pos as usize];
        if prefer(&agg.name, self.text.get(s.name)) {
            s.name = self.text.push(&agg.name);
        }
        (s.sx, s.sy, s.n) = (s.sx + agg.sx, s.sy + agg.sy, s.n + agg.n);
        if !agg.alt.is_empty() {
            let alts = self.alts.entry(pos).or_default();
            let mut all = std::mem::take(alts).into_vec();
            for a in &agg.alt {
                insert_name(&mut all, a);
            }
            *alts = all.into();
        }
        pos
    }
}

/// The shard and position of the segment street `(place, key)`.
fn find_seg(segs: &[SegShard], place: u32, key: &str) -> Option<(usize, u32)> {
    let h = hash(&(place, key));
    let sh = (h % SHARDS as u64) as usize;
    segs[sh].find(h, place, key).map(|pos| (sh, pos))
}

/// A segment's edges as `street_edges` cuts them.
type StreetEdges = Vec<(u64, (i8, i8))>;

/// Segment classes left out of `streets.bin`: a point beside a path is not on a street.
const NOT_STREETS: [&str; 6] = ["footway", "steps", "path", "cycleway", "bridleway", "track"];

fn pass_segments(dir: &Path, a: &Assigner, spill: &StreetSpill) -> Vec<SegShard> {
    let shards = sharded::<SegShard>();
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
                let h = hash(&(place, &*k));
                let s = (h % SHARDS as u64) as usize;
                let id = shards[s].lock().unwrap().add(h, place, &k, v) << 8 | s as u32;
                edges.extend(mine.into_iter().map(|(key, d)| (key, d, id)));
            }
        }
        spill.add(&mut edges);
    });
    println!("  {} segment rows", rows.into_inner());
    shards
        .into_iter()
        .map(|m| m.into_inner().unwrap())
        .collect()
}

#[derive(Default)]
struct AddrAgg {
    name: Str,
    primary: bool,
    /// Every house of it is a shop's land-lot address, so its name is a neighbourhood's, not a
    /// road's; reverse lookups skip it.
    lot: bool,
    /// Interned normalized postcodes, in arrival order (houses refer to positions), and postal city
    /// names, sorted by id. Once every source is read both are sorted by text and cut to
    /// `MAX_POSTCODES` and `MAX_CITIES`.
    pcs: Vec<u32>,
    cities: Vec<u32>,
    /// Sorted `town`s.
    postal_towns: Vec<u64>,
    sx: i64,
    sy: i64,
    n: u32,
}

/// A street as the shared shards hold it until every source is read.
#[derive(Default)]
struct Street {
    /// Its postcodes, then its cities (as `AddrAgg` holds them), in one box: most streets have
    /// a few of each, and tens of millions of separate boxes cost more than their contents.
    lists: Box<[u32]>,
    sx: i64,
    sy: i64,
    n: u32,
    place: u32,
    /// `text` offsets of its key and of its name.
    key: u32,
    name: u32,
    cc: [u8; 2],
    pcs: u8,
    primary: bool,
    lot: bool,
}

impl Street {
    fn pcs(&self) -> &[u32] {
        &self.lists[..usize::from(self.pcs)]
    }

    fn cities(&self) -> &[u32] {
        &self.lists[usize::from(self.pcs)..]
    }

    fn set_lists(&mut self, pcs: &[u32], cities: &[u32]) {
        self.pcs = u8::try_from(pcs.len()).expect("postcodes are capped");
        self.lists = [pcs, cities].concat().into();
    }

    /// `other`'s name, lists and point; its postal towns are spilled apart.
    fn add(&mut self, text: &mut Arena, other: &AddrAgg) {
        if prefer(&other.name, text.get(self.name)) {
            self.name = text.push(&other.name);
        }
        let mut pcs = self.pcs().to_vec();
        for &p in &other.pcs {
            push_unique(&mut pcs, p, MAX_HELD_POSTCODES);
        }
        let cities = union(self.cities(), &other.cities);
        if pcs.len() + cities.len() != self.lists.len() {
            self.set_lists(&pcs, &cities);
        }
        if self.primary == other.primary {
            (self.sx, self.sy, self.n) = (self.sx + other.sx, self.sy + other.sy, self.n + other.n);
        } else if other.primary {
            (self.sx, self.sy, self.n) = (other.sx, other.sy, other.n);
        }
        self.primary |= other.primary;
        self.lot &= other.lot;
    }
}

/// The sorted, duplicate-free values of both (each sorted and duplicate-free).
fn union<T: Ord + Copy>(a: &[T], b: &[T]) -> Vec<T> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => (out.push(a[i]), i += 1),
            std::cmp::Ordering::Greater => (out.push(b[j]), j += 1),
            std::cmp::Ordering::Equal => (out.push(a[i]), (i, j) = (i + 1, j + 1)),
        };
    }
    out.extend_from_slice(&a[i..]);
    out.extend_from_slice(&b[j..]);
    out
}

/// A postal town: its `Pairs` id and either its postcode (top bit set) or, for an address
/// without one, its house number. The other of the two is always empty, which is what lets
/// a town take 8 bytes instead of four ids' 16; these are a street's largest list.
fn town(postcode: bool, pair: u32, value: u32) -> u64 {
    assert!(pair < 1 << 31, "too many postal towns");
    (postcode as u64) << 63 | (pair as u64) << 32 | value as u64
}

/// A `town`'s postcode, house number, city and source, with `empty` for the missing one.
fn untown(t: u64, pairs: &[Pair], empty: u32) -> (u32, u32, u32, u32) {
    let (city, source) = pairs[(t >> 32 & 0x7fff_ffff) as usize];
    let value = t as u32;
    match t >> 63 {
        1 => (value, empty, city, source),
        _ => (empty, value, city, source),
    }
}

type Pair = (u32, u32);

/// Postal towns' (city, source) pairs, numbered as first seen.
#[derive(Default)]
struct Pairs(Mutex<(HashMap<Pair, u32>, Vec<Pair>)>);

impl Pairs {
    fn cached(&self, cache: &mut HashMap<Pair, u32>, p: Pair) -> u32 {
        if let Some(&i) = cache.get(&p) {
            return i;
        }
        let mut g = self.0.lock().unwrap();
        let (ids, all) = &mut *g;
        let i = *ids.entry(p).or_insert_with(|| {
            all.push(p);
            all.len() as u32 - 1
        });
        cache.insert(p, i);
        i
    }
}

type PcKey = ([u8; 2], Str);
const NO_PLACE: u32 = u32::MAX;

#[derive(Default)]
struct AddrShard {
    table: HashTable<u32>,
    streets: Vec<Street>,
    text: Arena,
    /// By position, sorted: postcodes of streets that have no slot left for them.
    more: HashMap<u32, Vec<u32>>,
}

impl AddrShard {
    /// Country, place and key (folded name and cell): what tells streets apart.
    fn key(&self, pos: u32) -> ([u8; 2], u32, &str) {
        let s = &self.streets[pos as usize];
        (s.cc, s.place, self.text.get(s.key))
    }

    /// Merges a task's street into the shard's, which it creates if new; returns its position.
    fn add(&mut self, h: u64, (cc, place, key): ([u8; 2], u32, &str), agg: &AddrAgg) -> u32 {
        let found = self
            .table
            .find(mix(h), |&i| self.key(i) == (cc, place, key));
        let pos = match found.copied() {
            Some(pos) => pos,
            None => {
                let pos = self.streets.len() as u32;
                assert!(pos < 1 << 24, "too many streets in one shard");
                let (key, name) = (self.text.push(key), self.text.push(&agg.name));
                grow(&mut self.streets, 1);
                self.streets.push(Street {
                    place,
                    key,
                    name,
                    cc,
                    lot: true,
                    ..Default::default()
                });
                let AddrShard {
                    table,
                    streets,
                    text,
                    ..
                } = self;
                table.insert_unique(mix(h), pos, |&i| {
                    let s = &streets[i as usize];
                    mix(hash(&(s.cc, s.place, text.get(s.key))))
                });
                pos
            }
        };
        self.streets[pos as usize].add(&mut self.text, agg);
        pos
    }
}

/// Streets' postal towns, as (position in shard, town) records in one file per street shard.
/// Only the street documents read them, and held in memory they were a street's largest list.
struct TownSpill {
    dir: PathBuf,
    files: Vec<Mutex<std::io::BufWriter<File>>>,
}

impl TownSpill {
    const RECORD: usize = 12;

    fn new(dir: PathBuf) -> TownSpill {
        std::fs::create_dir_all(&dir).unwrap();
        let files = (0..SHARDS)
            .map(|s| {
                let f = File::create(dir.join(format!("{s:03}"))).unwrap();
                Mutex::new(std::io::BufWriter::with_capacity(1 << 16, f))
            })
            .collect();
        TownSpill { dir, files }
    }

    fn add(&self, shard: usize, records: &[u8]) {
        use std::io::Write;
        self.files[shard]
            .lock()
            .unwrap()
            .write_all(records)
            .unwrap();
    }

    fn finish(&self) {
        use std::io::Write;
        for f in &self.files {
            f.lock().unwrap().flush().unwrap();
        }
    }

    /// A shard's towns, sorted and without duplicates, by the streets' final positions.
    fn read(&self, shard: usize, renumber: &[u32]) -> (Vec<u32>, Vec<u64>) {
        let bytes = std::fs::read(self.dir.join(format!("{shard:03}"))).unwrap();
        let mut all: Vec<(u32, u64)> = bytes
            .as_chunks::<{ Self::RECORD }>()
            .0
            .iter()
            .map(|r| {
                let pos = u32::from_le_bytes(r[..4].try_into().unwrap());
                let town = u64::from_le_bytes(r[4..].try_into().unwrap());
                (renumber[pos as usize], town)
            })
            .collect();
        all.sort_unstable();
        all.dedup();
        all.into_iter().unzip()
    }
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
    towns: &'a TownSpill,
    streets: Vec<Mutex<AddrShard>>,
    postcodes: Vec<Mutex<HashMap<PcKey, PcAgg>>>,
    strings: Interner,
    pairs: Pairs,
    rows: AtomicU64,
    /// Overture's address rows with a street, per country of the containing place.
    covered: Mutex<HashMap<[u8; 2], u64>>,
}

impl<'a> Shared<'a> {
    fn new(
        assigner: &'a Assigner,
        places: &[Place],
        numbers: &'a NumberSpill,
        towns: &'a TownSpill,
    ) -> Shared<'a> {
        Shared {
            assigner,
            countries: places.iter().map(|p| p.country).collect(),
            numbers,
            towns,
            streets: sharded(),
            postcodes: sharded(),
            strings: Interner::new(),
            pairs: Pairs::default(),
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

/// The source rank of Overture's places and Korea's shop register: a business's point gives way
/// to an address source's copy of the same house.
const LAST_RANK: u8 = 3;

/// Marks a number derived from a suffixed one ("47" of 47B). It ranks as `BARE_RANK`, after the
/// source's own house 47, which then keeps its point; Spain reads no OSM addresses, whose rank
/// it shares.
const BARE: char = '\u{1}';
const BARE_RANK: u8 = 2;

/// Rows a sink aggregates before flushing, to bound task-local memory on large files.
const SINK_ROWS: u64 = 100_000;

/// One task's address rows, aggregated locally and merged into `Shared` on flush or drop.
struct Sink<'s, 'a> {
    shared: &'s Shared<'a>,
    local: HashMap<([u8; 2], u32), HashMap<Str, u32>>,
    aggs: Vec<AddrAgg>,
    pcs: HashMap<[u8; 2], HashMap<Str, PcAgg>>,
    cache: HashMap<Str, u32>,
    pair_cache: HashMap<Pair, u32>,
    /// sid holds the index into `aggs` until the flush remaps it.
    nums: Bucket,
    /// By index into `aggs`, sorted: postcodes past the street's `MAX_HELD_POSTCODES`.
    more: HashMap<u32, Vec<u32>>,
    rows: u64,
    key: String,
    last: String,
    lower: String,
    cell_key: String,
    pcn: String,
    /// Which source these rows come from: see `schema::with_rank`.
    rank: u8,
    /// Whether the houses move their street's point and add to its weight.
    weighs: bool,
    supplemental: bool,
    /// Whether these rows are land-lot addresses (see `AddrAgg::lot`).
    lot: bool,
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
            pair_cache: HashMap::new(),
            nums: Bucket::default(),
            more: HashMap::new(),
            rows: 0,
            key: String::new(),
            last: String::new(),
            lower: String::new(),
            cell_key: String::new(),
            pcn: String::new(),
            rank: 0,
            weighs: true,
            supplemental: false,
            lot: false,
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
        cities: &[(&str, &'static str)],
    ) {
        self.rows += 1;
        if self.rows >= SINK_ROWS {
            self.flush();
        }
        let a = self.shared.assigner;
        if !a.within(x, y) {
            return;
        }
        let (mut x, mut y) = (x, y);
        let (c, place) = match cc {
            Some(c) => {
                let mut place = a.place_in(x, y, Some(c));
                // Some exports write [lat, lon] where GeoJSON wants [lon, lat] (OpenAddresses'
                // Spanish cadastre layers): a point only its own country holds once turned.
                if place.is_none()
                    && let Some(q) = a.place_in(y, x, Some(c))
                {
                    (x, y, place) = (y, x, Some(q));
                }
                (c, place)
            }
            None => match a.place(x, y) {
                Some(p) => (self.shared.countries[p as usize], Some(p)),
                None => return,
            },
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
        let Some(st) = nonempty(street)
            .or_else(|| nonempty(number).and(nonempty(cities.first().map(|c| c.0))))
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
        let place = place.unwrap_or(NO_PLACE);
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
                    primary: !self.supplemental,
                    lot: self.lot,
                    ..Default::default()
                });
                m.insert(celled.into(), self.aggs.len() as u32 - 1);
                self.aggs.len() as u32 - 1
            }
        };
        let e = &mut self.aggs[li as usize];
        e.lot &= self.lot;
        if st != &*e.name && prefer(st, &e.name) {
            e.name = st.into();
        }
        let pc_id = (pc.is_some() && !self.pcn.is_empty())
            .then(|| self.shared.strings.cached(&mut self.cache, &self.pcn));
        if let Some(id) = pc_id {
            push_unique(&mut e.pcs, id, MAX_HELD_POSTCODES);
            if !e.pcs.contains(&id) {
                insert_sorted(self.more.entry(li).or_default(), id);
            }
        }
        for &(city, source) in cities.iter().filter(|(name, _)| !name.trim().is_empty()) {
            let strings = &self.shared.strings;
            let id = strings.cached(&mut self.cache, city.trim());
            let source = strings.cached(&mut self.cache, source);
            insert_sorted(&mut e.cities, id);
            let pair = self.shared.pairs.cached(&mut self.pair_cache, (id, source));
            let towns: Vec<u64> = match pc_id {
                Some(pc) => vec![town(true, pair, pc)],
                None => number
                    .into_iter()
                    .flat_map(|n| n.split(';'))
                    .map(|n| schema::number_key(n.trim_end_matches(BARE)))
                    .filter(|n| !n.is_empty())
                    .map(|n| town(false, pair, strings.cached(&mut self.cache, &n)))
                    .collect(),
            };
            for town in towns {
                if e.postal_towns.is_empty() {
                    e.postal_towns = vec![town];
                } else {
                    insert_sorted(&mut e.postal_towns, town);
                }
            }
        }
        if self.weighs {
            (e.sx, e.sy, e.n) = (e.sx + quantize(x), e.sy + quantize(y), e.n + 1);
        }
        for n in number
            .into_iter()
            .flat_map(|n| n.split(';'))
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            let (n, rank) = match n.strip_suffix(BARE) {
                Some(bare) => (bare, BARE_RANK),
                None => (n, self.rank),
            };
            self.lower.clear();
            self.lower.push_str(&schema::number_key(n));
            // slot = 1-based position in this task's list for the street; remapped on flush
            let slot = pc_id
                .and_then(|id| e.pcs.iter().position(|&p| p == id))
                .map_or(0, |i| i as u8 + 1);
            let (px, py) = pack(x, y, slot);
            let unslotted = pc_id.filter(|_| slot == 0).unwrap_or(schema::NO_PC);
            match Num::new(li, &self.lower, with_rank(px, rank), py) {
                Ok(v) if unslotted == schema::NO_PC => self.nums.short.push(v),
                Ok(v) => self.nums.long.push(v.into_long(unslotted)),
                Err(mut v) => {
                    v.pc = unslotted;
                    self.nums.long.push(v);
                }
            }
        }
    }

    fn flush(&mut self) {
        let sh = self.shared;
        sh.rows.fetch_add(std::mem::take(&mut self.rows), Relaxed);
        self.cache.clear();
        self.pair_cache.clear();
        let mut covered = sh.covered.lock().unwrap();
        for (c, n) in self.covered.drain() {
            *covered.entry(c).or_default() += n;
        }
        drop(covered);
        let mut remap = vec![0u32; self.aggs.len()];
        // per local street: its local postcode slots -> slots in the merged street's list, and
        // the local list itself for the houses whose postcode has no merged slot
        let mut slots: Vec<Vec<u8>> = vec![vec![]; self.aggs.len()];
        let mut locals: Vec<Vec<u32>> = vec![vec![]; self.aggs.len()];
        let mut towns: Vec<Vec<u8>> = vec![vec![]; SHARDS];
        for ((c, place), m) in self.local.drain() {
            for (k, li) in m {
                let agg = std::mem::take(&mut self.aggs[li as usize]);
                let h = hash(&(c, place, &*k));
                let shard_ix = (h % SHARDS as u64) as usize;
                let mut s = sh.streets[shard_ix].lock().unwrap();
                let pos = s.add(h, (c, place, &k), &agg);
                let merged = s.streets[pos as usize].pcs();
                slots[li as usize] = agg
                    .pcs
                    .iter()
                    .map(|id| {
                        merged
                            .iter()
                            .position(|m| m == id)
                            .map_or(0, |i| i as u8 + 1)
                    })
                    .collect();
                let unslotted: Vec<u32> = (agg.pcs.iter())
                    .filter(|id| !merged.contains(id))
                    .copied()
                    .chain(self.more.remove(&li).unwrap_or_default())
                    .collect();
                if !unslotted.is_empty() {
                    let more = s.more.entry(pos).or_default();
                    for id in unslotted {
                        insert_sorted(more, id);
                    }
                }
                drop(s);
                locals[li as usize] = agg.pcs;
                remap[li as usize] = (pos << 8) | shard_ix as u32;
                for town in agg.postal_towns {
                    towns[shard_ix].extend(pos.to_le_bytes());
                    towns[shard_ix].extend(town.to_le_bytes());
                }
            }
        }
        for (shard_ix, records) in towns.iter().enumerate() {
            if !records.is_empty() {
                sh.towns.add(shard_ix, records);
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
        // a house's merged slot, or with none its postcode
        let slot = |li: u32, x: u32| match held_slot(x) {
            0 => (x, schema::NO_PC),
            k => match slots[li as usize][k as usize - 1] {
                0 => (x & 0xc0ff_ffff, locals[li as usize][k as usize - 1]),
                merged => ((x & 0xc0ff_ffff) | (merged as u32) << 24, schema::NO_PC),
            },
        };
        for mut n in nums.short {
            let (x, pc) = slot(n.sid, n.x);
            (n.x, n.sid) = (x, remap[n.sid as usize]);
            let o = &mut out[n.sid as usize % shards];
            match pc {
                schema::NO_PC => o.short.push(n),
                pc => o.long.push(n.into_long(pc)),
            }
        }
        for mut n in nums.long {
            if n.pc == schema::NO_PC {
                (n.x, n.pc) = slot(n.sid, n.x);
            }
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

fn numbers_with_unit<'a>(
    cc: [u8; 2],
    number: Option<&'a str>,
    unit: Option<&str>,
) -> Option<Cow<'a, str>> {
    let number = number?;
    // Italy's register (ANNCSU, through OpenAddresses) puts a civic number's exponent in the
    // unit: "35" with "A" is civic number 35/A, another house than 35.
    if cc == *b"IT"
        && let Some(exponent) = unit
            .map(str::trim)
            .filter(|u| (1..=6).contains(&u.len()) && u.bytes().all(|b| b.is_ascii_alphanumeric()))
        && !number.trim().is_empty()
        && number.trim().bytes().all(|b| b.is_ascii_digit())
    {
        return Some(Cow::Owned(format!("{}/{exponent}", number.trim())));
    }
    // CartoCiudad's extension (47 B, 12 BIS) is Overture's unit: part of the house number, as
    // an address writes it ("47B"). The bare 47 stands for the building only where the source
    // has no house 47 of its own (`BARE`).
    if cc == *b"ES"
        && let Some(ext) = unit.map(str::trim)
        && !ext.is_empty()
        && ext.bytes().all(|b| b.is_ascii_alphabetic())
        && !number.trim().is_empty()
        && number.trim().bytes().all(|b| b.is_ascii_digit())
    {
        return Some(Cow::Owned(format!("{0}{ext};{0}{BARE}", number.trim())));
    }
    // Taiwan's register puts the sub-number in the unit: "10" with "之26號三樓", or "10號" with
    // "之26", is house 10之26, another door than 10 (the floor after it stays with the house).
    // No bare 10 is kept: ReverseGeocode would answer it at 10之26's point.
    let digit = |c: char| c.is_ascii_digit() || ('０'..='９').contains(&c);
    if cc == *b"TW"
        && let Some(rest) = unit.and_then(|u| u.trim().strip_prefix('之'))
        && let base = number.trim().trim_end_matches(['號', '号'])
        && !base.is_empty()
        && base.chars().all(digit)
    {
        let sub: String = rest.chars().take_while(|&c| digit(c)).collect();
        if !sub.is_empty() {
            return Some(Cow::Owned(format!("{base}之{sub}")));
        }
    }
    let unit = unit.map(str::trim).filter(|unit| {
        cc == *b"NZ"
            && !unit.is_empty()
            && unit.bytes().all(|b| b.is_ascii_alphanumeric())
            && !number.trim().is_empty()
            && !number.contains([';', '/'])
    });
    Some(match unit {
        Some(unit) => Cow::Owned(format!("{number};{unit}/{}", number.trim())),
        None => Cow::Borrowed(number),
    })
}

/// Overture's Canadian rows write a suffixed civic number as a float and its suffix: "55.0 A"
/// for 55A, "29.0 1/2" for 29 1/2.
fn float_civic(number: &str) -> Option<String> {
    let (whole, suffix) = number.trim().split_once(".0 ")?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(whole) {
        return None;
    }
    if suffix.len() == 1 && suffix.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Some(format!("{whole}{suffix}"));
    }
    let (num, den) = suffix.split_once('/')?;
    (digits(num) && digits(den)).then(|| format!("{whole} {suffix}"))
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
                "unit",
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
            let unit = b.column_by_name("unit").map(|v| v.as_string::<i32>());
            let (levels, geom) = (
                col(&b, "address_levels").as_list::<i32>(),
                col(&b, "geometry").as_binary::<i32>(),
            );
            let (o, v) = (
                levels.value_offsets(),
                levels.values().as_struct().column(0).as_string::<i32>(),
            );
            for i in 0..b.num_rows() {
                let Some((x, y)) = (!geom.is_null(i))
                    .then(|| wkb_point(geom.value(i)))
                    .flatten()
                else {
                    continue;
                };
                let c = country(text(&cc, i));
                let level = |at: i32| {
                    (!levels.is_null(i) && o[i + 1] > o[i] && at >= o[i] && at < o[i + 1])
                        .then(|| text(v, at as usize))
                        .flatten()
                };
                let city = text(&postal, i)
                    .map(|name| (name, "postal_city"))
                    .or_else(|| level(o[i + 1] - 1).map(|name| (name, "address_levels")));
                // LINZ's levels are [town_city, suburb_locality]: addresses name the town too
                // ("49 Foster Road, Riverton/Aparima" is in Thornbury).
                let town = (c == *b"NZ")
                    .then(|| level(o[i]))
                    .flatten()
                    .filter(|town| city.is_some_and(|(suburb, _)| fold(suburb) != fold(town)))
                    .map(|name| (name, TOWN_CITY));
                let number = text(&number, i);
                let latvian = (c == *b"LV")
                    .then(|| latvian(text(&street, i), number?, level(o[i]), level(o[i + 1] - 1)))
                    .flatten();
                let city = match &latvian {
                    Some((_, place)) => Some((place.as_str(), "postal_city")),
                    None => city,
                };
                let cities: Vec<_> = city.into_iter().chain(town).collect();
                let civic = number.and_then(float_civic);
                let numbers = numbers_with_unit(
                    c,
                    civic.as_deref().or(number),
                    unit.and_then(|v| text(v, i)),
                );
                sink.add(
                    Some(c),
                    x,
                    y,
                    latvian
                        .as_ref()
                        .map(|(road, _)| road.as_str())
                        .or(text(&street, i)),
                    numbers.as_deref(),
                    text(&postcode, i),
                    &cities,
                );
            }
        }
        done.add(1);
    });
}

/// A Latvian address of the State Address Register as OpenAddresses' `lv/countrywide` gives
/// Overture it, whose street function fails: the street is the whole address ("Saules iela 15,
/// Liepāja, LV-3401", sometimes with the number's digits cut off the postcode), and a named
/// house ("Riņņi") has none, only its levels, the address above it with the first comma dropped,
/// first without and then with the place it is in ("Vecates pag. Valmieras nov., LV-4211",
/// "Riņņi"'s parish). The street ("Saules iela") and its town, or for a house numbered or named
/// without a street the village, town or parish (as "Vecates pagasts") as both.
fn latvian(
    street: Option<&str>,
    number: &str,
    first: Option<&str>,
    last: Option<&str>,
) -> Option<(String, String)> {
    if let Some(street) = street {
        let mut parts = street.split(", ");
        let road = parts.next()?.strip_suffix(number)?.strip_suffix(' ')?;
        let town = parts.next().filter(|t| !t.starts_with("LV-"))?;
        return Some((road.into(), town.into()));
    }
    let (first, last) = (first?, last?);
    let place = if first == last {
        let (place, code) = last.rsplit_once(' ')?;
        code.starts_with("LV-").then_some(place)?
    } else {
        last.match_indices(' ').find_map(|(at, _)| {
            (last[at + 1..].replacen(", ", " ", 1) == first).then(|| &last[..at])
        })?
    };
    let place = match place.strip_suffix(" pag.") {
        Some(stem) => format!("{stem} pagasts"),
        None => place.into(),
    };
    Some((place.clone(), place))
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
fn pass_places(dir: &Path, shared: &Shared, segs: &[SegShard]) {
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
        // A road with one listed business keeps its segments' middle and importance.
        let mut sink = Sink::new(shared).ranked(LAST_RANK);
        sink.weighs = false;
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
                    if find_seg(segs, p, with_cell(&key, x, y, &mut cell_key)).is_some() {
                        sink.add(Some(c), x, y, Some(street), Some(number), None, &[]);
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

/// The streets in their final form.
struct Canon {
    /// Per shard, in key order, so a street's id depends on the street and not on when it arrived.
    /// Postcodes and cities are sorted by text and cut to their limits.
    streets: Vec<AddrShard>,
    /// Per shard, each street's final position by its first one.
    renumber: Vec<Vec<u32>>,
    /// Per shard, for each street (by first position) with two or more postcodes, where each of
    /// its first postcode slots went (0: cut). A street with fewer keeps its slots.
    slots: Vec<HashMap<u32, Box<[u8]>>>,
}

impl Canon {
    /// A house names its street and postcode slot (or postcode) as first numbered; the street's
    /// final number and postcode list are known only now.
    fn resolve(&self, sid: u32, slot: u8, pc: u32) -> (u32, u8) {
        let shard = (sid & 0xff) as usize;
        let old = sid >> 8;
        let new = self.renumber[shard][old as usize];
        let moved = if pc != schema::NO_PC {
            let pcs = self.streets[shard].streets[new as usize].pcs();
            pcs.iter().position(|&p| p == pc).map_or(0, |i| i as u8 + 1)
        } else {
            match self.slots[shard].get(&old) {
                Some(moved) if slot > 0 => moved.get(slot as usize - 1).copied().unwrap_or(0),
                _ => slot,
            }
        };
        ((new << 8) | shard as u32, moved)
    }
}

fn canonical_streets(shards: Vec<Mutex<AddrShard>>, strings: &Strings) -> Canon {
    let by_text = |ids: &mut Vec<u32>, max: usize| {
        ids.sort_unstable_by(|&a, &b| strings.get(a).cmp(strings.get(b)));
        ids.dedup();
        ids.truncate(max);
    };
    let per_shard: Vec<_> = shards
        .into_par_iter()
        .map(|m| {
            let mut shard = m.into_inner().unwrap();
            shard.table = HashTable::new();
            let mut more = std::mem::take(&mut shard.more);
            let mut order: Vec<u32> = (0..shard.streets.len() as u32).collect();
            // Appending supplementary-only streets preserves other countries' candidate tie order.
            order.sort_unstable_by(|&a, &b| {
                let primary = |i: u32| !shard.streets[i as usize].primary;
                (primary(a), shard.key(a)).cmp(&(primary(b), shard.key(b)))
            });
            let mut renumber = vec![0; order.len()];
            let mut slots = HashMap::new();
            let mut first = std::mem::take(&mut shard.streets);
            shard.streets = order
                .into_iter()
                .enumerate()
                .map(|(new, old)| {
                    renumber[old as usize] = new as u32;
                    let mut st = std::mem::take(&mut first[old as usize]);
                    let (mut pcs, mut cities) = (st.pcs().to_vec(), st.cities().to_vec());
                    let arrived = pcs.clone();
                    pcs.extend(more.remove(&old).unwrap_or_default());
                    if pcs.len() > 1 {
                        by_text(&mut pcs, MAX_POSTCODES);
                        let moved = arrived
                            .iter()
                            .map(|id| pcs.iter().position(|p| p == id).map_or(0, |i| i as u8 + 1))
                            .collect();
                        slots.insert(old, moved);
                    }
                    by_text(&mut cities, MAX_CITIES);
                    st.set_lists(&pcs, &cities);
                    st
                })
                .collect();
            (shard, renumber, slots)
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
        Some("fetch") => job::fetch(args),
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

    // `--osm-landmarks` extracts are read for landmarks only: their addresses would change answers
    // in countries whose addresses come from Overture. On a network volume the passes wait on the
    // disk with cores idle, so the extracts are read beside them until `write` needs them, in a
    // pool of their own: in the shared one, each extract holds a worker for its whole file and
    // the passes starve. Where the passes keep every core busy, the two pools share them. The
    // `--osm` extracts' landmarks are read in their address passes.
    let landmark_dirs: Vec<PathBuf> = opt("--osm-landmarks").iter().map(PathBuf::from).collect();
    let osm_files: usize = (osm_dirs.iter())
        .map(|d| sources::files(Path::new(d), ".osm.pbf").len())
        .sum();
    let landmarks = (!landmark_dirs.is_empty() || !osm_dirs.is_empty()).then(|| {
        let pool = rayon::ThreadPoolBuilder::new().build().unwrap();
        let first = osm_files as u32;
        std::thread::spawn(move || pool.install(|| landmarks::read(&landmark_dirs, first)))
    });

    progress::phase("Parse places and boundaries");
    let (mut places, mut rows, idx) = load_places(&dir.join("division"));
    lap(&format!("places ({})", places.len()));
    let polys = PolyFile::new(Path::new(out).join(".polygons"));
    let mut assigner = load_assigner(&dir.join("division_area"), &places, &idx, polys);
    reconcile_parents(&mut places, &mut rows, &idx, &assigner);
    write_places(rows, &idx, out);
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
    let towns = TownSpill::new(Path::new(out).join(".towns-spill"));
    let shared = Shared::new(&assigner, &places, &spill, &towns);
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
    let (mut osm_landmarks, mut first) = (vec![], 0);
    for d in &osm_dirs {
        let (files, found) = sources::osm(Path::new(d), &shared, first);
        first += files as u32;
        osm_landmarks.extend(found);
        lap(&format!(
            "osm {d} ({files} files, {} rows)",
            shared.rows.swap(0, Relaxed)
        ));
    }
    for zip in &opt("--korea-shops") {
        let files = sources::korea_shops(Path::new(zip), &shared);
        lap(&format!(
            "korea shops {zip} ({files} files, {} rows)",
            shared.rows.swap(0, Relaxed)
        ));
    }
    if let Some(reading) = landmarks {
        let mut found = reading.join().unwrap();
        found.append(&mut osm_landmarks);
        let found = landmarks::best(found);
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
        pairs,
        ..
    } = shared;
    let empty = strings.id("");
    let strings = strings.table();
    let pairs = pairs.0.into_inner().unwrap().1;
    towns.finish();
    let canon = canonical_streets(streets, &strings);
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
    let n = spill.write_fsts(out, parallel, &|sid, slot, pc| canon.resolve(sid, slot, pc));
    let Canon {
        streets, renumber, ..
    } = canon;
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
                      postal_towns: &[u64],
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
            .map(|&t| {
                let (pc, number, city, source) = untown(t, &pairs, empty);
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
        .scan(0, |n, s| {
            let base = *n;
            *n += s.segs.len() as u32;
            Some(base)
        })
        .collect();
    let seg_count = segs.iter().map(|s| s.segs.len() as u32).sum();
    let seg_sid: Vec<AtomicU32> = (0..seg_count).map(|_| AtomicU32::new(u32::MAX)).collect();
    let seg_at = |id: u32| seg_base[(id & 0xff) as usize] as usize + (id >> 8) as usize;
    let seg_names = |sh: usize, pos: u32| -> Vec<&str> {
        let s = &segs[sh];
        let primary = s.text.get(s.segs[pos as usize].name);
        let alts = s.alts.get(&pos).map_or(&[][..], |a| a);
        std::iter::once(primary)
            .chain(
                alts.iter()
                    .take(MAX_ALT_NAMES)
                    .map(|a| &**a)
                    .filter(|a| *a != primary),
            )
            .collect()
    };
    let (merged, max_sid) = (AtomicU64::new(0), AtomicU32::new(0));
    let lots = Mutex::new(HashSet::new());
    streets.par_iter().enumerate().for_each(|(sh, s)| {
        let (town_pos, shard_towns) = towns.read(sh, &renumber[sh]);
        let mut t = 0;
        for (pos, st) in s.streets.iter().enumerate() {
            let (cc, place, key) = s.key(pos as u32);
            let sid = ((pos as u32) << 8) | sh as u32;
            max_sid.fetch_max(sid, Relaxed);
            // A segment belongs to its place's country, so no two address streets (which differ
            // only in country) share one.
            let seg = (place != NO_PLACE && places[place as usize].country == cc)
                .then(|| find_seg(&segs, place, key))
                .flatten();
            let names = match seg {
                Some((gs, gp)) => {
                    seg_sid[seg_at(gp << 8 | gs as u32)].store(sid, Relaxed);
                    merged.fetch_add(1, Relaxed);
                    seg_names(gs, gp)
                }
                None => vec![s.text.get(st.name)],
            };
            if seg.is_none() && st.lot {
                lots.lock().unwrap().insert(sid);
            }
            let g = seg.map(|(gs, gp)| segs[gs].segs[gp as usize]);
            let n = u64::from(if st.primary || g.is_none() { st.n } else { 0 })
                + g.map_or(0, |g| u64::from(g.n));
            // Supplementary business points must not move a mapped road or change its importance.
            let point = match g {
                Some(g) if st.n == 0 || !st.primary => {
                    (mean(g.sx, g.n.into()), mean(g.sy, g.n.into()))
                }
                _ => (mean(st.sx, st.n.into()), mean(st.sy, st.n.into())),
            };
            let from = t;
            while t < town_pos.len() && town_pos[t] == pos as u32 {
                t += 1;
            }
            street_doc(
                sid,
                country_str(&cc),
                place,
                &names,
                st.pcs(),
                st.cities(),
                &shard_towns[from..t],
                point,
                n,
            );
        }
    });
    let addr_streets: u64 = streets.iter().map(|s| s.streets.len() as u64).sum();
    drop((streets, renumber));
    std::fs::remove_dir_all(&towns.dir).unwrap();
    // Streets only the segments know, numbered in key order after the others.
    let mut only: Vec<(usize, u32)> = segs
        .iter()
        .enumerate()
        .flat_map(|(sh, s)| (0..s.segs.len() as u32).map(move |pos| (sh, pos)))
        .filter(|&(sh, pos)| seg_sid[seg_at(pos << 8 | sh as u32)].load(Relaxed) == u32::MAX)
        .collect();
    only.par_sort_unstable_by(|a, b| segs[a.0].key(a.1).cmp(&segs[b.0].key(b.1)));
    let first = max_sid.load(Relaxed) + 1;
    only.par_iter().enumerate().for_each(|(i, &(sh, pos))| {
        seg_sid[seg_at(pos << 8 | sh as u32)].store(first + i as u32, Relaxed);
        let g = segs[sh].segs[pos as usize];
        street_doc(
            first + i as u32,
            country_str(&places[g.place as usize].country),
            g.place,
            &seg_names(sh, pos),
            &[],
            &[],
            &[],
            (mean(g.sx, g.n.into()), mean(g.sy, g.n.into())),
            g.n.into(),
        );
    });
    let seg_only = only.len() as u64;
    drop((only, segs));
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
    let lots = lots.into_inner().unwrap();
    let (houses, streets) = reverse::build(
        Path::new(out),
        &index,
        f.kind,
        |id| Some(seg_sid[seg_at(id)].load(Relaxed)).filter(|&sid| sid != u32::MAX),
        |sid| !lots.contains(&sid),
    )
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
    fn units_add_exact_keys_and_join_suffixes_to_the_number() {
        for (cc, number, unit, expected) in [
            (*b"NZ", Some("19"), Some("1"), Some("19;1/19")),
            (*b"NZ", Some("19B"), Some("3A"), Some("19B;3A/19B")),
            (
                *b"NZ",
                Some("151-163"),
                Some("2"),
                Some("151-163;2/151-163"),
            ),
            (*b"NZ", Some("19"), Some(" 001 "), Some("19;001/19")),
            (*b"NZ", Some("1/19"), Some("1"), Some("1/19")),
            (*b"NZ", Some("19;21"), Some("1"), Some("19;21")),
            (*b"NZ", Some("19"), Some("1;2"), Some("19")),
            (*b"NZ", Some("19"), Some("1/2"), Some("19")),
            (*b"NZ", Some("19"), Some(""), Some("19")),
            (*b"NZ", Some("19"), None, Some("19")),
            (*b"NZ", Some(""), Some("1"), Some("")),
            (*b"NZ", None, Some("1"), None),
            (*b"AU", Some("19"), Some("1"), Some("19")),
            (*b"IT", Some("35"), Some("A"), Some("35/A")),
            (*b"IT", Some("9"), Some(" BIS "), Some("9/BIS")),
            (*b"IT", Some("285"), Some("1"), Some("285/1")),
            (*b"IT", Some("4"), Some("A/4"), Some("4")),
            (*b"IT", Some("4"), Some(""), Some("4")),
            (*b"IT", Some("4B"), Some("1"), Some("4B")),
            (*b"ES", Some("47"), Some("B"), Some("47B;47\u{1}")),
            (*b"ES", Some("12"), Some(" BIS "), Some("12BIS;12\u{1}")),
            (*b"ES", Some("12"), Some("A_A"), Some("12")),
            (*b"ES", Some("12"), Some("2"), Some("12")),
            (*b"ES", Some("12-14"), Some("A"), Some("12-14")),
            (*b"ES", Some("12"), None, Some("12")),
            (*b"TW", Some("１０"), Some("之２６號"), Some("１０之２６")),
            (*b"TW", Some("261號"), Some(" 之2 "), Some("261之2")),
            (
                *b"TW",
                Some("１３４"),
                Some("之２號三樓"),
                Some("１３４之２"),
            ),
            (
                *b"TW",
                Some("９６"),
                Some("之６２號五樓之２"),
                Some("９６之６２"),
            ),
            (*b"TW", Some("３９"), Some("之１I-"), Some("３９之１")),
            (*b"TW", Some("５０號"), Some("四樓之２"), Some("５０號")),
            (*b"TW", Some("９號"), Some("三樓"), Some("９號")),
            (*b"TW", Some("建51"), Some("之21號"), Some("建51")),
            (*b"TW", Some("12"), Some("之"), Some("12")),
            (*b"TW", Some(""), Some("之1號"), Some("")),
            (*b"JP", Some("10"), Some("之26號"), Some("10")),
        ] {
            assert_eq!(numbers_with_unit(cc, number, unit).as_deref(), expected);
        }
    }

    #[test]
    fn float_civic_numbers_keep_their_suffix() {
        for (number, expected) in [
            ("55.0 A", Some("55A")),
            ("29.0 1/2", Some("29 1/2")),
            ("55", None),
            ("1.6", None),
            ("55.0 AB", None),
            ("24.5 KM", None),
            ("x.0 A", None),
        ] {
            assert_eq!(float_civic(number).as_deref(), expected, "{number}");
        }
    }

    #[test]
    fn streets_merge_by_key_into_one_compact_copy() {
        // tens of millions are held at once
        assert_eq!(std::mem::size_of::<Street>(), 56);
        let mut shard = AddrShard::default();
        let key = (*b"US", 7, "main st\u{1}1:2");
        let h = hash(&key);
        let agg = |name: &str, pcs: Vec<u32>, cities: Vec<u32>| AddrAgg {
            name: name.into(),
            pcs,
            cities,
            n: 1,
            ..Default::default()
        };
        assert_eq!(
            shard.add(h, key, &agg("MAIN ST", vec![5, 3], vec![1, 3])),
            0
        );
        assert_eq!(
            shard.add(h, key, &agg("Main St", vec![4, 5], vec![2, 3])),
            0
        );
        let other = (*b"CA", 7, key.2);
        assert_eq!(
            shard.add(hash(&other), other, &agg("Main St", vec![], vec![])),
            1
        );
        let st = &shard.streets[0];
        assert_eq!((st.pcs(), st.cities()), (&[5, 3, 4][..], &[1, 2, 3][..]));
        assert_eq!((shard.text.get(st.name), st.n), ("Main St", 2));
        assert_eq!(shard.key(0), key);
        assert_eq!(shard.key(1), other);
        for len in [0, 127, 128, 20_000] {
            let mut arena = Arena::default();
            let (a, b) = (arena.push(&"x".repeat(len)), arena.push("y"));
            assert_eq!((arena.get(a).len(), arena.get(b)), (len, "y"));
        }
    }

    #[test]
    fn segments_merge_by_place_and_key_with_their_other_names() {
        let mut segs: Vec<SegShard> = (0..SHARDS).map(|_| SegShard::default()).collect();
        let agg = |name: &str, alt: &[&str]| SegAgg {
            name: name.into(),
            alt: alt.iter().map(|&a| a.into()).collect(),
            n: 1,
            ..Default::default()
        };
        let mut add = |place: u32, key: &str, a: SegAgg| {
            let h = hash(&(place, key));
            segs[(h % SHARDS as u64) as usize].add(h, place, key, a)
        };
        assert_eq!(add(1, "k", agg("B Road", &["Old"])), 0);
        assert_eq!(add(1, "k", agg("A Road", &["New", "Old"])), 0);
        add(2, "k", agg("C Road", &[]));
        let (sh, pos) = find_seg(&segs, 1, "k").unwrap();
        let s = &segs[sh];
        assert_eq!((s.text.get(s.segs[0].name), s.segs[0].n), ("A Road", 2));
        assert_eq!(&*s.alts[&pos], ["New".into(), "Old".into()]);
        assert!(find_seg(&segs, 3, "k").is_none());
    }

    #[test]
    fn postal_towns_come_back_once_per_street_in_final_order() {
        let dir = std::env::temp_dir().join(format!("towns-test-{}", std::process::id()));
        let spill = TownSpill::new(dir.clone());
        let record = |pos: u32, town: u64| [&pos.to_le_bytes()[..], &town.to_le_bytes()].concat();
        spill.add(3, &[record(0, 9), record(1, 4), record(0, 2)].concat());
        spill.add(3, &record(0, 9));
        spill.finish();
        let (pos, towns) = spill.read(3, &[1, 0]);
        assert_eq!((pos, towns), (vec![0, 1, 1], vec![4, 2, 9]));
        assert_eq!(spill.read(4, &[]), (vec![], vec![]));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_postal_town_packs_its_postcode_or_number_with_its_city_and_source() {
        let pairs = [(7, 8), (9, 10)];
        assert_eq!(untown(town(true, 1, 5), &pairs, 0), (5, 0, 9, 10));
        assert_eq!(untown(town(false, 0, 5), &pairs, 0), (0, 5, 7, 8));
        assert_ne!(town(true, 0, 5), town(false, 0, 5));
        let p = Pairs::default();
        let mut cache = HashMap::new();
        assert_eq!(p.cached(&mut cache, (3, 4)), 0);
        assert_eq!(p.cached(&mut HashMap::new(), (5, 4)), 1);
        assert_eq!(p.cached(&mut HashMap::new(), (3, 4)), 0);
    }

    #[test]
    fn supplementary_streets_keep_primary_centroids_and_ids() {
        for order in [[false, true], [true, false]] {
            let mut text = Arena::default();
            let mut merged = Street {
                name: text.push(""),
                ..Default::default()
            };
            for primary in order {
                let agg = AddrAgg {
                    primary,
                    sx: if primary { 10 } else { 1000 },
                    sy: if primary { 20 } else { 2000 },
                    n: 1,
                    ..Default::default()
                };
                merged.add(&mut text, &agg);
            }
            assert_eq!((merged.sx, merged.sy, merged.n), (10, 20, 1));
            assert!(merged.primary);
        }
        let strings = Interner::new().table();
        let build = |supplement: bool| {
            let mut shard = AddrShard::default();
            for (cc, primary) in [(*b"NG", true), (*b"US", true), (*b"KR", false)] {
                if !primary && !supplement {
                    continue;
                }
                let key = (cc, 0, "road");
                let agg = AddrAgg {
                    primary,
                    ..Default::default()
                };
                shard.add(hash(&key), key, &agg);
            }
            canonical_streets(vec![Mutex::new(shard)], &strings).streets
        };
        let (base, head) = (build(false), build(true));
        for i in 0..base[0].streets.len() as u32 {
            assert_eq!(base[0].key(i), head[0].key(i));
        }
        assert_eq!(head[0].key(2).0, *b"KR");
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
            let key = format!("s{}", shard.streets.len());
            let key = (*b"us", 0, key.as_str());
            let agg = AddrAgg {
                pcs,
                ..Default::default()
            };
            shard.add(hash(&key), key, &agg);
        }
        let canon = canonical_streets(vec![Mutex::new(shard)], &strings);
        let street = &canon.streets[0].streets[0];
        let text: Vec<&str> = street.pcs().iter().map(|&p| strings.get(p)).collect();
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
    fn a_street_keeps_its_postcodes_and_houses_whatever_their_arrival_order() {
        // One house per postcode, more postcodes than a street has slots for, read in different
        // orders and batches.
        let run = |name: &str, order: Vec<usize>, batch: usize| {
            let dir = std::env::temp_dir().join(format!("pc-order-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let spill = NumberSpill::new(dir.join("spill"), NUMBER_SHARDS);
            let towns = TownSpill::new(dir.join("towns"));
            std::fs::create_dir_all(&dir).unwrap();
            let assigner = Assigner::new(vec![], PolyFile::new(dir.join("polys")), vec![], vec![]);
            let shared = Shared::new(&assigner, &[], &spill, &towns);
            for chunk in order.chunks(batch) {
                let mut sink = Sink::new(&shared);
                for &i in chunk {
                    let (pc, n) = (format!("{}", 10000 + i), i.to_string());
                    let x = -70.0 + i as f64 * 1e-4;
                    sink.add(
                        Some(*b"US"),
                        x,
                        40.0,
                        Some("Main Street"),
                        Some(&n),
                        Some(&pc),
                        &[],
                    );
                }
            }
            let Shared {
                streets, strings, ..
            } = shared;
            let strings = strings.table();
            let canon = canonical_streets(streets, &strings);
            let out = dir.join("out");
            std::fs::create_dir_all(&out).unwrap();
            let out = out.to_str().unwrap();
            spill.write_fsts(out, 1, &|sid, slot, pc| canon.resolve(sid, slot, pc));
            let pcs: Vec<String> = (canon.streets.iter())
                .flat_map(|sh| sh.streets.iter().flat_map(|st| st.pcs()))
                .map(|&p| strings.get(p).to_string())
                .collect();
            let fsts: Vec<u8> = (0..NUMBER_SHARDS)
                .flat_map(|s| std::fs::read(format!("{out}/numbers-{s:02}.fst")).unwrap())
                .collect();
            std::fs::remove_dir_all(&dir).unwrap();
            (pcs, fsts)
        };
        let n = MAX_HELD_POSTCODES + 7;
        let first = run("forward", (0..n).collect(), n);
        assert_eq!(first.0.len(), MAX_POSTCODES);
        assert_eq!((&*first.0[0], &*first.0[19]), ("10000", "10019"));
        let shuffled = (0..n).map(|i| i * 31 % n).collect();
        for (name, other) in [
            ("backward", run("backward", (0..n).rev().collect(), 9)),
            ("shuffled", run("shuffled", shuffled, 5)),
        ] {
            assert_eq!(other.0, first.0, "{name}");
            assert!(other.1 == first.1, "{name}: house numbers differ");
        }
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

    #[test]
    fn latvian_streets_and_named_houses_from_their_register_address() {
        let pair = |a: &str, b: &str| Some((a.to_string(), b.to_string()));
        let street = |s, n| latvian(Some(s), n, None, None);
        assert_eq!(
            street("Saules iela 15, Liepāja, LV-3401", "15"),
            pair("Saules iela", "Liepāja")
        );
        assert_eq!(
            street("Amatas iela 11, Liepāja, LV-34", "11"),
            pair("Amatas iela", "Liepāja")
        );
        assert_eq!(
            street("Alejas iela 21 k-2, Jūrmala, LV-2012", "21 k-2"),
            pair("Alejas iela", "Jūrmala")
        );
        assert_eq!(street("Saules iela 15", "15"), None);
        let named = |first, last| latvian(None, "Riņņi", Some(first), Some(last));
        assert_eq!(
            named(
                "Garkalnes pag. Ropažu nov., LV-1024",
                "Berģi Garkalnes pag., Ropažu nov., LV-1024"
            ),
            pair("Berģi", "Berģi")
        );
        assert_eq!(
            named(
                "Dienvidkurzemes nov. LV-3473",
                "Nīcas pag. Dienvidkurzemes nov., LV-3473"
            ),
            pair("Nīcas pagasts", "Nīcas pagasts")
        );
        assert_eq!(
            named("Liepāja LV-3405", "Liepāja LV-3405"),
            pair("Liepāja", "Liepāja")
        );
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
