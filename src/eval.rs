//! Helpers shared by the evaluator (`src/bin/evaluate`) and external corpus samplers,
//! so TSV escaping and hold-out assignment are defined once.

use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use std::{fmt::Write as _, path::Path};

/// The public address corpus may carry only these columns: no source, vendor, coordinates or identities.
pub const ADDRESS_COLUMNS: [&str; 9] = [
    "id",
    "target",
    "parser_target",
    "country",
    "state",
    "script",
    "holdout",
    "location",
    "query",
];

/// The Mexico corpus samples DENUE establishments that entered the register in this month or
/// later (`fecha_alta`, "YYYY-MM"); one the index may read entered it before.
pub const DENUE_SAMPLED_FROM: &str = "2025";

pub type Row = IndexMap<String, String>;

pub fn sha256_hex(data: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, data)
        .as_ref()
        .iter()
        .fold(String::new(), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// Stable hold-out assignment: about one location in five, independent of corpus order.
pub fn holdout(location: &str) -> bool {
    ring::digest::digest(&ring::digest::SHA256, location.as_bytes())
        .as_ref()
        .iter()
        .fold(0u32, |rest, &byte| (rest * 256 + u32::from(byte)) % 5)
        == 0
}

fn separator(ch: char) -> bool {
    ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}')
}

/// Keeps every TSV row on one line: tabs, newlines and other separators become escapes.
pub fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            ch if separator(ch) => {
                let _ = write!(out, "\\u{:04x}", u32::from(ch));
            }
            ch => out.push(ch),
        }
    }
    out
}

pub fn unescape(value: &str) -> Result<String> {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                let code = u32::from_str_radix(&hex, 16).context("TSV \\u escape")?;
                out.push(char::from_u32(code).context("TSV \\u escape")?);
            }
            other => bail!("invalid TSV escape \\{}", other.unwrap_or(' ')),
        }
    }
    Ok(out)
}

pub fn write_tsv(path: &Path, columns: &[&str], rows: &[Row]) -> Result<()> {
    let mut out = columns.join("\t") + "\n";
    for row in rows {
        let values: Vec<_> = columns
            .iter()
            .map(|column| escape(row.get(*column).map_or("", String::as_str)))
            .collect();
        out += &values.join("\t");
        out.push('\n');
    }
    std::fs::write(path, out).with_context(|| format!("write {}", path.display()))
}

pub fn read_tsv(path: &Path) -> Result<(Vec<String>, Vec<Row>)> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut lines = text.split('\n');
    let columns: Vec<String> = lines
        .next()
        .context("TSV header")?
        .split('\t')
        .map(String::from)
        .collect();
    let mut rows = vec![];
    for (number, line) in lines.enumerate() {
        if line.is_empty() {
            continue;
        }
        let values: Vec<_> = line.split('\t').collect();
        if values.len() != columns.len() {
            bail!(
                "{}:{}: expected {} columns, found {}",
                path.display(),
                number + 2,
                columns.len(),
                values.len()
            );
        }
        rows.push(
            columns
                .iter()
                .cloned()
                .zip(values.into_iter().map(unescape))
                .map(|(column, value)| value.map(|value| (column, value)))
                .collect::<Result<Row>>()?,
        );
    }
    Ok((columns, rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsv_round_trips_separators_and_unicode() {
        let tricky = "D02\tAF30\u{1c}Dublin\\x\u{2028}Ireland\nÉire 東京\r";
        let escaped = escape(tricky);
        assert!(!escaped.contains(['\t', '\n', '\r', '\u{1c}', '\u{2028}']));
        assert_eq!(unescape(&escaped).unwrap(), tricky);
        let path = std::env::temp_dir().join(format!("eval-tsv-{}.tsv", std::process::id()));
        let row: Row = [("id", "a"), ("query", tricky)]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        write_tsv(&path, &["id", "query"], std::slice::from_ref(&row)).unwrap();
        assert_eq!(read_tsv(&path).unwrap().1, vec![row]);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn digests_match_the_reference_values() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // Digests mod 5: abc 0 (held out), z 4.
        assert!(holdout("abc") && !holdout("z"));
    }
}
