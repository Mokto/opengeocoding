//! Regenerates data/reference.json and data/sources.json from public CLDR, GeoNames and
//! Unicode Character Database files: `cargo run --bin update_reference [-- OUT_DIR]`.
//! Requires network access and changes checked-in data; review the result as a data update.

use anyhow::{Context, Result, bail};
use geolocation::eval::sha256_hex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

const CLDR_COMMIT: &str = "91c267402229a59e3ef2774544f001bf959e8809";
/// Pinned so character classes do not silently follow whichever toolchain runs the generator.
const UNICODE_VERSION: &str = "16.0.0";
const LANGUAGES: [&str; 16] = [
    "en", "de", "fr", "es", "da", "pt", "it", "nl", "pl", "ru", "el", "tr", "ar", "zh", "ja", "ko",
];

/// Unicode general categories per code point, from UnicodeData.txt (unlisted code points are Cn).
struct Categories(Vec<[u8; 2]>);

impl Categories {
    fn parse(text: &str) -> Result<Self> {
        let mut table = vec![*b"Cn"; 0x11_0000];
        let mut first = None;
        for line in text.lines() {
            let fields: Vec<_> = line.split(';').collect();
            let (code, name, category) =
                (u32::from_str_radix(fields[0], 16)?, fields[1], fields[2]);
            let category: [u8; 2] = category.as_bytes().try_into().context("category")?;
            if name.ends_with(", First>") {
                first = Some(code);
                continue;
            }
            let start = if name.ends_with(", Last>") {
                first.take().context("range start")?
            } else {
                code
            };
            table[start as usize..=code as usize].fill(category);
        }
        Ok(Self(table))
    }

    fn get(&self, ch: char) -> &[u8; 2] {
        &self.0[ch as usize]
    }

    /// Letters only (Lu, Ll, Lt, Lm, Lo), unlike `char::is_alphabetic`, which adds marks and numbers.
    fn alpha(&self, ch: char) -> bool {
        self.get(ch)[0] == b'L'
    }

    fn printable(&self, ch: char) -> bool {
        ch == ' '
            || !matches!(
                self.get(ch),
                b"Cc" | b"Cf" | b"Cs" | b"Co" | b"Cn" | b"Zl" | b"Zp" | b"Zs"
            )
    }

    fn ranges(&self, predicate: impl Fn(char) -> bool) -> Vec<[u32; 2]> {
        let mut ranges: Vec<[u32; 2]> = vec![];
        for ch in (0..0x11_0000)
            .filter_map(char::from_u32)
            .filter(|c| predicate(*c))
        {
            let code = ch as u32;
            match ranges.last_mut() {
                Some(last) if last[1] + 1 == code => last[1] = code,
                _ => ranges.push([code, code]),
            }
        }
        ranges
    }

    fn clean(&self, value: &str) -> String {
        let kept: String = value
            .to_lowercase()
            .chars()
            .filter(|c| self.alpha(*c) || "-/ ".contains(*c))
            .collect();
        kept.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

struct Downloads {
    client: reqwest::Client,
    sources: Vec<Value>,
}

impl Downloads {
    async fn get(&mut self, url: &str, license: &str) -> Result<String> {
        let response = self.client.get(url).send().await?.error_for_status()?;
        let data = response.bytes().await?;
        self.sources
            .push(json!({ "url": url, "sha256": sha256_hex(&data), "license": license }));
        String::from_utf8(data.to_vec()).with_context(|| format!("{url} is not UTF-8"))
    }
}

/// Pretty JSON with sorted keys, matching the checked-in formatting.
fn pretty(value: &Value) -> Result<String> {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let ordered: BTreeMap<_, _> =
                    map.iter().map(|(k, v)| (k.clone(), sorted(v))).collect();
                Value::Object(ordered.into_iter().collect())
            }
            Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    Ok(serde_json::to_string_pretty(&sorted(value))? + "\n")
}

#[tokio::main]
async fn main() -> Result<()> {
    let out = std::env::args().nth(1).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data"),
        PathBuf::from,
    );
    let mut downloads = Downloads {
        client: reqwest::Client::builder()
            .user_agent("geolocation-reference-builder")
            .timeout(std::time::Duration::from_secs(60))
            .build()?,
        sources: vec![],
    };
    let unicode = downloads
        .get(
            &format!("https://www.unicode.org/Public/{UNICODE_VERSION}/ucd/UnicodeData.txt"),
            "Unicode-3.0",
        )
        .await?;
    let categories = Categories::parse(&unicode)?;
    let mut countries: BTreeMap<String, String> = BTreeMap::new();
    let info = downloads
        .get(
            "https://download.geonames.org/export/dump/countryInfo.txt",
            "CC-BY-4.0",
        )
        .await?;
    let mut valid = BTreeSet::new();
    for line in info
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() < 5 {
            bail!("unexpected countryInfo line: {line}");
        }
        let code = fields[0].to_lowercase();
        valid.insert(code.clone());
        for name in [fields[0], fields[1], fields[4]] {
            countries.insert(categories.clean(name), code.clone());
        }
    }
    for language in LANGUAGES {
        let url = format!(
            "https://raw.githubusercontent.com/unicode-org/cldr-json/{CLDR_COMMIT}/cldr-json/cldr-localenames-full/main/{language}/territories.json"
        );
        let data: Value = serde_json::from_str(&downloads.get(&url, "Unicode-3.0").await?)?;
        let names = data["main"][language]["localeDisplayNames"]["territories"]
            .as_object()
            .context("CLDR territories")?;
        for (code, name) in names {
            let code = code.split('-').next().unwrap_or("").to_lowercase();
            if code.len() == 2
                && code.chars().all(|c| c.is_ascii_alphabetic())
                && valid.contains(&code)
            {
                countries.insert(categories.clean(name.as_str().unwrap_or("")), code.clone());
                countries.insert(code.clone(), code);
            }
        }
        println!("Loaded public country names: {language}");
    }
    let admin = downloads
        .get(
            "https://download.geonames.org/export/dump/admin1CodesASCII.txt",
            "CC-BY-4.0",
        )
        .await?;
    let mut owners: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for line in admin.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let [code, name, ascii_name, _] = fields.as_slice() else {
            bail!("unexpected admin1 line: {line}");
        };
        let country = code.split('.').next().unwrap_or("").to_lowercase();
        for value in [*name, *ascii_name] {
            owners.entry(value).or_default().insert(country.clone());
        }
    }
    let regions: BTreeMap<_, _> = owners
        .into_iter()
        .filter(|(_, codes)| codes.len() == 1)
        .map(|(name, codes)| (name, codes.into_iter().next().unwrap_or_default()))
        .collect();
    std::fs::create_dir_all(&out)?;
    std::fs::write(
        out.join("reference.json"),
        pretty(&json!({
            "countries": countries,
            "region_countries": regions,
            "overrides": {},
            "alpha_ranges": categories.ranges(|c| categories.alpha(c)),
            "printable_ranges": categories.ranges(|c| categories.printable(c)),
        }))?,
    )?;
    // Listed last so the established sources keep their positions.
    let unicode_source = downloads.sources.remove(0);
    downloads.sources.push(unicode_source);
    let sources = json!({ "sources": downloads.sources, "unicode_version": UNICODE_VERSION });
    std::fs::write(
        out.join("sources.json"),
        serde_json::to_string_pretty(&sources)? + "\n",
    )?;
    println!(
        "Wrote {} country aliases and {} region names",
        countries.len(),
        regions.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_follow_unicode_data() {
        let data = "0020;SPACE;Zs;0;WS;;;;;N;;;;;\n0041;LATIN CAPITAL LETTER A;Lu;0;L;;;;;N;;;;0061;\n\
                    0061;LATIN SMALL LETTER A;Ll;0;L;;;;;N;;;0041;;0041\n\
                    0301;COMBINING ACUTE ACCENT;Mn;230;NSM;;;;;N;;;;;\n00A0;NO-BREAK SPACE;Zs;0;CS;<noBreak> 0020;;;;N;;;;;\n\
                    4E00;<CJK Ideograph, First>;Lo;0;L;;;;;N;;;;;\n9FFF;<CJK Ideograph, Last>;Lo;0;L;;;;;N;;;;;";
        let categories = Categories::parse(data).unwrap();
        assert!(
            categories.alpha('A') && categories.alpha('\u{6771}') && !categories.alpha('\u{301}')
        );
        assert!(
            categories.printable(' ')
                && !categories.printable('\u{a0}')
                && !categories.printable('\u{378}')
        );
        assert_eq!(
            categories.ranges(|c| categories.alpha(c)),
            [[0x41, 0x41], [0x61, 0x61], [0x4e00, 0x9fff]]
        );
        // Marks and digits are dropped; letters, hyphens, slashes and single spaces remain.
        assert_eq!(categories.clean("  A\u{301}a -  A1/ "), "aa - a/");
    }
}
