//! Landmarks for `SuggestAddress`: OSM airports, stations, attractions, museums, universities,
//! hospitals and stadiums that carry a `wikidata` tag, which keeps the set small and notable. They
//! go to `landmarks.tsv`, apart from the Tantivy index, so a suggestion without them reads
//! nothing it did not read before.
use super::{
    NAME_SEP, cell,
    geo::Assigner,
    progress::Progress,
    sources::{Collect, Counted, files, for_blobs, layout, pack, unpack},
};
use osmpbf::RelMemberType;
use rayon::prelude::*;
use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering::Relaxed},
    },
};

/// Best first: the order a feature's tags are tried in, and the ranking tier the service reads.
pub const CATEGORIES: [&str; 7] = [
    "airport",
    "station",
    "attraction",
    "museum",
    "university",
    "hospital",
    "stadium",
];

/// A feature's category, Wikidata item and names, from its tags.
#[derive(Debug, PartialEq)]
struct Feature {
    category: u8,
    qid: u64,
    /// `name`, or `name:en` when it has none.
    primary: String,
    /// (`en` for `name:en`, or `alt_name`, `official_name`, ..., `iata`, name).
    names: Vec<(String, String)>,
}

const NAME_KEYS: [&str; 4] = ["alt_name", "official_name", "short_name", "int_name"];

/// `name:fr`, `name:zh-Hans`: a language's name, not `name:etymology` or `name:left`.
fn language(key: &str) -> Option<&str> {
    let lang = key.strip_prefix("name:")?;
    let base = lang.split('-').next()?;
    ((2..=3).contains(&base.len()) && base.bytes().all(|b| b.is_ascii_lowercase())).then_some(lang)
}

fn feature(tags: &[(&str, &str)]) -> Option<Feature> {
    let get = |k: &str| tags.iter().find(|t| t.0 == k).map(|t| t.1.trim());
    let qid: u64 = get("wikidata")?.strip_prefix('Q')?.parse().ok()?;
    let iata = get("iata").and_then(|v| {
        v.split(';')
            .map(str::trim)
            .find(|c| c.len() == 3 && c.bytes().all(|b| b.is_ascii_uppercase()))
    });
    let is = |k: &str, values: &[&str]| get(k).is_some_and(|v| values.contains(&v));
    let category = if is("aeroway", &["aerodrome"]) && iata.is_some() {
        "airport"
    } else if is("railway", &["station"])
        || is("public_transport", &["station"]) && is("train", &["yes"])
    {
        "station"
    } else if is("tourism", &["museum"]) {
        "museum"
    } else if is("tourism", &["attraction", "theme_park", "zoo"]) {
        "attraction"
    } else if is("amenity", &["university"]) {
        "university"
    } else if is("amenity", &["hospital"]) {
        "hospital"
    } else if is("leisure", &["stadium"]) {
        "stadium"
    } else {
        return None;
    };
    let mut names: Vec<(String, String)> = vec![];
    let mut add = |key: &str, value: &str| {
        let value = value.trim();
        if !value.is_empty() && !names.iter().any(|(k, v)| k == key && v == value) {
            names.push((key.to_string(), value.to_string()));
        }
    };
    for &(k, v) in tags {
        if let Some(lang) = language(k) {
            add(lang, v);
        } else if NAME_KEYS.contains(&k) {
            v.split(';').for_each(|v| add(k, v));
        }
    }
    let primary = get("name")
        .filter(|n| !n.is_empty())
        .or(get("name:en").filter(|n| !n.is_empty()))?
        .to_string();
    if let Some(code) = iata.filter(|_| category == "airport") {
        add("iata", code);
    }
    Some(Feature {
        category: CATEGORIES.iter().position(|c| *c == category)? as u8,
        qid,
        primary,
        names,
    })
}

const UNSEEN: u8 = u8::MAX;
const OTHER: u8 = 0;
const WIKIDATA: u8 = 1;
const WANTED: u8 = 2;

/// Which of a block's string-table entries are keys a landmark is read from, found on first use,
/// so the many other tags are never decoded (as `sources::Kinds` does for addresses).
struct Keys(Vec<u8>);

impl Keys {
    fn new(table: &[Vec<u8>]) -> Self {
        Keys(vec![UNSEEN; table.len()])
    }

    fn class(&mut self, table: &[Vec<u8>], i: usize) -> u8 {
        let Some(slot) = self.0.get_mut(i) else {
            return OTHER;
        };
        if *slot == UNSEEN {
            *slot = match std::str::from_utf8(&table[i]) {
                Ok("wikidata") => WIKIDATA,
                Ok(
                    "name" | "aeroway" | "iata" | "railway" | "public_transport" | "train"
                    | "tourism" | "amenity" | "leisure",
                ) => WANTED,
                Ok(k) if NAME_KEYS.contains(&k) || language(k).is_some() => WANTED,
                _ => OTHER,
            };
        }
        *slot
    }

    /// The feature an element's raw (key, value) table indices describe; `raw` is called twice,
    /// the second time only for the few elements with a `wikidata` tag.
    fn feature<K: Into<i64>, I: Iterator<Item = (K, K)>>(
        &mut self,
        table: &[Vec<u8>],
        raw: impl Fn() -> I,
    ) -> Option<Feature> {
        let index = |k: K| usize::try_from(k.into()).ok();
        if !raw().any(|(k, _)| index(k).is_some_and(|k| self.class(table, k) == WIKIDATA)) {
            return None;
        }
        let tags: Vec<(&str, &str)> = raw()
            .filter_map(|(k, v)| {
                let k = index(k)?;
                if self.class(table, k) == OTHER {
                    return None;
                }
                let v = table.get(index(v)?)?;
                Some((
                    std::str::from_utf8(&table[k]).ok()?,
                    std::str::from_utf8(v).ok()?,
                ))
            })
            .collect();
        feature(&tags)
    }
}

/// A landmark at its point: a node's, or the mean of its way's or relation's nodes.
pub struct Landmark {
    category: u8,
    qid: u64,
    primary: String,
    names: Vec<(String, String)>,
    x: f64,
    y: f64,
    /// Node 0, way 1, relation 2, and its id: the tie-break between copies of one item.
    element: (u8, i64),
}

// ponytail: planar centroids of closed outer rings, vertex mean for open ways or fragmented
// relations. Holes and relation-of-relation members are not assembled; a geometry library with
// ring stitching is the upgrade. Antimeridian-crossing geometries are omitted.
fn centroid<'r>(
    rings: impl Iterator<Item = &'r [i64]>,
    at: impl Fn(i64) -> Option<(f64, f64)>,
) -> Option<(f64, f64)> {
    let (mut sx, mut sy, mut n) = (0.0, 0.0, 0u64);
    let (mut ax, mut ay, mut area) = (0.0, 0.0, 0.0);
    let (mut west, mut east) = (180.0_f64, -180.0_f64);
    for refs in rings {
        let points: Vec<_> = refs.iter().map(|&r| at(r)).collect::<Option<_>>()?;
        let closed = refs.len() >= 4 && refs.first() == refs.last();
        let open = &points[..points.len().saturating_sub(usize::from(closed))];
        for &(x, y) in open {
            (sx, sy, n) = (sx + x, sy + y, n + 1);
            west = west.min(x);
            east = east.max(x);
        }
        if closed {
            let (ox, oy) = points[0];
            let (mut cx, mut cy, mut a) = (0.0, 0.0, 0.0);
            for edge in points.windows(2) {
                let ((x0, y0), (x1, y1)) = (edge[0], edge[1]);
                let cross = (x0 - ox) * (y1 - oy) - (x1 - ox) * (y0 - oy);
                a += cross;
                cx += (x0 + x1 - 2.0 * ox) * cross;
                cy += (y0 + y1 - 2.0 * oy) * cross;
            }
            if a.abs() > f64::EPSILON {
                ax += (ox + cx / (3.0 * a)) * a.abs();
                ay += (oy + cy / (3.0 * a)) * a.abs();
                area += a.abs();
            }
        }
    }
    if n == 0 || east - west > 180.0 {
        None
    } else if area > 0.0 {
        Some((ax / area, ay / area))
    } else {
        Some((sx / n as f64, sy / n as f64))
    }
}

struct Rel {
    feature: Feature,
    id: i64,
    ways: Vec<i64>,
    nodes: Vec<i64>,
}

/// One extract's landmarks, in three passes like `sources::osm`: relations name their member ways,
/// ways their nodes, and nodes give the points.
fn read_file(path: &Path) -> Vec<Landmark> {
    let (offsets, first_way, first_relation) = layout(path);
    let rels = Mutex::new(vec![]);
    for_blobs(
        path,
        &offsets,
        first_relation..offsets.len(),
        || Collect(vec![], &rels),
        |mine, block| {
            let table = block.raw_stringtable();
            let mut keys = Keys::new(table);
            for r in block.groups().flat_map(|g| g.relations()) {
                let Some(feature) = keys.feature(table, || r.raw_tags()) else {
                    continue;
                };
                let members = |t: RelMemberType| {
                    r.members()
                        .filter(move |m| m.member_type == t)
                        .map(|m| (m.member_id, m.role().unwrap_or("")))
                };
                let outer: Vec<i64> = members(RelMemberType::Way)
                    .filter(|m| m.1 == "outer")
                    .map(|m| m.0)
                    .collect();
                let ways = if outer.is_empty() {
                    members(RelMemberType::Way).map(|m| m.0).collect()
                } else {
                    outer
                };
                let nodes = members(RelMemberType::Node).map(|m| m.0).collect();
                mine.0.push(Rel {
                    feature,
                    id: r.id(),
                    ways,
                    nodes,
                });
            }
        },
    );
    let rels = rels.into_inner().unwrap();
    let mut member_ways: Vec<i64> = rels.iter().flat_map(|r| r.ways.iter().copied()).collect();
    member_ways.par_sort_unstable();
    member_ways.dedup();

    let (ways, members) = (Mutex::new(vec![]), Mutex::new(vec![]));
    for_blobs(
        path,
        &offsets,
        first_way..first_relation,
        || (Collect(vec![], &ways), Collect(vec![], &members)),
        |(ways, members), block| {
            let table = block.raw_stringtable();
            let mut keys = Keys::new(table);
            for w in block.groups().flat_map(|g| g.ways()) {
                if member_ways.binary_search(&w.id()).is_ok() {
                    members.0.push((w.id(), w.refs().collect::<Vec<i64>>()));
                }
                if let Some(feature) = keys.feature(table, || w.raw_tags()) {
                    ways.0
                        .push((feature, w.id(), w.refs().collect::<Vec<i64>>()));
                }
            }
        },
    );
    let ways = ways.into_inner().unwrap();
    let mut members = members.into_inner().unwrap();
    members.par_sort_unstable_by_key(|m| m.0);
    let mut needed: Vec<i64> = ways
        .iter()
        .flat_map(|w| w.2.iter().copied())
        .chain(members.iter().flat_map(|m| m.1.iter().copied()))
        .chain(rels.iter().flat_map(|r| r.nodes.iter().copied()))
        .collect();
    needed.par_sort_unstable();
    needed.dedup();

    let coords: Vec<AtomicU64> = (0..needed.len())
        .map(|_| AtomicU64::new(u64::MAX))
        .collect();
    let nodes = Mutex::new(vec![]);
    for_blobs(
        path,
        &offsets,
        1..first_way,
        || Collect(vec![], &nodes),
        |mine, block| {
            let table = block.raw_stringtable();
            let mut keys = Keys::new(table);
            for g in block.groups() {
                // Ids ascend (checked in `layout`), so `needed` is walked once per group, not
                // searched for each of a file's hundreds of millions of nodes.
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
                    if let Some(feature) = keys.feature(table, || n.raw_tags()) {
                        mine.0.push((feature, n.id(), n.lon(), n.lat()));
                    }
                }
                for n in g.nodes() {
                    if let Ok(k) = needed.binary_search(&n.id()) {
                        coords[k].store(pack(n.lon(), n.lat()), Relaxed);
                    }
                    if let Some(feature) = keys.feature(table, || n.raw_tags()) {
                        mine.0.push((feature, n.id(), n.lon(), n.lat()));
                    }
                }
            }
        },
    );
    let at = |node: i64| {
        let v = coords[needed.binary_search(&node).ok()?].load(Relaxed);
        (v != u64::MAX).then(|| unpack(v))
    };
    let landmark = |feature: Feature, element, (x, y)| Landmark {
        category: feature.category,
        qid: feature.qid,
        primary: feature.primary,
        names: feature.names,
        x,
        y,
        element,
    };
    let mut out: Vec<Landmark> = nodes
        .into_inner()
        .unwrap()
        .into_iter()
        .map(|(f, id, x, y)| landmark(f, (0, id), (x, y)))
        .collect();
    for (f, id, refs) in ways {
        if let Some(xy) = centroid(std::iter::once(refs.as_slice()), at) {
            out.push(landmark(f, (1, id), xy));
        }
    }
    for r in rels {
        let refs = |w: &i64| {
            let k = members.binary_search_by_key(w, |m| m.0).ok()?;
            Some(members[k].1.as_slice())
        };
        let xy = if r.ways.is_empty() {
            centroid(r.nodes.iter().map(std::slice::from_ref), at)
        } else {
            r.ways
                .iter()
                .map(refs)
                .collect::<Option<Vec<_>>>()
                .and_then(|rings| centroid(rings.into_iter(), at))
        };
        if let Some(xy) = xy {
            out.push(landmark(r.feature, (2, r.id), xy));
        }
    }
    out
}

/// The landmarks of every extract in `dirs`, one per Wikidata item: the copy with the most names,
/// then a node before a way before a relation, then the lowest id, so overlapping extracts and
/// thread order don't change the answer.
// ponytail: decoding blobs is nine tenths of this pass, and it reads and decodes the `--osm`
// extracts a second time, beside the address passes. Reading landmarks in `sources::osm`'s passes
// is the upgrade.
pub fn read(dirs: &[PathBuf]) -> Vec<Landmark> {
    let files: Vec<PathBuf> = dirs.iter().flat_map(|d| files(d, ".osm.pbf")).collect();
    let size = |p: &Path| std::fs::metadata(p).map_or(0, |m| m.len());
    let done = Progress::new("landmarks".into(), files.iter().map(|f| size(f)).sum());
    let all: Vec<Landmark> = files
        .par_iter()
        .flat_map_iter(|path| {
            let _counted = Counted(&done, size(path));
            read_file(path)
        })
        .collect();
    let mut best: HashMap<u64, Landmark> = HashMap::new();
    for l in all {
        let key = |l: &Landmark| {
            (
                std::cmp::Reverse(l.names.len()),
                l.element,
                l.x.to_bits(),
                l.y.to_bits(),
            )
        };
        match best.get(&l.qid) {
            Some(b) if key(b) <= key(&l) => {}
            _ => {
                best.insert(l.qid, l);
            }
        }
    }
    let mut out: Vec<Landmark> = best.into_values().collect();
    out.sort_unstable_by_key(|l| l.qid);
    out
}

/// Writes `landmarks.tsv`, one row per landmark in a place, by Wikidata item: category, place,
/// x, y, Wikidata item, name, and `language=name` (or `alt_name=`, `iata=`...) pairs joined by
/// NAME_SEP. Returns the count per category and of those in no place.
pub fn write(out: &Path, landmarks: &[Landmark], assigner: &Assigner) -> ([u64; 7], u64) {
    let places: Vec<Option<u32>> = landmarks
        .par_iter()
        .map(|l| assigner.place(l.x, l.y))
        .collect();
    let mut w = std::io::BufWriter::new(std::fs::File::create(out.join("landmarks.tsv")).unwrap());
    let (mut counts, mut placeless) = ([0u64; 7], 0);
    for (l, place) in landmarks.iter().zip(places) {
        let Some(place) = place else {
            placeless += 1;
            continue;
        };
        counts[l.category as usize] += 1;
        let pairs: Vec<String> = l
            .names
            .iter()
            .map(|(k, v)| format!("{k}={}", cell(v)))
            .collect();
        writeln!(
            w,
            "{}\t{place}\t{}\t{}\tQ{}\t{}\t{}",
            CATEGORIES[l.category as usize],
            l.x,
            l.y,
            l.qid,
            cell(&l.primary),
            pairs.join(&NAME_SEP.to_string())
        )
        .unwrap();
    }
    w.flush().unwrap();
    (counts, placeless)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_make_a_landmark_only_with_a_wikidata_item_and_a_category() {
        let cdg = feature(&[
            ("aeroway", "aerodrome"),
            ("iata", "CDG"),
            ("wikidata", "Q46168"),
            ("name", "Aéroport de Paris-Charles-de-Gaulle"),
            ("name:en", "Paris Charles de Gaulle Airport"),
            ("name:etymology", "x"),
            ("alt_name", "Roissy;CDG Airport"),
        ])
        .unwrap();
        assert_eq!(
            (CATEGORIES[cdg.category as usize], cdg.qid),
            ("airport", 46168)
        );
        assert_eq!(cdg.primary, "Aéroport de Paris-Charles-de-Gaulle");
        let keys: Vec<&str> = cdg.names.iter().map(|n| n.0.as_str()).collect();
        assert_eq!(keys, ["en", "alt_name", "alt_name", "iata"]);
        // an airfield without an IATA code is not an airport, and nothing else either
        assert!(feature(&[("aeroway", "aerodrome"), ("wikidata", "Q1"), ("name", "A")]).is_none());
        // no item, no name, no category
        assert!(feature(&[("railway", "station"), ("name", "Nord")]).is_none());
        assert!(feature(&[("railway", "station"), ("wikidata", "Q2")]).is_none());
        assert!(feature(&[("shop", "bakery"), ("wikidata", "Q3"), ("name", "B")]).is_none());
        let station = |extra: (&'static str, &'static str)| {
            feature(&[
                ("public_transport", "station"),
                ("wikidata", "Q4"),
                ("name", "S"),
                extra,
            ])
            .map(|f| CATEGORIES[f.category as usize])
        };
        assert_eq!(station(("train", "yes")), Some("station"));
        assert_eq!(station(("bus", "yes")), None);
        // a station that is also an attraction ranks as a station
        let both = feature(&[
            ("tourism", "museum"),
            ("railway", "station"),
            ("wikidata", "Q5"),
            ("name:en", "Old Station"),
        ])
        .unwrap();
        assert_eq!((both.category, both.primary.as_str()), (1, "Old Station"));
    }

    #[test]
    fn a_ring_counts_its_first_node_once() {
        let at = |r: i64| Some((r as f64, 0.0));
        assert_eq!(
            centroid(std::iter::once(&[1, 2, 6, 1][..]), at),
            Some((3.0, 0.0))
        );
        assert_eq!(centroid(std::iter::once(&[4][..]), at), Some((4.0, 0.0)));
        assert_eq!(centroid(std::iter::once(&[1, 9][..]), |_| None), None);
        let polygon = [(0., 0.), (4., 0.), (4., 1.), (2., 1.), (0., 1.)];
        let at = |r: i64| Some(polygon[r as usize]);
        assert_eq!(
            centroid(std::iter::once(&[0, 1, 2, 3, 4, 0][..]), at),
            Some((2., 0.5))
        );
        assert_eq!(
            centroid(std::iter::once(&[0, 4, 3, 2, 1, 0][..]), at),
            Some((2., 0.5))
        );
    }
}
