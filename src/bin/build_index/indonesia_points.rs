use super::*;
use std::collections::BTreeMap;

#[derive(serde::Deserialize)]
pub(super) struct Point(u64, Str, f64, f64, Str, Str);

pub(super) fn decode(input: &[u8], units: &[Unit]) -> Result<Vec<Point>> {
    let points: Vec<Point> =
        serde_json::from_reader(flate2::read::GzDecoder::new(input).take(4 * 1024 * 1024))?;
    ensure!(
        points.len() <= 10_000,
        "too many Indonesian district points"
    );
    let codes: HashMap<_, _> = units.iter().map(|u| (u.code.as_ref(), u)).collect();
    let mut ids = HashSet::new();
    let mut used = HashSet::new();
    let mut locations = HashSet::new();
    for Point(geoname, code, x, y, primary, evidence) in &points {
        let unit = codes
            .get(code.as_ref())
            .ok_or_else(|| anyhow::anyhow!("unknown Indonesian district point code"))?;
        ensure!(
            unit.level == 3
                && *geoname > 0
                && ids.insert(*geoname)
                && used.insert(code)
                && locations.insert((x.to_bits(), y.to_bits()))
                && x.is_finite()
                && y.is_finite()
                && (-180.0..=180.0).contains(x)
                && (-90.0..=90.0).contains(y)
                && [primary, evidence].iter().all(|s| {
                    !s.trim().is_empty() && s.len() <= 400 && !s.chars().any(char::is_control)
                })
                && id::hierarchy_key(evidence, 3) == id::hierarchy_key(&unit.name, 3),
            "invalid or ambiguous Indonesian district point"
        );
    }
    Ok(points)
}

pub(super) fn add(
    places: &mut Vec<Place>,
    path: &Path,
    units: &[Unit],
    mapped: &[Option<u32>],
    ambiguous: &HashSet<usize>,
    assigner: &Assigner,
    points: &[Point],
) -> Result<usize> {
    if points.is_empty() {
        return Ok(0);
    }
    let mut metadata = HashMap::new();
    for (i, line) in BufReader::new(File::open(path)?).lines().enumerate() {
        let line = line?;
        let fields: Vec<_> = line.split('\t').collect();
        if fields.get(1) == Some(&id::HIERARCHY_CLASS) {
            metadata.insert(i as u32, line);
        }
    }
    let codes: HashMap<_, _> = units
        .iter()
        .enumerate()
        .map(|(i, u)| (u.code.as_ref(), i))
        .collect();
    let mut changes = HashMap::new();
    let mut appended = vec![];
    let mut audit = String::from(
        "place\tgeoname\tgovernment_code\tlongitude\tlatitude\tsource_name\tevidence_name\taction\n",
    );
    let mut summary = BTreeMap::<&str, usize>::new();
    for Point(geoname, code, x, y, primary, evidence) in points {
        let unit = codes[code.as_ref()];
        let Some(county) = units[unit].parent.and_then(|c| mapped[c]) else {
            *summary.entry("unmapped_county").or_default() += 1;
            continue;
        };
        if ambiguous.contains(&unit) {
            *summary.entry("ambiguous_existing_district").or_default() += 1;
            continue;
        }
        if assigner.county(*x, *y, *b"ID") != Some(county) {
            *summary.entry("outside_or_ambiguous_county").or_default() += 1;
            continue;
        }
        let existing = mapped[unit];
        if let Some(existing) = existing {
            let p = &places[existing as usize];
            if p.xy.is_some() || !metadata.contains_key(&existing) {
                *summary.entry("preserved_existing_district").or_default() += 1;
                continue;
            }
            let fields: Vec<_> = metadata[&existing].split('\t').collect();
            if p.subtype != "localadmin"
                || scope(places, p, "county") != Some(county)
                || id::hierarchy_key(fields[13], 3) != id::hierarchy_key(&units[unit].name, 3)
            {
                *summary.entry("conflicting_existing_district").or_default() += 1;
                continue;
            }
        }
        let area = &places[county as usize];
        let mut ancestors = area.anc.to_vec();
        ancestors.push(county);
        ancestors.sort_unstable();
        ancestors.dedup();
        let p = Place {
            subtype: "localadmin",
            country: *b"ID",
            region: area.region.clone(),
            names: units[unit].name.clone(),
            population: 0,
            xy: Some((*x, *y)),
            anc: ancestors.into(),
            classed: true,
        };
        let row = if let Some(existing) = existing {
            let mut fields: Vec<String> =
                metadata[&existing].split('\t').map(str::to_owned).collect();
            fields[1] = id::ADMIN_POINT_CLASS.into();
            fields[6] = x.to_string();
            fields[7] = y.to_string();
            let row = fields.join("\t");
            changes.insert(existing, row.clone());
            places[existing as usize] = p;
            row
        } else {
            let row = format!(
                "localadmin\t{}\tID\t{}\t6\t0\t{x}\t{y}\tNaN\tNaN\tNaN\tNaN\t{county}\t{}\t\t",
                id::ADMIN_POINT_CLASS,
                p.region,
                cell(&units[unit].name)
            );
            appended.push(row.clone());
            places.push(p);
            row
        };
        let place = existing.unwrap_or(places.len() as u32 - 1);
        let action = if existing.is_some() {
            "point_for_metadata"
        } else {
            "new_district"
        };
        *summary.entry(action).or_default() += 1;
        ensure!(row.split('\t').count() == 16, "invalid district point row");
        use std::fmt::Write;
        writeln!(
            audit,
            "{place}\t{geoname}\t{code}\t{x}\t{y}\t{}\t{}\t{action}",
            cell(primary),
            cell(evidence)
        )?;
    }
    let count = changes.len() + appended.len();
    if count > 0 {
        let temp = path.with_extension("tsv.points.tmp");
        let mut output = BufWriter::new(File::create(&temp)?);
        for (i, line) in BufReader::new(File::open(path)?).lines().enumerate() {
            let line = line?;
            writeln!(output, "{}", changes.get(&(i as u32)).unwrap_or(&line))?;
        }
        for row in appended {
            writeln!(output, "{row}")?;
        }
        output.flush()?;
        drop(output);
        std::fs::rename(temp, path)?;
    }
    std::fs::write(path.with_file_name("id-admin-point-links.tsv"), audit)?;
    std::fs::write(
        path.with_file_name("id-admin-point-summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    eprintln!("Indonesian district points: {summary:?}");
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn run(kind: &str) -> (Vec<Place>, String, String, usize) {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "id-points-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("places.tsv");
        let mut places = super::super::tests::places();
        let units = super::super::tests::units("Tegowanu", "Kenanga");
        let mut mapped = vec![Some(1), Some(2), Some(3), None];
        let mut ambiguous = HashSet::new();
        let mut point = Point(
            123,
            "33.15.90".into(),
            110.2,
            -7.2,
            "Kecamatan Tegowanu".into(),
            "Tegowanu".into(),
        );
        if kind != "existing" {
            places[3].xy = None;
            places[3].names = Str::default();
        }
        places[4].anc = vec![0, 1, 2, 3].into();
        match kind {
            "new" => mapped[2] = None,
            "outside" => point.2 = 111.0,
            "ambiguous" => {
                ambiguous.insert(2);
            }
            "conflict" => {
                places[3].anc = vec![0, 1].into();
            }
            _ => (),
        }
        let rows: String = places.iter().enumerate().map(|(i, p)| {
            let class = if i == 3 && kind != "untyped" && kind != "new" && kind != "existing" { id::HIERARCHY_CLASS } else { "village" };
            let parent = if i == 4 { "3" } else { "2" };
            let name = if i == 3 { "Tegowanu" } else { &p.names };
            let (x, y) = p.xy.unwrap_or((f64::NAN, f64::NAN));
            format!("{}\t{class}\tID\tID-JT\t6\t0\t{x}\t{y}\tNaN\tNaN\tNaN\tNaN\t{parent}\t{name}\t\t\n", p.subtype)
        }).collect();
        std::fs::write(&path, &rows).unwrap();
        let file = PolyFile::new(root.join(".polygons"));
        let xy = vec![
            [110.0, -7.5],
            [110.5, -7.5],
            [110.5, -7.0],
            [110.0, -7.0],
            [110.0, -7.5],
        ];
        let mut polygons = vec![Poly::new(2, 2, xy.clone(), &[(0, 5)], &file).unwrap()];
        if kind == "overlap" {
            polygons.push(Poly::new(3, 2, xy, &[(0, 5)], &file).unwrap());
        }
        let assigner = Assigner::new(
            polygons,
            file,
            vec![],
            places.iter().map(|p| (p.country, p.country)).collect(),
        );
        let n = add(
            &mut places,
            &path,
            &units,
            &mapped,
            &ambiguous,
            &assigner,
            &[point],
        )
        .unwrap();
        let result = std::fs::read_to_string(path).unwrap();
        std::fs::remove_dir_all(root).unwrap();
        (places, rows, result, n)
    }

    #[test]
    fn supplies_only_district_points_and_preserves_original_villages() {
        for kind in ["metadata", "new"] {
            let (places, original, result, n) = run(kind);
            assert_eq!(n, 1, "{kind}");
            let i = if kind == "new" { 5 } else { 3 };
            assert_eq!(places[i].xy, Some((110.2, -7.2)));
            assert_eq!(places[i].subtype, "localadmin");
            assert_eq!(&*places[i].anc, &[0, 1, 2]);
            assert_eq!(&*places[4].anc, &[0, 1, 2, 3]);
            assert_eq!(original.lines().nth(4), result.lines().nth(4));
            if kind == "new" {
                assert!(result.starts_with(&original));
            }
            let fields: Vec<_> = result.lines().nth(i).unwrap().split('\t').collect();
            assert_eq!(
                (fields[0], fields[1], fields[12], fields[13]),
                ("localadmin", id::ADMIN_POINT_CLASS, "2", "Tegowanu")
            );
        }
    }

    #[test]
    fn established_points_conflicts_and_ambiguous_containment_are_untouched() {
        for kind in [
            "existing",
            "untyped",
            "outside",
            "overlap",
            "ambiguous",
            "conflict",
        ] {
            let (_, original, result, n) = run(kind);
            assert_eq!(n, 0, "{kind}");
            assert_eq!(original, result, "{kind}");
        }
    }

    fn encoded(value: serde_json::Value) -> Vec<u8> {
        let mut out = flate2::write::GzEncoder::new(vec![], flate2::Compression::default());
        out.write_all(&serde_json::to_vec(&value).unwrap()).unwrap();
        out.finish().unwrap()
    }

    #[test]
    fn source_validation_checks_identity_coordinates_and_duplicates() {
        let units = super::super::tests::units("Tegowanu", "Kenanga");
        let valid = serde_json::json!([
            123,
            "33.15.90",
            110.2,
            -7.2,
            "Kecamatan Tegowanu",
            "Tegowanu"
        ]);
        assert_eq!(
            decode(&encoded(serde_json::json!([valid.clone()])), &units)
                .unwrap()
                .len(),
            1
        );
        for row in [
            serde_json::json!([123, "33.15.90", 190.0, -7.2, "Tegowanu", "Tegowanu"]),
            serde_json::json!([123, "33.15.90", 110.2, -7.2, "Other", "Other"]),
            serde_json::json!([123, "33.15.90.2001", 110.2, -7.2, "Kenanga", "Kenanga"]),
            serde_json::json!([123, "33.15.91", 110.2, -7.2, "Other", "Other"]),
        ] {
            assert!(decode(&encoded(serde_json::json!([row])), &units).is_err());
        }
        assert!(decode(&encoded(serde_json::json!([valid.clone(), valid])), &units).is_err());
        let units =
            super::super::decode(include_bytes!("../../../data/id-hierarchy.json.gz")).unwrap();
        let input = include_bytes!("../../../data/id-admin-points.json.gz");
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../data/id-admin-points.sources.json"))
                .unwrap();
        assert_eq!(
            opengeocoding::eval::sha256_hex(input),
            manifest["output_sha256"].as_str().unwrap()
        );
        assert_eq!(
            decode(input, &units).unwrap().len(),
            manifest["points"].as_u64().unwrap() as usize
        );
    }
}
