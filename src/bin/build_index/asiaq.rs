use super::geo::Assigner;
use anyhow::{Context, Result, ensure};
use opengeocoding::index::{country::gl, schema::fold};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};

pub fn write(out: &Path, assigner: &Assigner, source: Option<&str>) -> Result<usize> {
    let input = source.map(std::fs::read_to_string).transpose()?;
    let input = input
        .as_deref()
        .unwrap_or(include_str!("../../../data/asiaq-buildings.tsv"));
    let mut rows = BTreeMap::new();
    for (line, row) in input.lines().enumerate() {
        let fields: Vec<_> = row.split('\t').collect();
        let [locality, town, number, lat, lon] = fields.as_slice() else {
            anyhow::bail!("Asiaq line {} needs five TSV columns", line + 1);
        };
        let digits = number.strip_prefix("B-").unwrap_or("");
        let (lat, lon): (f64, f64) = (
            lat.parse().context("Asiaq latitude")?,
            lon.parse().context("Asiaq longitude")?,
        );
        ensure!(
            (1..=5).contains(&digits.len())
                && !digits.starts_with('0')
                && digits.bytes().all(|c| c.is_ascii_digit())
                && !fold(town).is_empty()
                && !town.chars().any(|c| c.is_control() || c.is_ascii_digit())
                && locality.len() == 7
                && locality.bytes().all(|c| c.is_ascii_alphanumeric())
                && (59. ..84.).contains(&lat)
                && (-74. ..-10.).contains(&lon),
            "invalid Asiaq reference on line {}",
            line + 1
        );
        if !assigner.within(lon, lat) {
            continue;
        }
        let Some(place) = assigner.place_in(lon, lat, Some(*b"GL")) else {
            continue;
        };
        let key = format!("{}:{}", fold(town), number.to_ascii_lowercase());
        ensure!(rows.insert(key, json!({"locality":locality,"town":town,"number":number,"point":[lat,lon],"place":place})).is_none(), "duplicate Asiaq building reference");
    }
    std::fs::write(out.join(gl::FILE), serde_json::to_vec(&rows)?)?;
    std::fs::write(
        out.join("BUILDINGS-NOTICE.txt"),
        include_str!("../../../data/asiaq-buildings.NOTICE"),
    )?;
    Ok(rows.len())
}

#[cfg(test)]
mod tests {
    #[test]
    fn malformed_references_are_rejected_before_geographic_filtering() {
        let dir = std::env::temp_dir().join(format!("asiaq-input-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let assigner = super::Assigner::new(
            vec![],
            crate::geo::PolyFile::new(dir.join("polys")),
            vec![],
            vec![],
        );
        let file = dir.join("source.tsv");
        for row in [
            "bad\tNuuk\tB-12\t64\t-52\n",
            "0600NUK\t \tB-12\t64\t-52\n",
            "0600NUK\tNuuk\t12\t64\t-52\n",
            "0600NUK\tNuuk\tB-012\t64\t-52\n",
            "0600NUK\tNuuk\tB-12\t-52\t64\n",
            "0600NUK\tNuuk\tB-12\tNaN\t-52\n",
        ] {
            std::fs::write(&file, row).unwrap();
            assert!(
                super::write(&dir, &assigner, file.to_str()).is_err(),
                "{row:?}"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn pinned_snapshot_is_the_full_six_town_frame_not_gold_samples() {
        let input = include_str!("../../../data/asiaq-buildings.tsv");
        assert_eq!(input.lines().count(), 7874);
        let mut keys = std::collections::BTreeSet::new();
        let mut towns = std::collections::BTreeSet::new();
        for row in input.lines() {
            let fields: Vec<_> = row.split('\t').collect();
            assert_eq!(fields.len(), 5);
            assert!(keys.insert((fields[0], fields[2])));
            towns.insert(fields[1]);
        }
        assert_eq!(
            towns,
            std::collections::BTreeSet::from([
                "Qaqortoq",
                "Nuuk",
                "Sisimiut",
                "Aasiaat",
                "Ilulissat",
                "Tasiilaq"
            ])
        );
        assert_eq!(
            opengeocoding::eval::sha256_hex(input.as_bytes()),
            "88fa6292038887eb0cfd92a43c00e8594460849e08ad4109975a04bc0a62439b"
        );
    }
}
