use super::*;
use anyhow::Result;
use opengeocoding::index::country::pl;
use std::{collections::BTreeSet, io::BufWriter};
use tantivy::directory::{Directory, MmapDirectory};
use tantivy_fst::{IntoStreamer, Streamer};

fn region(places: &[Place], id: u32) -> Option<&str> {
    let place = &places[id as usize];
    let mut regions = std::iter::once(id)
        .chain(place.anc.iter().copied())
        .map(|p| &places[p as usize])
        .filter(|p| p.subtype == "region" && p.country == *b"PL")
        .map(|p| p.region.as_ref());
    let first = regions.next()?;
    (!first.is_empty() && regions.all(|r| r == first)).then_some(first)
}

fn source(b: &RecordBatch, i: usize) -> Option<(&str, &str, &str)> {
    let sources = b.column_by_name("sources")?.as_list::<i32>();
    if sources.is_null(i) {
        return None;
    }
    let o = sources.value_offsets();
    if o[i + 1] - o[i] != 1 {
        return None;
    }
    let values = sources.values().as_struct();
    let get = |name: &str| {
        text(
            values.column_by_name(name)?.as_string::<i32>(),
            o[i] as usize,
        )
    };
    if get("provider")? != "open_addresses"
        || get("license")? != "LicenseRef-Public-Domain-PL-Geodesy-Law"
        || get("property").is_some()
    {
        return None;
    }
    let id = get("record_id")?;
    if id.len() != 36
        || !id.bytes().enumerate().all(|(at, b)| {
            if [8, 13, 18, 23].contains(&at) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return None;
    }
    let dataset = get("dataset")?;
    pl::source_region(dataset)?;
    Some((dataset, get("version")?, get("update_time")?))
}

fn province_matches(dataset: &str, first: &str, second: &str) -> bool {
    let Some(region) = pl::source_region(dataset) else {
        return false;
    };
    let province = fold(dataset.strip_prefix("pl/").unwrap());
    let names = [fold(first), fold(second)];
    names
        .iter()
        .filter(|name| name.as_str() == province)
        .count()
        == 1
        && names.iter().all(|name| {
            !name.is_empty()
                && pl::source_region(&format!("pl/{}", name.replace(' ', "-")))
                    .is_none_or(|code| code == region)
        })
}

fn point_key(number: &str, x: f64, y: f64) -> Vec<u8> {
    let (a, b) = schema::pack(x, y, 0);
    let (x, y, _) = schema::unpack((u64::from(a) << 32) | u64::from(b));
    schema::house_point_key(0, number, x, y)
}

fn descendant(places: &[Place], candidates: &[u32], parent: u32) -> Option<u32> {
    let mut candidates = candidates.iter().copied().filter(|&p| {
        let primary = fold(
            places[p as usize]
                .names
                .split(NAME_SEP)
                .next()
                .unwrap_or(""),
        );
        p != parent
            && places[p as usize].anc.contains(&parent)
            && !std::iter::once(parent)
                .chain(places[parent as usize].anc.iter().copied())
                .map(|id| &places[id as usize])
                .filter(|place| place.subtype == "locality")
                .any(|place| {
                    place
                        .names
                        .split(NAME_SEP)
                        .any(|name| fold(name) == primary)
                })
            && region(places, p).is_some_and(|r| Some(r) == region(places, parent))
    });
    let first = candidates.next()?;
    candidates.next().is_none().then_some(first)
}

pub fn write(dir: &Path, out: &Path, places: &[Place], streets: &[AddrShard]) -> Result<u64> {
    let mut named: HashMap<(String, String), usize> = HashMap::new();
    let mut candidates: Vec<Vec<u32>> = vec![];
    for (id, place) in places.iter().enumerate() {
        if place.country != *b"PL" || place.subtype != "locality" {
            continue;
        }
        let Some(region) = region(places, id as u32) else {
            continue;
        };
        let name = fold(place.names.split(NAME_SEP).next().unwrap_or(""));
        if name.is_empty() {
            continue;
        }
        let key = (name, region.to_string());
        let at = *named.entry(key).or_insert_with(|| {
            candidates.push(vec![]);
            candidates.len() - 1
        });
        candidates[at].push(id as u32);
    }
    let mut evidence: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut sources = BTreeSet::new();
    for task in tasks(dir) {
        for b in read(
            &task,
            &["country", "number", "address_levels", "sources", "geometry"],
        ) {
            let cc = col(&b, "country").as_string::<i32>();
            let numbers = col(&b, "number").as_string::<i32>();
            let levels = col(&b, "address_levels").as_list::<i32>();
            let values = levels.values().as_struct().column(0).as_string::<i32>();
            let geometry = col(&b, "geometry").as_binary::<i32>();
            for i in 0..b.num_rows() {
                if text(cc, i) != Some("PL") || levels.is_null(i) || geometry.is_null(i) {
                    continue;
                }
                let Some((dataset, version, updated)) = source(&b, i) else {
                    continue;
                };
                let o = levels.value_offsets();
                // Overture September/October encode PRG as [powiat, province, city]/[province, gmina, city].
                if o[i + 1] - o[i] != 3
                    || text(values, o[i] as usize)
                        .zip(text(values, o[i] as usize + 1))
                        .is_none_or(|(first, second)| !province_matches(dataset, first, second))
                {
                    continue;
                }
                let (Some(name), Some(number), Some((x, y))) = (
                    text(values, o[i + 1] as usize - 1),
                    text(numbers, i),
                    wkb_point(geometry.value(i)),
                ) else {
                    continue;
                };
                if !x.is_finite() || !y.is_finite() || x.abs() > 180. || y.abs() > 90. {
                    continue;
                }
                let name = fold(name);
                if name.is_empty() {
                    continue;
                }
                let name = (name, pl::source_region(dataset).unwrap().to_string());
                let name = *named.entry(name).or_insert_with(|| {
                    candidates.push(vec![]);
                    candidates.len() - 1
                });
                let key = point_key(number, x, y);
                evidence
                    .entry(key)
                    .and_modify(|old| {
                        if *old != name {
                            *old = usize::MAX;
                        }
                    })
                    .or_insert(name);
                sources.insert((
                    dataset.to_string(),
                    version.to_string(),
                    updated.to_string(),
                ));
            }
        }
    }
    let mmap = MmapDirectory::open(out)?;
    let numbers: Vec<Vec<_>> = (0..NUMBER_SHARDS)
        .map(|s| {
            [format!("numbers-{s:02}.fst"), schema::far_file(s)]
                .into_iter()
                .map(|name| {
                    let file = mmap.open_read(Path::new(&name))?;
                    Ok(tantivy_fst::Map::from(tantivy_fst::raw::Fst::new(
                        file.read_bytes()?,
                    )?))
                })
                .collect::<Result<_>>()
        })
        .collect::<Result<_>>()?;
    let mut selected: Vec<_> = streets
        .iter()
        .enumerate()
        .flat_map(|(shard, s)| {
            s.streets
                .iter()
                .enumerate()
                .filter_map(move |(position, _)| {
                    let (country, place, _) = s.key(position as u32);
                    (country == *b"PL" && place != NO_PLACE)
                        .then_some(((position as u32) << 8 | shard as u32, place))
                })
        })
        .collect();
    selected.sort_unstable();
    let mut writer =
        tantivy_fst::MapBuilder::new(BufWriter::new(File::create(out.join(pl::LOCALITIES_FILE))?))?;
    let (mut entries, mut count) = (vec![], 0);
    for (sid, parent) in selected {
        entries.clear();
        for numbers in &numbers[sid as usize % NUMBER_SHARDS] {
            let mut range = numbers.range().ge(sid.to_be_bytes());
            if let Some(next) = sid.checked_add(1) {
                range = range.lt(next.to_be_bytes());
            }
            let mut stream = range.into_stream();
            while let Some((key, value)) = stream.next() {
                let number = std::str::from_utf8(&key[4..])?;
                let (x, y, _) = schema::unpack(value);
                let Some(&name) = evidence.get(&point_key(number, x, y)) else {
                    continue;
                };
                let Some(candidates) = candidates.get(name) else {
                    continue;
                };
                let Some(place) = descendant(places, candidates, parent) else {
                    continue;
                };
                entries.push((schema::house_point_key(sid, number, x, y), u64::from(place)));
            }
        }
        entries.sort_unstable();
        entries.dedup();
        for (key, place) in &entries {
            writer.insert(key, *place)?;
            count += 1;
        }
    }
    writer.finish()?;
    std::fs::write(
        out.join("poland-source.json"),
        serde_json::to_vec_pretty(&sources)?,
    )?;
    std::fs::write(
        out.join("POLAND-LOCALITIES-NOTICE.txt"),
        include_str!("../../../data/pl-localities.NOTICE"),
    )?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn province_supports_both_geographic_layouts_without_accepting_conflicts() {
        let dataset = "pl/kujawsko-pomorskie";
        assert!(province_matches(dataset, "County", "kujawsko-pomorskie"));
        assert!(province_matches(
            dataset,
            "kujawsko-pomorskie",
            "Municipality"
        ));
        assert!(province_matches("pl/lodzkie", "Łódzkie", "Municipality"));
        assert!(!province_matches(dataset, "County", "Municipality"));
        assert!(!province_matches(dataset, "County", "lubuskie"));
        assert!(!province_matches(dataset, "kujawsko-pomorskie", "lubuskie"));
        assert!(!province_matches(
            dataset,
            "kujawsko-pomorskie",
            "kujawsko-pomorskie"
        ));
        assert!(!province_matches(dataset, "kujawsko-pomorskie", ""));
        assert!(!province_matches("pl/unknown", "unknown", "Municipality"));
    }

    #[test]
    fn membership_requires_an_unambiguous_descendant_and_preserves_existing_names() {
        let place = |subtype, names: &str, region: &str, anc: &[u32]| Place {
            subtype,
            country: *b"PL",
            names: names.into(),
            region: region.into(),
            anc: anc.into(),
            population: 0,
            xy: None,
        };
        let mut places = vec![
            place("region", "West", "PL-04", &[]),
            place("locality", "Parent", "", &[0]),
            place("locality", "Child", "", &[0, 1]),
            place("locality", "Child", "", &[0, 1]),
            place("region", "East", "PL-08", &[]),
            place("locality", "Child", "", &[4]),
        ];
        assert_eq!(descendant(&places, &[2], 1), Some(2));
        assert_eq!(descendant(&places, &[2, 3], 1), None);
        assert_eq!(descendant(&places, &[5], 1), None);
        assert_eq!(descendant(&places, &[1], 1), None);
        places[1].names = "Parent\u{1f}Child".into();
        assert_eq!(descendant(&places, &[2], 1), None);
        places[1].names = "Parent".into();
        places[2].anc = [0, 1, 4].into();
        assert_eq!(descendant(&places, &[2], 1), None);
    }

    #[test]
    fn evidence_keys_keep_complete_numbers_and_exact_packed_points() {
        let (x, y) = (20.9999, 52.003);
        let key = point_key("12B/14", x, y);
        let (a, b) = schema::pack(x, y, 0);
        let (px, py, _) = schema::unpack((u64::from(a) << 32) | u64::from(b));
        assert_eq!(key, point_key("12b/14", px, py));
        assert_ne!(key, point_key("12B", x, y));
        assert_ne!(key, point_key("12B/14", x + 0.00001, y));
        assert_ne!(key, point_key("12B/14", x, y + 0.00001));
    }
}
