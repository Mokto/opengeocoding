//! Address sources beside Overture: OpenAddresses GeoJSON-lines exports and OpenStreetMap PBF
//! extracts. Both feed the same `Sink` as Overture rows, so a house number that several sources
//! hold on the same street collapses to one FST key.
use super::{
    Shared, Sink, country, numbers_with_unit,
    progress::{self, Progress},
};
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

pub(super) fn files(dir: &Path, suffix: &str) -> Vec<PathBuf> {
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
    unit: Option<Cow<'a, str>>,
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

/// Sources reviewed against their publisher's terms: file stem, terms, who to credit, where read.
const REVIEWED: &str = include_str!("openaddresses_reviewed.tsv");

fn reviewed(stem: &str) -> Option<String> {
    REVIEWED
        .lines()
        .filter(|l| !l.starts_with('#'))
        .find_map(|l| l.strip_prefix(stem)?.strip_prefix('\t'))
        .map(|rest| format!("reviewed: {}", rest.replace('\t', " | ")))
}

/// The licence of an OpenAddresses export, read from `<stem>.source.json` beside it: the source
/// definition its batch job ran, whose `layers.addresses` entry named after the file's layer
/// carries the licence (older definitions keep it at the top level).
pub fn oa_licence(data: &Path) -> (Licence, String) {
    let name = data.file_name().unwrap().to_string_lossy();
    let stem = name.trim_end_matches(".geojson.gz");
    let (class, shown) = stated_licence(data, stem);
    // Only an unstated licence is overridden; a recognised ban or share-alike stands.
    if class == Licence::Unknown
        && let Some(shown) = reviewed(stem)
    {
        return (Licence::Open, shown);
    }
    (class, shown)
}

fn stated_licence(data: &Path, stem: &str) -> (Licence, String) {
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
        progress::warn(format!(
            "skipped {n} OpenAddresses files ({:.2} GB), licence {class:?}",
            *bytes as f64 / 1e9
        ));
    }
    let bad = AtomicU64::new(0);
    let size = |p: &Path| std::fs::metadata(p).map_or(0, |m| m.len());
    let done = Progress::new(
        format!("openaddresses {}", dir.display()),
        files.iter().map(|f| size(f)).sum(),
    );
    files.par_iter().for_each(|path| {
        let _counted = Counted(&done, size(path));
        let name = path.file_name().unwrap().to_string_lossy();
        let cc = country(name.split('_').next().map(str::to_uppercase).as_deref());
        if cc == [0; 2] {
            progress::warn(format!("skipping {name}: no country prefix"));
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
                                progress::warn(format!("{name}: stopped reading: {e}"));
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
                || Sink::new(shared).ranked(1),
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
                            .map(|name| (name, "city"))
                            .or_else(|| p.district.as_deref().map(|name| (name, "district")));
                        let numbers = numbers_with_unit(cc, p.number.as_deref(), p.unit.as_deref());
                        let postcode = p.postcode.as_deref().map(|pc| oa_postcode(cc, pc));
                        sink.add(
                            Some(cc),
                            x,
                            y,
                            p.street.as_deref(),
                            numbers.as_deref(),
                            postcode.as_deref(),
                            city.as_slice(),
                        );
                    }
                },
            );
        });
    });
    let bad = bad.into_inner();
    if bad > 0 {
        progress::warn(format!("{bad} unreadable OpenAddresses lines or files"));
    }
    files.len()
}

/// Spanish postcodes have five digits; the cadastre's OpenAddresses layers drop a leading zero
/// ("6207" is 06207).
fn oa_postcode(cc: [u8; 2], pc: &str) -> Cow<'_, str> {
    if cc == *b"ES" && pc.len() == 4 && pc.bytes().all(|b| b.is_ascii_digit()) {
        format!("0{pc}").into()
    } else {
        pc.into()
    }
}

/// Adds a file's size to the progress when its pass ends, however it ends.
pub(super) struct Counted<'a>(pub(super) &'a Progress, pub(super) u64);

impl Drop for Counted<'_> {
    fn drop(&mut self) {
        self.0.add(self.1);
    }
}

/// Decodes blobs `range` in parallel, with one `S` per rayon job. One thread reads ahead so
/// workers don't queue on file reads.
pub(super) fn for_blobs<S>(
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
pub(super) fn layout(path: &Path) -> (Vec<ByteOffset>, usize, usize) {
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
    building: bool,
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
                "building" | "building:part" if !v.is_empty() && v != "no" => a.building = true,
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
            building: self.building,
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
            self.city.map(|name| (name, "addr:city")).as_slice(),
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

    fn from<'a>(w: &osmpbf::Way, tags: impl Iterator<Item = (&'a str, &'a str)>) -> Option<Interp> {
        let (mut step, mut street, mut postcode) = (None, None, None);
        for (k, v) in tags {
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
        let step = step?;
        let refs: Vec<i64> = w.refs().collect();
        (refs.len() >= 2).then_some(())?;
        Some(Interp {
            refs,
            step,
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
    building: bool,
}

/// Per-job buffer, appended to the shared one when rayon drops it.
pub(super) struct Collect<'m, T>(pub(super) Vec<T>, pub(super) &'m Mutex<Vec<T>>);

impl<T> Drop for Collect<'_, T> {
    fn drop(&mut self) {
        self.1.lock().unwrap().append(&mut self.0);
    }
}

const ADDR_KEYS: [&str; 8] = [
    "addr:housenumber",
    "addr:street",
    "addr:postcode",
    "addr:city",
    "addr:place",
    "addr:interpolation",
    "building",
    "building:part",
];

/// Which of a block's string-table entries are address keys, found on first use. Elements name
/// their tags by table index, so the many other keys and all their values are never decoded as
/// UTF-8, which was a tenth of the whole pass.
struct Kinds(Vec<u8>);

impl Kinds {
    const UNSEEN: u8 = u8::MAX;

    fn new(table: &[Vec<u8>]) -> Self {
        Kinds(vec![Self::UNSEEN; table.len()])
    }

    /// The address tags among an element's raw (key, value) table indices.
    fn tags<'k, 'a: 'k, K: Into<i64>>(
        &'k mut self,
        table: &'a [Vec<u8>],
        raw: impl Iterator<Item = (K, K)> + 'k,
    ) -> impl Iterator<Item = (&'a str, &'a str)> + 'k {
        raw.filter_map(move |(k, v)| {
            let i = usize::try_from(k.into()).ok()?;
            let slot = self.0.get_mut(i)?;
            if *slot == Self::UNSEEN {
                *slot = ADDR_KEYS
                    .iter()
                    .position(|n| n.as_bytes() == table[i].as_slice())
                    .map_or(0, |p| p as u8 + 1);
            }
            let key = ADDR_KEYS[usize::from(slot.checked_sub(1)?)];
            let value = table.get(usize::try_from(v.into()).ok()?)?;
            Some((key, std::str::from_utf8(value).ok()?))
        })
    }
}

/// Addresses of ways, kept from the first pass: a u32 node count, i64 node refs, then four
/// fields, each a u16 length (MAX for none) and its bytes.
/// Dropping a buffer hands it to the shared list.
struct Packed<'m>(Vec<u8>, &'m Mutex<Vec<Vec<u8>>>);

impl Packed<'_> {
    fn put(&mut self, refs: &[i64], a: &Addr) {
        self.0
            .extend(u32::try_from(refs.len()).unwrap().to_le_bytes());
        for r in refs {
            self.0.extend(r.to_le_bytes());
        }
        for f in [a.number, a.street, a.postcode, a.city] {
            match f.filter(|s| s.len() < usize::from(u16::MAX)) {
                Some(s) => {
                    self.0.extend((s.len() as u16).to_le_bytes());
                    self.0.extend(s.as_bytes());
                }
                None => self.0.extend(u16::MAX.to_le_bytes()),
            }
        }
    }
}

impl Drop for Packed<'_> {
    fn drop(&mut self) {
        if !self.0.is_empty() {
            self.1.lock().unwrap().push(std::mem::take(&mut self.0));
        }
    }
}

/// The records `Packed::put` wrote.
fn unpack_addrs(mut b: &[u8]) -> impl Iterator<Item = (&[u8], Addr<'_>)> {
    std::iter::from_fn(move || {
        let (len, rest) = b.split_first_chunk::<4>()?;
        let (refs, rest) = rest.split_at(u32::from_le_bytes(*len) as usize * 8);
        b = rest;
        let mut field = || {
            let (len, rest) = b.split_first_chunk::<2>()?;
            let len = u16::from_le_bytes(*len);
            b = rest;
            if len == u16::MAX {
                return Some(None);
            }
            let (s, rest) = b.split_at(usize::from(len));
            b = rest;
            Some(std::str::from_utf8(s).ok())
        };
        let (number, street, postcode, city) = (field()?, field()?, field()?, field()?);
        let a = Addr {
            number,
            street,
            postcode,
            city,
            building: false,
        };
        Some((refs, a))
    })
}

pub(super) fn pack(x: f64, y: f64) -> u64 {
    ((x as f32).to_bits() as u64) << 32 | (y as f32).to_bits() as u64
}

pub(super) fn unpack(v: u64) -> (f64, f64) {
    (
        f32::from_bits((v >> 32) as u32) as f64,
        f32::from_bits(v as u32) as f64,
    )
}

fn address_point(mut points: impl Iterator<Item = Option<(f64, f64)>>) -> Option<(f64, f64)> {
    let first = points.next()??;
    if !(-180.0..=180.0).contains(&first.0) || !(-90.0..=90.0).contains(&first.1) {
        return None;
    }
    points
        .all(|p| {
            p.is_some_and(|(x, y)| {
                let dx = (x - first.0 + 180.).rem_euclid(360.) - 180.;
                (-180.0..=180.0).contains(&x)
                    && (-90.0..=90.0).contains(&y)
                    && (dx * ((y + first.1) / 2.).to_radians().cos()).hypot(y - first.1) * 111_195.
                        <= 300.
            })
        })
        .then_some(first)
}

/// Addressed nodes, buildings and compact sites. Untyped ways/relations keep a house number
/// only when all vertices fit the approximate 300 m house-copy merge scale; broader street
/// and postcode metadata survives. The country is the containing place's. Returns files read.
pub fn osm(dir: &Path, shared: &Shared) -> usize {
    let files = files(dir, ".osm.pbf");
    let size = |p: &Path| std::fs::metadata(p).map_or(0, |m| m.len());
    let done = Progress::new(
        format!("osm {}", dir.display()),
        files.iter().map(|f| size(f)).sum(),
    );
    let skipped = AtomicU64::new(0);
    files.par_iter().for_each(|path| {
        let _counted = Counted(&done, size(path));
        let (offsets, first_way, first_relation) = layout(path);
        let (nodes, ways, relations) = (
            1..first_way,
            first_way..first_relation,
            first_relation..offsets.len(),
        );

        // All outer ways are needed to detect a widely spread multipolygon.
        let found = Mutex::new(vec![]);
        for_blobs(
            path,
            &offsets,
            relations,
            || Collect(vec![], &found),
            |mine, block| {
                let table = block.raw_stringtable();
                let mut kinds = Kinds::new(table);
                for r in block.groups().flat_map(|g| g.relations()) {
                    let Some(a) = Addr::from(kinds.tags(table, r.raw_tags())) else {
                        continue;
                    };
                    let ways: Vec<_> = r
                        .members()
                        .filter(|m| {
                            m.member_type == RelMemberType::Way
                                && m.role().is_ok_and(|role| role != "inner")
                        })
                        .map(|m| m.member_id)
                        .collect();
                    if !ways.is_empty() {
                        mine.0.push((ways, a.owned()));
                    }
                }
            },
        );
        let relations = found.into_inner().unwrap();
        let mut member_ways: Vec<i64> = relations
            .iter()
            .flat_map(|(ways, _)| ways.iter().copied())
            .collect();
        member_ways.par_sort_unstable();
        member_ways.dedup();

        let (refs, firsts, interps, packed) = (
            Mutex::new(vec![]),
            Mutex::new(vec![]),
            Mutex::new(vec![]),
            Mutex::new(vec![]),
        );
        for_blobs(
            path,
            &offsets,
            ways.clone(),
            || {
                (
                    Collect(vec![], &refs),
                    Collect(vec![], &firsts),
                    Collect(vec![], &interps),
                    Packed(vec![], &packed),
                )
            },
            |(mine, first, interp, addrs), block| {
                let table = block.raw_stringtable();
                let mut kinds = Kinds::new(table);
                for w in block.groups().flat_map(|g| g.ways()) {
                    if let Some(i) = Interp::from(&w, kinds.tags(table, w.raw_tags())) {
                        mine.0.extend(&i.refs);
                        interp.0.push(i);
                    }
                    let member = member_ways.binary_search(&w.id()).is_ok();
                    let addr = Addr::from(kinds.tags(table, w.raw_tags()));
                    if member || addr.is_some() {
                        let refs: Vec<_> = w.refs().collect();
                        if let Some(a) = addr {
                            let own = if a.building {
                                &refs[..refs.len().min(1)]
                            } else {
                                &refs
                            };
                            mine.0.extend(own);
                            addrs.put(own, &a);
                        }
                        if member {
                            mine.0.extend(&refs);
                            first.0.push((w.id(), refs));
                        }
                    }
                }
            },
        );
        let packed = packed.into_inner().unwrap();
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

        // 2: addressed nodes and footprint coordinates
        let coords: Vec<AtomicU64> = (0..needed.len())
            .map(|_| AtomicU64::new(u64::MAX))
            .collect();
        for_blobs(
            path,
            &offsets,
            nodes,
            || (Sink::new(shared).ranked(2), Collect(vec![], &ended)),
            |(sink, end), block| {
                let table = block.raw_stringtable();
                let mut kinds = Kinds::new(table);
                for g in block.groups() {
                    // Ids ascend (checked in `layout`), so `needed` is walked once per group.
                    let mut p = usize::MAX;
                    for n in g.dense_nodes() {
                        let id = n.id();
                        if p == usize::MAX {
                            p = needed.partition_point(|&r| r < id);
                        }
                        while p < needed.len() && needed[p] < id {
                            p += 1;
                        }
                        if needed.get(p) == Some(&id) {
                            coords[p].store(pack(n.lon(), n.lat()), Relaxed);
                        }
                        if let Some(a) = Addr::from(kinds.tags(table, n.raw_tags())) {
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
                        if let Some(a) = Addr::from(kinds.tags(table, n.raw_tags())) {
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

        // 3: addressed ways, from the first pass
        packed.par_iter().for_each_init(
            || Sink::new(shared).ranked(2),
            |sink, chunk| {
                for (refs, mut a) in unpack_addrs(chunk) {
                    let points = refs.as_chunks::<8>().0.iter().map(|r| {
                        let k = needed.binary_search(&i64::from_le_bytes(*r)).ok()?;
                        let v = coords[k].load(Relaxed);
                        (v != u64::MAX).then(|| unpack(v))
                    });
                    let first = points.clone().next().flatten();
                    if address_point(points).is_none() && a.number.take().is_some() {
                        skipped.fetch_add(1, Relaxed);
                    }
                    if let Some((x, y)) = first {
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
        interps.par_iter().for_each_init(
            || Sink::new(shared).ranked(2),
            |sink, i| {
                let line: Vec<Option<(f64, f64)>> = i.refs.iter().map(|&r| coord(r)).collect();
                let end = |r: i64| {
                    ended
                        .binary_search_by_key(&r, |(id, _)| *id)
                        .ok()
                        .map(|k| &ended[k].1)
                };
                for (x, y, street, number, postcode) in i.expand(&line, end) {
                    sink.add(None, x, y, Some(street), Some(&number), postcode, &[]);
                }
            },
        );
        relations.par_iter().for_each_init(
            || Sink::new(shared).ranked(2),
            |sink, (ways, a)| {
                let Some((x, y)) = ways.first().and_then(|way| {
                    let k = firsts.binary_search_by_key(way, |(w, _)| *w).ok()?;
                    coord(*firsts[k].1.first()?)
                }) else {
                    skipped.fetch_add(1, Relaxed);
                    return;
                };
                let refs = ways
                    .iter()
                    .map(|way| {
                        firsts
                            .binary_search_by_key(way, |(w, _)| *w)
                            .ok()
                            .map(|k| firsts[k].1.as_slice())
                    })
                    .collect::<Option<Vec<_>>>();
                let precise = a.building
                    || refs.is_some_and(|refs| {
                        address_point(refs.into_iter().flatten().map(|&r| coord(r))).is_some()
                    });
                if !precise && a.number.is_some() {
                    skipped.fetch_add(1, Relaxed);
                }
                sink.add(
                    None,
                    x,
                    y,
                    a.street.as_deref(),
                    a.number.as_deref().filter(|_| precise),
                    a.postcode.as_deref(),
                    a.city.as_deref().map(|name| (name, "addr:city")).as_slice(),
                );
            },
        );
    });
    eprintln!(
        "OSM: omitted house numbers on {} untyped ways/relations with incomplete footprints or vertices over 300 m from their address point",
        skipped.load(Relaxed)
    );
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
    fn reviewed_sources_open_only_unstated_licences() {
        let dir = std::env::temp_dir().join(format!("oa-reviewed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let data = dir.join("us_ky_statewide-state.geojson.gz");
        assert_eq!(
            oa_licence(&dir.join("us_xx_town-county.geojson.gz")).0,
            Licence::Unknown
        );
        std::fs::write(dir.join("us_ky_statewide-state.source.json"), "{}").unwrap();
        let (class, shown) = oa_licence(&data);
        assert_eq!(class, Licence::Open);
        assert!(shown.starts_with("reviewed: CC0 1.0 | "), "{shown}");
        std::fs::write(
            dir.join("us_ky_statewide-state.source.json"),
            r#"{"license":"CC BY-NC"}"#,
        )
        .unwrap();
        assert_eq!(oa_licence(&data).0, Licence::Restricted);
        assert!(reviewed("us_xx_town-county").is_none());
        std::fs::remove_dir_all(&dir).unwrap();
        let stems: Vec<_> = REVIEWED
            .lines()
            .filter(|l| !l.starts_with('#'))
            .map(|l| {
                let f: Vec<_> = l.split('\t').collect();
                assert_eq!(f.len(), 4, "{l}");
                assert!(f[3].starts_with("http"), "{l}");
                f[0]
            })
            .collect();
        let unique: std::collections::HashSet<_> = stems.iter().collect();
        assert_eq!(stems.len(), unique.len());
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
            building: false,
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
        assert!(a.building);
        for (key, value, expected) in [
            ("building", "no", false),
            ("building", "retail", true),
            ("building:part", "yes", true),
        ] {
            assert_eq!(
                Addr::from([("addr:housenumber", "1"), (key, value)].into_iter())
                    .unwrap()
                    .building,
                expected
            );
        }
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
    fn raw_address_tags_and_packed_records() {
        let table: Vec<Vec<u8>> = [
            &b""[..],
            b"building",
            b"yes",
            b"addr:housenumber",
            b"12",
            b"addr:street",
            b"Main St",
            b"addr:postcode",
            &[0xff],
            b"addr:place",
            b"Hamlet",
        ]
        .map(<[u8]>::to_vec)
        .to_vec();
        let mut kinds = Kinds::new(&table);
        let tags = [(1u32, 2u32), (3, 4), (5, 6), (7, 8), (99, 1)];
        let a = Addr::from(kinds.tags(&table, tags.into_iter())).unwrap();
        assert!(a.building);
        assert_eq!(
            (a.number, a.street, a.postcode),
            (Some("12"), Some("Main St"), None)
        );
        let place = Addr::from(kinds.tags(&table, [(3u32, 4u32), (9, 10)].into_iter())).unwrap();
        assert_eq!(place.street, Some("Hamlet"));
        assert!(Addr::from(kinds.tags(&table, [(1u32, 2u32)].into_iter())).is_none());

        let list = Mutex::new(vec![]);
        {
            let mut p = Packed(vec![], &list);
            p.put(&[7, 8, 9, 7], &a);
            p.put(&[-3], &place);
        }
        let list = list.into_inner().unwrap();
        let got: Vec<_> = unpack_addrs(&list[0])
            .map(|(refs, a)| {
                let refs: Vec<_> = refs
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .map(|r| i64::from_le_bytes(*r))
                    .collect();
                (refs, a.number, a.street, a.postcode, a.city)
            })
            .collect();
        assert_eq!(
            got,
            [
                (vec![7, 8, 9, 7], Some("12"), Some("Main St"), None, None),
                (vec![-3], Some("12"), Some("Hamlet"), None, None)
            ]
        );
    }

    #[test]
    fn only_compact_complete_footprints_become_address_points() {
        let first = Some((140., 35.));
        assert_eq!(
            address_point([first, Some((140.001, 35.001)), first].into_iter()),
            first
        );
        for points in [
            vec![],
            vec![None],
            vec![first, None],
            vec![Some((f64::NAN, 35.))],
            vec![first, Some((140., 91.))],
            vec![first, Some((140.01, 35.))],
            vec![first, Some((140., 35.01))],
            vec![first, first, Some((140.02, 35.)), first],
        ] {
            assert_eq!(address_point(points.into_iter()), None);
        }
        for (a, b) in [((179.9999, 0.), (-179.9999, 0.)), ((0., 85.), (0.01, 85.))] {
            assert_eq!(address_point([Some(a), Some(b)].into_iter()), Some(a));
        }
    }

    #[test]
    fn openaddresses_feature() {
        let line = r#"{"type": "Feature", "properties": {"hash": "f", "number": "113", "street": "\u0634\u0627\u0631\u0639", "unit": "1", "city": "", "district": "D", "region": "", "postcode": "", "id": "5"}, "geometry": {"type": "Point", "coordinates": [51.63, 24.11]}}"#;
        let f: Feature = serde_json::from_str(line).unwrap();
        assert_eq!(f.properties.street.as_deref(), Some("شارع"));
        assert_eq!(f.properties.district.as_deref(), Some("D"));
        assert_eq!(f.properties.unit.as_deref(), Some("1"));
        assert_eq!(
            numbers_with_unit(
                *b"NZ",
                f.properties.number.as_deref(),
                f.properties.unit.as_deref()
            )
            .as_deref(),
            Some("113;1/113")
        );
        assert_eq!(f.geometry.unwrap().coordinates[1].as_f64(), Some(24.11));
    }

    #[test]
    fn spanish_cadastre_postcodes_keep_their_leading_zero() {
        assert_eq!(oa_postcode(*b"ES", "6207"), "06207");
        assert_eq!(oa_postcode(*b"ES", "16330"), "16330");
        assert_eq!(oa_postcode(*b"DK", "2100"), "2100");
    }
}
