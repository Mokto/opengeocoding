//! `evaluate coverage`: house numbers and streets per country in a `build_index` output, read
//! straight from its files (no service, no libpostal).

use anyhow::{Context, Result, ensure};
use rayon::prelude::*;
use std::{collections::HashMap, fs, path::Path};
use tantivy::{DocSet, schema::IndexRecordOption};
use tantivy_fst::Streamer;

// ponytail: one ratio for every country; countries of large apartment blocks (Korea, Russia) or
// large households (India) need fewer addresses per person, so their gap is overstated. A
// per-country building count from OSM would be the better denominator.
const PER_PERSON: f64 = 0.3;

#[derive(Default, Clone)]
pub struct Row {
    pub streets: u64,
    pub numbered: u64,
    pub houses: u64,
}

/// Per country code (uppercase; "" when the street has none): streets, streets with a house
/// number, house numbers.
pub fn count(dir: &Path) -> Result<HashMap<String, Row>> {
    let index = tantivy::Index::open_in_dir(dir).with_context(|| dir.display().to_string())?;
    let searcher = index.reader()?.searcher();
    let country = index.schema().get_field("country")?;
    let mut codes: Vec<String> = vec![String::new()];
    let mut by_code: HashMap<String, u16> = HashMap::from([(String::new(), 0)]);
    let mut street_country: Vec<u16> = vec![];
    for reader in searcher.segment_readers() {
        let mut doc_country = vec![0u16; reader.max_doc() as usize];
        let inverted = reader.inverted_index(country)?;
        let mut terms = inverted.terms().stream()?;
        while terms.advance() {
            let code = String::from_utf8_lossy(terms.key()).into_owned();
            let next = u16::try_from(codes.len()).context("too many country terms")?;
            let n = *by_code.entry(code.clone()).or_insert_with(|| {
                codes.push(code);
                next
            });
            let mut postings =
                inverted.read_postings_from_terminfo(terms.value(), IndexRecordOption::Basic)?;
            while postings.doc() != tantivy::TERMINATED {
                doc_country[postings.doc() as usize] = n;
                postings.advance();
            }
        }
        let fast = reader.fast_fields();
        let (id, place) = (fast.u64("id")?, fast.u64("place")?);
        for doc in 0..reader.max_doc() {
            // streets are the documents with both a place and an id
            if !place.index.has_value(doc) {
                continue;
            }
            let Some(sid) = id.first(doc) else { continue };
            let sid = sid as usize;
            if street_country.len() <= sid {
                street_country.resize(sid + 1, u16::MAX);
            }
            street_country[sid] = doc_country[doc as usize];
        }
    }
    let shards: usize = fs::read_to_string(dir.join("numbers.shards"))
        .context("numbers.shards")?
        .trim()
        .parse()?;
    ensure!(shards > 0, "numbers.shards must be positive");
    let per_shard: Vec<(Vec<u64>, Vec<u64>)> = (0..shards)
        .into_par_iter()
        .map(|s| -> Result<_> {
            let bytes = fs::read(dir.join(format!("numbers-{s:02}.fst")))?;
            let map = tantivy_fst::Map::from_bytes(bytes)?;
            let (mut houses, mut numbered) = (vec![0u64; codes.len()], vec![0u64; codes.len()]);
            let mut stream = map.stream();
            let mut last = None;
            while let Some((key, _)) = stream.next() {
                let sid = u32::from_be_bytes(key.get(..4).context("invalid house key")?.try_into()?)
                    as usize;
                let c = *street_country
                    .get(sid)
                    .filter(|&&c| c != u16::MAX)
                    .context("house of an unknown street")? as usize;
                houses[c] += 1;
                if last != Some(sid) {
                    numbered[c] += 1;
                    last = Some(sid);
                }
            }
            Ok((houses, numbered))
        })
        .collect::<Result<_>>()?;
    let mut rows: HashMap<String, Row> = HashMap::new();
    for &c in &street_country {
        if c != u16::MAX {
            rows.entry(codes[c as usize].clone()).or_default().streets += 1;
        }
    }
    for (houses, numbered) in per_shard {
        for (c, code) in codes.iter().enumerate() {
            if houses[c] > 0 {
                let row = rows.entry(code.clone()).or_default();
                row.houses += houses[c];
                row.numbered += numbered[c];
            }
        }
    }
    Ok(rows)
}

/// GeoNames `countryInfo.txt`: ISO code -> (name, population).
pub fn populations(path: &Path) -> Result<HashMap<String, (String, u64)>> {
    let text = fs::read_to_string(path).with_context(|| path.display().to_string())?;
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let f: Vec<_> = l.split('\t').collect();
            ensure!(f.len() >= 8 && f[0].len() == 2, "invalid countryInfo row");
            Ok((f[0].to_string(), (f[4].to_string(), f[7].parse()?)))
        })
        .collect::<Result<_>>()
}

/// A TSV ordered by the estimated number of missing houses, largest first.
pub fn table(rows: &HashMap<String, Row>, population: &HashMap<String, (String, u64)>) -> String {
    let mut countries: std::collections::BTreeSet<_> = rows.keys().collect();
    countries.extend(population.keys());
    let mut lines: Vec<_> = countries
        .into_iter()
        .map(|cc| {
            let r = rows.get(cc).cloned().unwrap_or_default();
            let (name, people) = population.get(cc).cloned().unwrap_or_default();
            let gap = (people as f64 * PER_PERSON - r.houses as f64).max(0.) as u64;
            (gap, cc.clone(), name, people, r)
        })
        .collect();
    lines.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(b.4.houses.cmp(&a.4.houses))
            .then(a.1.cmp(&b.1))
    });
    let mut out = String::from(
        "country\tname\tpopulation\thouses\thouses_per_1000\tstreets\tnumbered_streets\tnumbered_share\tgap\n",
    );
    for (gap, cc, name, people, r) in lines {
        let per_1000 = if people > 0 {
            format!("{:.0}", r.houses as f64 * 1000. / people as f64)
        } else {
            String::new()
        };
        let share = r.numbered as f64 / r.streets.max(1) as f64;
        out += &format!(
            "{cc}\t{name}\t{people}\t{}\t{per_1000}\t{}\t{}\t{share:.3}\t{gap}\n",
            r.houses, r.streets, r.numbered
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_final_houses_and_segment_only_streets() -> Result<()> {
        use opengeocoding::index::schema;
        let dir = std::env::temp_dir().join(format!("coverage-{}", std::process::id()));
        fs::create_dir_all(&dir)?;
        let (index, mut writer, f) = schema::create(dir.to_str().unwrap(), 1, 15_000_000);
        for (sid, cc) in [(2u64, "GB"), (7, "GB"), (10, "IN"), (12, "")] {
            writer.add_document(
                tantivy::doc!(f.kind => "s", f.id => sid, f.place => 0u64, f.country => cc),
            )?;
        }
        writer.add_document(tantivy::doc!(f.kind => "p", f.id => 1u64, f.country => "GB"))?;
        writer.add_document(tantivy::doc!(f.kind => "z", f.place => 0u64, f.country => "GB"))?;
        writer.commit()?;
        drop((writer, index));
        fs::write(dir.join("numbers.shards"), "2")?;
        for (s, keys) in [
            (0, vec![(2u32, "1"), (2, "2"), (12, "1")]),
            (1, vec![(7, "1")]),
        ] {
            let mut fst = tantivy_fst::MapBuilder::memory();
            for (sid, number) in keys {
                fst.insert(
                    [sid.to_be_bytes().as_slice(), number.as_bytes()].concat(),
                    0,
                )?;
            }
            fs::write(dir.join(format!("numbers-{s:02}.fst")), fst.into_inner()?)?;
        }
        let rows = count(&dir)?;
        assert_eq!(
            (rows["GB"].houses, rows["GB"].streets, rows["GB"].numbered),
            (3, 2, 2)
        );
        assert_eq!(
            (rows["IN"].houses, rows["IN"].streets, rows["IN"].numbered),
            (0, 1, 0)
        );
        assert_eq!(rows[""].houses, 1);
        assert_eq!(rows.len(), 3);
        fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn the_largest_gap_comes_first() {
        let rows = HashMap::from([
            (
                "FR".to_string(),
                Row {
                    streets: 10,
                    numbered: 9,
                    houses: 30,
                },
            ),
            (
                "IN".to_string(),
                Row {
                    streets: 10,
                    numbered: 1,
                    houses: 2,
                },
            ),
        ]);
        let people = HashMap::from([
            ("FR".to_string(), ("France".to_string(), 100)),
            ("IN".to_string(), ("India".to_string(), 1000)),
            ("ZZ".to_string(), ("No streets".to_string(), 10)),
        ]);
        let t = table(&rows, &people);
        let lines: Vec<_> = t.lines().collect();
        assert_eq!(lines[1], "IN\tIndia\t1000\t2\t2\t10\t1\t0.100\t298");
        assert_eq!(lines[2], "ZZ\tNo streets\t10\t0\t0\t0\t0\t0.000\t3");
        assert_eq!(lines[3], "FR\tFrance\t100\t30\t300\t10\t9\t0.900\t0");
    }
}
