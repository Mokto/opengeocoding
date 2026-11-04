use super::*;
use anyhow::{Result, ensure};
use opengeocoding::index::{area, country::us};
use std::{collections::BTreeMap, io::Read};

type Government = (String, String, String, Vec<Vec<[i32; 2]>>);

fn decode(input: impl Read) -> Result<Vec<Government>> {
    let mut records: Vec<Government> =
        serde_json::from_reader(flate2::read::GzDecoder::new(input).take(256 * 1024 * 1024))?;
    ensure!(records.len() <= 100_000, "too many US government polygons");
    let mut labels = HashMap::new();
    for (id, region, name, rings) in &mut records {
        let valid_id = id.split_once('/').is_some_and(|(kind, code)| {
            matches!((kind, code.len()), ("place", 7) | ("cousub", 10))
                && code.bytes().all(|c| c.is_ascii_digit())
        });
        ensure!(
            valid_id
                && region.len() == 5
                && region.starts_with("US-")
                && region[3..].bytes().all(|c| c.is_ascii_uppercase())
                && !name.trim().is_empty()
                && !name.chars().any(char::is_control),
            "invalid US government identity/name/state"
        );
        if let Some(previous) = labels.insert(id.clone(), (region.clone(), name.clone())) {
            ensure!(
                previous == (region.clone(), name.clone()),
                "conflicting US government labels"
            );
        }
        ensure!(!rings.is_empty(), "US government polygon has no rings");
        for ring in rings {
            let mut point = [0i32; 2];
            for delta in ring.iter_mut() {
                for axis in 0..2 {
                    point[axis] = point[axis]
                        .checked_add(delta[axis])
                        .ok_or_else(|| anyhow::anyhow!("US government coordinate overflow"))?;
                }
                ensure!(
                    (-1_800_000_000..=1_800_000_000).contains(&point[0])
                        && (-900_000_000..=900_000_000).contains(&point[1]),
                    "US government coordinate out of range"
                );
                *delta = point;
            }
            ensure!(
                ring.len() >= 4 && ring.first() == ring.last(),
                "US government ring is not closed"
            );
        }
    }
    Ok(records)
}

pub fn write(out: &Path, enabled: bool, source: Option<&str>) -> Result<usize> {
    let mut provenance = serde_json::json!({"input": "disabled"});
    let records = if enabled || source.is_some() {
        let input = source.map(std::fs::read).transpose()?;
        let bytes = input
            .as_deref()
            .unwrap_or(include_bytes!("../../../data/us-municipalities.json.gz"));
        provenance = if source.is_some() {
            serde_json::json!({"input": "override", "sha256": opengeocoding::eval::sha256_hex(bytes)})
        } else {
            serde_json::from_str(include_str!("../../../data/us-municipalities.sources.json"))?
        };
        decode(bytes)?
    } else {
        vec![]
    };
    let labels: BTreeMap<_, _> = records
        .iter()
        .map(|(id, region, name, _)| (id.clone(), (region.clone(), name.clone())))
        .collect();
    let labels: Vec<_> = labels
        .into_iter()
        .map(|(id, (region, name))| (id, region, name))
        .collect();
    let ids: HashMap<_, _> = labels
        .iter()
        .enumerate()
        .map(|(i, (id, _, _))| (id.as_str(), i as u32))
        .collect();
    let areas = records
        .into_iter()
        .map(|(id, _, _, rings)| area::Area {
            place: ids[id.as_str()],
            kind: 0,
            rings,
        })
        .collect();
    area::write(&out.join(us::MUNICIPALITIES_FILE), areas)?;
    std::fs::write(
        out.join(us::MUNICIPALITIES_LABELS),
        serde_json::to_vec(&labels)?,
    )?;
    std::fs::write(
        out.join("us-municipalities-source.json"),
        serde_json::to_vec(&provenance)?,
    )?;
    std::fs::write(
        out.join("US-MUNICIPALITIES-NOTICE.txt"),
        include_str!("../../../data/us-municipalities.NOTICE"),
    )?;
    Ok(labels.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn encoded(value: serde_json::Value) -> Vec<u8> {
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(value.to_string().as_bytes()).unwrap();
        gzip.finish().unwrap()
    }

    #[test]
    fn pinned_governments_match_the_source_manifest() {
        let input = include_bytes!("../../../data/us-municipalities.json.gz");
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../data/us-municipalities.sources.json"))
                .unwrap();
        assert_eq!(
            opengeocoding::eval::sha256_hex(input),
            manifest["gzip_sha256"].as_str().unwrap()
        );
        let records = decode(&input[..]).unwrap();
        assert_eq!(
            records.len(),
            manifest["polygons"].as_u64().unwrap() as usize
        );
        assert_eq!(
            records.iter().map(|r| &r.0).collect::<HashSet<_>>().len(),
            manifest["entities"].as_u64().unwrap() as usize
        );
    }

    #[test]
    fn government_records_require_identity_consistent_labels_and_closed_bounded_rings() {
        let record = serde_json::json!([
            "place/4912345",
            "US-UT",
            "Elmwood town",
            [[[0, 0], [10, 0], [0, 10], [-10, 0], [0, -10]]]
        ]);
        assert_eq!(
            decode(&encoded(serde_json::json!([record.clone()]))[..]).unwrap()[0].3[0][2],
            [10, 10]
        );
        for field in ["place/invalid", "village/4912345"] {
            let mut bad = record.clone();
            bad[0] = field.into();
            assert!(decode(&encoded(serde_json::json!([bad]))[..]).is_err());
        }
        let mut conflict = record.clone();
        conflict[2] = "Another town".into();
        assert!(decode(&encoded(serde_json::json!([record.clone(), conflict]))[..]).is_err());
        let mut bad = record.clone();
        bad[3][0][4] = serde_json::json!([0, -9]);
        assert!(decode(&encoded(serde_json::json!([bad]))[..]).is_err());
        let mut bad = record;
        bad[3][0][0] = serde_json::json!([2_000_000_000, 0]);
        assert!(decode(&encoded(serde_json::json!([bad]))[..]).is_err());
    }
}
