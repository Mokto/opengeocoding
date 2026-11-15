//! Reads the Census TIGER/Line address features (`tl_<year>_<county>_addrfeat.zip`, one per
//! county, public domain) into `us-ranges.fst` and `us-ranges.bin` (see `index::ranges`).
//!
//! Counties are read in parallel straight from their zips (the `.shp` and `.dbf` entries are
//! parsed in memory, nothing is unpacked), each edge is simplified to 3 m and encoded at once, and
//! only a 16-byte entry per segment is kept for sorting; the encoded segments stay in one arena.
//! Segments are sorted by key then by their bytes, so the output does not depend on the order the
//! counties finish in.
use super::*;
use anyhow::{Context, Result, ensure};
use opengeocoding::index::{
    ranges::{self, Side},
    schema::fold,
};
use std::{
    io::{Read, Write},
    sync::Mutex,
};

/// Douglas-Peucker tolerance in degrees (about 3 m): well inside the range's own error.
const TOLERANCE: f64 = 3e-5;

#[derive(Default)]
struct Pool {
    names: HashMap<String, u32>,
    /// (ZIP << 32 | name id, arena offset << 16 | length)
    entries: Vec<(u64, u64)>,
    arena: Vec<u8>,
    edges: u64,
}

struct County {
    names: Vec<String>,
    /// (ZIP << 32 | local name id, local arena offset << 16 | length)
    entries: Vec<(u64, u64)>,
    arena: Vec<u8>,
    edges: u64,
}

fn u32_le(b: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(at..at + 4).context("truncated")?.try_into()?,
    ))
}

/// The rows of a dBASE file as the values of the `wanted` columns, in that order.
fn dbf_rows<'a, const N: usize>(
    dbf: &'a [u8],
    wanted: [&str; N],
) -> Result<impl Iterator<Item = [&'a [u8]; N]>> {
    let rows = u32_le(dbf, 4)? as usize;
    let header = u16::from_le_bytes(dbf.get(8..10).context("truncated")?.try_into()?) as usize;
    let width = u16::from_le_bytes(dbf.get(10..12).context("truncated")?.try_into()?) as usize;
    let mut columns = [None; N];
    let (mut at, mut offset) = (32, 1);
    while dbf.get(at).is_some_and(|b| *b != 0x0d) {
        let field = dbf.get(at..at + 32).context("truncated dbf header")?;
        let name = field[..11].split(|b| *b == 0).next().unwrap_or_default();
        let len = field[16] as usize;
        if let Some(i) = wanted.iter().position(|w| w.as_bytes() == name) {
            columns[i] = Some((offset, len));
        }
        offset += len;
        at += 32;
    }
    ensure!(
        offset <= width && header + rows * width <= dbf.len(),
        "inconsistent dbf"
    );
    let mut found = [(0, 0); N];
    for (i, c) in columns.into_iter().enumerate() {
        found[i] = c.with_context(|| format!("no {} column", wanted[i]))?;
    }
    Ok((0..rows).map(move |r| {
        let row = &dbf[header + r * width..header + (r + 1) * width];
        found.map(|(o, l)| row[o..o + l].trim_ascii())
    }))
}

/// The polyline records of a shapefile in file order, one at a time into a reused buffer.
struct Lines<'a> {
    shp: &'a [u8],
    at: usize,
}

impl<'a> Lines<'a> {
    fn new(shp: &'a [u8]) -> Result<Self> {
        ensure!(shp.get(..4) == Some(&[0, 0, 0x27, 0x0a]), "not a shapefile");
        Ok(Self { shp, at: 100 })
    }

    /// Fills `line` with the next record's (lon, lat) vertices: `Ok(true)`, or `Ok(false)` for a
    /// record that isn't a one-part polyline (`line` is then empty).
    fn next(&mut self, line: &mut Vec<[f64; 2]>) -> Result<bool> {
        let shp = self.shp;
        ensure!(self.at + 8 <= shp.len(), "fewer shapes than rows");
        let len = u32::from_be_bytes(shp[self.at + 4..self.at + 8].try_into()?) as usize * 2;
        let body = self.at + 8;
        ensure!(body + len <= shp.len(), "truncated shapefile record");
        self.at = body + len;
        line.clear();
        if len < 48 || u32_le(shp, body)? != 3 || u32_le(shp, body + 36)? != 1 {
            return Ok(false);
        }
        let n = u32_le(shp, body + 40)? as usize;
        ensure!(48 + n * 16 <= len, "bad polyline");
        let f64_at = |at: usize| f64::from_le_bytes(shp[at..at + 8].try_into().unwrap());
        line.extend((0..n).map(|i| {
            let at = body + 48 + i * 16;
            [f64_at(at), f64_at(at + 8)]
        }));
        Ok(true)
    }
}

/// Douglas-Peucker on an open line, in degrees.
fn simplify(line: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let n = line.len();
    if n < 3 {
        return line.to_vec();
    }
    let mut keep = vec![false; n];
    (keep[0], keep[n - 1]) = (true, true);
    let mut todo = vec![(0, n - 1)];
    while let Some((a, b)) = todo.pop() {
        let ([ax, ay], [bx, by]) = (line[a], line[b]);
        let (dx, dy) = (bx - ax, by - ay);
        let len2 = dx * dx + dy * dy;
        let (mut far, mut at) = (TOLERANCE * TOLERANCE, None);
        for (i, &[px, py]) in line.iter().enumerate().take(b).skip(a + 1) {
            let t = if len2 > 0.0 {
                (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (ex, ey) = (ax + t * dx - px, ay + t * dy - py);
            if ex * ex + ey * ey > far {
                (far, at) = (ex * ex + ey * ey, Some(i));
            }
        }
        if let Some(i) = at {
            keep[i] = true;
            todo.extend([(a, i), (i, b)]);
        }
    }
    line.iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
}

/// A number written as the Census writes it: digits, perhaps with a letter suffix. A hyphenated
/// number (Queens' "89-12") is not a position on a range.
fn number(text: &[u8]) -> Option<u32> {
    let digits = text.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 || digits > 8 || !text[digits..].iter().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    std::str::from_utf8(&text[..digits]).ok()?.parse().ok()
}

fn side(row: &[&[u8]; 9], at: usize) -> Option<(u32, Side)> {
    let (from, to, zip, parity) = (
        number(row[at])?,
        number(row[at + 1])?,
        row[at + 2],
        row[at + 3],
    );
    if zip.len() != 5 || !zip.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some((
        std::str::from_utf8(zip).ok()?.parse().ok()?,
        Side {
            from,
            to,
            parity: match parity {
                b"O" => 1,
                b"E" => 2,
                _ => 0,
            },
        },
    ))
}

fn county(zip: &Path) -> Result<County> {
    let entries = crate::sources::zip_entries(zip)?;
    let read = |suffix: &str| -> Result<Vec<u8>> {
        let entry = (entries.iter())
            .find(|e| e.name.ends_with(suffix.as_bytes()))
            .with_context(|| format!("{} holds no {suffix}", zip.display()))?;
        let mut bytes = vec![];
        entry.open(zip)?.take(512 << 20).read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    let table = read(".dbf")?;
    let shapes = read(".shp")?;
    let rows = dbf_rows(
        &table,
        [
            "FULLNAME", "LFROMHN", "LTOHN", "ZIPL", "PARITYL", "RFROMHN", "RTOHN", "ZIPR",
            "PARITYR",
        ],
    )?;
    let (mut lines, mut line) = (Lines::new(&shapes)?, vec![]);
    let mut out = County {
        names: vec![],
        entries: vec![],
        arena: vec![],
        edges: 0,
    };
    let (mut ids, mut folded) = (
        HashMap::<&[u8], Option<u32>>::new(),
        HashMap::<String, u32>::new(),
    );
    for row in rows {
        if !lines.next(&mut line)? {
            continue;
        }
        let (left, right) = (side(&row, 1), side(&row, 5));
        if line.len() < 2 || left.is_none() && right.is_none() {
            continue;
        }
        // a street's segments repeat its name: fold each spelling once
        let id = match ids.get(row[0]) {
            Some(id) => *id,
            None => {
                let name = fold(&String::from_utf8_lossy(row[0]));
                let id = if name.is_empty() {
                    None
                } else {
                    Some(*folded.entry(name.clone()).or_insert_with(|| {
                        out.names.push(name);
                        out.names.len() as u32 - 1
                    }))
                };
                ids.insert(row[0], id);
                id
            }
        };
        let Some(id) = id else { continue };
        let mut points: Vec<[i32; 2]> = simplify(&line)
            .iter()
            .map(|p| [ranges::fixed(p[0]), ranges::fixed(p[1])])
            .collect();
        points.dedup();
        if points.len() < 2 {
            continue;
        }
        out.edges += 1;
        // both sides share one segment when they share a ZIP code
        let groups = match (left, right) {
            (Some((a, l)), Some((b, r))) if a == b => vec![(a, [Some(l), Some(r)])],
            (l, r) => [(l, 0), (r, 1)]
                .into_iter()
                .filter_map(|(s, i)| {
                    s.map(|(z, s)| {
                        let mut sides = [None; 2];
                        sides[i] = Some(s);
                        (z, sides)
                    })
                })
                .collect(),
        };
        for (zip, sides) in groups {
            let start = out.arena.len();
            ranges::encode(&sides, &points, &mut out.arena);
            let len = out.arena.len() - start;
            if len > 0xffff {
                out.arena.truncate(start);
                continue;
            }
            out.entries.push((
                u64::from(zip) << 32 | u64::from(id),
                (start as u64) << 16 | len as u64,
            ));
        }
    }
    Ok(out)
}

fn add(pool: &mut Pool, county: County) {
    let ids: Vec<u32> = county
        .names
        .into_iter()
        .map(|name| {
            let next = pool.names.len() as u32;
            *pool.names.entry(name).or_insert(next)
        })
        .collect();
    let base = pool.arena.len() as u64;
    pool.entries
        .extend(county.entries.into_iter().map(|(key, at)| {
            let id = ids[(key & 0xffff_ffff) as usize];
            (key & !0xffff_ffff | u64::from(id), at + (base << 16))
        }));
    pool.arena.extend(county.arena);
    pool.edges += county.edges;
}

/// `build_index us-ranges ZIP_DIR INDEX_DIR`: adds the range files to a built index, which needs no
/// other part rebuilt.
pub fn command(args: &[String]) -> Result<()> {
    let (Some(zips), Some(index)) = (args.get(2), args.get(3)) else {
        anyhow::bail!("usage: build_index us-ranges ZIP_DIR INDEX_DIR");
    };
    let start = std::time::Instant::now();
    let (segments, counties) = write(Path::new(index), Path::new(zips))?;
    ensure!(counties > 0, "no *_addrfeat.zip files in {zips}");
    eprintln!(
        "{segments} segments from {counties} counties in {:.1}s",
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

/// Writes the range files from the `*_addrfeat.zip` files in `dir`; returns the number of
/// segments and counties. Writes nothing without any.
pub fn write(out: &Path, dir: &Path) -> Result<(usize, usize)> {
    use rayon::prelude::*;
    let mut zips: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| dir.display().to_string())?
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.to_string_lossy().ends_with("_addrfeat.zip"))
        .collect();
    zips.sort();
    if zips.is_empty() {
        return Ok((0, 0));
    }
    let pool = Mutex::new(Pool::default());
    zips.par_iter().try_for_each(|zip| -> Result<()> {
        let county = county(zip).with_context(|| zip.display().to_string())?;
        add(&mut pool.lock().unwrap(), county);
        Ok(())
    })?;
    let Pool {
        names,
        mut entries,
        arena,
        ..
    } = pool.into_inner().unwrap();
    let mut by_name: Vec<(&str, u32)> = names.iter().map(|(n, i)| (n.as_str(), *i)).collect();
    by_name.sort_unstable();
    let mut rank = vec![0u32; by_name.len()];
    for (r, (_, id)) in by_name.iter().enumerate() {
        rank[*id as usize] = r as u32;
    }
    for (key, _) in &mut entries {
        *key = *key & !0xffff_ffff | u64::from(rank[(*key & 0xffff_ffff) as usize]);
    }
    let bytes = |at: u64| &arena[(at >> 16) as usize..((at >> 16) + (at & 0xffff)) as usize];
    entries.par_sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| bytes(a.1).cmp(bytes(b.1))));
    entries.dedup_by(|b, a| a.0 == b.0 && bytes(a.1) == bytes(b.1));

    let mut blob = std::io::BufWriter::new(std::fs::File::create(out.join(ranges::BLOB_FILE))?);
    let mut fst = tantivy_fst::MapBuilder::new(std::io::BufWriter::new(std::fs::File::create(
        out.join(ranges::FST_FILE),
    )?))?;
    let (mut pos, mut head) = (0u64, 0);
    while head < entries.len() {
        let key = entries[head].0;
        let tail = head + entries[head..].iter().take_while(|e| e.0 == key).count();
        let (zip, name) = (key >> 32, by_name[(key & 0xffff_ffff) as usize].0);
        fst.insert(format!("{zip:05}\0{name}"), pos)?;
        let mut count = vec![];
        let mut n = (tail - head) as u32;
        while n >= 0x80 {
            count.push(n as u8 | 0x80);
            n >>= 7;
        }
        count.push(n as u8);
        blob.write_all(&count)?;
        pos += count.len() as u64;
        for e in &entries[head..tail] {
            blob.write_all(bytes(e.1))?;
            pos += bytes(e.1).len() as u64;
        }
        head = tail;
    }
    blob.into_inner()?.sync_all()?;
    fst.into_inner()?.into_inner()?.sync_all()?;
    std::fs::write(
        out.join("US-RANGES-NOTICE.txt"),
        "US address ranges: U.S. Census Bureau, TIGER/Line Address Features (ADDRFEAT).\n\
         Public domain, a work of the United States government (17 USC 105). The Census Bureau\n\
         requests source citation. No Census endorsement is implied.\n\
         Modified: street names folded, edge geometry simplified to about 3 m and rounded to 1e-6\n\
         degrees. A point interpolated along a range is approximate, not a surveyed address.\n",
    )?;
    Ok((entries.len(), zips.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dbf(rows: &[[&str; 9]]) -> Vec<u8> {
        let cols = [
            ("FULLNAME", 30),
            ("LFROMHN", 12),
            ("LTOHN", 12),
            ("ZIPL", 5),
            ("PARITYL", 1),
            ("RFROMHN", 12),
            ("RTOHN", 12),
            ("ZIPR", 5),
            ("PARITYR", 1),
        ];
        let width: usize = 1 + cols.iter().map(|c| c.1).sum::<usize>();
        let header = 32 + 32 * cols.len() + 1;
        let mut out = vec![3, 0, 0, 0];
        out.extend((rows.len() as u32).to_le_bytes());
        out.extend((header as u16).to_le_bytes());
        out.extend((width as u16).to_le_bytes());
        out.resize(32, 0);
        for (name, len) in cols {
            let mut field = [0u8; 32];
            field[..name.len()].copy_from_slice(name.as_bytes());
            (field[11], field[16]) = (b'C', len as u8);
            out.extend(field);
        }
        out.push(0x0d);
        for row in rows {
            out.push(b' ');
            for (value, (_, len)) in row.iter().zip(cols) {
                let mut cell = vec![b' '; len];
                cell[..value.len()].copy_from_slice(value.as_bytes());
                out.extend(cell);
            }
        }
        out
    }

    fn shp(lines: &[&[[f64; 2]]]) -> Vec<u8> {
        let mut out = vec![0, 0, 0x27, 0x0a];
        out.resize(100, 0);
        for (i, line) in lines.iter().enumerate() {
            let words = (44 + 4 + line.len() * 16) / 2;
            out.extend((i as u32 + 1).to_be_bytes());
            out.extend((words as u32).to_be_bytes());
            out.extend(3u32.to_le_bytes());
            out.extend([0u8; 32]);
            out.extend(1u32.to_le_bytes());
            out.extend((line.len() as u32).to_le_bytes());
            out.extend(0u32.to_le_bytes());
            for p in *line {
                out.extend(p[0].to_le_bytes());
                out.extend(p[1].to_le_bytes());
            }
        }
        out
    }

    /// A zip of stored entries; the reader checks no CRC.
    fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let (mut out, mut dir) = (vec![], vec![]);
        for (name, data) in entries {
            let offset = out.len() as u32;
            let (size, nlen) = (
                (data.len() as u32).to_le_bytes(),
                (name.len() as u16).to_le_bytes(),
            );
            // version, flags, method, time, date, CRC
            let head = [20u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            out.extend([0x50, 0x4b, 3, 4]);
            out.extend(head);
            out.extend(size);
            out.extend(size);
            out.extend(nlen);
            out.extend([0, 0]);
            out.extend(name.as_bytes());
            out.extend(*data);
            dir.extend([0x50, 0x4b, 1, 2, 20, 0]);
            dir.extend(head);
            dir.extend(size);
            dir.extend(size);
            dir.extend(nlen);
            // extra, comment, disk, internal and external attributes
            dir.extend([0u8; 12]);
            dir.extend(offset.to_le_bytes());
            dir.extend(name.as_bytes());
        }
        let at = out.len() as u32;
        out.extend(&dir);
        out.extend([0x50, 0x4b, 5, 6, 0, 0, 0, 0]);
        out.extend((entries.len() as u16).to_le_bytes());
        out.extend((entries.len() as u16).to_le_bytes());
        out.extend((dir.len() as u32).to_le_bytes());
        out.extend(at.to_le_bytes());
        out.extend([0, 0]);
        out
    }

    #[test]
    fn counties_become_ranges_that_place_a_house() {
        let dir = std::env::temp_dir().join(format!("us-ranges-{}", std::process::id()));
        let (input, out) = (dir.join("in"), dir.join("out"));
        std::fs::create_dir_all(&input).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        // 0.001 degree east at 40 N; odd numbers 1-99 on the right (08540), even on the left
        // under another ZIP code; a hyphenated and an unnamed edge are left out
        let east: &[[f64; 2]] = &[[-74.0, 40.0], [-73.9995, 40.0], [-73.999, 40.0]];
        let rows = [
            ["Main St", "2", "100", "08541", "E", "1", "99", "08540", "O"],
            ["Hyphen St", "89-12", "89-98", "11101", "E", "", "", "", ""],
            ["", "1", "9", "08540", "O", "", "", "", ""],
        ];
        let zipped = zip(&[
            ("tl_2024_34021_addrfeat.dbf", &dbf(&rows)),
            ("tl_2024_34021_addrfeat.shp", &shp(&[east, east, east])),
        ]);
        std::fs::write(input.join("tl_2024_34021_addrfeat.zip"), &zipped).unwrap();
        assert_eq!(write(&out, &input).unwrap(), (2, 1));

        let open = |name: &str| {
            let bytes = std::fs::read(out.join(name)).unwrap();
            tantivy::directory::OwnedBytes::new(bytes)
        };
        let ranges = ranges::Ranges::new(open(ranges::FST_FILE), open(ranges::BLOB_FILE)).unwrap();
        let (lat, lon) = ranges.locate("08540", "Main Street", 51).unwrap();
        assert!((lon - -73.9995).abs() < 2e-5 && lat < 40.0, "{lat} {lon}");
        let (lat, _) = ranges.locate("08541", "Main Street", 50).unwrap();
        assert!(lat > 40.0);
        assert!(ranges.locate("08540", "Main Street", 52).is_none());
        assert!(ranges.locate("11101", "Hyphen St", 89).is_none());
        // the same input builds the same files
        let again = dir.join("again");
        std::fs::create_dir_all(&again).unwrap();
        write(&again, &input).unwrap();
        for file in [ranges::FST_FILE, ranges::BLOB_FILE] {
            assert_eq!(
                std::fs::read(out.join(file)).unwrap(),
                std::fs::read(again.join(file)).unwrap()
            );
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_line_is_simplified_to_its_bends() {
        let line = [[0.0, 0.0], [0.5, 0.000_001], [1.0, 0.0], [1.0, 1.0]];
        assert_eq!(simplify(&line), vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]);
        assert_eq!(number(b"12A"), Some(12));
        assert_eq!(number(b"89-12"), None);
        assert_eq!(number(b""), None);
    }
}
