use crate::model::Case;
use anyhow::{Context, Result, ensure};
use opengeocoding::index::{reverse, schema};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
use tantivy::{DocAddress, TantivyDocument, directory::Directory, schema::Value as _};
use tantivy_fst::{IntoStreamer, Streamer};

pub fn inspect(dir: &Path, cases: &[Case]) -> Result<Vec<Value>> {
    ensure!(
        cases
            .iter()
            .all(|c| !c.holdout && !opengeocoding::eval::holdout(&c.location)),
        "inspection is training-only"
    );
    let mmap = tantivy::directory::MmapDirectory::open(dir)?;
    let read = |name: &str| {
        mmap.open_read(Path::new(name))?
            .read_bytes()
            .map_err(anyhow::Error::from)
    };
    let streets = reverse::Streets::new(read(reverse::STREETS)?)?;
    let houses = reverse::Reverse::new(read(reverse::FILE)?)?;
    let shards: usize = std::fs::read_to_string(dir.join("numbers.shards"))?
        .trim()
        .parse()?;
    ensure!(shards > 0, "numbers.shards must be positive");
    let numbers = (0..shards)
        .map(|s| {
            Ok(tantivy_fst::Map::from(tantivy_fst::raw::Fst::new(read(
                &format!("numbers-{s:02}.fst"),
            )?)?))
        })
        .collect::<Result<Vec<_>>>()?;
    let index = tantivy::Index::open_in_dir(dir)?;
    let searcher = index.reader()?.searcher();
    let fields = index.schema();
    let name = fields.get_field("name")?;
    let pcode = fields.get_field("pcode")?;
    let mut result = vec![];
    for case in cases {
        let expect = case
            .expect
            .as_ref()
            .context("inspection needs an expectation")?;
        let [lat, lon] = expect.point.context("inspection needs a point")?;
        ensure!(
            lat.is_finite() && lon.is_finite() && lat.abs() < 85. && lon.abs() <= 180.,
            "unsupported inspection point"
        );
        let radius = 1000.;
        let mut docs = BTreeMap::<u32, Option<f64>>::new();
        let (cx, cy) = reverse::street_cell_at(lon, lat);
        let cos = lat.to_radians().cos();
        ensure!(
            lon.abs() + radius / (111_195. * cos) < 180.,
            "inspection window crosses the antimeridian"
        );
        let rx =
            (radius / (reverse::STREET_CELL_LON_M * cos) + reverse::STREET_SLACK).ceil() as i64 + 1;
        let ry = (radius / reverse::STREET_CELL_LAT_M + reverse::STREET_SLACK).ceil() as i64 + 1;
        let p = reverse::steps(lon, lat);
        let relative = |q: (f64, f64)| ((q.0 - p.0) * 2. * cos, q.1 - p.1);
        let mut edges = vec![];
        for x in cx - rx..=cx + rx {
            for y in cy - ry..=cy + ry {
                streets.edges(x, y, &mut edges);
            }
        }
        for (a, b, doc) in edges {
            let distance =
                reverse::nearest((0., 0.), relative(a), relative(b)).0 * reverse::STEP_LAT_M;
            if distance <= radius {
                let old = docs.entry(doc).or_default();
                *old = Some(old.unwrap_or(f64::INFINITY).min(distance));
            }
        }
        let (cx, cy) = reverse::cell_at(lon, lat);
        let rx = (radius / (reverse::CELL_LON_M * cos)).ceil() as i64 + 1;
        let ry = (radius / reverse::CELL_LAT_M).ceil() as i64 + 1;
        let mut numbered = vec![];
        for x in cx - rx..=cx + rx {
            for y in cy - ry..=cy + ry {
                houses.docs(x, y, &mut numbered);
            }
        }
        for doc in numbered {
            docs.entry(doc).or_default();
        }
        let mut found = vec![];
        for (packed, distance) in docs {
            let (segment_ord, doc_id) = reverse::unpack_doc(packed);
            let reader = searcher.segment_reader(segment_ord);
            let sid = reader
                .fast_fields()
                .u64("id")?
                .first(doc_id)
                .context("street without id")? as u32;
            let place = reader.fast_fields().u64("place")?.first(doc_id);
            let doc = searcher.doc::<TantivyDocument>(DocAddress::new(segment_ord, doc_id))?;
            let text = |field| {
                doc.get_all(field)
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("\u{1f}")
            };
            let mut range = numbers[sid as usize % shards].range().ge(sid.to_be_bytes());
            if let Some(next) = sid.checked_add(1) {
                range = range.lt(next.to_be_bytes());
            }
            let mut stream = range.into_stream();
            let mut keys = vec![];
            let mut nearby = false;
            while let Some((key, value)) = stream.next() {
                let (x, y, slot) = schema::unpack(value);
                let metres = ((x - lon) * cos).hypot(y - lat) * 111_195.;
                nearby |= metres <= radius;
                keys.push(json!({"number": std::str::from_utf8(&key[4..])?, "point": [y, x], "distance_m": metres, "postcode_slot": slot}));
            }
            if distance.is_some() || nearby {
                found.push(json!({"segment": segment_ord, "doc": doc_id, "sid": sid, "place": place, "name": text(name), "postcodes": text(pcode), "line_distance_m": distance, "houses": keys}));
            }
        }
        result.push(
            json!({"id": case.id, "point": [lat, lon], "radius_m": radius, "streets": found}),
        );
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_beyond_ranked_probes_and_a_streets_centroid() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("inspect-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let (index, mut writer, f) = schema::create(dir.to_str().unwrap(), 1, 15_000_000);
        for sid in 0..12u64 {
            writer.add_document(tantivy::doc!(
                f.kind => "s", f.id => sid, f.place => 0u64,
                f.name => format!("Authored Road {sid}"),
                f.x => 40f64, f.y => 10f64
            ))?;
        }
        writer.commit()?;
        drop(writer);
        let mut fst = tantivy_fst::MapBuilder::memory();
        let mut posts = vec![];
        for sid in 0..12u32 {
            let (x, y) = schema::pack(30. + sid as f64 * 0.0001, 10., 0);
            fst.insert(
                [sid.to_be_bytes().as_slice(), b"123"].concat(),
                (x as u64) << 32 | y as u64,
            )?;
            reverse::street_postings(&mut vec![reverse::key(x, y)], sid, &mut posts);
        }
        let (x, y) = schema::pack(30.03, 10., 0);
        fst.insert(
            [11u32.to_be_bytes().as_slice(), b"999"].concat(),
            (x as u64) << 32 | y as u64,
        )?;
        std::fs::write(dir.join("numbers-00.fst"), fst.into_inner()?)?;
        std::fs::write(dir.join("numbers.shards"), "1")?;
        reverse::write_spill(&dir.join(reverse::SPILL), 0, posts);
        let spill = reverse::StreetSpill::new(&dir.join(reverse::STREETS_SPILL));
        let mut edges = vec![];
        reverse::street_edges(&[[30., 10.], [30.02, 10.]], &mut edges);
        spill.add(&mut edges.into_iter().map(|(k, d)| (k, d, 11)).collect());
        spill.finish();
        reverse::build(&dir, &index, f.kind, Some)?;
        drop(index);
        let case: Case = serde_json::from_value(json!({
            "id": "authored", "query": "123 Authored Road 11", "cohort": "synthetic",
            "location": "synthetic", "expect": {"point": [10., 30.], "country": ["gb"]}
        }))?;
        let rows = inspect(&dir, std::slice::from_ref(&case))?;
        let streets = rows[0]["streets"].as_array().unwrap();
        assert_eq!(streets.len(), 12);
        let target = streets.iter().find(|s| s["sid"] == 11).unwrap();
        assert!(target["line_distance_m"].as_f64().unwrap() < 5.);
        assert_eq!(target["houses"][0]["number"], "123");
        assert_eq!(target["houses"][1]["number"], "999");
        assert!(target["houses"][1]["distance_m"].as_f64().unwrap() > 1000.);
        let mut dateline = case;
        dateline.expect.as_mut().unwrap().point = Some([0., 179.999]);
        assert!(
            inspect(&dir, &[dateline])
                .unwrap_err()
                .to_string()
                .contains("antimeridian")
        );
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn rejects_held_out_cases_before_opening_the_index() {
        for (holdout, location) in [(true, "z"), (false, "abc")] {
            let case = Case {
                holdout,
                location: location.into(),
                ..Default::default()
            };
            assert!(
                inspect(Path::new("does-not-exist"), &[case])
                    .unwrap_err()
                    .to_string()
                    .contains("training-only")
            );
        }
    }
}
