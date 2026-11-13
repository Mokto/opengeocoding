use super::*;
use anyhow::{Result, ensure};
use opengeocoding::index::country::id;
use std::{
    collections::BTreeSet,
    io::{BufReader, BufWriter, Read, Write},
};

struct Unit {
    code: Str,
    name: Str,
    level: u8,
    parent: Option<usize>,
}

fn decode(input: &[u8]) -> Result<Vec<Unit>> {
    let rows: Vec<(Str, Str)> =
        serde_json::from_reader(flate2::read::GzDecoder::new(input).take(16 * 1024 * 1024))?;
    ensure!(
        rows.len() <= 100_000,
        "too many Indonesian administrative units"
    );
    let mut codes = HashMap::new();
    for (i, (code, name)) in rows.iter().enumerate() {
        let parts: Vec<_> = code.split('.').collect();
        ensure!(
            (1..=4).contains(&parts.len())
                && parts.iter().enumerate().all(|(i, p)| {
                    p.len() == if i == 3 { 4 } else { 2 } && p.bytes().all(|b| b.is_ascii_digit())
                })
                && !name.trim().is_empty()
                && name.len() <= 400
                && !name.chars().any(char::is_control),
            "invalid Indonesian administrative code/name"
        );
        ensure!(
            codes.insert(code.as_ref(), i).is_none(),
            "duplicate Indonesian code"
        );
    }
    rows.iter()
        .map(|(code, name)| {
            let parent = code
                .rsplit_once('.')
                .map(|(p, _)| {
                    codes
                        .get(p)
                        .copied()
                        .ok_or_else(|| anyhow::anyhow!("missing Indonesian parent code"))
                })
                .transpose()?;
            Ok(Unit {
                code: code.clone(),
                name: name.clone(),
                level: code.bytes().filter(|b| *b == b'.').count() as u8 + 1,
                parent,
            })
        })
        .collect()
}

fn scope(places: &[Place], p: &Place, subtype: &str) -> Option<u32> {
    if p.country != *b"ID" || p.anc.iter().any(|&a| places[a as usize].country != *b"ID") {
        return None;
    }
    let mut ancestors = p
        .anc
        .iter()
        .copied()
        .filter(|&a| places[a as usize].subtype == subtype);
    let parent = ancestors.next()?;
    let area = &places[parent as usize];
    (ancestors.next().is_none()
        && area.country == *b"ID"
        && (p.region.is_empty() || area.region.is_empty() || p.region == area.region))
        .then_some(parent)
}

fn crosswalk(
    units: &[Unit],
    places: &[Place],
    level: u8,
    mapped: &[Option<u32>],
) -> (Vec<Option<u32>>, HashSet<usize>) {
    let mut keys: HashMap<(Option<u32>, String), Vec<usize>> = HashMap::new();
    let mut bare: HashMap<(Option<u32>, String), Vec<usize>> = HashMap::new();
    for (i, unit) in units.iter().enumerate().filter(|(_, u)| u.level == level) {
        let parent = if level == 1 {
            None
        } else {
            let Some(parent) = unit.parent.and_then(|p| mapped[p]) else {
                continue;
            };
            Some(parent)
        };
        keys.entry((parent, id::hierarchy_key(&unit.name, level)))
            .or_default()
            .push(i);
        if level == 2 {
            bare.entry((parent, id::hierarchy_county_key(&unit.name)))
                .or_default()
                .push(i);
        }
    }
    for (key, value) in bare {
        if value.len() == 1 {
            keys.entry(key).or_insert(value);
        }
    }
    let mut candidates = vec![vec![]; units.len()];
    let mut counts = HashMap::new();
    for (i, p) in places
        .iter()
        .enumerate()
        .filter(|(_, p)| p.country == *b"ID")
    {
        let eligible = match level {
            1 => p.subtype == "region",
            2 => p.subtype == "county",
            3 => p.subtype == "localadmin",
            _ => false,
        };
        if !eligible || scope(places, p, "country").is_none() {
            continue;
        }
        let parent = match level {
            1 => None,
            2 => match scope(places, p, "region") {
                Some(p) => Some(p),
                None => continue,
            },
            _ => match scope(places, p, "county") {
                Some(p) => Some(p),
                None => continue,
            },
        };
        let found: BTreeSet<_> = p
            .names
            .split(NAME_SEP)
            .filter_map(|name| keys.get(&(parent, id::hierarchy_key(name, level))))
            .flatten()
            .copied()
            .collect();
        counts.insert(i as u32, found.len());
        for u in found {
            candidates[u].push(i as u32);
        }
    }
    let mut ambiguous = HashSet::new();
    let links = candidates
        .into_iter()
        .enumerate()
        .map(|(unit, found)| {
            if found.len() == 1 && counts[&found[0]] == 1 {
                Some(found[0])
            } else {
                if !found.is_empty() {
                    ambiguous.insert(unit);
                }
                None
            }
        })
        .collect();
    (links, ambiguous)
}

fn enrich(
    places: &mut Vec<Place>,
    path: &Path,
    units: &[Unit],
    assigner: &Assigner,
) -> Result<(usize, usize)> {
    let mut mapped = vec![None; units.len()];
    let mut districts = HashSet::new();
    for level in 1..=3 {
        let (candidates, ambiguous) = crosswalk(units, places, level, &mapped);
        for (i, found) in candidates.into_iter().enumerate() {
            if found.is_some() {
                mapped[i] = found;
            }
        }
        if level == 3 {
            districts = ambiguous;
        }
    }
    let mut named: HashMap<(u32, String), Vec<usize>> = HashMap::new();
    for (i, unit) in units.iter().enumerate().filter(|(_, u)| u.level == 4) {
        let district = unit.parent.unwrap();
        let Some(county) = units[district].parent.and_then(|c| mapped[c]) else {
            continue;
        };
        named
            .entry((county, id::hierarchy_key(&unit.name, 4)))
            .or_default()
            .push(i);
    }
    let mut candidates = vec![];
    let mut counts = HashMap::<usize, usize>::new();
    for (i, line) in BufReader::new(File::open(path)?).lines().enumerate() {
        let line = line?;
        let p = &places[i];
        if scope(places, p, "country").is_none() {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        if fields.first() != Some(&"locality")
            || !matches!(
                fields.get(1).copied(),
                Some("village" | "hamlet" | "geonames" | schema::SMALL_GEONAMES)
            )
        {
            continue;
        }
        let Some(parent) = fields.get(12).and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        if places[parent as usize].subtype != "county"
            || scope(places, p, "county") != Some(parent)
            || !p.xy.is_some_and(|(x, y)| {
                x.is_finite() && y.is_finite() && assigner.county(x, y, *b"ID") == Some(parent)
            })
            || p.names.split(NAME_SEP).any(id::hierarchy_district)
            || p.anc.iter().any(|&a| {
                places[a as usize].subtype == "localadmin"
                    || places[a as usize]
                        .names
                        .split(NAME_SEP)
                        .any(id::hierarchy_district)
            })
        {
            continue;
        }
        let found: BTreeSet<_> = p
            .names
            .split(NAME_SEP)
            .filter_map(|name| named.get(&(parent, id::hierarchy_key(name, 4))))
            .flatten()
            .copied()
            .collect();
        let Some(&unit) = found.iter().next().filter(|_| found.len() == 1) else {
            continue;
        };
        let district = units[unit].parent.unwrap();
        if districts.contains(&district)
            || p.anc.contains(&(i as u32))
            || places[parent as usize].anc.contains(&(i as u32))
            || mapped[district].is_some_and(|d| places[d as usize].anc.contains(&(i as u32)))
        {
            continue;
        }
        *counts.entry(unit).or_default() += 1;
        candidates.push((i as u32, unit));
    }
    candidates.retain(|(_, unit)| counts[unit] == 1);
    let needed: BTreeSet<_> = candidates
        .iter()
        .map(|(_, u)| units[*u].parent.unwrap())
        .collect();
    let mut added = vec![];
    for district in needed {
        if mapped[district].is_some() {
            continue;
        }
        let unit = &units[district];
        let county = mapped[unit.parent.unwrap()].unwrap();
        let area = &places[county as usize];
        let mut anc = area.anc.to_vec();
        anc.push(county);
        anc.sort_unstable();
        anc.dedup();
        let region = area.region.clone();
        let number = places.len() as u32;
        added.push(format!(
            "localadmin\t{}\tID\t{}\t6\t0\tNaN\tNaN\tNaN\tNaN\tNaN\tNaN\t{}\t{}\t\t",
            id::HIERARCHY_CLASS,
            region,
            county,
            cell(&unit.name)
        ));
        // A names-only parent must have no result document or borrowed coordinates.
        places.push(Place {
            subtype: "localadmin",
            country: *b"ID",
            region,
            names: Str::default(),
            population: 0,
            xy: None,
            anc: anc.into(),
            classed: false,
        });
        mapped[district] = Some(number);
    }
    let mut changes = HashMap::new();
    let mut links = String::from("place\tsource_village\tdistrict\tsource_district\n");
    for (place, unit) in candidates {
        let district = mapped[units[unit].parent.unwrap()].unwrap();
        let mut anc = places[place as usize].anc.to_vec();
        anc.push(district);
        anc.extend_from_slice(&places[district as usize].anc);
        anc.sort_unstable();
        anc.dedup();
        places[place as usize].anc = anc.into();
        changes.insert(place, district);
        use std::fmt::Write;
        writeln!(
            links,
            "{place}\t{}\t{district}\t{}",
            units[unit].code,
            units[units[unit].parent.unwrap()].code
        )?;
    }
    std::fs::write(path.with_file_name("id-hierarchy-links.tsv"), links)?;
    if !changes.is_empty() {
        let updated = path.with_extension("tsv.id.tmp");
        let mut out = BufWriter::new(File::create(&updated)?);
        for (i, line) in BufReader::new(File::open(path)?).lines().enumerate() {
            let line = line?;
            if let Some(parent) = changes.get(&(i as u32)) {
                let mut fields: Vec<_> = line.split('\t').collect();
                let parent = parent.to_string();
                fields[12] = &parent;
                writeln!(out, "{}", fields.join("\t"))?;
            } else {
                writeln!(out, "{line}")?;
            }
        }
        for row in &added {
            writeln!(out, "{row}")?;
        }
        out.flush()?;
        drop(out);
        std::fs::rename(updated, path)?;
    }
    Ok((changes.len(), added.len()))
}

pub fn write(
    places: &mut Vec<Place>,
    assigner: &Assigner,
    out: &Path,
    source: Option<&str>,
) -> Result<(usize, usize)> {
    let input = source.map(std::fs::read).transpose()?;
    let input = input
        .as_deref()
        .unwrap_or(include_bytes!("../../../data/id-hierarchy.json.gz"));
    let result = if source.is_some() || places.iter().any(|p| p.country == *b"ID") {
        enrich(places, &out.join("places.tsv"), &decode(input)?, assigner)?
    } else {
        (0, 0)
    };
    let manifest = match source {
        Some(_) => {
            serde_json::json!({"input":"override", "sha256":opengeocoding::eval::sha256_hex(input)})
                .to_string()
        }
        None => include_str!("../../../data/id-hierarchy.sources.json").to_string(),
    };
    std::fs::write(out.join("id-hierarchy-source.json"), manifest)?;
    std::fs::write(
        out.join("INDONESIA-HIERARCHY-NOTICE.txt"),
        include_str!("../../../data/id-hierarchy.NOTICE"),
    )?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(rows: serde_json::Value) -> Vec<u8> {
        let mut out = flate2::write::GzEncoder::new(vec![], flate2::Compression::default());
        out.write_all(&serde_json::to_vec(&rows).unwrap()).unwrap();
        out.finish().unwrap()
    }

    fn units(district: &str, village: &str) -> Vec<Unit> {
        decode(&encoded(serde_json::json!([
            ["33", "Jawa Tengah"],
            ["33.15", "Kabupaten Grobogan"],
            ["33.15.90", district],
            ["33.15.90.2001", village]
        ])))
        .unwrap()
    }

    fn places() -> Vec<Place> {
        [
            ("country", "Indonesia", vec![]),
            ("region", "Jawa Tengah", vec![0]),
            ("county", "Kabupaten Grobogan", vec![0, 1]),
            ("localadmin", "Tegowanu", vec![0, 1, 2]),
            ("locality", "Kenanga", vec![0, 1, 2]),
        ]
        .into_iter()
        .map(|(subtype, name, anc)| Place {
            subtype,
            names: name.into(),
            country: *b"ID",
            region: "ID-JT".into(),
            population: 100,
            xy: Some((110.1, -7.2)),
            anc: anc.into(),
            classed: subtype == "locality",
        })
        .collect()
    }

    fn run(
        places: &mut Vec<Place>,
        units: &[Unit],
        label: &str,
        parent: u32,
        class: &str,
    ) -> (usize, usize, String) {
        let root =
            std::env::temp_dir().join(format!("id-hierarchy-{}-{label}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("places.tsv");
        let rows: String = places
            .iter()
            .map(|p| {
                let (x, y) = p.xy.unwrap();
                format!(
                    "{}\t{}\t{}\t{}\t0\t100\t{x}\t{y}\tNaN\tNaN\tNaN\tNaN\t{}\t{}\t\t\n",
                    p.subtype,
                    if p.subtype == "locality" { class } else { "" },
                    country_str(&p.country),
                    p.region,
                    if p.subtype == "locality" {
                        Some(parent)
                    } else {
                        p.anc.last().copied()
                    }
                    .map(|p| p.to_string())
                    .unwrap_or_default(),
                    p.names
                )
            })
            .collect();
        std::fs::write(&path, rows).unwrap();
        let file = PolyFile::new(root.join(".polygons"));
        let xy = vec![
            [110.0, -7.5],
            [110.5, -7.5],
            [110.5, -7.0],
            [110.0, -7.0],
            [110.0, -7.5],
        ];
        let mut polygons = vec![Poly::new(2, 2, xy.clone(), &[(0, 5)], &file).unwrap()];
        if label == "overlap" {
            polygons.push(Poly::new(3, 2, xy, &[(0, 5)], &file).unwrap());
        }
        let assigner = Assigner::new(
            polygons,
            file,
            vec![],
            places.iter().map(|p| (p.country, p.country)).collect(),
        );
        let (linked, parents) = enrich(places, &path, units, &assigner).unwrap();
        let result = std::fs::read_to_string(path).unwrap();
        std::fs::remove_dir_all(root).unwrap();
        (linked, parents, result)
    }

    #[test]
    fn links_only_unique_villages_without_borrowing_district_coordinates() {
        let mut p = places();
        let mut town = places().remove(4);
        town.names = format!("Rimba{NAME_SEP}Kecamatan Rimba").into();
        p.push(town);
        let xy = p[4].xy;
        let (linked, parents, result) =
            run(&mut p, &units("Rimba", "Kenanga"), "new", 2, "village");
        assert_eq!((linked, parents), (1, 1));
        assert_eq!(p[4].xy, xy);
        assert_eq!(&*p[4].anc, &[0, 1, 2, 6]);
        assert_eq!(p[5].xy, xy);
        assert!(p[6].xy.is_none() && p[6].names.is_empty());
        let row: Vec<_> = result.lines().nth(6).unwrap().split('\t').collect();
        assert_eq!(
            (row[1], row[6], row[7], row[12], row[13]),
            (id::HIERARCHY_CLASS, "NaN", "NaN", "2", "Rimba")
        );
        assert_eq!(
            result.lines().nth(4).unwrap().split('\t').nth(12),
            Some("6")
        );
        let mut p = places();
        let (linked, parents, _) = run(
            &mut p,
            &units("Tegowanu", "Kenanga"),
            "existing",
            2,
            "village",
        );
        assert_eq!((linked, parents), (1, 0));
        assert_eq!(&*p[4].anc, &[0, 1, 2, 3]);
        let mut p = places();
        p[4].names = "Kota Baru".into();
        assert_eq!(
            run(
                &mut p,
                &units("Rimba", "Kota Baru"),
                "literal",
                2,
                "village"
            )
            .0,
            1
        );
    }

    #[test]
    fn conflicts_ambiguities_existing_parents_and_nonvillages_abstain() {
        for label in [
            "country",
            "province",
            "region",
            "county",
            "source-duplicate",
            "point-duplicate",
            "district-duplicate",
            "existing-parent",
            "town",
            "outside",
            "overlap",
            "country-ancestor",
            "ambiguous-country",
            "province-country",
            "county-country",
            "county-prefix-collision",
            "district-place",
            "county-cycle",
            "district-cycle",
        ] {
            let mut p = places();
            let mut u = units("Rimba", "Kenanga");
            let mut parent = 2;
            let mut class = "village";
            match label {
                "country" => p[4].country = *b"FR",
                "country-ancestor" | "ambiguous-country" | "province-country"
                | "county-country" => {
                    let mut country = places().remove(0);
                    if label != "ambiguous-country" {
                        country.country = *b"FR";
                    }
                    let id = p.len() as u32;
                    p.push(country);
                    let at = match label {
                        "province-country" => 1,
                        "county-country" => 2,
                        _ => 4,
                    };
                    let mut ancestors = p[at].anc.to_vec();
                    ancestors.push(id);
                    p[at].anc = ancestors.into();
                }
                "province" => p[1].names = "Other province".into(),
                "region" => p[4].region = "ID-YO".into(),
                "county" => p[2].names = "Kota Grobogan".into(),
                "county-prefix-collision" => {
                    p[2].names = "Kota Baru".into();
                    u[1].name = "Kabupaten Kotabaru".into();
                }
                "district-place" => {
                    p[4].names = format!("Kecamatan Kenanga{NAME_SEP}Kenanga").into()
                }
                "county-cycle" => p[2].anc = vec![0, 1, 4].into(),
                "district-cycle" => {
                    p[3].anc = vec![0, 1, 2, 4].into();
                    u[2].name = "Tegowanu".into();
                }
                "source-duplicate" => {
                    u.push(Unit {
                        code: "33.15.91".into(),
                        name: "Seroja".into(),
                        level: 3,
                        parent: Some(1),
                    });
                    u.push(Unit {
                        code: "33.15.91.2001".into(),
                        name: "Kenanga".into(),
                        level: 4,
                        parent: Some(4),
                    });
                }
                "point-duplicate" => {
                    let mut more = places();
                    p.push(more.pop().unwrap());
                }
                "district-duplicate" => {
                    for _ in 0..2 {
                        let mut more = places();
                        let mut district = more.remove(3);
                        district.names = "Rimba".into();
                        p.push(district);
                    }
                }
                "existing-parent" => {
                    parent = 3;
                    p[4].anc = vec![0, 1, 2, 3].into();
                }
                "town" => class = "town",
                "outside" => p[4].xy = Some((120.0, -7.2)),
                "overlap" => {
                    p[3].subtype = "county";
                    p[3].names = "Other county".into();
                }
                _ => unreachable!(),
            }
            assert_eq!(run(&mut p, &u, label, parent, class).0, 0, "{label}");
        }
    }

    #[test]
    fn pinned_names_and_malformed_hierarchies_are_checked() {
        let input = include_bytes!("../../../data/id-hierarchy.json.gz");
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../data/id-hierarchy.sources.json")).unwrap();
        assert_eq!(
            opengeocoding::eval::sha256_hex(input),
            manifest["gzip_sha256"].as_str().unwrap()
        );
        let u = decode(input).unwrap();
        for (level, count) in [(1, 38), (2, 514), (3, 7285), (4, 83762)] {
            assert_eq!(u.iter().filter(|u| u.level == level).count(), count);
        }
        for value in [
            serde_json::json!([["33", "A"], ["33", "B"]]),
            serde_json::json!([["33.01", "Missing parent"]]),
            serde_json::json!([["3", "Invalid code"]]),
            serde_json::json!([["33", "Bad\tname"]]),
            serde_json::json!([["33", ""]]),
        ] {
            assert!(decode(&encoded(value)).is_err());
        }
        assert!(decode(b"not gzip").is_err());
    }
}
