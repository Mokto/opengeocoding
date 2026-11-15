use crate::{NAME_SEP, Place, Str, cell, geo::Assigner};
use anyhow::{Context, Result, ensure};
use arrow_array::{Array, BinaryArray, Int32Array, StringArray};
use opengeocoding::index::schema::fold;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::File,
    io::Write,
    path::Path,
};

pub const URL: &str = "https://github.com/ramSeraph/indian_admin_boundaries/releases/download/habitations/PMGSY_Habitations.parquet";
pub const SHA256: &str = "f1daf758403f9499a0a9818ddd1ac8eff621fa4fa5f089dc70daadc2d02c02b9";
pub const NOTICE: &str = include_str!("../../../data/in-geosadak.NOTICE");
const CLASS: &str = "pmgsy_habitation";

struct Row {
    id: i32,
    name: String,
    population: u64,
    xy: (f64, f64),
}

#[derive(Default, Serialize)]
struct Counts {
    read: usize,
    invalid: usize,
    duplicate_id: usize,
    namesake: usize,
    already_named: usize,
    outside: usize,
    added: usize,
    clustered: usize,
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat, lon) = ((b.1 - a.1).to_radians(), (b.0 - a.0).to_radians());
    let h = (lat / 2.0).sin().powi(2)
        + a.1.to_radians().cos() * b.1.to_radians().cos() * (lon / 2.0).sin().powi(2);
    12_742.0 * h.clamp(0.0, 1.0).sqrt().asin()
}

pub fn add_places(
    places: &mut Vec<Place>,
    assigner: &Assigner,
    path: &Path,
    expected: &str,
    out: &Path,
) -> Result<usize> {
    ensure!(
        crate::store::sha256(path)? == expected,
        "GeoSadak capture SHA-256 mismatch"
    );
    let mut counts = Counts::default();
    let mut ids = HashMap::<i32, usize>::new();
    let mut rows = Vec::new();
    let reader = ParquetRecordBatchReaderBuilder::try_new(File::open(path)?)?.build()?;
    for batch in reader {
        let batch = batch?;
        let id = batch
            .column_by_name("HAB_ID")
            .and_then(|a| a.as_any().downcast_ref::<Int32Array>())
            .context("GeoSadak HAB_ID must be int32")?;
        let names = batch
            .column_by_name("HAB_NAME")
            .and_then(|a| a.as_any().downcast_ref::<StringArray>())
            .context("GeoSadak HAB_NAME must be UTF-8")?;
        let population = batch
            .column_by_name("TOT_POPULA")
            .and_then(|a| a.as_any().downcast_ref::<Int32Array>())
            .context("GeoSadak TOT_POPULA must be int32")?;
        let geometry = batch
            .column_by_name("geometry")
            .and_then(|a| a.as_any().downcast_ref::<BinaryArray>())
            .context("GeoSadak geometry must be WKB")?;
        for i in 0..batch.num_rows() {
            counts.read += 1;
            if !id.is_null(i) {
                *ids.entry(id.value(i)).or_default() += 1;
            }
            let row = (|| {
                if id.is_null(i)
                    || id.value(i) <= 0
                    || names.is_null(i)
                    || population.is_null(i)
                    || population.value(i) <= 0
                    || geometry.is_null(i)
                {
                    return None;
                }
                let name = names.value(i).trim();
                if !name.chars().any(char::is_alphabetic) {
                    return None;
                }
                let bytes = geometry.value(i);
                if bytes.len() != 21 || bytes[..5] != [1, 1, 0, 0, 0] {
                    return None;
                }
                let xy = crate::geo::wkb_point(bytes)?;
                if !xy.0.is_finite()
                    || !xy.1.is_finite()
                    || !(68.0..=98.0).contains(&xy.0)
                    || !(6.0..=38.0).contains(&xy.1)
                {
                    return None;
                }
                Some(Row {
                    id: id.value(i),
                    name: name.to_owned(),
                    population: population.value(i) as u64,
                    xy,
                })
            })();
            if let Some(row) = row {
                rows.push(row);
            } else {
                counts.invalid += 1;
            }
        }
    }
    ensure!(counts.read > 0, "GeoSadak capture has no records");
    let known: HashSet<String> = places
        .iter()
        .filter(|p| p.country == *b"IN")
        .flat_map(|p| p.names.split(NAME_SEP).map(fold))
        .collect();
    let mut named = BTreeMap::<String, Vec<Row>>::new();
    for row in rows {
        if ids[&row.id] != 1 {
            counts.duplicate_id += 1;
        } else {
            named.entry(fold(&row.name)).or_default().push(row);
        }
    }
    let mut tsv = std::io::BufWriter::new(
        std::fs::OpenOptions::new()
            .append(true)
            .open(out.join("places.tsv"))?,
    );
    for (key, mut group) in named {
        if known.contains(&key) {
            counts.already_named += group.len();
            continue;
        }
        group.sort_unstable_by_key(|row| row.id);
        let row = &group[0];
        // A five-kilometre radius bounds the retained name's cluster diameter to ten kilometres.
        if group.iter().any(|other| distance(row.xy, other.xy) >= 5.0) {
            counts.namesake += group.len();
            continue;
        }
        let parent = assigner
            .within(row.xy.0, row.xy.1)
            .then(|| assigner.region(row.xy.0, row.xy.1, *b"IN"))
            .flatten();
        let Some(parent) = parent.filter(|&p| !places[p as usize].region.is_empty()) else {
            counts.outside += group.len();
            continue;
        };
        let region = places[parent as usize].region.clone();
        let mut ancestors = places[parent as usize].anc.to_vec();
        ancestors.push(parent);
        ancestors.sort_unstable();
        ancestors.dedup();
        writeln!(
            tsv,
            "locality\t{CLASS}\tIN\t{region}\t0\t{}\t{}\t{}\tNaN\tNaN\tNaN\tNaN\t{parent}\t{}\t\t",
            row.population,
            row.xy.0,
            row.xy.1,
            cell(&row.name)
        )?;
        places.push(Place {
            subtype: "other",
            country: *b"IN",
            region,
            names: Str::from(row.name.as_str()),
            population: row.population.min(49),
            xy: Some(row.xy),
            anc: ancestors.into(),
            classed: true,
        });
        counts.added += 1;
        counts.clustered += group.len() - 1;
    }
    tsv.flush()?;
    std::fs::write(out.join("GEOSADAK-NOTICE.txt"), NOTICE)?;
    std::fs::write(
        out.join("geosadak-source.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "publisher": "Ministry of Rural Development, PMGSY/GeoSadak",
            "page": "https://pmgsygeosadak.dord.gov.in/OpenData",
            "mirror": URL,
            "snapshot": "2024-10-20",
            "snapshot_date_basis": "mirror release notes, not a current official export",
            "sha256": expected,
            "counts": counts,
        }))?,
    )?;
    Ok(counts.added)
}
