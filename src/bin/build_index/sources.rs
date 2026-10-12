//! Address sources beside Overture: OpenAddresses GeoJSON-lines exports and OpenStreetMap PBF
//! extracts. Both feed the same `Sink` as Overture rows, so a house number that several sources
//! hold on the same street collapses to one FST key.
use super::{Shared, Sink, country};
use osmpbf::{BlobDecode, BlobReader, ByteOffset, PrimitiveBlock, RelMemberType};
use rayon::prelude::*;
use serde::Deserialize;
use std::{
    borrow::Cow,
    fs::File,
    io::{BufRead, BufReader},
    ops::Range,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering::Relaxed},
    },
};

fn files(dir: &Path, suffix: &str) -> Vec<PathBuf> {
    let mut v: Vec<(u64, PathBuf)> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_str().is_some_and(|s| s.ends_with(suffix)))
        .map(|p| (std::fs::metadata(&p).map_or(0, |m| m.len()), p))
        .collect();
    // largest first, so the long files don't start last
    v.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    v.into_iter().map(|(_, p)| p).collect()
}

#[derive(Deserialize)]
struct Feature<'a> {
    #[serde(borrow)]
    properties: Props<'a>,
    geometry: Option<Geometry>,
}

#[derive(Deserialize)]
struct Props<'a> {
    #[serde(borrow, default)]
    number: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    street: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    postcode: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    city: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    district: Option<Cow<'a, str>>,
}

#[derive(Deserialize)]
struct Geometry {
    #[serde(rename = "type")]
    kind: String,
    coordinates: serde_json::Value,
}

const OA_BLOCK: usize = 4 << 20;

/// `*.geojson.gz` files as published by OpenAddresses batch jobs, one Feature per line, named
/// `<cc>_<rest of the source path>-<layer name>.geojson.gz`; the country comes from the name.
/// Returns the number of files read.
pub fn openaddresses(dir: &Path, shared: &Shared) -> usize {
    let files = files(dir, ".geojson.gz");
    let bad = AtomicU64::new(0);
    files.par_iter().for_each(|path| {
        let name = path.file_name().unwrap().to_string_lossy();
        let cc = country(name.split('_').next().map(str::to_uppercase).as_deref());
        if cc == [0; 2] {
            eprintln!("  skipping {name}: no country prefix");
            return;
        }
        // One thread decompresses into blocks of whole lines; workers parse blocks in parallel,
        // so one large file doesn't leave the other cores idle.
        let (tx, rx) = std::sync::mpsc::sync_channel::<String>(32);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let file = File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                let mut r =
                    BufReader::with_capacity(1 << 20, flate2::read::MultiGzDecoder::new(file));
                loop {
                    let mut block = String::with_capacity(OA_BLOCK + 4096);
                    while block.len() < OA_BLOCK {
                        match r.read_line(&mut block) {
                            Ok(0) => break,
                            Ok(_) => {}
                            Err(e) => {
                                eprintln!("  {name}: stopped reading: {e}");
                                bad.fetch_add(1, Relaxed);
                                break;
                            }
                        }
                    }
                    if block.is_empty() || tx.send(block).is_err() {
                        break;
                    }
                }
                drop(tx);
            });
            rx.into_iter().par_bridge().for_each_init(
                || Sink::new(shared),
                |sink, block| {
                    for line in block.lines() {
                        let Ok(f) = serde_json::from_str::<Feature>(line) else {
                            bad.fetch_add(1, Relaxed);
                            continue;
                        };
                        let Some(g) = f.geometry.filter(|g| g.kind == "Point") else {
                            continue;
                        };
                        let (Some(x), Some(y)) =
                            (g.coordinates[0].as_f64(), g.coordinates[1].as_f64())
                        else {
                            continue;
                        };
                        let p = &f.properties;
                        let city = p
                            .city
                            .as_deref()
                            .filter(|c| !c.trim().is_empty())
                            .or(p.district.as_deref());
                        sink.add(
                            Some(cc),
                            x,
                            y,
                            p.street.as_deref(),
                            p.number.as_deref(),
                            p.postcode.as_deref(),
                            city,
                        );
                    }
                },
            );
        });
    });
    let bad = bad.into_inner();
    if bad > 0 {
        println!("  {bad} unreadable OpenAddresses lines or files");
    }
    files.len()
}

/// Decodes blobs `range` in parallel, with one `S` per rayon job. One thread reads ahead so
/// workers don't queue on file reads.
fn for_blobs<S>(
    path: &Path,
    offsets: &[ByteOffset],
    range: Range<usize>,
    init: impl Fn() -> S + Sync + Send,
    f: impl Fn(&mut S, PrimitiveBlock) + Sync + Send,
) {
    let mut reader = BlobReader::seekable_from_path(path).unwrap_or_else(|e| fail(path, e));
    if range.is_empty() {
        return;
    }
    reader
        .seek(offsets[range.start])
        .unwrap_or_else(|e| fail(path, e));
    let (tx, rx) = std::sync::mpsc::sync_channel(32);
    std::thread::scope(|scope| {
        scope.spawn(move || {
            for b in reader.take(range.len()) {
                if tx.send(b.unwrap_or_else(|e| fail(path, e))).is_err() {
                    break;
                }
            }
        });
        rx.into_iter().par_bridge().for_each_init(init, |s, b| {
            if let BlobDecode::OsmData(block) = b.decode().unwrap_or_else(|e| fail(path, e)) {
                f(s, block);
            }
        });
    });
}

fn fail(path: &Path, e: osmpbf::Error) -> ! {
    panic!("{}: {e}", path.display())
}

/// Blob offsets, and the indexes of the first blob after the nodes and after the ways. Reads only blob headers and
/// decodes about log2(blobs) blocks. Needs a file sorted by type then id, as Geofabrik's are.
fn layout(path: &Path) -> (Vec<ByteOffset>, usize, usize) {
    let mut r = BlobReader::seekable_from_path(path).unwrap_or_else(|e| fail(path, e));
    let mut offsets = vec![];
    while let Some(h) = r.next_header_skip_blob() {
        offsets.push(
            h.unwrap_or_else(|e| fail(path, e))
                .1
                .expect("seekable reader gives offsets"),
        );
    }
    let header = r
        .blob_from_offset(offsets[0])
        .and_then(|b| b.to_headerblock())
        .unwrap_or_else(|e| fail(path, e));
    assert!(
        header
            .optional_features()
            .iter()
            .any(|f| f == "Sort.Type_then_ID"),
        "{}: not sorted by type then id; run `osmium sort` first",
        path.display()
    );
    let has_nodes = |b: &PrimitiveBlock| {
        b.groups()
            .any(|g| g.dense_nodes().next().is_some() || g.nodes().next().is_some())
    };
    let has_ways = |b: &PrimitiveBlock| b.groups().any(|g| g.ways().next().is_some());
    let mut first = |from: usize, before: &dyn Fn(&PrimitiveBlock) -> bool| {
        from + (from..offsets.len())
            .collect::<Vec<_>>()
            .partition_point(|&i| {
                let block = r
                    .blob_from_offset(offsets[i])
                    .and_then(|b| b.to_primitiveblock());
                before(&block.unwrap_or_else(|e| fail(path, e)))
            })
    };
    let first_way = first(1, &has_nodes);
    let first_relation = first(first_way, &|b| has_nodes(b) || has_ways(b));
    (offsets, first_way, first_relation)
}

#[derive(Default)]
struct Addr<'a> {
    number: Option<&'a str>,
    street: Option<&'a str>,
    postcode: Option<&'a str>,
    city: Option<&'a str>,
}

impl<'a> Addr<'a> {
    fn from(tags: impl Iterator<Item = (&'a str, &'a str)>) -> Option<Addr<'a>> {
        let mut a = Addr::default();
        for (k, v) in tags {
            match k {
                "addr:housenumber" => a.number = Some(v),
                "addr:street" => a.street = Some(v),
                "addr:postcode" => a.postcode = Some(v),
                "addr:city" => a.city = Some(v),
                _ => {}
            }
        }
        (a.number.is_some() || a.postcode.is_some()).then_some(a)
    }

    fn owned(&self) -> OwnedAddr {
        let o = |v: Option<&str>| v.map(str::to_string);
        OwnedAddr {
            number: o(self.number),
            street: o(self.street),
            postcode: o(self.postcode),
            city: o(self.city),
        }
    }

    fn add(&self, sink: &mut Sink, x: f64, y: f64) {
        sink.add(
            None,
            x,
            y,
            self.street,
            self.number,
            self.postcode,
            self.city,
        );
    }
}

struct OwnedAddr {
    number: Option<String>,
    street: Option<String>,
    postcode: Option<String>,
    city: Option<String>,
}

/// Per-job buffer, appended to the shared one when rayon drops it.
struct Collect<'m, T>(Vec<T>, &'m Mutex<Vec<T>>);

impl<T> Drop for Collect<'_, T> {
    fn drop(&mut self) {
        self.1.lock().unwrap().append(&mut self.0);
    }
}

fn pack(x: f64, y: f64) -> u64 {
    ((x as f32).to_bits() as u64) << 32 | (y as f32).to_bits() as u64
}

fn unpack(v: u64) -> (f64, f64) {
    (
        f32::from_bits((v >> 32) as u32) as f64,
        f32::from_bits(v as u32) as f64,
    )
}

/// Addressed nodes, ways (buildings) and relations (multipolygon buildings), the latter two
/// located at their (first outer way's) first node. The country is
/// the containing place's. Returns the number of files read.
// ponytail: a building's first node is within metres of its centroid; addr:interpolation ways
// and addr:place addresses without addr:street are skipped. Each needs node coordinates or place
// linking that this pass doesn't keep.
pub fn osm(dir: &Path, shared: &Shared) -> usize {
    let files = files(dir, ".osm.pbf");
    files.par_iter().for_each(|path| {
        let (offsets, first_way, first_relation) = layout(path);
        let (nodes, ways, relations) = (
            1..first_way,
            first_way..first_relation,
            first_relation..offsets.len(),
        );

        // 0: addressed relations (multipolygon buildings) and their first way, outer if any
        let found = Mutex::new(vec![]);
        for_blobs(
            path,
            &offsets,
            relations,
            || Collect(vec![], &found),
            |mine, block| {
                for r in block.groups().flat_map(|g| g.relations()) {
                    let Some(a) = Addr::from(r.tags()) else {
                        continue;
                    };
                    let ways = || r.members().filter(|m| m.member_type == RelMemberType::Way);
                    let way = ways()
                        .find(|m| m.role().is_ok_and(|role| role == "outer"))
                        .or_else(|| ways().next());
                    if let Some(w) = way {
                        mine.0.push((w.member_id, a.owned()));
                    }
                }
            },
        );
        let relations = found.into_inner().unwrap();
        let mut member_ways: Vec<i64> = relations.iter().map(|(w, _)| *w).collect();
        member_ways.par_sort_unstable();
        member_ways.dedup();

        // 1: first node of each addressed way and of those relations' ways
        let (refs, firsts) = (Mutex::new(vec![]), Mutex::new(vec![]));
        for_blobs(
            path,
            &offsets,
            ways.clone(),
            || (Collect(vec![], &refs), Collect(vec![], &firsts)),
            |(mine, first), block| {
                for w in block.groups().flat_map(|g| g.ways()) {
                    let Some(r) = w.refs().next() else { continue };
                    if member_ways.binary_search(&w.id()).is_ok() {
                        first.0.push((w.id(), r));
                        mine.0.push(r);
                    } else if Addr::from(w.tags()).is_some() {
                        mine.0.push(r);
                    }
                }
            },
        );
        let mut firsts = firsts.into_inner().unwrap();
        firsts.par_sort_unstable();
        let mut needed = refs.into_inner().unwrap();
        needed.par_sort_unstable();
        needed.dedup();

        // 2: addressed nodes, and coordinates of those first nodes
        let coords: Vec<AtomicU64> = (0..needed.len())
            .map(|_| AtomicU64::new(u64::MAX))
            .collect();
        for_blobs(
            path,
            &offsets,
            nodes,
            || Sink::new(shared),
            |sink, block| {
                for g in block.groups() {
                    let (lo, hi) = g.dense_nodes().fold((i64::MAX, i64::MIN), |(lo, hi), n| {
                        (lo.min(n.id()), hi.max(n.id()))
                    });
                    let s0 = needed.partition_point(|&r| r < lo);
                    let s1 = needed.partition_point(|&r| r <= hi);
                    for n in g.dense_nodes() {
                        if s0 < s1
                            && let Ok(k) = needed[s0..s1].binary_search(&n.id())
                        {
                            coords[s0 + k].store(pack(n.lon(), n.lat()), Relaxed);
                        }
                        if let Some(a) = Addr::from(n.tags()) {
                            a.add(sink, n.lon(), n.lat());
                        }
                    }
                    for n in g.nodes() {
                        if let Ok(k) = needed.binary_search(&n.id()) {
                            coords[k].store(pack(n.lon(), n.lat()), Relaxed);
                        }
                        if let Some(a) = Addr::from(n.tags()) {
                            a.add(sink, n.lon(), n.lat());
                        }
                    }
                }
            },
        );

        // 3: addressed ways
        for_blobs(
            path,
            &offsets,
            ways,
            || Sink::new(shared),
            |sink, block| {
                for w in block.groups().flat_map(|g| g.ways()) {
                    let Some(a) = Addr::from(w.tags()) else {
                        continue;
                    };
                    let Some(r) = w.refs().next() else { continue };
                    let Ok(k) = needed.binary_search(&r) else {
                        continue;
                    };
                    let v = coords[k].load(Relaxed);
                    if v != u64::MAX {
                        let (x, y) = unpack(v);
                        a.add(sink, x, y);
                    }
                }
            },
        );
        let coord = |node: i64| {
            let k = needed.binary_search(&node).ok()?;
            let v = coords[k].load(Relaxed);
            (v != u64::MAX).then(|| unpack(v))
        };
        let mut sink = Sink::new(shared);
        for (way, a) in &relations {
            let Ok(k) = firsts.binary_search_by_key(way, |(w, _)| *w) else {
                continue;
            };
            if let Some((x, y)) = coord(firsts[k].1) {
                sink.add(
                    None,
                    x,
                    y,
                    a.street.as_deref(),
                    a.number.as_deref(),
                    a.postcode.as_deref(),
                    a.city.as_deref(),
                );
            }
        }
    });
    files.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osm_tags_and_coordinates() {
        let tags = [
            ("building", "yes"),
            ("addr:housenumber", "12;14"),
            ("addr:street", "Main St"),
        ];
        let a = Addr::from(tags.into_iter()).unwrap();
        assert_eq!(
            (a.number, a.street, a.postcode),
            (Some("12;14"), Some("Main St"), None)
        );
        assert!(Addr::from([("building", "yes")].into_iter()).is_none());
        assert!(Addr::from([("addr:postcode", "SW1A 1AA")].into_iter()).is_some());
        let (x, y) = unpack(pack(-0.1276, 51.5072));
        assert!((x + 0.1276).abs() < 1e-5 && (y - 51.5072).abs() < 1e-5);
    }

    #[test]
    fn openaddresses_feature() {
        let line = r#"{"type": "Feature", "properties": {"hash": "f", "number": "113", "street": "\u0634\u0627\u0631\u0639", "unit": "", "city": "", "district": "D", "region": "", "postcode": "", "id": "5"}, "geometry": {"type": "Point", "coordinates": [51.63, 24.11]}}"#;
        let f: Feature = serde_json::from_str(line).unwrap();
        assert_eq!(f.properties.street.as_deref(), Some("شارع"));
        assert_eq!(f.properties.district.as_deref(), Some("D"));
        assert_eq!(f.geometry.unwrap().coordinates[1].as_f64(), Some(24.11));
    }
}
