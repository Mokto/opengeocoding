use super::*;
use anyhow::{Result, ensure};
use opengeocoding::index::{area, country::dk};
use std::io::Read;

type Settlement = (u32, String, Vec<Vec<[i32; 2]>>);

fn contains(rings: &[Vec<[i32; 2]>], x: f64, y: f64) -> bool {
    let (x, y) = (x * 1e7, y * 1e7);
    let mut inside = false;
    for ring in rings {
        for edge in ring.windows(2) {
            let [ax, ay] = edge[0].map(f64::from);
            let [bx, by] = edge[1].map(f64::from);
            if (ay > y) != (by > y) && x < ax + (y - ay) * (bx - ax) / (by - ay) {
                inside = !inside;
            }
        }
    }
    inside
}

fn decode(input: impl Read) -> Result<Vec<Settlement>> {
    let mut records: Vec<Settlement> =
        serde_json::from_reader(flate2::read::GzDecoder::new(input).take(256 * 1024 * 1024))?;
    ensure!(
        records.len() <= 20_000,
        "too many Danish settlement polygons"
    );
    let mut names = HashMap::new();
    for (code, name, rings) in &mut records {
        ensure!(
            *code > 0 && !name.trim().is_empty() && !name.chars().any(char::is_control),
            "invalid Danish settlement name/code"
        );
        if let Some(previous) = names.insert(*code, name.clone()) {
            ensure!(previous == *name, "conflicting Danish settlement names");
        }
        ensure!(!rings.is_empty(), "Danish settlement has no rings");
        for ring in rings {
            let mut point = [0i32; 2];
            for delta in ring.iter_mut() {
                for axis in 0..2 {
                    point[axis] = point[axis]
                        .checked_add(delta[axis])
                        .ok_or_else(|| anyhow::anyhow!("Danish coordinate overflow"))?;
                }
                ensure!(
                    (75_000_000..=160_000_000).contains(&point[0])
                        && (540_000_000..=585_000_000).contains(&point[1]),
                    "Danish polygon outside Denmark"
                );
                *delta = point;
            }
            ensure!(
                ring.len() >= 4 && ring.first() == ring.last(),
                "Danish polygon ring is not closed"
            );
        }
    }
    Ok(records)
}

pub fn write(out: &Path, places: &[Place], source: Option<&str>) -> Result<usize> {
    let input = source.map(std::fs::read).transpose()?;
    let input = input
        .as_deref()
        .unwrap_or(include_bytes!("../../../data/dk-localities.json.gz"));
    let records = decode(input)?;
    let mut named: HashMap<String, Vec<u32>> = HashMap::new();
    for (id, place) in places.iter().enumerate() {
        if place.country == *b"DK" && place.subtype == "locality" {
            let name = place.names.split(NAME_SEP).next().unwrap_or("");
            named.entry(fold(name)).or_default().push(id as u32);
        }
    }
    let mut candidates: HashMap<u32, HashSet<u32>> = HashMap::new();
    for (code, name, rings) in &records {
        for &id in named.get(&fold(name)).into_iter().flatten() {
            if places[id as usize]
                .xy
                .is_some_and(|(x, y)| contains(rings, x, y))
            {
                candidates.entry(*code).or_default().insert(id);
            }
        }
    }
    let areas: Vec<_> = records
        .into_iter()
        .filter_map(|(code, _, rings)| {
            let candidates = candidates.get(&code)?;
            (candidates.len() == 1).then(|| area::Area {
                place: *candidates.iter().next().unwrap(),
                kind: 0,
                rings,
            })
        })
        .collect();
    let count = areas.len();
    area::write(&out.join(dk::LOCALITIES_FILE), areas)?;
    std::fs::write(
        out.join("DENMARK-LOCALITIES-NOTICE.txt"),
        include_str!("../../../data/dk-localities.NOTICE"),
    )?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn encoded(value: serde_json::Value) -> Vec<u8> {
        let mut writer = flate2::write::GzEncoder::new(vec![], flate2::Compression::default());
        writer
            .write_all(&serde_json::to_vec(&value).unwrap())
            .unwrap();
        writer.finish().unwrap()
    }

    #[test]
    fn crosswalk_requires_one_same_named_locality_inside_the_polygon() {
        let root = std::env::temp_dir().join(format!("danish-polygons-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("source.json.gz");
        std::fs::write(
            &source,
            encoded(serde_json::json!([[
                1,
                "Testby",
                [[
                    [110000000, 570000000],
                    [1000000, 0],
                    [0, 1000000],
                    [-1000000, 0],
                    [0, -1000000]
                ]]
            ]])),
        )
        .unwrap();
        let place = |country, name: &str, xy| Place {
            subtype: "locality",
            country,
            region: "DK-81".into(),
            names: name.into(),
            population: 20_000,
            xy: Some(xy),
            anc: Box::default(),
            classed: true,
        };
        assert_eq!(
            write(
                &root,
                &[place(*b"DK", "Testby", (11.05, 57.05))],
                source.to_str()
            )
            .unwrap(),
            1
        );
        for places in [
            vec![place(*b"DK", "Testby", (11.2, 57.05))],
            vec![place(*b"DK", "Otherby", (11.05, 57.05))],
            vec![place(*b"SE", "Testby", (11.05, 57.05))],
            vec![
                place(*b"DK", "Testby", (11.04, 57.05)),
                place(*b"DK", "Testby", (11.06, 57.05)),
            ],
        ] {
            assert_eq!(write(&root, &places, source.to_str()).unwrap(), 0);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pinned_polygons_and_malformed_inputs_are_checked() {
        let records = decode(&include_bytes!("../../../data/dk-localities.json.gz")[..]).unwrap();
        assert_eq!(records.len(), 8006);
        assert_eq!(
            opengeocoding::eval::sha256_hex(include_bytes!("../../../data/dk-localities.json.gz")),
            "bf8bc9b3bd82f213b0764718aa28d13c17b7309770277174e5dd3f4b994e3912"
        );
        for value in [
            serde_json::json!([[0, "Town", []]]),
            serde_json::json!([[1, "Town", [[[0, 0], [1, 1], [0, 0], [0, 0]]]]]),
            serde_json::json!([[
                1,
                "Town",
                [[[110000000, 570000000], [1, 0], [0, 1], [0, 0]]]
            ]]),
            serde_json::json!([[
                1,
                "Town",
                [[[110000000, 570000000], [i32::MAX, 0], [0, 1], [-1, -1]]]
            ]]),
        ] {
            assert!(decode(encoded(value).as_slice()).is_err());
        }
        let ring = vec![
            [110000000, 570000000],
            [111000000, 570000000],
            [111000000, 571000000],
            [110000000, 571000000],
            [110000000, 570000000],
        ];
        let hole = vec![
            [110400000, 570400000],
            [110600000, 570400000],
            [110600000, 570600000],
            [110400000, 570600000],
            [110400000, 570400000],
        ];
        assert!(contains(std::slice::from_ref(&ring), 11.05, 57.05));
        assert!(!contains(&[ring.clone(), hole], 11.05, 57.05));
        assert!(!contains(&[ring], 11.2, 57.05));
    }
}
