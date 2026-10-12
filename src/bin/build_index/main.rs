//! Builds the address index from a local Overture release directory (division, division_area,
//! segment and address parquet), without Nominatim and without intermediate files.
mod geo;
mod index;

use arrow_array::{
    Array, ArrayRef, RecordBatch, StringArray,
    cast::AsArray,
    types::{Float64Type, Int32Type},
};
use geo::{Assigner, KINDS, Poly, wkb_point, wkb_polygons};
use index::{LongNum, Num, fold, fold_into, pc_norm};
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

const USAGE: &str =
    "usage: build_index <overture-release-dir> <out-dir> [--geonames-postal allCountries.txt]";
const SHARDS: usize = 256;
const MAX_POSTCODES: usize = 20;
const MAX_CITIES: usize = 5;
const MAX_ALT_NAMES: usize = 64;
const NUMBER_SHARDS: usize = 16;

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
fn read(t: &Task, roots: &[&str]) -> Vec<RecordBatch> {
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
        .collect()
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

/// names.primary and the values of names.common
fn names_at(names: &arrow_array::StructArray, i: usize, out: &mut Vec<String>) {
    out.clear();
    if names.is_null(i) {
        return;
    }
    if let Some(p) = text(
        names.column_by_name("primary").unwrap().as_string::<i32>(),
        i,
    ) {
        out.push(p.to_string());
    }
    let common = names.column_by_name("common").unwrap().as_map();
    if common.is_null(i) {
        return;
    }
    let (o, values) = (common.value_offsets(), common.values().as_string::<i32>());
    for j in o[i] as usize..o[i + 1] as usize {
        if let Some(v) = text(values, j)
            && !out.iter().any(|n| n == v)
        {
            out.push(v.to_string());
        }
    }
}

fn shard<K: Hash>(k: &K) -> usize {
    BuildHasherDefault::<DefaultHasher>::default().hash_one(k) as usize % SHARDS
}

fn sharded<T: Default>() -> Vec<Mutex<T>> {
    (0..SHARDS).map(|_| Mutex::default()).collect()
}

struct Place {
    id: String,
    subtype: String,
    country: String,
    names: Vec<String>,
    population: u64,
    xy: Option<(f64, f64)>,
    parents: Vec<String>,
    anc: Vec<u32>,
}

fn load_places(dir: &Path) -> (Vec<Place>, HashMap<String, u32>) {
    let mut places: Vec<Place> = tasks(dir)
        .par_iter()
        .flat_map_iter(|t| {
            let mut out = vec![];
            let mut names = vec![];
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
                            parents.extend(
                                (0..ids.len())
                                    .filter_map(|k| text(ids, k))
                                    .map(str::to_string),
                            );
                        }
                    }
                    out.push(Place {
                        id: id.value(i).to_string(),
                        subtype: sub.value(i).to_string(),
                        country: text(cc, i).unwrap_or("").to_string(),
                        names: names.clone(),
                        population: if pop.is_null(i) {
                            0
                        } else {
                            pop.value(i).max(0) as u64
                        },
                        xy: (!geom.is_null(i))
                            .then(|| wkb_point(geom.value(i)))
                            .flatten(),
                        parents,
                        anc: vec![],
                    });
                }
            }
            out
        })
        .collect();
    places.par_sort_unstable_by(|a, b| a.id.cmp(&b.id));
    let idx: HashMap<String, u32> = places
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.clone(), i as u32))
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
        p.anc = anc;
        p.parents = vec![];
    });
    (places, idx)
}

fn load_assigner(dir: &Path, places: &[Place], idx: &HashMap<String, u32>) -> Assigner {
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
    name: String,
    alt: Vec<String>,
    sx: f64,
    sy: f64,
    n: u64,
}

type SegKey = (u32, String);

fn pass_segments(dir: &Path, a: &Assigner) -> Vec<Mutex<HashMap<SegKey, SegAgg>>> {
    let shards = sharded::<HashMap<SegKey, SegAgg>>();
    let rows = AtomicU64::new(0);
    tasks(dir).par_iter().for_each(|t| {
        let mut local: HashMap<SegKey, SegAgg> = HashMap::new();
        let (mut names, mut key, mut last) = (vec![], String::new(), String::new());
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
            rows.fetch_add(b.num_rows() as u64, Relaxed);
            for i in 0..b.num_rows() {
                if sub.is_null(i) || sub.value(i) != "road" || bbox.is_null(i) {
                    continue;
                }
                names_at(nm, i, &mut names);
                let Some(primary) = names.first() else {
                    continue;
                };
                let (x, y) = (
                    (x0.value(i) + x1.value(i)) / 2.0,
                    (y0.value(i) + y1.value(i)) / 2.0,
                );
                let Some(place) = a.place(x, y) else { continue };
                if *primary != last {
                    fold_into(primary, &mut key);
                    last.clone_from(primary);
                }
                if key.is_empty() {
                    continue;
                }
                let e = local.entry((place, key.clone())).or_default();
                if e.n == 0 {
                    e.name.clone_from(primary);
                }
                for alt in &names[1..] {
                    if e.alt.len() < MAX_ALT_NAMES && !e.alt.contains(alt) {
                        e.alt.push(alt.clone());
                    }
                }
                (e.sx, e.sy, e.n) = (e.sx + x, e.sy + y, e.n + 1);
            }
        }
        for (k, v) in local {
            let mut s = shards[shard(&k)].lock().unwrap();
            let e = s.entry(k).or_default();
            if e.n == 0 {
                e.name = v.name;
            }
            for alt in v.alt {
                if e.alt.len() < MAX_ALT_NAMES && !e.alt.contains(&alt) {
                    e.alt.push(alt);
                }
            }
            (e.sx, e.sy, e.n) = (e.sx + v.sx, e.sy + v.sy, e.n + v.n);
        }
    });
    println!("  {} segment rows", rows.into_inner());
    shards
}

#[derive(Default)]
struct AddrAgg {
    key: AddrKey,
    name: String,
    pcs: Vec<String>,
    cities: Vec<String>,
    sx: f64,
    sy: f64,
    n: u64,
}

type AddrKey = ([u8; 2], u32, String);
type PcKey = ([u8; 2], String);
const NO_PLACE: u32 = u32::MAX;

impl AddrAgg {
    fn add(&mut self, other: AddrAgg) {
        for p in other.pcs {
            if self.pcs.len() < MAX_POSTCODES && !self.pcs.contains(&p) {
                self.pcs.push(p);
            }
        }
        for c in other.cities {
            if self.cities.len() < MAX_CITIES && !self.cities.contains(&c) {
                self.cities.push(c);
            }
        }
        (self.sx, self.sy, self.n) = (self.sx + other.sx, self.sy + other.sy, self.n + other.n);
    }
}

#[derive(Default)]
struct AddrShard {
    ids: HashMap<AddrKey, u32>,
    streets: Vec<AddrAgg>,
}

#[derive(Default)]
struct PcAgg {
    display: String,
    sx: f64,
    sy: f64,
    n: u64,
}

struct Addresses {
    streets: Vec<Mutex<AddrShard>>,
    postcodes: Vec<Mutex<HashMap<PcKey, PcAgg>>>,
    short: Vec<Vec<Num>>,
    long: Vec<Vec<LongNum>>,
}

/// Street ids are (position in shard) << 8 | shard, so they're known when a task flushes.
fn pass_addresses(dir: &Path, a: &Assigner) -> Addresses {
    let streets = sharded::<AddrShard>();
    let postcodes = sharded::<HashMap<PcKey, PcAgg>>();
    let rows = AtomicU64::new(0);
    let (short, long): (Vec<Vec<Num>>, Vec<Vec<LongNum>>) = tasks(dir)
        .par_iter()
        .map(|t| {
            let mut local: HashMap<AddrKey, u32> = HashMap::new();
            let mut aggs: Vec<AddrAgg> = vec![];
            let mut pcs: HashMap<PcKey, PcAgg> = HashMap::new();
            let mut nums: Vec<(u32, String, f32, f32)> = vec![];
            let (mut key, mut last) = (String::new(), String::new());
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
                    let Some(c) = text(&cc, i).filter(|c| c.len() == 2) else {
                        continue;
                    };
                    let c: [u8; 2] = c.as_bytes().try_into().unwrap();
                    let Some((x, y)) = (!geom.is_null(i))
                        .then(|| wkb_point(geom.value(i)))
                        .flatten()
                    else {
                        continue;
                    };
                    let pc = text(&postcode, i);
                    if let Some(pc) = pc {
                        let e = pcs.entry((c, pc_norm(pc))).or_default();
                        if e.n == 0 {
                            e.display = pc.to_string();
                        }
                        (e.sx, e.sy, e.n) = (e.sx + x, e.sy + y, e.n + 1);
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
                    let k: AddrKey = (c, place, key.clone());
                    let li = *local.entry(k).or_insert_with_key(|k| {
                        aggs.push(AddrAgg {
                            key: k.clone(),
                            name: st.to_string(),
                            ..Default::default()
                        });
                        aggs.len() as u32 - 1
                    });
                    let e = &mut aggs[li as usize];
                    let city = text(&postal, i).or_else(|| {
                        let (o, v) = (
                            levels.value_offsets(),
                            levels.values().as_struct().column(0).as_string::<i32>(),
                        );
                        (!levels.is_null(i) && o[i + 1] > o[i])
                            .then(|| text(v, o[i + 1] as usize - 1))
                            .flatten()
                    });
                    if let Some(pc) = pc
                        && e.pcs.len() < MAX_POSTCODES
                        && !e.pcs.iter().any(|p| p == pc)
                    {
                        e.pcs.push(pc.to_string());
                    }
                    if let Some(city) = city
                        && e.cities.len() < MAX_CITIES
                        && !e.cities.iter().any(|p| p == city)
                    {
                        e.cities.push(city.to_string());
                    }
                    (e.sx, e.sy, e.n) = (e.sx + x, e.sy + y, e.n + 1);
                    if let Some(n) = text(&number, i) {
                        nums.push((li, n.to_lowercase(), x as f32, y as f32));
                    }
                }
            }
            let remap: Vec<u32> = aggs
                .into_iter()
                .map(|agg| {
                    let sh = shard(&agg.key);
                    let mut s = streets[sh].lock().unwrap();
                    let pos = match s.ids.get(&agg.key) {
                        Some(&p) => {
                            s.streets[p as usize].add(agg);
                            p
                        }
                        None => {
                            let p = s.streets.len() as u32;
                            s.ids.insert(agg.key.clone(), p);
                            s.streets.push(agg);
                            p
                        }
                    };
                    assert!(pos < 1 << 24, "too many streets in one shard");
                    (pos << 8) | sh as u32
                })
                .collect();
            for (k, v) in pcs {
                let mut s = postcodes[shard(&k)].lock().unwrap();
                let e = s.entry(k).or_default();
                if e.n == 0 {
                    e.display = v.display;
                }
                (e.sx, e.sy, e.n) = (e.sx + v.sx, e.sy + v.sy, e.n + v.n);
            }
            let (mut short, mut long) = (Vec::with_capacity(nums.len()), vec![]);
            for (li, n, x, y) in nums {
                match Num::new(remap[li as usize], &n, x, y) {
                    Ok(v) => short.push(v),
                    Err(v) => long.push(v),
                }
            }
            (short, long)
        })
        .unzip();
    println!("  {} address rows", rows.into_inner());
    Addresses {
        streets,
        postcodes,
        short,
        long,
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
) -> Vec<([u8; 2], String, String, f64, f64)> {
    let mut agg: HashMap<PcKey, PcAgg> = HashMap::new();
    for l in std::fs::read_to_string(path).unwrap().lines() {
        let c: Vec<&str> = l.split('\t').collect();
        let (Some(cc), Some(pc)) = (c.first().filter(|s| s.len() == 2), c.get(1)) else {
            continue;
        };
        let cc: [u8; 2] = cc.as_bytes().try_into().unwrap();
        let (Some(y), Some(x)) = (
            c.get(9).and_then(|v| v.parse::<f64>().ok()),
            c.get(10).and_then(|v| v.parse::<f64>().ok()),
        ) else {
            continue;
        };
        if covered.contains(&cc) {
            continue;
        }
        let e = agg.entry((cc, pc_norm(pc))).or_default();
        if e.n == 0 {
            e.display = pc.to_string();
        }
        (e.sx, e.sy, e.n) = (e.sx + x, e.sy + y, e.n + 1);
    }
    agg.into_iter()
        .map(|((cc, pc), v)| (cc, pc, v.display, v.sx / v.n as f64, v.sy / v.n as f64))
        .collect()
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
    let (t0, mut t) = (Instant::now(), Instant::now());
    let mut lap = |what: &str| {
        println!("{what}: {:.1}s", t.elapsed().as_secs_f64());
        t = Instant::now();
    };

    let (places, idx) = load_places(&dir.join("division"));
    lap(&format!("places ({})", places.len()));
    let assigner = load_assigner(&dir.join("division_area"), &places, &idx);
    drop(idx);
    lap("polygons");
    let segs = pass_segments(&dir.join("segment"), &assigner);
    lap("segments");
    let addr = pass_addresses(&dir.join("address"), &assigner);
    drop(assigner);
    lap("addresses");

    let (index, writer, f) = index::create(out);
    places
        .par_iter()
        .enumerate()
        .filter(|(_, p)| !p.names.is_empty())
        .for_each(|(i, p)| {
            let mut d = doc!(
                f.kind => "p",
                f.names => p.names.join(" "),
                f.country => p.country.as_str(),
                f.ids => i.to_string(),
                f.name => p.names[0].as_str(),
                f.id => i as u64,
                f.importance => p.population + place_weight(&p.subtype),
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
                      pcs: &[String],
                      cities: &[String],
                      (x, y): (f64, f64),
                      weight: u64| {
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
        for pc in pcs {
            d.add_text(f.pcode, pc_norm(pc));
        }
        if place != NO_PLACE {
            d.add_text(f.ids, place.to_string());
            for a in &places[place as usize].anc {
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
    addr.streets.par_iter().for_each(|s| {
        let s = s.lock().unwrap();
        for (pos, st) in s.streets.iter().enumerate() {
            let (cc, place, key) = &st.key;
            let sid = ((pos as u32) << 8) | shard(&st.key) as u32;
            max_sid.fetch_max(sid, Relaxed);
            let seg = (*place != NO_PLACE)
                .then(|| {
                    segs[shard(&(*place, key.clone()))]
                        .lock()
                        .unwrap()
                        .remove(&(*place, key.clone()))
                })
                .flatten();
            let primary = seg.as_ref().map_or(st.name.as_str(), |g| g.name.as_str());
            let mut names = vec![primary];
            if let Some(g) = &seg {
                merged.fetch_add(1, Relaxed);
                names.extend(g.alt.iter().map(String::as_str).filter(|a| *a != primary));
            }
            let n = st.n + seg.as_ref().map_or(0, |g| g.n);
            street_doc(
                sid,
                std::str::from_utf8(cc).unwrap(),
                *place,
                &names,
                &st.pcs,
                &st.cities,
                (st.sx / st.n as f64, st.sy / st.n as f64),
                n,
            );
        }
    });
    let addr_streets: usize = addr
        .streets
        .iter()
        .map(|s| s.lock().unwrap().streets.len())
        .sum();
    let next = AtomicU32::new(max_sid.load(Relaxed) + 1);
    segs.par_iter().for_each(|s| {
        for ((place, _), g) in s.lock().unwrap().drain() {
            let sid = next.fetch_add(1, Relaxed);
            let names: Vec<&str> = std::iter::once(g.name.as_str())
                .chain(g.alt.iter().map(String::as_str).filter(|a| *a != g.name))
                .collect();
            street_doc(
                sid,
                &places[place as usize].country,
                place,
                &names,
                &[],
                &[],
                (g.sx / g.n as f64, g.sy / g.n as f64),
                g.n,
            );
        }
    });
    let seg_only = next.load(Relaxed) - max_sid.load(Relaxed) - 1;
    lap(&format!(
        "street docs ({} streets: {} merged, {} address only, {seg_only} segment only)",
        addr_streets as u64 + seg_only as u64,
        merged.load(Relaxed),
        addr_streets as u64 - merged.load(Relaxed)
    ));

    let mut covered = std::collections::HashSet::new();
    let mut n_pc = 0;
    for s in &addr.postcodes {
        for ((cc, pc), v) in s.lock().unwrap().iter() {
            covered.insert(*cc);
            n_pc += 1;
            writer
                .add_document(doc!(
                    f.kind => "z",
                    f.pcode => pc.as_str(),
                    f.country => std::str::from_utf8(cc).unwrap(),
                    f.name => v.display.as_str(),
                    f.importance => v.n,
                    f.x => v.sx / v.n as f64,
                    f.y => v.sy / v.n as f64,
                ))
                .unwrap();
        }
    }
    if let Some(path) = geonames {
        for (cc, pc, display, x, y) in geonames_postcodes(&path, &covered) {
            n_pc += 1;
            writer
                .add_document(doc!(
                    f.kind => "z",
                    f.pcode => pc,
                    f.country => std::str::from_utf8(&cc).unwrap(),
                    f.name => display,
                    f.importance => 0u64,
                    f.x => x,
                    f.y => y,
                ))
                .unwrap();
        }
    }
    lap(&format!("postcode docs ({n_pc})"));

    let mut writer = writer;
    writer.commit().unwrap();
    lap(&format!(
        "index commit ({} segments)",
        index.searchable_segment_ids().unwrap().len()
    ));
    let n = index::numbers(
        out,
        &addr.short,
        &addr.long,
        max_sid.load(Relaxed),
        NUMBER_SHARDS,
    );
    lap(&format!("house numbers ({n})"));
    println!("total: {:.1}s", t0.elapsed().as_secs_f64());
}
