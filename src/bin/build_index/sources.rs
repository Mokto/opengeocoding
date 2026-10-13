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

/// What an OpenAddresses source's licence lets a commercial geocoder do with its data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Licence {
    /// Public domain, or open with at most attribution.
    Open,
    /// ODbL: share-alike, but the index already carries ODbL through OSM.
    Odbl,
    /// Another share-alike licence, which may not combine with ODbL.
    ShareAlike,
    /// Forbids sale or commercial use.
    Restricted,
    /// No licence stated, or none recognised.
    Unknown,
}

impl Licence {
    pub fn usable(self) -> bool {
        matches!(self, Licence::Open | Licence::Odbl)
    }
}

/// Classifies the `license` of an OpenAddresses source layer: a string, or an object with `url`,
/// `text` and `share-alike`.
// ponytail: substring rules over free-text licences, conservative (unrecognised = Unknown). A
// per-source allowlist reviewed by counsel is the upgrade path.
pub fn classify(license: Option<&serde_json::Value>) -> Licence {
    let Some(l) = license else {
        return Licence::Unknown;
    };
    let field = |k: &str| l.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let t = match l.as_str() {
        Some(s) => s.to_lowercase(),
        None => format!("{} {}", field("url"), field("text")).to_lowercase(),
    };
    let any = |words: &[&str]| words.iter().any(|w| t.contains(w));
    if any(&[
        "non-commercial",
        "noncommercial",
        "non commercial",
        "by-nc",
        "by-nd",
        "not for commercial",
        "no commercial",
        "shall sell",
        "not be sold",
        "may not sell",
        "resale",
        "resell",
        "personal use only",
        "internal use only",
    ]) {
        return Licence::Restricted;
    }
    if any(&["odbl", "opendatacommons.org/licenses/odbl"]) {
        return Licence::Odbl;
    }
    if l.get("share-alike").and_then(|v| v.as_bool()) == Some(true)
        || any(&["by-sa", "by sa", "iodl/1.0", "iodl 1.0"])
    {
        return Licence::ShareAlike;
    }
    if any(&[
        "publicdomain",
        "public-domain",
        "public domain",
        "pddl",
        "cc0",
        "cc 0",
        "creativecommons.org/licenses/by/",
        "creativecommons.org/licences/by/",
        "cc by",
        "cc-by",
        "ccby",
        "odc-by",
        "opendatacommons.org/licenses/by",
        "etalab",
        "licence ouverte",
        "open government licen",
        "open data licen",
        "public sector information",
        "dati.gov.it/iodl/2.0",
        "iodl 2.0",
        "datos.gob.mx/libreusomx",
        "licencia de datos abiertos",
        "dl-de/by",
        "dl-de-by",
        "dl-de/zero",
        "nlod",
        "catastro.minhap.es",
        "bev.gv.at",
        "no restrictions",
        "commercial",
    ]) {
        return Licence::Open;
    }
    Licence::Unknown
}

/// The licence of an OpenAddresses export, read from `<stem>.source.json` beside it: the source
/// definition its batch job ran, whose `layers.addresses` entry named after the file's layer
/// carries the licence (older definitions keep it at the top level).
pub fn oa_licence(data: &Path) -> (Licence, String) {
    let name = data.file_name().unwrap().to_string_lossy();
    let stem = name.trim_end_matches(".geojson.gz");
    let layer = stem.rsplit('-').next().unwrap_or("");
    let Ok(text) = std::fs::read_to_string(data.with_file_name(format!("{stem}.source.json")))
    else {
        return (Licence::Unknown, "no source definition".into());
    };
    let Ok(src) = serde_json::from_str::<serde_json::Value>(&text) else {
        return (Licence::Unknown, "unreadable source definition".into());
    };
    let license = src["layers"]["addresses"]
        .as_array()
        .and_then(|ls| ls.iter().find(|l| l["name"] == layer))
        .and_then(|l| l.get("license"))
        .or(src.get("license"));
    let shown = license.map_or(String::new(), |l| match l.as_str() {
        Some(s) => s.to_string(),
        None => [&l["url"], &l["text"], &l["attribution name"]]
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join(" | "),
    });
    (classify(license), shown.replace(['\t', '\n', '\r'], " "))
}

/// `*.geojson.gz` files as published by OpenAddresses batch jobs, one Feature per line, named
/// `<cc>_<rest of the source path>-<layer name>.geojson.gz`; the country comes from the name.
/// Unless `any_licence`, only files whose licence is `usable` are read. Every file's licence goes
/// to `openaddresses.tsv` in `out`. Returns the number of files read.
pub fn openaddresses(dir: &Path, out: &Path, any_licence: bool, shared: &Shared) -> usize {
    let mut report = String::from("file\tlicence\tread\tbytes\tlicense\n");
    let mut skipped = std::collections::BTreeMap::<Licence, (usize, u64)>::new();
    let files: Vec<PathBuf> = files(dir, ".geojson.gz")
        .into_iter()
        .filter(|path| {
            let (class, shown) = oa_licence(path);
            let read = any_licence || class.usable();
            let bytes = std::fs::metadata(path).map_or(0, |m| m.len());
            if !read {
                let e = skipped.entry(class).or_default();
                (e.0, e.1) = (e.0 + 1, e.1 + bytes);
            }
            let name = path.file_name().unwrap().to_string_lossy();
            report += &format!("{name}\t{class:?}\t{read}\t{bytes}\t{shown}\n");
            read
        })
        .collect();
    let report_path = out.join("openaddresses.tsv");
    let mut existing = std::fs::read_to_string(&report_path).unwrap_or_default();
    if !existing.is_empty() {
        report = report.split_once('\n').unwrap().1.to_string();
    }
    existing += &report;
    std::fs::write(&report_path, existing).unwrap();
    for (class, (n, bytes)) in &skipped {
        println!(
            "  skipped {n} OpenAddresses files ({:.2} GB), licence {class:?}",
            *bytes as f64 / 1e9
        );
    }
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
        let mut place = None;
        for (k, v) in tags {
            match k {
                "addr:housenumber" => a.number = Some(v),
                "addr:street" => a.street = Some(v),
                "addr:postcode" => a.postcode = Some(v),
                "addr:city" => a.city = Some(v),
                "addr:place" => place = Some(v),
                _ => {}
            }
        }
        // addr:place numbers within a hamlet or square that has no street of its own
        a.street = a.street.or(place);
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

/// An OSM addr:interpolation way: the numbers between its addressed nodes, spaced along it.
struct Interp {
    refs: Vec<i64>,
    step: u32,
    street: Option<String>,
    postcode: Option<String>,
}

impl Interp {
    /// Ranges longer than this are mapping errors, not streets.
    const MAX: u32 = 500;

    fn from(w: &osmpbf::Way) -> Option<Interp> {
        let (mut step, mut street, mut postcode) = (None, None, None);
        for (k, v) in w.tags() {
            match k {
                "addr:interpolation" => {
                    step = match v {
                        "odd" | "even" => Some(2),
                        "all" => Some(1),
                        n => n.parse().ok().filter(|&n| n > 0),
                    }
                }
                "addr:street" => street = Some(v.to_string()),
                "addr:postcode" => postcode = Some(v.to_string()),
                _ => {}
            }
        }
        let refs: Vec<i64> = w.refs().collect();
        (refs.len() >= 2).then_some(())?;
        Some(Interp {
            refs,
            step: step?,
            street,
            postcode,
        })
    }

    /// (x, y, street, number, postcode) for each number strictly between consecutive addressed
    /// nodes; `line` holds the nodes' coordinates.
    fn expand<'s>(
        &'s self,
        line: &[Option<(f64, f64)>],
        end: impl Fn(i64) -> Option<&'s OwnedAddr>,
    ) -> Vec<(f64, f64, &'s str, String, Option<&'s str>)> {
        let mut out = vec![];
        if line.iter().any(Option::is_none) {
            return out;
        }
        let pts: Vec<(f64, f64)> = line.iter().map(|p| p.unwrap()).collect();
        let mut along = vec![0.0];
        for w in pts.windows(2) {
            let ((x0, y0), (x1, y1)) = (w[0], w[1]);
            let d = ((x1 - x0) * y0.to_radians().cos()).hypot(y1 - y0);
            along.push(along.last().unwrap() + d);
        }
        let at = |t: f64| {
            let k = along.partition_point(|&a| a < t).clamp(1, pts.len() - 1);
            let span = along[k] - along[k - 1];
            let f = if span > 0.0 {
                (t - along[k - 1]) / span
            } else {
                0.0
            };
            let ((x0, y0), (x1, y1)) = (pts[k - 1], pts[k]);
            (x0 + (x1 - x0) * f, y0 + (y1 - y0) * f)
        };
        let numbered: Vec<(usize, u32, &OwnedAddr)> = self
            .refs
            .iter()
            .enumerate()
            .filter_map(|(k, &r)| {
                let a = end(r)?;
                Some((k, a.number.as_deref()?.trim().parse().ok()?, a))
            })
            .collect();
        for w in numbered.windows(2) {
            let ((k0, n0, a0), (k1, n1, a1)) = (w[0], w[1]);
            let (lo, hi) = (n0.min(n1), n0.max(n1));
            let Some(street) = self
                .street
                .as_deref()
                .or(a0.street.as_deref())
                .or(a1.street.as_deref())
            else {
                continue;
            };
            if hi - lo > Self::MAX || (hi - lo) % self.step != 0 {
                continue;
            }
            let postcode = self.postcode.as_deref().or(a0.postcode.as_deref());
            let mut n = lo + self.step;
            while n < hi {
                let f = (n - n0.min(n1)) as f64 / (hi - lo) as f64;
                let f = if n0 <= n1 { f } else { 1.0 - f };
                let (x, y) = at(along[k0] + (along[k1] - along[k0]) * f);
                out.push((x, y, street, n.to_string(), postcode));
                n += self.step;
            }
        }
        out
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
// ponytail: a building's first node is within metres of its centroid.
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

        // 1: first node of each addressed way and of those relations' ways, and every node of
        // addr:interpolation ways
        let (refs, firsts, interps) = (Mutex::new(vec![]), Mutex::new(vec![]), Mutex::new(vec![]));
        for_blobs(
            path,
            &offsets,
            ways.clone(),
            || {
                (
                    Collect(vec![], &refs),
                    Collect(vec![], &firsts),
                    Collect(vec![], &interps),
                )
            },
            |(mine, first, interp), block| {
                for w in block.groups().flat_map(|g| g.ways()) {
                    if let Some(i) = Interp::from(&w) {
                        mine.0.extend(&i.refs);
                        interp.0.push(i);
                        continue;
                    }
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
        let interps = interps.into_inner().unwrap();
        let mut ends: Vec<i64> = interps
            .iter()
            .flat_map(|i| i.refs.iter().copied())
            .collect();
        ends.par_sort_unstable();
        ends.dedup();
        let ended = Mutex::new(vec![]);

        // 2: addressed nodes, and coordinates of those first nodes
        let coords: Vec<AtomicU64> = (0..needed.len())
            .map(|_| AtomicU64::new(u64::MAX))
            .collect();
        for_blobs(
            path,
            &offsets,
            nodes,
            || (Sink::new(shared), Collect(vec![], &ended)),
            |(sink, end), block| {
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
                            if ends.binary_search(&n.id()).is_ok() {
                                end.0.push((n.id(), a.owned()));
                            }
                            a.add(sink, n.lon(), n.lat());
                        }
                    }
                    for n in g.nodes() {
                        if let Ok(k) = needed.binary_search(&n.id()) {
                            coords[k].store(pack(n.lon(), n.lat()), Relaxed);
                        }
                        if let Some(a) = Addr::from(n.tags()) {
                            if ends.binary_search(&n.id()).is_ok() {
                                end.0.push((n.id(), a.owned()));
                            }
                            a.add(sink, n.lon(), n.lat());
                        }
                    }
                }
            },
        );
        let mut ended = ended.into_inner().unwrap();
        ended.par_sort_unstable_by_key(|(id, _)| *id);

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
        for i in &interps {
            let line: Vec<Option<(f64, f64)>> = i.refs.iter().map(|&r| coord(r)).collect();
            let end = |r: i64| {
                ended
                    .binary_search_by_key(&r, |(id, _)| *id)
                    .ok()
                    .map(|k| &ended[k].1)
            };
            for (x, y, street, number, postcode) in i.expand(&line, end) {
                sink.add(None, x, y, Some(street), Some(&number), postcode, None);
            }
        }
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
    fn licences() {
        let c = |v: serde_json::Value| classify(Some(&v));
        assert_eq!(classify(None), Licence::Unknown);
        assert_eq!(c(serde_json::json!("Unknown")), Licence::Unknown);
        assert_eq!(
            c(
                serde_json::json!({"url": "https://creativecommons.org/licenses/by/4.0/", "text": "CC BY 4.0"})
            ),
            Licence::Open
        );
        assert_eq!(
            c(
                serde_json::json!({"url": "http://opendatacommons.org/licenses/odbl/1.0/", "share-alike": true})
            ),
            Licence::Odbl
        );
        assert_eq!(c(serde_json::json!("CC-BY-SA")), Licence::ShareAlike);
        assert_eq!(
            c(
                serde_json::json!({"text": "RLIS Open Database End User License", "share-alike": true})
            ),
            Licence::ShareAlike
        );
        assert_eq!(
            c(
                serde_json::json!({"text": "No person shall sell, give, reproduce... any portion of the data"})
            ),
            Licence::Restricted
        );
        assert_eq!(
            c(serde_json::json!({"text": "CC BY-NC 4.0, for non-commercial use"})),
            Licence::Restricted
        );
        assert_eq!(
            c(
                serde_json::json!({"text": "license to exploit the Data for commercial and/or personal use"})
            ),
            Licence::Open
        );
        assert_eq!(
            c(serde_json::json!({"url": "", "text": ""})),
            Licence::Unknown
        );
        for usable in [Licence::Open, Licence::Odbl] {
            assert!(usable.usable());
        }
        for unusable in [Licence::ShareAlike, Licence::Restricted, Licence::Unknown] {
            assert!(!unusable.usable());
        }
    }

    #[test]
    fn licence_of_the_file_layer() {
        let dir = std::env::temp_dir().join(format!("oa-licence-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let data = dir.join("us_xx_town-county.geojson.gz");
        std::fs::write(
            dir.join("us_xx_town-county.source.json"),
            r#"{"layers":{"addresses":[
                {"name":"city","license":"CC0"},
                {"name":"county","license":{"text":"Not for resale"}}]}}"#,
        )
        .unwrap();
        assert_eq!(oa_licence(&data).0, Licence::Restricted);
        std::fs::write(
            dir.join("us_xx_town-county.source.json"),
            r#"{"license":"CC0"}"#,
        )
        .unwrap();
        assert_eq!(oa_licence(&data).0, Licence::Open);
        assert_eq!(
            oa_licence(&dir.join("none-city.geojson.gz")).0,
            Licence::Unknown
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn interpolation_spaces_numbers_along_the_way() {
        let i = Interp {
            refs: vec![1, 2, 3],
            step: 2,
            street: None,
            postcode: None,
        };
        let addr = |n: &str| OwnedAddr {
            number: Some(n.into()),
            street: Some("High St".into()),
            postcode: Some("AB1".into()),
            city: None,
        };
        let (a, c) = (addr("10"), addr("2"));
        let line = [Some((0.0, 0.0)), Some((0.5, 0.0)), Some((1.0, 0.0))];
        let got = i.expand(&line, |r| match r {
            1 => Some(&a),
            3 => Some(&c),
            _ => None,
        });
        let numbers: Vec<(&str, f64)> = got.iter().map(|g| (g.3.as_str(), g.0)).collect();
        assert_eq!(numbers, [("4", 0.75), ("6", 0.5), ("8", 0.25)]);
        assert!(got.iter().all(|g| g.2 == "High St" && g.4 == Some("AB1")));
        let odd = addr("3");
        assert!(
            i.expand(&line, |r| (r != 2).then_some(if r == 1 {
                &a
            } else {
                &odd
            }))
            .is_empty()
        );
    }

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
