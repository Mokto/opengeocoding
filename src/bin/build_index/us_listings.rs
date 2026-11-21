use super::{Assigner, Place, col, read, tasks, text, wkb_point};
use arrow_array::{Array, cast::AsArray};
use opengeocoding::index::country::us::{LISTED_HOUSES, listed_civic, listed_house_key};
use rayon::prelude::*;
use std::{collections::HashMap, fs::File, io::BufWriter, path::Path, sync::Mutex};

#[derive(Clone, Copy)]
struct Point<'a> {
    x: f64,
    y: f64,
    provider: u8,
    extension: &'a str,
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
            out.entry(p.extension).or_insert_with(|| {
                (u64::from((p.x as f32).to_bits()) << 32) | u64::from((p.y as f32).to_bits())
            });
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
) -> anyhow::Result<usize> {
    if !places.iter().any(|p| p.country == *b"US") {
        return Ok(0);
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
    let mut n = 0;
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
        n += usize::from(!choices.is_empty());
        for (extension, point) in choices {
            fst.insert(format!("{key}\t{extension}"), point)?;
        }
    }
    fst.finish()?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

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
