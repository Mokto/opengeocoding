use super::{Assigner, Place, col, read, tasks, text, wkb_point};
use arrow_array::{Array, cast::AsArray};
use opengeocoding::index::country::us::{
    LISTED_FOOTPRINT_SUFFIX, LISTED_HOUSES, listed_civic, listed_house_key,
};
use osmpbf::{PrimitiveBlock, Way};
use rayon::prelude::*;
use std::{collections::HashMap, fs::File, io::BufWriter, path::Path, sync::Mutex};

pub(super) fn addressed_ways(
    block: &PrimitiveBlock,
) -> impl Iterator<Item = (String, Vec<i64>)> + '_ {
    let table = block.raw_stringtable();
    let keys = [
        "addr:housenumber",
        "addr:street",
        "addr:city",
        "addr:state",
        "addr:postcode",
        "building",
    ]
    .map(|key| {
        table
            .iter()
            .position(|s| s.as_slice() == key.as_bytes())
            .map(|i| i as u32)
    });
    let exclude = ["building:part", "addr:interpolation", "area"].map(|key| {
        table
            .iter()
            .position(|s| s.as_slice() == key.as_bytes())
            .map(|i| i as u32)
    });
    let read = move |way: Way<'_>| {
        let mut vals = [None; 6];
        for (k, v) in way.raw_tags() {
            if exclude[0] == Some(k)
                || exclude[1] == Some(k)
                || exclude[2] == Some(k) && table.get(v as usize)?.as_slice() == b"no"
            {
                return None;
            }
            if let Some(i) = keys.iter().position(|&key| key == Some(k)) {
                vals[i] = Some(std::str::from_utf8(table.get(v as usize)?).ok()?.trim());
            }
        }
        let [
            Some(number),
            Some(road),
            Some(town),
            Some(state),
            Some(zip),
            Some(building),
        ] = vals
        else {
            return None;
        };
        if matches!(
            building,
            "" | "no" | "construction" | "proposed" | "ruins" | "demolished" | "collapsed"
        ) {
            return None;
        }
        let (key, _) = listed_house_key(number, road, town, state, zip)?;
        // ponytail: only small single rings; larger buildings and multipolygons need ring assembly.
        let refs: Vec<_> = way.refs().take(65).collect();
        let closed = (4..=64).contains(&refs.len()) && refs.first() == refs.last();
        Some((key, if closed { refs } else { vec![] }))
    };
    std::iter::once(block)
        .filter(move |_| keys.iter().all(Option::is_some))
        .flat_map(|block| block.groups().flat_map(|g| g.ways()))
        .filter_map(read)
}

type Ring = Vec<(f64, f64)>;
pub(super) type Footprints = Vec<(String, Option<Ring>)>;

fn valid_ring(ring: &[(f64, f64)]) -> bool {
    if !(4..=64).contains(&ring.len()) || ring.first() != ring.last() {
        return false;
    }
    let (x0, y0) = ring[0];
    if !ring.iter().all(|&(x, y)| {
        (-180.0..=180.0).contains(&x)
            && (-90.0..=90.0).contains(&y)
            && (x - x0).abs() <= 0.006
            && (y - y0).abs() <= 0.003
    }) {
        return false;
    }
    let fixed: Vec<_> = ring
        .iter()
        .map(|&(x, y)| {
            (
                ((x - x0) * 1e7).round() as i64,
                ((y - y0) * 1e7).round() as i64,
            )
        })
        .collect();
    let cross = |(ax, ay), (bx, by), (cx, cy)| (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
    let edges: Vec<_> = fixed.windows(2).map(|w| (w[0], w[1])).collect();
    if edges.iter().map(|&(a, b)| cross((0, 0), a, b)).sum::<i64>() == 0 {
        return false;
    }
    let overlaps = |a: i64, b: i64, c: i64, d: i64| a.min(b) <= c.max(d) && c.min(d) <= a.max(b);
    for (i, &(a, b)) in edges.iter().enumerate() {
        if a == b {
            return false;
        }
        for &(c, d) in edges
            .iter()
            .take(if i == 0 { edges.len() - 1 } else { edges.len() })
            .skip(i + 2)
        {
            if overlaps(a.0, b.0, c.0, d.0)
                && overlaps(a.1, b.1, c.1, d.1)
                && cross(a, b, c).signum() * cross(a, b, d).signum() <= 0
                && cross(c, d, a).signum() * cross(c, d, b).signum() <= 0
            {
                return false;
            }
        }
    }
    true
}

fn inside_building(ring: &[(f64, f64)], points: &[Point<'_>]) -> Option<u64> {
    let contains = |(x, y)| {
        ring.windows(2)
            .filter(|w| {
                let [(ax, ay), (bx, by)] = [w[0], w[1]];
                (ay > y) != (by > y) && x < ax + (y - ay) * (bx - ax) / (by - ay)
            })
            .count()
            % 2
            == 1
    };
    let inside: Vec<_> = points.iter().filter(|p| contains((p.x, p.y))).collect();
    inside
        .iter()
        .filter_map(|p| {
            let cos = p.y.to_radians().cos();
            let metres = |q: &Point<'_>| 111_195.0 * ((p.x - q.x) * cos).hypot(p.y - q.y);
            let near = inside
                .iter()
                .filter(|q| q.provider != p.provider)
                .map(|q| metres(q))
                .min_by(f64::total_cmp)?;
            (near <= 20.0
                && contains((f64::from(p.x as f32), f64::from(p.y as f32)))
                && points.iter().all(|q| metres(q) <= 100.0))
            .then_some((near, *p))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, p)| packed_point(p))
}

#[derive(Clone, Copy)]
struct Point<'a> {
    x: f64,
    y: f64,
    provider: u8,
    extension: &'a str,
}

fn zip5_building(points: &[Point<'_>]) -> Option<u64> {
    if points
        .iter()
        .fold(0u8, |seen, p| seen | (1 << p.provider))
        .count_ones()
        < 3
    {
        return None;
    }
    points
        .iter()
        .filter_map(|p| {
            let cos = p.y.to_radians().cos();
            let mut spread = 0.0f64;
            let mut nearby_provider = false;
            for q in points {
                let metres = 111_195.0 * ((p.x - q.x) * cos).hypot(p.y - q.y);
                if metres > 45.0 {
                    return None;
                }
                spread = spread.max(metres);
                nearby_provider |= p.provider != q.provider && metres <= 20.0;
            }
            nearby_provider.then_some((spread, p))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, p)| packed_point(p))
}

fn packed_point(p: &Point<'_>) -> u64 {
    (u64::from((p.x as f32).to_bits()) << 32) | u64::from((p.y as f32).to_bits())
}

fn confirmed<'a>(points: &[Point<'a>]) -> std::collections::BTreeMap<&'a str, u64> {
    let mut out = std::collections::BTreeMap::new();
    for p in points.iter().filter(|p| !p.extension.is_empty()) {
        let cos = p.y.to_radians().cos();
        let distance = |q: &Point<'_>| 111_195.0 * ((p.x - q.x) * cos).hypot(p.y - q.y);
        // A second provider confirms the building, not the delivery suffix. Return a point
        // that actually carried the suffix; a distant listing makes that point ambiguous.
        if points
            .iter()
            .any(|q| q.provider != p.provider && distance(q) <= 20.0)
            && points.iter().all(|q| {
                distance(q)
                    <= if q.extension == p.extension {
                        20.0
                    } else {
                        45.0
                    }
            })
        {
            out.entry(p.extension).or_insert_with(|| packed_point(p));
        }
    }
    out
}

const PROVIDERS: &[(&str, &str)] = &[
    ("meta", "CDLA-Permissive-2.0"),
    ("BrightQuery", "CDLA-Permissive-2.0"),
    ("Microsoft", "CDLA-Permissive-2.0"),
    ("Foursquare", "Apache-2.0"),
];

const MAX_ROWS: usize = 64;

type Listings = HashMap<String, Vec<(f64, f64, u8, String)>>;

pub(super) fn write(
    dir: &Path,
    out: &Path,
    assigner: &Assigner,
    places: &[Place],
    footprints: Footprints,
) -> anyhow::Result<(usize, usize)> {
    if !places.iter().any(|p| p.country == *b"US") {
        return Ok((0, 0));
    }
    let tasks = tasks(dir);
    let done = super::progress::Progress::items(
        "US corroborated place addresses".into(),
        tasks.len() as u64,
        "row groups",
    );
    let all = Mutex::new(Listings::new());
    tasks.par_iter().for_each(|t| {
        let mut local = Listings::new();
        for b in read(t, &["addresses", "geometry", "sources", "operating_status"]) {
            let (Some(sources), Some(status)) = (
                b.column_by_name("sources"),
                b.column_by_name("operating_status"),
            ) else {
                continue;
            };
            let status = status.as_string::<i32>();
            let sources = sources.as_list::<i32>();
            let so = sources.value_offsets();
            let provenance = sources.values().as_struct();
            let field = |name| {
                provenance
                    .column_by_name(name)
                    .map(|c| c.as_string::<i32>())
            };
            let (Some(property), Some(dataset), Some(license)) =
                (field("property"), field("dataset"), field("license"))
            else {
                continue;
            };
            let addresses = col(&b, "addresses").as_list::<i32>();
            let ao = addresses.value_offsets();
            let components = addresses.values().as_struct();
            let field = |name| {
                components
                    .column_by_name(name)
                    .map(|c| c.as_string::<i32>())
            };
            let (Some(country), Some(region), Some(freeform), Some(postcode), Some(locality)) = (
                field("country"),
                field("region"),
                field("freeform"),
                field("postcode"),
                field("locality"),
            ) else {
                continue;
            };
            let geom = col(&b, "geometry").as_binary::<i32>();
            for i in 0..b.num_rows() {
                if addresses.is_null(i) || geom.is_null(i) || text(status, i) != Some("open") {
                    continue;
                }
                let indices = ao[i] as usize..ao[i + 1] as usize;
                if !indices.clone().any(|j| text(country, j) == Some("US")) {
                    continue;
                }
                let mut root = None;
                let mut mixed = false;
                for k in so[i] as usize..so[i + 1] as usize {
                    let prop = text(property, k).unwrap_or("");
                    if prop.is_empty() {
                        if root.replace(k).is_some() {
                            mixed = true;
                            break;
                        }
                    } else if prop.starts_with("/geometry")
                        || prop.starts_with("/properties/addresses")
                    {
                        mixed = true;
                        break;
                    }
                }
                let Some(root) = root.filter(|_| !mixed) else {
                    continue;
                };
                let Some(provider) = PROVIDERS.iter().position(|&(name, terms)| {
                    text(dataset, root) == Some(name) && text(license, root) == Some(terms)
                }) else {
                    continue;
                };
                let Some((x, y)) = wkb_point(geom.value(i)).filter(|&(x, y)| {
                    (-180.0..=180.0).contains(&x)
                        && (-90.0..=90.0).contains(&y)
                        && assigner.within(x, y)
                }) else {
                    continue;
                };
                let Some(state) = assigner.region(x, y, *b"US") else {
                    continue;
                };
                for j in indices {
                    let (Some(raw), Some(region), Some(zip), Some(city)) = (
                        text(freeform, j),
                        text(region, j),
                        text(postcode, j),
                        text(locality, j),
                    ) else {
                        continue;
                    };
                    if text(country, j) != Some("US")
                        || region.strip_prefix("US-").unwrap_or(region)
                            != places[state as usize].region.get(3..).unwrap_or("")
                    {
                        continue;
                    }
                    let Some((number, road)) = listed_civic(raw) else {
                        continue;
                    };
                    let Some((key, extension)) = listed_house_key(number, road, city, region, zip)
                    else {
                        continue;
                    };
                    let rows = local.entry(key).or_default();
                    if rows.len() <= MAX_ROWS {
                        rows.push((x, y, provider as u8, extension.into()));
                    }
                }
            }
        }
        let mut all = all.lock().unwrap();
        for (key, mut rows) in local {
            let group = all.entry(key).or_default();
            if group.len() <= MAX_ROWS {
                rows.truncate(MAX_ROWS + 1 - group.len());
                group.append(&mut rows);
            }
        }
        done.add(1);
    });
    let mut keys: Vec<_> = all
        .into_inner()
        .unwrap()
        .into_iter()
        .filter(|(_, rows)| (2..=MAX_ROWS).contains(&rows.len()))
        .collect();
    keys.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    let mut fst =
        tantivy_fst::MapBuilder::new(BufWriter::new(File::create(out.join(LISTED_HOUSES))?))?;
    let mut rings = HashMap::new();
    for (key, ring) in footprints {
        rings
            .entry(key)
            .and_modify(|ring| *ring = None)
            .or_insert(ring);
    }
    let (mut n, mut buildings) = (0, 0);
    for (key, rows) in keys {
        let mut points: Vec<_> = rows
            .iter()
            .map(|(x, y, provider, extension)| Point {
                x: *x,
                y: *y,
                provider: *provider,
                extension,
            })
            .collect();
        points.sort_unstable_by(|a, b| {
            a.x.total_cmp(&b.x)
                .then(a.y.total_cmp(&b.y))
                .then(a.provider.cmp(&b.provider))
                .then(a.extension.cmp(b.extension))
        });
        let choices = confirmed(&points);
        let zip5 = zip5_building(&points);
        if let Some(point) = zip5 {
            fst.insert(&key, point)?;
        }
        n += usize::from(zip5.is_some() || !choices.is_empty());
        for (extension, point) in choices {
            fst.insert(format!("{key}\t{extension}"), point)?;
        }
        if let Some(Some(ring)) = rings.remove(&key)
            && valid_ring(&ring)
            && let Some(point) = inside_building(&ring, &points)
        {
            fst.insert(format!("{key}{LISTED_FOOTPRINT_SUFFIX}"), point)?;
            buildings += 1;
        }
    }
    fst.finish()?;
    Ok((n, buildings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip5_needs_three_providers_and_keeps_an_observed_point() {
        let a = Point {
            x: -90.0,
            y: 40.0,
            provider: 0,
            extension: "",
        };
        let b = Point {
            x: -90.0001,
            provider: 1,
            ..a
        };
        let c = Point {
            x: -90.0002,
            provider: 2,
            ..a
        };
        assert!(zip5_building(&[a, b, b]).is_none());
        assert_eq!(zip5_building(&[a, b, c]), Some(packed_point(&b)));
        assert!(zip5_building(&[a, b, c, Point { x: -90.001, ..a }]).is_none());
    }

    #[test]
    fn footprints_require_a_valid_ring_and_two_independent_points_inside() {
        let ring = [
            (-90.0002, 40.),
            (-89.9998, 40.),
            (-89.9998, 40.0002),
            (-90.0002, 40.0002),
            (-90.0002, 40.),
        ];
        assert!(valid_ring(&ring));
        assert!(!valid_ring(&ring[..4]));
        assert!(!valid_ring(&[
            ring[0],
            ring[2],
            ring[1],
            (-90.0003, 40.0002),
            ring[0]
        ]));
        let a = Point {
            x: -90.0001,
            y: 40.0001,
            provider: 0,
            extension: "",
        };
        let b = Point {
            x: -90.,
            provider: 1,
            ..a
        };
        let outside = Point {
            x: -90.0003,
            provider: 2,
            ..a
        };
        assert_eq!(
            inside_building(&ring, &[a, b, outside]),
            Some(packed_point(&a))
        );
        assert_eq!(
            inside_building(&ring, &[a, Point { provider: 0, ..b }, outside]),
            None
        );
        assert_eq!(
            inside_building(
                &ring,
                &[
                    a,
                    b,
                    Point {
                        x: -90.003,
                        ..outside
                    }
                ]
            ),
            None
        );
    }

    #[test]
    fn rejects_single_provider_and_conflicting_points() {
        let one = Point {
            x: -90.0,
            y: 40.0,
            provider: 0,
            extension: "4321",
        };
        let nearby = Point {
            x: -90.0001,
            extension: "",
            ..one
        };
        assert!(confirmed(&[one, nearby]).is_empty());
        let another = Point {
            provider: 1,
            ..nearby
        };
        let matches = confirmed(&[one, another]);
        assert_eq!(matches.keys().copied().collect::<Vec<_>>(), ["4321"]);
        assert!(
            confirmed(&[
                one,
                another,
                Point {
                    x: -90.002,
                    ..another
                }
            ])
            .is_empty()
        );
    }
}
