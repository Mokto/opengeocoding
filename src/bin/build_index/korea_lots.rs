use super::{LAST_RANK, Place, Shared, Sink, fold};
use anyhow::{Context, Result, ensure};
use opengeocoding::index::country::kr;
use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, BufWriter, Cursor, Write},
};

pub type Pending = HashMap<String, HashMap<String, Vec<Row>>>;

pub struct Row {
    address: String,
    x: f64,
    y: f64,
}

pub fn read(source: Option<&str>) -> Result<(Pending, serde_json::Value)> {
    if source == Some("") {
        return Ok((Pending::new(), serde_json::json!({"disabled": true})));
    }
    let input: Box<dyn std::io::Read> = match source {
        Some(path) if path.ends_with(".gz") => {
            Box::new(flate2::read::GzDecoder::new(std::fs::File::open(path)?))
        }
        Some(path) => Box::new(std::fs::File::open(path)?),
        None => Box::new(flate2::read::GzDecoder::new(Cursor::new(include_bytes!(
            "../../../data/kr-localdata-lots.tsv.gz"
        )))),
    };
    let mut lines = BufReader::new(input).lines();
    ensure!(
        lines.next().transpose()?.as_deref() == Some("lot_address\tlon\tlat"),
        "Korean lot input needs lot_address, lon, lat TSV columns"
    );
    let mut pending = Pending::new();
    let mut seen = HashSet::new();
    for (i, line) in lines.enumerate() {
        let line = line?;
        let columns: Vec<_> = line.split('\t').collect();
        let [address, x, y] = columns.as_slice() else {
            anyhow::bail!("Korean lot line {} needs three columns", i + 2)
        };
        let (x, y): (f64, f64) = (
            x.parse().context("Korean lot longitude")?,
            y.parse().context("Korean lot latitude")?,
        );
        ensure!(
            x.is_finite()
                && y.is_finite()
                && (-180. ..=180.).contains(&x)
                && (-90. ..=90.).contains(&y),
            "invalid Korean lot coordinate on line {}",
            i + 2
        );
        let (street, number) = kr::listed_lot(address, |_| true)
            .with_context(|| format!("invalid Korean lot address on line {}", i + 2))?;
        ensure!(
            seen.insert((address.to_string(), x.to_bits(), y.to_bits())),
            "duplicate Korean lot row"
        );
        pending
            .entry(fold(street))
            .or_default()
            .entry(number)
            .or_default()
            .push(Row {
                address: address.to_string(),
                x,
                y,
            });
    }
    let metadata = match source {
        None => serde_json::from_str(include_str!("../../../data/kr-localdata-lots.json"))?,
        Some(path) => {
            serde_json::json!({"override_sha256": super::store::sha256(std::path::Path::new(path))?, "prepared_rows": seen.len()})
        }
    };
    Ok((pending, metadata))
}

pub fn add(
    shared: &Shared,
    places: &[Place],
    segments: &[super::SegShard],
    out: &std::path::Path,
    mut metadata: serde_json::Value,
) -> Result<usize> {
    let pending = std::mem::take(&mut *shared.korea_lots.lock().unwrap());
    let mut rows: Vec<_> = pending
        .into_values()
        .flat_map(HashMap::into_values)
        .flatten()
        .collect();
    rows.sort_unstable_by(|a, b| {
        a.address
            .cmp(&b.address)
            .then(a.x.total_cmp(&b.x))
            .then(a.y.total_cmp(&b.y))
    });
    let remaining = rows.len();
    let mut audit = BufWriter::new(std::fs::File::create(out.join("korea-lots-admitted.tsv"))?);
    writeln!(audit, "lot_address\tlon\tlat")?;
    let mut sink = Sink::new(shared).ranked(LAST_RANK);
    sink.street_priority = 0;
    sink.lot = true;
    let mut added = 0;
    let mut mixed = 0;
    let mut mapped = 0;
    let mut administration = 0;
    for row in rows {
        if !shared.assigner.within(row.x, row.y) {
            continue;
        }
        let Some(p) = shared.assigner.place(row.x, row.y) else {
            continue;
        };
        if shared.countries[p as usize] != *b"KR" {
            continue;
        }
        if shared
            .assigner
            .admin(row.x, row.y)
            .is_none_or(|admin| admin != p && !places[p as usize].anc.contains(&admin))
        {
            // A locality polygon can cross its parent's administrative boundary.
            administration += 1;
            continue;
        }
        let Some((street, number)) = kr::listed_lot(&row.address, |area| {
            std::iter::once(&p)
                .chain(places[p as usize].anc.iter())
                .any(|id| {
                    places[*id as usize]
                        .names
                        .split(super::NAME_SEP)
                        .any(|name| name == area)
                })
        }) else {
            continue;
        };
        let mut cell = String::new();
        let folded = fold(street);
        let key = super::with_cell(&folded, row.x, row.y, &mut cell);
        if super::find_seg(segments, p, key).is_some() {
            mapped += 1;
            continue;
        }
        let h = super::hash(&(*b"KR", p, key));
        let shard = shared.streets[h as usize % super::SHARDS].lock().unwrap();
        if let Some(&sid) = shard
            .table
            .find(super::mix(h), |&id| shard.key(id) == (*b"KR", p, key))
            && !shard.streets[sid as usize].lot
        {
            // Reverse-house membership is per street, so these lots cannot share a non-lot street.
            mixed += 1;
            continue;
        }
        drop(shard);
        sink.add(
            Some(*b"KR"),
            row.x,
            row.y,
            Some(street),
            Some(&number),
            None,
            &[],
        );
        writeln!(audit, "{}\t{:.9}\t{:.9}", row.address, row.x, row.y)?;
        added += 1;
    }
    audit.flush()?;
    metadata["remaining_after_existing_pairs"] = remaining.into();
    metadata["mixed_streets_rejected"] = mixed.into();
    metadata["mapped_roads_rejected"] = mapped.into();
    metadata["administration_guard_rejected"] = administration.into();
    metadata["geographically_corroborated_rows"] = added.into();
    std::fs::write(
        out.join("korea-lots-source.json"),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    std::fs::write(
        out.join("KOREA-LOTS-NOTICE.txt"),
        include_str!("../../../data/kr-localdata-lots.NOTICE"),
    )?;
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_capture_is_a_whole_source_frame_not_benchmark_cases() {
        let (pending, metadata) = read(None).unwrap();
        assert_eq!(
            pending
                .values()
                .flat_map(HashMap::values)
                .map(Vec::len)
                .sum::<usize>(),
            289082
        );
        assert_eq!(
            metadata["source_sha256"],
            "06634a6fc9e4cd626642b179441524c3728df5e7ad5a71ebd5e492f8850996b0"
        );
        assert_eq!(metadata["raw_counts"]["active_rows"], 674252);
        assert_eq!(metadata["prepared_rows"], 289082);
    }

    #[test]
    fn malformed_prepared_rows_fail_before_geographic_filtering() {
        let file = std::env::temp_dir().join(format!("kr-lots-input-{}.tsv", std::process::id()));
        for row in [
            "새동 1\tNaN\t35",
            "새동 1\t181\t35",
            "새동 01\t127\t35",
            "새동 1 2층\t127\t35",
            "새로 1\t127\t35",
            "새동 1\t127\t35\textra",
            "새동 1\t127\t35\n새동 1\t127\t35",
        ] {
            std::fs::write(&file, format!("lot_address\tlon\tlat\n{row}\n")).unwrap();
            assert!(read(file.to_str()).is_err(), "{row}");
        }
        std::fs::remove_file(file).unwrap();
        assert!(read(Some("")).unwrap().0.is_empty());
    }
}
