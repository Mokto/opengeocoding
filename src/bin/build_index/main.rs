//! Builds the address index from a local Overture release directory (division, division_area,
//! segment and address parquet), without Nominatim. Only house numbers are spilled to disk.
mod geo;
mod index;
mod mem;

use arrow_array::{
    Array, ArrayRef, RecordBatch, StringArray,
    cast::AsArray,
    types::{Float64Type, Int32Type},
};
use geo::{Assigner, KINDS, Poly, wkb_point, wkb_polygons};
use index::{Bucket, Num, NumberSpill, fold, fold_into, pc_norm};
use parquet::arrow::{
    ProjectionMask,
    arrow_reader::{ArrowReaderMetadata, ParquetRecordBatchReaderBuilder},
};
use rayon::prelude::*;
use std::{
    collections::HashMap,
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

const USAGE: &str =
    "usage: build_index <overture-release-dir> <out-dir> [--geonames-postal allCountries.txt]";
const SHARDS: usize = 256;
const MAX_POSTCODES: usize = 20;
const MAX_CITIES: usize = 5;
const MAX_ALT_NAMES: usize = 64;
const NUMBER_SHARDS: usize = 16;
const MAX_WRITER_THREADS: usize = 16;
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

fn push_unique<T: PartialEq>(v: &mut Vec<T>, x: T, max: usize) {
    if v.len() < max && !v.contains(&x) {
        v.push(x);
    }
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
    /// All names, primary first, joined by NAME_SEP.
    names: Str,
    population: u64,
    xy: Option<(f64, f64)>,
    parents: Vec<Str>,
    anc: Box<[u32]>,
}

fn load_places(dir: &Path) -> (Vec<Place>, HashMap<Str, u32>) {
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
                    "population",
                    "hierarchies",
                    "geometry",
                ],
            ) {
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
                for i in 0..b.num_rows() {
                    names_at(nm, i, &mut names);
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
                    out.push(Place {
                        id: id.value(i).into(),
                        subtype: SUBTYPES
                            .iter()
                            .find(|s| **s == sub.value(i))
                            .unwrap_or(&"other"),
                        country: country(text(cc, i)),
                        names: names.join(&NAME_SEP.to_string()).into(),
                        population: if pop.is_null(i) {
                            0
                        } else {
                            pop.value(i).max(0) as u64
                        },
                        xy: (!geom.is_null(i))
                            .then(|| wkb_point(geom.value(i)))
                            .flatten(),
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

#[derive(Default)]
struct SegAgg {
    name: Str,
    alt: Vec<Str>,
    sx: f64,
    sy: f64,
    n: u64,
}

impl SegAgg {
    fn add(&mut self, other: SegAgg) {
        if self.n == 0 {
            self.name = other.name;
        }
        for a in other.alt {
            push_unique(&mut self.alt, a, MAX_ALT_NAMES);
        }
        (self.sx, self.sy, self.n) = (self.sx + other.sx, self.sy + other.sy, self.n + other.n);
    }
}

type SegKey = (u32, Str);

fn pass_segments(dir: &Path, a: &Assigner) -> Vec<Mutex<HashMap<SegKey, SegAgg>>> {
    let shards = sharded::<HashMap<SegKey, SegAgg>>();
    let rows = AtomicU64::new(0);
    tasks(dir).par_iter().for_each(|t| {
        // place -> folded name, so lookups borrow the key instead of allocating one per row
        let mut local: HashMap<u32, HashMap<Str, SegAgg>> = HashMap::new();
        let (mut key, mut last) = (String::new(), String::new());
        for b in read(t, &["subtype", "names.primary", "names.common", "bbox"]) {
            let (sub, nm, bbox) = (
                col(&b, "subtype").as_string::<i32>(),
                col(&b, "names").as_struct(),
                col(&b, "bbox").as_struct(),
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
                let e = match m.get_mut(key.as_str()) {
                    Some(e) => e,
                    None => m.entry(key.as_str().into()).or_insert_with(|| SegAgg {
                        name: primary.into(),
                        ..Default::default()
                    }),
                };
                for alt in &names[1..] {
                    if e.alt.len() < MAX_ALT_NAMES && !e.alt.iter().any(|a| &**a == *alt) {
                        e.alt.push((*alt).into());
                    }
                }
                (e.sx, e.sy, e.n) = (e.sx + x, e.sy + y, e.n + 1);
            }
        }
        for (place, m) in local {
            for (k, v) in m {
                let k = (place, k);
                shards[shard(&k)]
                    .lock()
                    .unwrap()
                    .entry(k)
                    .or_default()
                    .add(v);
            }
        }
    });
    println!("  {} segment rows", rows.into_inner());
    shards
}

#[derive(Default)]
struct AddrAgg {
    name: Str,
    /// Interned normalized postcodes and postal city names.
    pcs: Vec<u32>,
    cities: Vec<u32>,
    sx: f64,
    sy: f64,
    n: u64,
}

impl AddrAgg {
    fn add(&mut self, other: AddrAgg) {
        for p in other.pcs {
            push_unique(&mut self.pcs, p, MAX_POSTCODES);
        }
        for c in other.cities {
            push_unique(&mut self.cities, c, MAX_CITIES);
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
    sx: f64,
    sy: f64,
    n: u64,
}

impl PcAgg {
    fn add(&mut self, display: &str, x: f64, y: f64, n: u64) {
        if self.n == 0 {
            self.display = display.into();
        }
        (self.sx, self.sy, self.n) = (self.sx + x, self.sy + y, self.n + n);
    }
}

struct Addresses {
    streets: Vec<Mutex<AddrShard>>,
    postcodes: Vec<Mutex<HashMap<PcKey, PcAgg>>>,
    strings: Interner,
}

/// Street ids are (position in shard) << 8 | shard, so they're known when a task flushes.
fn pass_addresses(dir: &Path, a: &Assigner, numbers: &NumberSpill) -> Addresses {
    let streets = sharded::<AddrShard>();
    let postcodes = sharded::<HashMap<PcKey, PcAgg>>();
    let strings = Interner::new();
    let rows = AtomicU64::new(0);
    tasks(dir).par_iter().for_each(|t| {
        let mut local: HashMap<([u8; 2], u32), HashMap<Str, u32>> = HashMap::new();
        let mut aggs: Vec<AddrAgg> = vec![];
        let mut pcs: HashMap<[u8; 2], HashMap<Str, PcAgg>> = HashMap::new();
        let mut cache: HashMap<Str, u32> = HashMap::new();
        // sid holds the task-local street index until the flush remaps it
        let mut nums = Bucket::default();
        let (mut key, mut last, mut lower, mut pcn) =
            (String::new(), String::new(), String::new(), String::new());
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
            rows.fetch_add(b.num_rows() as u64, Relaxed);
            for i in 0..b.num_rows() {
                let c = country(text(&cc, i));
                if c == [0; 2] {
                    continue;
                }
                let Some((x, y)) = (!geom.is_null(i))
                    .then(|| wkb_point(geom.value(i)))
                    .flatten()
                else {
                    continue;
                };
                let pc = text(&postcode, i);
                if let Some(pc) = pc {
                    pcn.clear();
                    pcn.extend(
                        pc.chars()
                            .filter(|c| c.is_alphanumeric())
                            .flat_map(char::to_uppercase),
                    );
                    let m = pcs.entry(c).or_default();
                    match m.get_mut(pcn.as_str()) {
                        Some(e) => e.add(pc, x, y, 1),
                        None => m.entry(pcn.as_str().into()).or_default().add(pc, x, y, 1),
                    }
                }
                let Some(st) = text(&street, i) else { continue };
                if st != last {
                    fold_into(st, &mut key);
                    last.clear();
                    last.push_str(st);
                }
                if key.is_empty() {
                    continue;
                }
                let place = a.place(x, y).unwrap_or(NO_PLACE);
                let m = local.entry((c, place)).or_default();
                let li = match m.get(key.as_str()) {
                    Some(&li) => li,
                    None => {
                        aggs.push(AddrAgg {
                            name: st.into(),
                            ..Default::default()
                        });
                        m.insert(key.as_str().into(), aggs.len() as u32 - 1);
                        aggs.len() as u32 - 1
                    }
                };
                let e = &mut aggs[li as usize];
                if pc.is_some() && !pcn.is_empty() && e.pcs.len() < MAX_POSTCODES {
                    let id = strings.cached(&mut cache, &pcn);
                    push_unique(&mut e.pcs, id, MAX_POSTCODES);
                }
                let city = text(&postal, i).or_else(|| {
                    let (o, v) = (
                        levels.value_offsets(),
                        levels.values().as_struct().column(0).as_string::<i32>(),
                    );
                    (!levels.is_null(i) && o[i + 1] > o[i])
                        .then(|| text(v, o[i + 1] as usize - 1))
                        .flatten()
                });
                if let Some(city) = city
                    && e.cities.len() < MAX_CITIES
                {
                    let id = strings.cached(&mut cache, city);
                    push_unique(&mut e.cities, id, MAX_CITIES);
                }
                (e.sx, e.sy, e.n) = (e.sx + x, e.sy + y, e.n + 1);
                if let Some(n) = text(&number, i) {
                    lower.clear();
                    lower.extend(n.chars().flat_map(char::to_lowercase));
                    match Num::new(li, &lower, x as f32, y as f32) {
                        Ok(v) => nums.short.push(v),
                        Err(v) => nums.long.push(v),
                    }
                }
            }
        }
        let mut remap = vec![0u32; aggs.len()];
        for ((c, place), m) in local {
            for (k, li) in m {
                let key: AddrKey = (c, place, k);
                let agg = std::mem::take(&mut aggs[li as usize]);
                let sh = shard(&key);
                let mut s = streets[sh].lock().unwrap();
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
                remap[li as usize] = (pos << 8) | sh as u32;
            }
        }
        for (c, m) in pcs {
            for (k, v) in m {
                let k = (c, k);
                let mut s = postcodes[shard(&k)].lock().unwrap();
                s.entry(k).or_default().add(&v.display, v.sx, v.sy, v.n);
            }
        }
        let shards = numbers.shards();
        let mut out: Vec<Bucket> = (0..shards).map(|_| Bucket::default()).collect();
        for mut n in nums.short {
            n.sid = remap[n.sid as usize];
            out[n.sid as usize % shards].short.push(n);
        }
        for mut n in nums.long {
            n.sid = remap[n.sid as usize];
            out[n.sid as usize % shards].long.push(n);
        }
        numbers.add(out);
    });
    println!("  {} address rows", rows.into_inner());
    Addresses {
        streets,
        postcodes,
        strings,
    }
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

fn geonames_postcodes(
    path: &str,
    covered: &std::collections::HashSet<[u8; 2]>,
) -> HashMap<PcKey, PcAgg> {
    let mut agg: HashMap<PcKey, PcAgg> = HashMap::new();
    for l in std::fs::read_to_string(path).unwrap().lines() {
        let c: Vec<&str> = l.split('\t').collect();
        let (Some(cc), Some(pc)) = (c.first().filter(|s| s.len() == 2), c.get(1)) else {
            continue;
        };
        let cc = country(Some(cc));
        let (Some(y), Some(x)) = (
            c.get(9).and_then(|v| v.parse::<f64>().ok()),
            c.get(10).and_then(|v| v.parse::<f64>().ok()),
        ) else {
            continue;
        };
        if !covered.contains(&cc) {
            agg.entry((cc, pc_norm(pc).into()))
                .or_default()
                .add(pc, x, y, 1);
        }
    }
    agg
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(release), Some(out)) = (args.get(1), args.get(2)) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    let geonames = args
        .iter()
        .position(|a| a == "--geonames-postal")
        .map(|i| args.get(i + 1).expect(USAGE).clone());
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
        t = Instant::now();
    };

    let (places, idx) = load_places(&dir.join("division"));
    lap(&format!("places ({})", places.len()));
    let assigner = load_assigner(&dir.join("division_area"), &places, &idx);
    drop(idx);
    lap("polygons");
    let segs = pass_segments(&dir.join("segment"), &assigner);
    lap("segments");
    let spill = NumberSpill::new(Path::new(out).join(".numbers-spill"), NUMBER_SHARDS);
    let Addresses {
        streets,
        postcodes,
        strings,
    } = pass_addresses(&dir.join("address"), &assigner, &spill);
    drop(assigner);
    lap("addresses");

    // Each shard sorts in memory; allow as many at once as half the headroom fits.
    let shard_bytes = spill.bytes() / NUMBER_SHARDS as u64;
    let parallel = (headroom() / 2 / shard_bytes.max(1)).clamp(1, NUMBER_SHARDS as u64) as usize;
    println!("  sorting {parallel} of {NUMBER_SHARDS} house-number shards at a time");
    let n = spill.write_fsts(out, parallel);
    let strings = strings.table();
    lap(&format!("house numbers ({n})"));

    let headroom = headroom();
    let (threads, budget) = mem::writer_budget(
        headroom,
        MAX_WRITER_THREADS.min(rayon::current_num_threads()),
    );
    println!(
        "  writer: {threads} threads, {:.2} GB arena (headroom {:.1} GB)",
        budget as f64 / GB,
        headroom.min(1 << 50) as f64 / GB
    );
    let (index, writer, f) = index::create(out, threads, budget);
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
                      (x, y): (f64, f64),
                      weight: u64| {
        let cities: Vec<&str> = cities.iter().map(|&c| strings.get(c)).collect();
        let mut d = doc!(
            f.kind => "s",
            f.names => names.join(" "),
            f.city => cities.join(" "),
            f.country => cc,
            f.name => names[0],
            f.id => sid as u64,
            f.importance => weight,
            f.x => x,
            f.y => y,
        );
        for &pc in pcs {
            d.add_text(f.pcode, strings.get(pc));
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

    let (merged, max_sid) = (AtomicU64::new(0), AtomicU32::new(0));
    streets.par_iter().enumerate().for_each(|(sh, s)| {
        let s = s.lock().unwrap();
        for ((cc, place, key), &pos) in &s.ids {
            let st = &s.streets[pos as usize];
            let sid = (pos << 8) | sh as u32;
            max_sid.fetch_max(sid, Relaxed);
            let seg = (*place != NO_PLACE)
                .then(|| {
                    let k = (*place, key.clone());
                    segs[shard(&k)].lock().unwrap().remove(&k)
                })
                .flatten();
            let primary = seg.as_ref().map_or(&*st.name, |g| &*g.name);
            let mut names = vec![primary];
            if let Some(g) = &seg {
                merged.fetch_add(1, Relaxed);
                names.extend(g.alt.iter().map(|a| &**a).filter(|a| *a != primary));
            }
            let n = st.n + seg.as_ref().map_or(0, |g| g.n);
            street_doc(
                sid,
                country_str(cc),
                *place,
                &names,
                &st.pcs,
                &st.cities,
                (st.sx / st.n as f64, st.sy / st.n as f64),
                n,
            );
        }
    });
    let addr_streets: u64 = streets
        .iter()
        .map(|s| s.lock().unwrap().streets.len() as u64)
        .sum();
    drop(streets);
    let next = AtomicU32::new(max_sid.load(Relaxed) + 1);
    segs.par_iter().for_each(|s| {
        for ((place, _), g) in s.lock().unwrap().drain() {
            let sid = next.fetch_add(1, Relaxed);
            let names: Vec<&str> = std::iter::once(&*g.name)
                .chain(g.alt.iter().map(|a| &**a).filter(|a| *a != &*g.name))
                .collect();
            street_doc(
                sid,
                country_str(&places[place as usize].country),
                place,
                &names,
                &[],
                &[],
                (g.sx / g.n as f64, g.sy / g.n as f64),
                g.n,
            );
        }
    });
    drop(segs);
    let seg_only = (next.load(Relaxed) - max_sid.load(Relaxed) - 1) as u64;
    let merged = merged.load(Relaxed);
    lap(&format!(
        "street docs ({} streets: {merged} merged, {} address only, {seg_only} segment only)",
        addr_streets + seg_only,
        addr_streets - merged
    ));

    let mut postcodes: HashMap<PcKey, PcAgg> = postcodes
        .into_iter()
        .flat_map(|m| m.into_inner().unwrap())
        .collect();
    if let Some(path) = geonames {
        let covered = postcodes.keys().map(|(cc, _)| *cc).collect();
        postcodes.extend(geonames_postcodes(&path, &covered));
    }
    postcodes.par_iter().for_each(|((cc, pc), v)| {
        writer
            .add_document(doc!(
                f.kind => "z",
                f.pcode => &**pc,
                f.country => country_str(cc),
                f.name => &*v.display,
                f.importance => v.n,
                f.x => v.sx / v.n as f64,
                f.y => v.sy / v.n as f64,
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
    println!("total: {:.1}s", t0.elapsed().as_secs_f64());
}
