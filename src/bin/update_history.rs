//! Regenerates data/historical_sources.json and data/historical_memberships.tsv: dated
//! settlement-to-ancestor memberships from official administrative datasets, linked to Wikidata
//! items through the datasets' own codes. `cargo run --bin update_history -- CACHE_DIR [--refresh]`
//!
//! Raw inputs are kept in CACHE_DIR, outside the repository. Without `--refresh`, every input must
//! match the SHA-256 already recorded in data/historical_sources.json, so the same inputs always
//! give the same files; `--refresh` downloads new releases and records them. Review the diff as a
//! data update.

use anyhow::{Context, Result, bail, ensure};
use opengeocoding::eval::sha256_hex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
};

/// `(code, name, Wikidata item)`. The code is the dataset's own, read at the snapshot's date; the
/// name is checked against the dataset where it has one. `None` is an ancestor reviewed as
/// unchanged since, whose memberships are left out.
type Ancestors = &'static [(&'static str, &'static str, Option<&'static str>)];

struct Spec {
    id: &'static str,
    country: &'static str,
    license: &'static str,
    attribution: &'static str,
    url: String,
    /// Wikidata property holding the dataset's settlement code.
    property: &'static str,
    ancestors: Ancestors,
    review: &'static str,
}

struct Membership {
    code: String,
    name: String,
    ancestor: String,
    /// The settlement's ancestor's name in the dataset, when it has one.
    ancestor_name: Option<String>,
    observed_on: String,
}

const SSB: &str = "Statistics Norway (SSB), KLASS classifications 131 and 104, CC BY 4.0, https://www.ssb.no/en/diverse/lisens";
const INSEE: &str = "Insee, Code officiel géographique, Licence Ouverte / Open Licence 2.0, https://www.etalab.gouv.fr/licence-ouverte-open-licence/";
const ANPR: &str = "Ministero dell'Interno, ANPR archivio storico dei comuni (with Istat), CC BY 4.0, https://www.anagrafenazionale.interno.it/note-legali/";

/// Counties 2018–2019 (Trøndelag merged). Oslo is its own municipality and county.
const NO_2019: Ancestors = &[
    ("01", "Østfold", Some("Q50614")),
    ("02", "Akershus", Some("Q50615")),
    ("03", "Oslo", None),
    ("04", "Hedmark", Some("Q50616")),
    ("05", "Oppland", Some("Q50617")),
    ("06", "Buskerud", Some("Q50618")),
    ("07", "Vestfold", Some("Q50619")),
    ("08", "Telemark", Some("Q2254")),
    ("09", "Aust-Agder", Some("Q50621")),
    ("10", "Vest-Agder", Some("Q50623")),
    ("11", "Rogaland", Some("Q50624")),
    ("12", "Hordaland", Some("Q50625")),
    ("14", "Sogn og Fjordane", Some("Q50626")),
    ("15", "Møre og Romsdal", Some("Q50627")),
    ("18", "Nordland", Some("Q50630")),
    ("19", "Troms", Some("Q50631")),
    ("20", "Finnmark", Some("Q50632")),
    ("50", "Trøndelag", Some("Q127676")),
    ("99", "Uoppgitt", None),
];

/// Before 2018, only the two Trøndelag counties differ from 2019.
const NO_2017: Ancestors = &[
    ("16", "Sør-Trøndelag", Some("Q50628")),
    ("17", "Nord-Trøndelag", Some("Q50629")),
    ("01", "Østfold", None),
    ("02", "Akershus", None),
    ("03", "Oslo", None),
    ("04", "Hedmark", None),
    ("05", "Oppland", None),
    ("06", "Buskerud", None),
    ("07", "Vestfold", None),
    ("08", "Telemark", None),
    ("09", "Aust-Agder", None),
    ("10", "Vest-Agder", None),
    ("11", "Rogaland", None),
    ("12", "Hordaland", None),
    ("14", "Sogn og Fjordane", None),
    ("15", "Møre og Romsdal", None),
    ("18", "Nordland", None),
    ("19", "Troms", None),
    ("20", "Finnmark", None),
    ("99", "Uoppgitt", None),
];

/// Counties 2020–2023.
const NO_2023: Ancestors = &[
    ("03", "Oslo", None),
    ("11", "Rogaland", Some("Q50624")),
    ("15", "Møre og Romsdal", Some("Q50627")),
    ("18", "Nordland", Some("Q50630")),
    ("30", "Viken", Some("Q56407178")),
    ("34", "Innlandet", Some("Q56404886")),
    ("38", "Vestfold og Telemark", Some("Q56405721")),
    ("42", "Agder", Some("Q2729021")),
    ("46", "Vestland", Some("Q56407177")),
    ("50", "Trøndelag", Some("Q127676")),
    ("54", "Troms og Finnmark", Some("Q52600648")),
    ("99", "Uoppgitt", None),
];

/// Regions on 1 January 2015, the last COG before the 2016 mergers. Regions that kept their
/// territory and name are left out; Centre (now Centre-Val de Loire) kept its territory.
const FR_2015: Ancestors = &[
    ("01", "Guadeloupe", None),
    ("02", "Martinique", None),
    ("03", "Guyane", None),
    ("04", "La Réunion", None),
    ("06", "Mayotte", None),
    ("11", "Île-de-France", None),
    ("21", "Champagne-Ardenne", Some("Q14103")),
    ("22", "Picardie", Some("Q13950")),
    ("23", "Haute-Normandie", Some("Q16961")),
    ("24", "Centre", Some("Q13947")),
    ("25", "Basse-Normandie", Some("Q16954")),
    ("26", "Bourgogne", Some("Q1173")),
    ("31", "Nord-Pas-de-Calais", Some("Q16987")),
    ("41", "Lorraine", Some("Q22248793")),
    ("42", "Alsace", Some("Q22010895")),
    ("43", "Franche-Comté", Some("Q60846019")),
    ("52", "Pays de la Loire", None),
    ("53", "Bretagne", None),
    ("54", "Poitou-Charentes", Some("Q17009")),
    ("72", "Aquitaine", Some("Q1179")),
    ("73", "Midi-Pyrénées", Some("Q16393")),
    ("74", "Limousin", Some("Q1190")),
    ("82", "Rhône-Alpes", Some("Q463")),
    ("83", "Auvergne", Some("Q1152")),
    ("91", "Languedoc-Roussillon", Some("Q17005")),
    ("93", "Provence-Alpes-Côte d'Azur", None),
    ("94", "Corse", None),
];

/// `(Istat province code, car-plate code)` → province, for the provinces a comune has left
/// since 2000. Istat codes are reused (104 is now Gallura Nord-Est Sardegna), so the name is
/// the province's at the time; the plate code is checked against the archive.
const IT_PROVINCES: Ancestors = &[
    ("015/MI", "Milano", Some("Q15121")),
    ("025/BL", "Belluno", Some("Q16126")),
    ("041/PU", "Pesaro e Urbino", Some("Q16241")),
    ("044/AP", "Ascoli Piceno", Some("Q16117")),
    ("071/FG", "Foggia", Some("Q15129")),
    ("072/BA", "Bari", Some("Q16123")),
    ("091/NU", "Nuoro", Some("Q16220")),
    ("092/CA", "Cagliari", Some("Q16149")),
    ("095/OR", "Oristano", Some("Q16225")),
    ("097/LC", "Lecco", Some("Q16199")),
    ("104/OT", "Olbia-Tempio", Some("Q16224")),
    ("105/OG", "Ogliastra", Some("Q16223")),
    ("106/VS", "Medio Campidano", Some("Q16207")),
    ("107/CI", "Carbonia-Iglesias", Some("Q16152")),
];

/// The first year of the Italian archive's intervals that are imported.
const IT_SINCE: &str = "2000-01-01";

fn specs() -> Vec<Spec> {
    let klass = |date: &str| {
        format!(
            "https://data.ssb.no/api/klass/v1/classifications/131/correspondsAt?targetClassificationId=104&date={date}"
        )
    };
    let norway = "Statistics Norway's date-scoped KLASS correspondence maps each municipality \
        (classification 131) to its county (classification 104) on the observation date. \
        Codes are read at that date, not as timeless identifiers; municipalities are linked to \
        Wikidata through their municipality number (P2504) valid on that date, and counties \
        through a reviewed table. A municipality is observed in that county on that date; \
        unchanged boundaries and membership intervals are not asserted.";
    vec![
        Spec {
            id: "no-ssb-2017",
            country: "NO",
            license: "CC-BY-4.0",
            attribution: SSB,
            url: klass("2017-12-31"),
            property: "P2504",
            ancestors: NO_2017,
            review: norway,
        },
        Spec {
            id: "no-ssb-2019",
            country: "NO",
            license: "CC-BY-4.0",
            attribution: SSB,
            url: klass("2019-12-31"),
            property: "P2504",
            ancestors: NO_2019,
            review: norway,
        },
        Spec {
            id: "no-ssb-2023",
            country: "NO",
            license: "CC-BY-4.0",
            attribution: SSB,
            url: klass("2023-12-31"),
            property: "P2504",
            ancestors: NO_2023,
            review: norway,
        },
        Spec {
            id: "fr-insee-cog-2015",
            country: "FR",
            license: "etalab-2.0",
            attribution: INSEE,
            url: "https://www.insee.fr/fr/statistiques/fichier/2560698/comsimp2015-txt.zip".into(),
            property: "P374",
            ancestors: FR_2015,
            review: "Insee's Code officiel géographique lists every commune existing on \
                1 January 2015 with its region code; region names come from the same release. \
                Communes are linked to Wikidata through their Insee code (P374) valid on that \
                date, regions through a reviewed table (the 2015 codes, not today's). A commune \
                is observed in that region on that date; communes in regions unchanged by the \
                2016 reform are not imported.",
        },
        Spec {
            id: "it-anpr",
            country: "IT",
            license: "CC-BY-4.0",
            attribution: ANPR,
            url: "https://www.anagrafenazionale.interno.it/wp-content/uploads/ANPR_archivio_comuni.csv"
                .into(),
            property: "P806",
            ancestors: IT_PROVINCES,
            review: "The ANPR/Istat historical archive dates each comune's name and province. \
                An interval ending since 2000 in a province other than the comune's current one \
                (or, for a suppressed comune, in a province that no longer exists) is observed \
                on its last day. Comuni are linked to Wikidata through their cadastral code \
                (P806), which survives province changes; provinces through a reviewed table \
                keyed by the Istat code and plate code of the time.",
        },
    ]
}

fn identity_query(property: &str) -> String {
    let query = format!(
        "SELECT ?item ?value ?start ?end WHERE {{ ?item p:{property} ?s . ?s ps:{property} ?value ; \
         wikibase:rank ?rank . FILTER(?rank != wikibase:DeprecatedRank) \
         OPTIONAL {{ ?s pq:P580 ?start }} OPTIONAL {{ ?s pq:P582 ?end }} }} ORDER BY ?value ?item ?start ?end"
    );
    let mut url = reqwest::Url::parse("https://query.wikidata.org/sparql").expect("static URL");
    url.query_pairs_mut()
        .append_pair("format", "json")
        .append_pair("query", &query);
    url.into()
}

/// Windows-1252, the encoding of COG files before 2019 (Œ and œ are 0x8C and 0x9C). Bytes it
/// leaves undefined are refused.
fn cp1252(bytes: &[u8]) -> Result<String> {
    const HIGH: &str = "€\u{81}‚ƒ„…†‡ˆ‰Š‹Œ\u{8d}Ž\u{8f}\u{90}‘’“”•–—˜™š›œ\u{9d}žŸ";
    let high: Vec<char> = HIGH.chars().collect();
    bytes
        .iter()
        .map(|&b| match b {
            0x81 | 0x8D | 0x8F | 0x90 | 0x9D => bail!("byte {b:#x} is not Windows-1252"),
            0x80..=0x9F => Ok(high[usize::from(b - 0x80)]),
            _ => Ok(char::from(b)),
        })
        .collect()
}

fn unzip(path: &Path, name: &str) -> Result<Vec<u8>> {
    let out = std::process::Command::new("unzip")
        .arg("-p")
        .arg(path)
        .arg(name)
        .output()
        .context("unzip")?;
    ensure!(
        out.status.success() && !out.stdout.is_empty(),
        "unzip {name} from {}",
        path.display()
    );
    Ok(out.stdout)
}

/// Rows of a tab- or comma-separated file with a header, by column name.
fn table(text: &str, sep: char) -> Result<Vec<HashMap<&str, &str>>> {
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().context("header")?.split(sep).collect();
    lines
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split(sep).collect();
            ensure!(fields.len() == header.len(), "row width: {line}");
            Ok(header.iter().copied().zip(fields).collect())
        })
        .collect()
}

/// KLASS adds the county or a Sami name to some municipality names: "Herøy (Nordland)",
/// "Kautokeino - Guovdageaidnu". The Norwegian name comes first.
fn norwegian(name: &str) -> &str {
    let name = name.split(" - ").next().unwrap_or(name);
    name.split(" (").next().unwrap_or(name).trim()
}

fn norway(json: &Value, date: &str) -> Result<Vec<Membership>> {
    json["correspondenceItems"]
        .as_array()
        .context("correspondenceItems")?
        .iter()
        .map(|item| {
            let field = |k: &str| item[k].as_str().with_context(|| k.to_string());
            Ok(Membership {
                code: field("sourceCode")?.into(),
                name: norwegian(field("sourceName")?).into(),
                ancestor: field("targetCode")?.into(),
                ancestor_name: Some(field("targetName")?.into()),
                observed_on: date.into(),
            })
        })
        .collect()
}

fn france(communes: &str, regions: &str) -> Result<Vec<Membership>> {
    let names: HashMap<&str, &str> = table(regions, '\t')?
        .iter()
        .map(|r| (r["REGION"], r["NCCENR"]))
        .collect();
    table(communes, '\t')?
        .iter()
        .map(|r| {
            let article = r["ARTMIN"].trim_matches(|c| c == '(' || c == ')');
            let name = match article {
                "" => r["NCCENR"].to_string(),
                a if a.ends_with('\'') => format!("{a}{}", r["NCCENR"]),
                a => format!("{a} {}", r["NCCENR"]),
            };
            Ok(Membership {
                code: format!("{}{}", r["DEP"], r["COM"]),
                name,
                ancestor: r["REG"].into(),
                ancestor_name: Some(
                    names
                        .get(r["REG"])
                        .with_context(|| format!("region {}", r["REG"]))?
                        .to_string(),
                ),
                observed_on: "2015-01-01".into(),
            })
        })
        .collect()
}

fn italy(csv: &str) -> Result<Vec<Membership>> {
    let unquote = |s: &str| s.trim_matches('"').to_string();
    let rows: Vec<HashMap<&str, String>> = table(csv, ',')?
        .into_iter()
        .map(|r| {
            r.into_iter()
                .map(|(k, v)| (k.trim_matches('"'), unquote(v)))
                .collect()
        })
        .collect();
    const OPEN: &str = "9999-12-31";
    let current: HashMap<&str, &str> = rows
        .iter()
        .filter(|r| r["DATACESSAZIONE"] == OPEN)
        .map(|r| (r["CODCATASTALE"].as_str(), r["SIGLAPROVINCIA"].as_str()))
        .collect();
    let active: BTreeSet<&str> = current.values().copied().collect();
    Ok(rows
        .iter()
        .filter(|r| {
            let (end, plate) = (r["DATACESSAZIONE"].as_str(), r["SIGLAPROVINCIA"].as_str());
            end != OPEN
                && end >= IT_SINCE
                && match current.get(r["CODCATASTALE"].as_str()) {
                    Some(&now) => now != plate,
                    None => !active.contains(plate),
                }
        })
        .map(|r| Membership {
            code: r["CODCATASTALE"].clone(),
            name: r["DENOMINAZIONE_IT"].clone(),
            ancestor: format!("{}/{}", r["IDPROVINCIAISTAT"], r["SIGLAPROVINCIA"]),
            ancestor_name: None,
            observed_on: r["DATACESSAZIONE"].clone(),
        })
        .collect())
}

/// Wikidata items by code, with each statement's validity: `(item, start, end)`.
type Identities = HashMap<String, Vec<(String, String, String)>>;

fn identities(json: &Value) -> Result<Identities> {
    let mut out: Identities = HashMap::new();
    for row in json["results"]["bindings"]
        .as_array()
        .context("SPARQL bindings")?
    {
        let get = |k: &str| row[k]["value"].as_str().unwrap_or("");
        let item = get("item")
            .strip_prefix("http://www.wikidata.org/entity/")
            .context("item URI")?;
        let day = |k: &str| get(k).get(..10).unwrap_or("").to_string();
        out.entry(get("value").into())
            .or_default()
            .push((item.into(), day("start"), day("end")));
    }
    Ok(out)
}

/// The one item whose statement for `code` holds on `date`; none when several do.
fn identity<'a>(ids: &'a Identities, code: &str, date: &str) -> Result<&'a str, &'static str> {
    let items: BTreeSet<&str> = ids
        .get(code)
        .into_iter()
        .flatten()
        .filter(|(_, start, end)| {
            (start.is_empty() || start.as_str() <= date) && (end.is_empty() || end.as_str() >= date)
        })
        .map(|(item, _, _)| item.as_str())
        .collect();
    match items.len() {
        1 => Ok(items.into_iter().next().unwrap()),
        0 => Err("no item"),
        _ => Err("ambiguous item"),
    }
}

struct Inputs {
    client: reqwest::Client,
    cache: PathBuf,
    refresh: bool,
    pinned: HashMap<String, (String, String)>,
    today: String,
    /// One download per run, shared by the snapshots that read the same input.
    fetched: HashMap<String, (PathBuf, Value)>,
}

impl Inputs {
    /// The input's path in the cache and its record, downloaded unless the cache holds the pinned
    /// bytes.
    async fn get(&mut self, url: &str) -> Result<(PathBuf, Value)> {
        if let Some(done) = self.fetched.get(url) {
            return Ok(done.clone());
        }
        let path = self.cache.join(&sha256_hex(url.as_bytes())[..24]);
        let pin = self.pinned.get(url).filter(|_| !self.refresh);
        let cached = std::fs::read(&path).ok();
        let (data, retrieved) = match (cached, pin) {
            (Some(data), Some((sha, day))) if &sha256_hex(&data) == sha => (data, day.clone()),
            _ => {
                eprintln!("downloading {url}");
                let response = self
                    .client
                    .get(url)
                    .header("Accept", "application/json, text/csv, */*")
                    .send()
                    .await?
                    .error_for_status()?;
                let data = response.bytes().await?.to_vec();
                if let Some((sha, _)) = pin {
                    ensure!(
                        &sha256_hex(&data) == sha,
                        "{url} changed since it was pinned; rerun with --refresh to import the new release"
                    );
                }
                std::fs::write(&path, &data)?;
                let day = pin.map_or_else(|| self.today.clone(), |(_, day)| day.clone());
                (data, day)
            }
        };
        let record = json!({ "url": url, "sha256": sha256_hex(&data), "retrieved_on": retrieved });
        self.fetched
            .insert(url.into(), (path.clone(), record.clone()));
        Ok((path, record))
    }
}

fn today() -> Result<String> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
    Ok(civil(now.as_secs() as i64 / 86_400))
}

/// The UTC date of a day counted from 1970-01-01 (Howard Hinnant's algorithm).
fn civil(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cache = PathBuf::from(
        args.iter()
            .find(|a| !a.starts_with("--"))
            .context("usage: update_history CACHE_DIR [--refresh]")?,
    );
    std::fs::create_dir_all(&cache)?;
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
    let manifest_path = data.join("historical_sources.json");
    let previous: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&manifest_path)?)?;
    let pinned = previous
        .iter()
        .flat_map(|s| {
            let membership = s["membership"].as_array().into_iter().flatten();
            membership.chain([&s["identity"]])
        })
        .filter_map(|i| {
            Some((
                i["url"].as_str()?.to_string(),
                (
                    i["sha256"].as_str()?.to_string(),
                    i["retrieved_on"].as_str()?.to_string(),
                ),
            ))
        })
        .collect();
    let mut inputs = Inputs {
        client: reqwest::Client::builder()
            .user_agent("opengeocoding-history-import (https://github.com/Mokto/opengeocoding)")
            .timeout(std::time::Duration::from_secs(300))
            .build()?,
        cache,
        refresh: args.iter().any(|a| a == "--refresh"),
        pinned,
        today: today()?,
        fetched: HashMap::new(),
    };
    let mut snapshots = vec![];
    let mut rows = BTreeSet::new();
    for spec in specs() {
        let (path, record) = inputs.get(&spec.url).await?;
        let mut membership = vec![record];
        let memberships = match spec.country {
            "NO" => {
                let date = spec.url.rsplit('=').next().context("KLASS date")?;
                norway(&serde_json::from_slice(&std::fs::read(&path)?)?, date)?
            }
            "FR" => {
                let regions_url =
                    "https://www.insee.fr/fr/statistiques/fichier/2560698/reg2015-txt.zip";
                let (regions, record) = inputs.get(regions_url).await?;
                membership.push(record);
                france(
                    &cp1252(&unzip(&path, "comsimp2015.txt")?)?,
                    &cp1252(&unzip(&regions, "reg2015.txt")?)?,
                )?
            }
            "IT" => italy(&String::from_utf8(std::fs::read(&path)?)?)?,
            other => bail!("no adapter for {other}"),
        };
        let (path, link) = inputs.get(&identity_query(spec.property)).await?;
        let ids = identities(&serde_json::from_slice(&std::fs::read(&path)?)?)?;
        let ancestors: HashMap<&str, (&str, Option<&str>)> = spec
            .ancestors
            .iter()
            .map(|&(code, name, item)| (code, (name, item)))
            .collect();
        let mut used = BTreeMap::new();
        let mut skipped: BTreeMap<&str, usize> = BTreeMap::new();
        for m in &memberships {
            let &(name, item) = ancestors.get(m.ancestor.as_str()).with_context(|| {
                format!(
                    "{}: unreviewed ancestor {} ({:?})",
                    spec.id, m.ancestor, m.ancestor_name
                )
            })?;
            if let Some(source) = &m.ancestor_name {
                ensure!(
                    source.split(" - ").any(|part| part == name),
                    "{}: ancestor {} is {source} in the dataset, not {name}",
                    spec.id,
                    m.ancestor
                );
            }
            let Some(item) = item else {
                *skipped.entry("unchanged ancestor").or_default() += 1;
                continue;
            };
            if m.name.is_empty() || m.name.contains(['\t', '\n']) {
                bail!("{}: unusable name for {}", spec.id, m.code);
            }
            match identity(&ids, &m.code, &m.observed_on) {
                Ok(settlement) if settlement != item => {
                    used.insert(
                        m.ancestor.clone(),
                        json!({ "name": name, "wikidata": item }),
                    );
                    rows.insert(format!(
                        "{}\t{}\t{}\t{settlement}\t{}\t{}",
                        spec.id, m.code, m.name, m.ancestor, m.observed_on
                    ));
                }
                Ok(_) => *skipped.entry("settlement is its ancestor").or_default() += 1,
                Err(why) => *skipped.entry(why).or_default() += 1,
            }
        }
        eprintln!(
            "{}: {} memberships, {} imported, skipped {skipped:?}",
            spec.id,
            memberships.len(),
            memberships.len() - skipped.values().sum::<usize>()
        );
        // Deterministic: an import is reviewed as of its latest input.
        let reviewed = membership
            .iter()
            .chain([&link])
            .filter_map(|i| i["retrieved_on"].as_str())
            .max()
            .context("retrieval date")?
            .to_string();
        snapshots.push(json!({
            "id": spec.id,
            "country": spec.country,
            "reviewed_on": reviewed,
            "review": spec.review,
            "license": spec.license,
            "attribution": spec.attribution,
            "membership": membership,
            "identity": link,
            "ancestors": used,
        }));
    }
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&snapshots)? + "\n",
    )?;
    let mut tsv = String::from("snapshot\tcode\tname\twikidata\tancestor\tobserved_on\n");
    for row in rows {
        tsv.push_str(&row);
        tsv.push('\n');
    }
    std::fs::write(data.join("historical_memberships.tsv"), tsv)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_read_at_their_date_and_ambiguity_is_left_out() {
        let ids = identities(&json!({"results": {"bindings": [
            {"item": {"value": "http://www.wikidata.org/entity/Q1"}, "value": {"value": "0602"},
             "end": {"value": "2019-12-31T00:00:00Z"}},
            {"item": {"value": "http://www.wikidata.org/entity/Q1"}, "value": {"value": "3005"},
             "start": {"value": "2020-01-01T00:00:00Z"}},
            {"item": {"value": "http://www.wikidata.org/entity/Q2"}, "value": {"value": "3005"},
             "end": {"value": "2015-12-31T00:00:00Z"}},
            {"item": {"value": "http://www.wikidata.org/entity/Q3"}, "value": {"value": "0101"}},
            {"item": {"value": "http://www.wikidata.org/entity/Q4"}, "value": {"value": "0101"}},
        ]}}))
        .unwrap();
        assert_eq!(identity(&ids, "0602", "2019-12-31"), Ok("Q1"));
        assert_eq!(identity(&ids, "0602", "2020-01-01"), Err("no item"));
        assert_eq!(identity(&ids, "3005", "2023-12-31"), Ok("Q1"));
        assert_eq!(identity(&ids, "3005", "2015-01-01"), Ok("Q2"));
        assert_eq!(identity(&ids, "0101", "2019-12-31"), Err("ambiguous item"));
        assert_eq!(identity(&ids, "9999", "2019-12-31"), Err("no item"));
        assert_eq!(civil(0), "1970-01-01");
        assert_eq!(civil(11_016), "2000-02-29");
        assert_eq!(civil(20_746), "2026-10-20");
    }

    #[test]
    fn datasets_are_read_with_their_own_names_and_dates() {
        let no = norway(
            &json!({"correspondenceItems": [
                {"sourceCode": "1515", "sourceName": "Herøy (Møre og Romsdal)", "targetCode": "15", "targetName": "Møre og Romsdal"},
                {"sourceCode": "5430", "sourceName": "Kautokeino - Guovdageaidnu", "targetCode": "54", "targetName": "Troms og Finnmark - Romsa ja Finnmárku"},
            ]}),
            "2023-12-31",
        )
        .unwrap();
        assert_eq!(
            no.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
            ["Herøy", "Kautokeino"]
        );
        assert_eq!(no[1].observed_on, "2023-12-31");

        let fr = france(
            "CDC\tCHEFLIEU\tREG\tDEP\tCOM\tAR\tCT\tTNCC\tARTMAJ\tNCC\tARTMIN\tNCCENR\r\n\
             0\t0\t82\t01\t001\t2\t08\t5\t(L')\tABERGEMENT\t(L')\tAbergement-Clémenciat\r\n\
             0\t1\t21\t51\t454\t2\t99\t0\t\tREIMS\t\tReims\r\n\
             0\t0\t21\t08\t001\t1\t01\t2\t(LES)\tAYVELLES\t(Les)\tAyvelles\r\n",
            "REGION\tCHEFLIEU\tTNCC\tNCC\tNCCENR\n21\t51108\t0\tCHAMPAGNE-ARDENNE\tChampagne-Ardenne\n82\t69123\t0\tRHONE-ALPES\tRhône-Alpes\n",
        )
        .unwrap();
        assert_eq!(
            fr.iter()
                .map(|m| (m.code.as_str(), m.name.as_str(), m.ancestor_name.as_deref()))
                .collect::<Vec<_>>(),
            [
                ("01001", "L'Abergement-Clémenciat", Some("Rhône-Alpes")),
                ("51454", "Reims", Some("Champagne-Ardenne")),
                ("08001", "Les Ayvelles", Some("Champagne-Ardenne")),
            ]
        );
        assert_eq!(cp1252(&[0x43, 0x9C, 0xE9, 0x8C]).unwrap(), "CœéŒ");
        assert!(cp1252(&[0x81]).is_err());

        let header = "\"ID\",\"DATAISTITUZIONE\",\"DATACESSAZIONE\",\"CODISTAT\",\"CODCATASTALE\",\"DENOMINAZIONE_IT\",\"IDPROVINCIAISTAT\",\"SIGLAPROVINCIA\"";
        let it = italy(&format!(
            "{header}\n\
             307,\"1861-03-17\",\"2009-07-26\",\"072005\",\"A285\",\"ANDRIA\",\"072\",\"BA\"\n\
             306,\"2009-07-27\",\"9999-12-31\",\"110001\",\"A285\",\"ANDRIA\",\"110\",\"BT\"\n\
             1,\"1861-03-17\",\"1995-01-01\",\"072999\",\"Z001\",\"OLD\",\"072\",\"BA\"\n\
             2,\"1861-03-17\",\"2016-04-27\",\"104001\",\"Z002\",\"GONE\",\"104\",\"OT\"\n\
             3,\"1861-03-17\",\"2019-01-01\",\"072998\",\"Z003\",\"MERGED\",\"072\",\"BA\"\n\
             4,\"1861-03-17\",\"2012-01-01\",\"072997\",\"Z004\",\"RENAMED\",\"072\",\"BA\"\n\
             5,\"2012-01-02\",\"9999-12-31\",\"072997\",\"Z004\",\"NEW NAME\",\"072\",\"BA\"\n"
        ))
        .unwrap();
        assert_eq!(
            it.iter()
                .map(|m| (m.code.as_str(), m.ancestor.as_str(), m.observed_on.as_str()))
                .collect::<Vec<_>>(),
            [
                ("A285", "072/BA", "2009-07-26"),
                ("Z002", "104/OT", "2016-04-27")
            ]
        );
    }
}
