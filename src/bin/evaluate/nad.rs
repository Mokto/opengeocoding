//! The US National Address Database (USDOT, public domain) as an evaluation source.
//!
//! `sample` draws cases registry-first: from the registry's own rows, not from the index under
//! test, so that index recall is measurable. The draw depends only on the seed and each row's UUID,
//! never on file order. `lookup` re-reads the same release for `build-gold`, which copies the
//! expectation from the registry row.

use anyhow::{Context, Result, bail, ensure};
use opengeocoding::eval::sha256_hex;
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet},
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

const STATES: [(&str, &str); 51] = [
    ("AL", "Alabama"),
    ("AK", "Alaska"),
    ("AZ", "Arizona"),
    ("AR", "Arkansas"),
    ("CA", "California"),
    ("CO", "Colorado"),
    ("CT", "Connecticut"),
    ("DE", "Delaware"),
    ("DC", "District of Columbia"),
    ("FL", "Florida"),
    ("GA", "Georgia"),
    ("HI", "Hawaii"),
    ("ID", "Idaho"),
    ("IL", "Illinois"),
    ("IN", "Indiana"),
    ("IA", "Iowa"),
    ("KS", "Kansas"),
    ("KY", "Kentucky"),
    ("LA", "Louisiana"),
    ("ME", "Maine"),
    ("MD", "Maryland"),
    ("MA", "Massachusetts"),
    ("MI", "Michigan"),
    ("MN", "Minnesota"),
    ("MS", "Mississippi"),
    ("MO", "Missouri"),
    ("MT", "Montana"),
    ("NE", "Nebraska"),
    ("NV", "Nevada"),
    ("NH", "New Hampshire"),
    ("NJ", "New Jersey"),
    ("NM", "New Mexico"),
    ("NY", "New York"),
    ("NC", "North Carolina"),
    ("ND", "North Dakota"),
    ("OH", "Ohio"),
    ("OK", "Oklahoma"),
    ("OR", "Oregon"),
    ("PA", "Pennsylvania"),
    ("RI", "Rhode Island"),
    ("SC", "South Carolina"),
    ("SD", "South Dakota"),
    ("TN", "Tennessee"),
    ("TX", "Texas"),
    ("UT", "Utah"),
    ("VT", "Vermont"),
    ("VA", "Virginia"),
    ("WA", "Washington"),
    ("WV", "West Virginia"),
    ("WI", "Wisconsin"),
    ("WY", "Wyoming"),
];

/// Placements that put the point on or at the building or lot. "Unknown" (about 70% of the
/// release) and linear geocodes say nothing about how good the point is, so they are not sampled.
const TRUSTED: [&str; 6] = [
    "Structure - Rooftop",
    "Structure - Entrance",
    "Structure - Interior",
    "Site",
    "Property Access",
    "Parcel - Centroid",
];

/// Lifecycle values of live addresses; the field is empty in most rows.
const LIVE: [&str; 6] = ["", "Active", "ACTIVE", "Current", "A", "Final"];

/// Locations per (kind, density) cell. 300 house addresses and 100 units with a trusted placement
/// make the headline sample. Trusted placements exist in few states (see the README), so a
/// separate "broad" sample takes house addresses of any placement, scored at a looser radius, to
/// cover the others; it is reported apart and never mixed into the headline.
const TARGETS: [(&str, &str, usize); 7] = [
    ("house", "urban", 120),
    ("house", "suburban", 90),
    ("house", "rural", 90),
    ("unit", "urban", 60),
    ("unit", "suburban", 30),
    ("unit", "rural", 10),
    ("broad", "any", 150),
];

/// Candidates kept per kind: a uniform random subset, since rows are kept by smallest hash.
const POOL: usize = 30_000;
const MAX_PER_ZIP: usize = 3;

const TYPES: [(&str, &str); 12] = [
    ("Street", "St"),
    ("Avenue", "Ave"),
    ("Road", "Rd"),
    ("Drive", "Dr"),
    ("Boulevard", "Blvd"),
    ("Lane", "Ln"),
    ("Court", "Ct"),
    ("Circle", "Cir"),
    ("Highway", "Hwy"),
    ("Place", "Pl"),
    ("Parkway", "Pkwy"),
    ("Terrace", "Ter"),
];
const DIRECTIONS: [(&str, &str); 8] = [
    ("North", "N"),
    ("South", "S"),
    ("East", "E"),
    ("West", "W"),
    ("Northeast", "NE"),
    ("Northwest", "NW"),
    ("Southeast", "SE"),
    ("Southwest", "SW"),
];

#[derive(Clone, Debug, Default)]
pub struct Address {
    pub uuid: String,
    pub number: String,
    pub street: String,
    pub unit: String,
    pub city: String,
    pub state: String,
    pub zip: String,
    pub plus4: String,
    /// [latitude, longitude]
    pub point: [f64; 2],
    pub placement: String,
}

impl Address {
    pub fn parcel(&self) -> bool {
        self.placement.starts_with("Parcel")
    }
}

/// Splits one CSV line, honouring double quotes. Quote-free lines, nearly all of them, borrow.
fn split(line: &str) -> Vec<Cow<'_, str>> {
    if !line.contains('"') {
        return line.split(',').map(Cow::Borrowed).collect();
    }
    let (mut fields, mut field, mut quoted) = (vec![], String::new(), false);
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(Cow::Owned(std::mem::take(&mut field))),
            c => field.push(c),
        }
    }
    fields.push(Cow::Owned(field));
    fields
}

fn title(text: &str) -> String {
    const KEEP_UPPER: [&str; 11] = ["n", "s", "e", "w", "ne", "nw", "se", "sw", "us", "sr", "cr"];
    text.split_whitespace()
        .map(|word| {
            let lower = word.to_lowercase();
            if KEEP_UPPER.contains(&lower.as_str()) {
                return lower.to_uppercase();
            }
            let mut chars = lower.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

struct Columns(HashMap<String, usize>);

impl Columns {
    fn get<'a>(&self, fields: &'a [Cow<'a, str>], name: &str) -> &'a str {
        self.0
            .get(name)
            .and_then(|&i| fields.get(i))
            .map_or("", |f| f.trim())
    }
}

/// A row that can be a query: a plain house number, a street, a five-digit ZIP, a live record in
/// a state. What the registry says about the point's quality is kept for the caller to judge.
fn address(columns: &Columns, fields: &[Cow<'_, str>]) -> Option<Address> {
    let get = |name| columns.get(fields, name);
    let (number, state, zip) = (get("AddNo_Full"), get("State"), get("Zip_Code"));
    if !LIVE.contains(&get("Lifecycle"))
        || number.is_empty()
        || number.len() > 7
        || !number.chars().next()?.is_ascii_digit()
        || !number.chars().all(|c| c.is_ascii_alphanumeric())
        || zip.len() != 5
        || !zip.bytes().all(|b| b.is_ascii_digit())
        || zip == "00000"
        || !STATES.iter().any(|(code, _)| *code == state)
    {
        return None;
    }
    let street = get("StNam_Full");
    if !street_name(street) {
        return None;
    }
    let city = [
        get("Post_City"),
        get("Inc_Muni"),
        get("Uninc_Comm"),
        get("Census_Plc"),
    ]
    .into_iter()
    .find(|c| usable_city(c))?;
    let (lon, lat) = (
        get("Longitude").parse::<f64>().ok()?,
        get("Latitude").parse::<f64>().ok()?,
    );
    // Alaska and Hawaii included; anything else is a placeholder or a data-entry error.
    if !(17.0..72.0).contains(&lat) || !(-180.0..-64.0).contains(&lon) {
        return None;
    }
    Some(Address {
        uuid: get("UUID").to_string(),
        number: number.to_uppercase(),
        street: title(street),
        unit: get("Unit").to_string(),
        city: title(city),
        state: state.to_string(),
        zip: zip.to_string(),
        plus4: get("Plus_4").to_string(),
        point: [lat, lon],
        placement: get("Placement").to_string(),
    })
}

/// Registries put placeholders and county names where a town should be; nobody types those.
fn usable_city(city: &str) -> bool {
    let lower = city.to_lowercase();
    !lower.is_empty()
        && ![
            "not stated",
            "unincorporated",
            "unknown",
            "none",
            "n/a",
            "na",
        ]
        .contains(&lower.as_str())
        && !lower.ends_with(" county")
}

/// Some jurisdictions load a unit or a bare direction into the street field; a query built from
/// that is not an address anyone would type.
fn street_name(street: &str) -> bool {
    const NOT_STREET: [&str; 12] = [
        "apt", "unit", "suite", "ste", "bldg", "building", "lot", "trlr", "room", "rm", "fl", "po",
    ];
    let words: Vec<String> = street
        .to_lowercase()
        .split_whitespace()
        .map(String::from)
        .collect();
    let generic = |w: &str| {
        TYPES
            .iter()
            .chain(DIRECTIONS.iter())
            .any(|(full, short)| w == full.to_lowercase() || w == short.to_lowercase())
    };
    street.chars().filter(|c| c.is_alphabetic()).count() >= 2
        && !street.contains('#')
        && !words.iter().any(|w| NOT_STREET.contains(&w.as_str()))
        && !(words.len() == 1 && generic(&words[0]))
        && !words.windows(2).any(|pair| {
            TYPES.iter().chain(DIRECTIONS.iter()).any(|(full, short)| {
                let (full, short) = (full.to_lowercase(), short.to_lowercase());
                (pair[0] == full && pair[1] == short) || (pair[0] == short && pair[1] == full)
            })
        })
}

fn for_each_row(path: &Path, mut f: impl FnMut(&Columns, &[Cow<'_, str>])) -> Result<()> {
    let mut reader = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut buffer = vec![];
    reader.read_until(b'\n', &mut buffer)?;
    let header = String::from_utf8_lossy(&buffer).trim().to_string();
    let columns = Columns(
        split(&header)
            .into_iter()
            .enumerate()
            .map(|(i, name)| (name.to_string(), i))
            .collect(),
    );
    ensure!(
        ["UUID", "AddNo_Full", "StNam_Full", "Zip_Code", "Placement"]
            .iter()
            .all(|c| columns.0.contains_key(*c)),
        "{} is not a NAD text file",
        path.display()
    );
    loop {
        buffer.clear();
        if reader.read_until(b'\n', &mut buffer)? == 0 {
            return Ok(());
        }
        let line = String::from_utf8_lossy(&buffer);
        let line = line.trim_end_matches(['\n', '\r']);
        if !line.is_empty() {
            f(&columns, &split(line));
        }
    }
}

/// The registry rows for these UUIDs. A missing one means the release changed: resample.
pub fn lookup(path: &Path, uuids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each_row(path, |columns, fields| {
        let uuid = columns.get(fields, "UUID");
        if uuids.contains(uuid)
            && let Some(row) = address(columns, fields)
        {
            found.insert(row.uuid.clone(), row);
        }
    })?;
    if let Some(missing) = uuids.iter().find(|u| !found.contains_key(*u)) {
        bail!(
            "NAD row {missing} is not in {} (or no longer a plain address)",
            path.display()
        );
    }
    Ok(found)
}

fn key(seed: &str, what: &str, uuid: &str) -> u64 {
    u64::from_str_radix(
        &sha256_hex(format!("{seed}|{what}|{uuid}").as_bytes())[..16],
        16,
    )
    .expect("hex")
}

fn density(zip_rows: usize) -> &'static str {
    match zip_rows {
        10_000.. => "urban",
        2_000.. => "suburban",
        _ => "rural",
    }
}

fn query(a: &Address, unit_style: &str, with_zip: bool) -> String {
    let unit = match (a.unit.is_empty(), unit_style) {
        (true, _) => String::new(),
        (false, "hash") => format!(" #{}", a.unit),
        (false, "word") => format!(" Unit {}", a.unit),
        (false, _) => format!(" Apt {}", a.unit),
    };
    let zip = if with_zip {
        format!(" {}", a.zip)
    } else {
        String::new()
    };
    format!(
        "{} {}{unit}, {}, {}{zip}",
        a.number, a.street, a.city, a.state
    )
}

fn typo(text: &str, h: u64) -> Option<String> {
    let words: Vec<&str> = text.split(' ').collect();
    let longest = words.iter().max_by_key(|w| w.chars().count())?;
    let letters: Vec<char> = longest.chars().collect();
    if letters.len() < 5 || !letters.iter().all(|c| c.is_alphabetic()) {
        return None;
    }
    let at = 1 + (h as usize) % (letters.len() - 2);
    if letters[at] == letters[at + 1] {
        return None;
    }
    let mut swapped = letters.clone();
    swapped.swap(at, at + 1);
    let swapped: String = swapped.into_iter().collect();
    Some(text.replacen(longest, &swapped, 1))
}

fn abbreviate(street: &str) -> Option<String> {
    let words: Vec<String> = street
        .split(' ')
        .map(|w| {
            TYPES
                .iter()
                .chain(DIRECTIONS.iter())
                .find(|(full, _)| *full == w)
                .map_or(w, |(_, short)| short)
                .to_string()
        })
        .collect();
    let out = words.join(" ");
    (out != street).then_some(out)
}

const HOUSE_KINDS: [&str; 13] = [
    "no_postcode",
    "with_country",
    "typo_street",
    "typo_town",
    "wrong_postcode",
    "wrong_town",
    "reordered",
    "uppercase",
    "lowercase",
    "abbreviated",
    "no_commas",
    "state_name",
    "zip_plus4",
];
const UNIT_KINDS: [&str; 2] = ["unit_hash", "unit_word"];

/// (kind, query, goodMatch expectation): true when the edit leaves the address consistent, null
/// (either answer is fine) when it contradicts itself.
fn mutate(
    kind: &str,
    a: &Address,
    h: u64,
    zips: &[String],
    cities: &[String],
) -> Option<(String, Option<bool>)> {
    let pick = |pool: &[String], own: &str| {
        let others: Vec<_> = pool.iter().filter(|p| *p != own).collect();
        (!others.is_empty()).then(|| others[(h as usize) % others.len()].clone())
    };
    let edited = |f: &dyn Fn(&mut Address)| {
        let mut copy = a.clone();
        f(&mut copy);
        query(&copy, "apt", true)
    };
    let (text, consistent) = match kind {
        "no_postcode" => (query(a, "apt", false), true),
        "with_country" => (format!("{}, United States", query(a, "apt", true)), true),
        "typo_street" => {
            let street = typo(&a.street, h)?;
            (edited(&|c| c.street = street.clone()), false)
        }
        "typo_town" => {
            let city = typo(&a.city, h)?;
            (edited(&|c| c.city = city.clone()), false)
        }
        "wrong_postcode" => {
            let zip = pick(zips, &a.zip)?;
            (edited(&|c| c.zip = zip.clone()), false)
        }
        "wrong_town" => {
            let city = pick(cities, &a.city)?;
            (edited(&|c| c.city = city.clone()), false)
        }
        "reordered" => {
            let unit = if a.unit.is_empty() {
                String::new()
            } else {
                format!(" Apt {}", a.unit)
            };
            (
                format!(
                    "United States, {} {}, {}, {} {}{unit}",
                    a.state, a.zip, a.city, a.number, a.street
                ),
                true,
            )
        }
        "uppercase" => (query(a, "apt", true).to_uppercase(), true),
        "lowercase" => (query(a, "apt", true).to_lowercase(), true),
        "abbreviated" => {
            let street = abbreviate(&a.street)?;
            (edited(&|c| c.street = street.clone()), true)
        }
        "no_commas" => (query(a, "apt", true).replace(',', ""), true),
        "state_name" => {
            let name = STATES.iter().find(|(code, _)| *code == a.state)?.1;
            (query(a, "apt", true).replace(&a.state, name), true)
        }
        "zip_plus4" => {
            ensure_plus4(&a.plus4)?;
            (format!("{}-{}", query(a, "apt", true), a.plus4), true)
        }
        "unit_hash" | "unit_word" if !a.unit.is_empty() => {
            (query(a, kind.trim_start_matches("unit_"), true), true)
        }
        _ => return None,
    };
    (text != query(a, "apt", true)).then_some((text, consistent.then_some(true)))
}

fn ensure_plus4(plus4: &str) -> Option<()> {
    (plus4.len() == 4 && plus4.bytes().all(|b| b.is_ascii_digit())).then_some(())
}

fn plain(unit: &str) -> bool {
    !unit.is_empty() && unit.len() <= 6 && unit.chars().all(|c| c.is_ascii_alphanumeric())
}

struct Candidate {
    key: u64,
    address: Address,
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl Eq for Candidate {}
impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key.cmp(&other.key)
    }
}

/// Seed lines for `tests/registry/us.seeds.jsonl`: one per location plus its degraded variants.
pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_stem()
        .and_then(|s| s.to_str())
        .context("NAD file name")?
        .to_lowercase();
    let mut zip_rows: HashMap<String, usize> = HashMap::new();
    let mut pools: BTreeMap<&str, BinaryHeap<Candidate>> = BTreeMap::new();
    let (mut rows, mut trusted) = (0usize, 0usize);
    for_each_row(path, |columns, fields| {
        let Some(a) = address(columns, fields) else {
            return;
        };
        rows += 1;
        *zip_rows.entry(a.zip.clone()).or_default() += 1;
        let kind = match (TRUSTED.contains(&a.placement.as_str()), plain(&a.unit)) {
            (true, true) => "unit",
            (true, false) if a.unit.is_empty() => "house",
            (false, false) if a.unit.is_empty() => "broad",
            _ => return,
        };
        trusted += usize::from(kind != "broad");
        let pool = pools.entry(kind).or_default();
        let k = key(seed, "pick", &a.uuid);
        if pool.len() < POOL || pool.peek().is_some_and(|worst| k < worst.key) {
            pool.push(Candidate { key: k, address: a });
            if pool.len() > POOL {
                pool.pop();
            }
        }
    })?;
    eprintln!(
        "{rows} usable rows, {trusted} with a trusted placement, {} ZIPs",
        zip_rows.len()
    );
    // Ascending by hash: the smallest first, so a prefix of each list is a uniform draw.
    let ordered: BTreeMap<&str, Vec<Candidate>> = pools
        .into_iter()
        .map(|(kind, pool)| (kind, pool.into_sorted_vec()))
        .collect();
    let mut zips_by_state: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut cities_by_state: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for c in ordered.values().flatten() {
        zips_by_state
            .entry(c.address.state.clone())
            .or_default()
            .push(c.address.zip.clone());
        cities_by_state
            .entry(c.address.state.clone())
            .or_default()
            .push(c.address.city.clone());
    }
    let mut lines = vec![];
    let mut taken_streets = HashSet::new();
    let mut taken_zips: HashMap<String, usize> = HashMap::new();
    for (kind, bucket, target) in TARGETS {
        let candidates = ordered.get(kind).map_or(&[][..], Vec::as_slice);
        let mut done = 0;
        let mut i = 0;
        while done < target && i < candidates.len() {
            let a = &candidates[i].address;
            i += 1;
            if bucket != "any" && density(zip_rows[&a.zip]) != bucket {
                continue;
            }
            let street = format!("{}|{}|{}", a.state, a.city, a.street).to_lowercase();
            if taken_streets.contains(&street)
                || taken_zips.get(&a.zip).copied() >= Some(MAX_PER_ZIP)
            {
                continue;
            }
            taken_streets.insert(street);
            *taken_zips.entry(a.zip.clone()).or_default() += 1;
            done += 1;
            let tags = |extra: &[&str]| {
                let mut tags = vec![
                    "registry-sample".to_string(),
                    "us".into(),
                    "nad".into(),
                    release.clone(),
                    format!("kind:{kind}"),
                    format!("density:{bucket}"),
                    format!("state:{}", a.state.to_lowercase()),
                    format!(
                        "placement:{}",
                        a.placement
                            .to_lowercase()
                            .split(|c: char| !c.is_alphanumeric())
                            .filter(|w| !w.is_empty())
                            .collect::<Vec<_>>()
                            .join("_")
                    ),
                ];
                tags.extend(extra.iter().map(|t| t.to_string()));
                tags
            };
            let regions = json!([format!("US-{}", a.state)]);
            let cohort = if kind == "broad" {
                "us_broad".to_string()
            } else {
                format!("us_{kind}_{bucket}")
            };
            // a lookup tolerance for points of unknown quality; trusted ones use the default
            let radius = (kind == "broad").then_some(0.5);
            let mut line = json!({
                "kind": "address", "source": "nad", "ref": a.uuid,
                "cohort": cohort, "queries": [query(a, "apt", true)], "label": query(a, "apt", true),
                "tags": tags(&[]), "extra_regions": regions,
            });
            if let Some(radius) = radius {
                line["radius_km"] = json!(radius);
            }
            lines.push(line);
            // Half the locations also get one damaged variant; same location key, so it is not
            // an independent sample.
            if key(seed, "degrade", &a.uuid).is_multiple_of(2) {
                let kinds: Vec<&str> = HOUSE_KINDS
                    .iter()
                    .chain(UNIT_KINDS.iter())
                    .copied()
                    .collect();
                let h = key(seed, "kind", &a.uuid);
                let zips = &zips_by_state[&a.state];
                let cities = &cities_by_state[&a.state];
                let found = (0..kinds.len()).find_map(|n| {
                    let kind = kinds[(h as usize + n) % kinds.len()];
                    mutate(kind, a, h, zips, cities).map(|(q, good)| (kind, q, good))
                });
                if let Some((mutation, text, good)) = found {
                    let mut line = json!({
                        "kind": "address", "source": "nad", "ref": a.uuid, "cohort": "us_degraded",
                        "queries": [text], "tags": tags(&["variant", "degraded", mutation]),
                        "extra_regions": regions, "good_match": good,
                    });
                    if let Some(radius) = radius {
                        line["radius_km"] = json!(radius);
                    }
                    lines.push(line);
                }
            }
        }
        eprintln!("{kind}/{bucket}: {done} of {target}");
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_address() -> Address {
        Address {
            uuid: "u".into(),
            number: "417".into(),
            street: "Foxtail Lily Drive".into(),
            unit: "4B".into(),
            city: "Holly Springs".into(),
            state: "NC".into(),
            zip: "27540".into(),
            plus4: "1234".into(),
            point: [35.6, -78.8],
            placement: "Structure - Rooftop".into(),
        }
    }

    #[test]
    fn splits_quoted_fields() {
        assert_eq!(split("a,b,,c"), ["a", "b", "", "c"]);
        assert_eq!(split(r#"a,"b,""x""",c"#), ["a", r#"b,"x""#, "c"]);
    }

    #[test]
    fn rejects_streets_that_are_not_names() {
        assert!(street_name("South Road"));
        assert!(street_name("Broadway"));
        assert!(!street_name("Apt Broadway Village Drive"));
        assert!(!street_name("Southwest"));
        assert!(!street_name("Main St #4"));
        assert!(!street_name("X"));
        // a direction or type written twice, in full and abbreviated
        assert!(!street_name("West W Red Oak St Street"));
        assert!(street_name("North West Street"));
    }

    #[test]
    fn rejects_placeholder_towns() {
        assert!(usable_city("Cuba"));
        assert!(!usable_city("UNINCORPORATED"));
        assert!(!usable_city("Not stated"));
        assert!(!usable_city("Denton County"));
    }

    #[test]
    fn titles_street_names() {
        assert_eq!(title("SOUTH Road"), "South Road");
        assert_eq!(title("NW 5TH st"), "NW 5th St");
        assert_eq!(title("us highway 1"), "US Highway 1");
    }

    #[test]
    fn every_mutation_is_deterministic_and_changes_the_query() {
        let a = sample_address();
        let zips = vec!["27540".to_string(), "27511".into()];
        let cities = vec!["Holly Springs".to_string(), "Cary".into()];
        let base = query(&a, "apt", true);
        assert_eq!(
            base,
            "417 Foxtail Lily Drive Apt 4B, Holly Springs, NC 27540"
        );
        for kind in HOUSE_KINDS.iter().chain(UNIT_KINDS.iter()) {
            let one = mutate(kind, &a, 7, &zips, &cities).unwrap_or_else(|| panic!("{kind}"));
            assert_ne!(one.0, base, "{kind}");
            assert_eq!(Some(one.clone()), mutate(kind, &a, 7, &zips, &cities));
            // contradictions accept either goodMatch value, consistent edits expect true
            let contradictory = kind.starts_with("typo") || kind.starts_with("wrong");
            assert_eq!(one.1, (!contradictory).then_some(true), "{kind}");
        }
        assert_eq!(
            mutate("wrong_postcode", &a, 7, &zips, &cities).unwrap().0,
            "417 Foxtail Lily Drive Apt 4B, Holly Springs, NC 27511"
        );
        assert_eq!(
            mutate("abbreviated", &a, 7, &zips, &cities).unwrap().0,
            "417 Foxtail Lily Dr Apt 4B, Holly Springs, NC 27540"
        );
    }

    /// A small synthetic release: the draw must not depend on row order, and must keep one
    /// address per street, units apart from houses, and only trusted placements.
    #[test]
    fn sampling_is_order_independent_and_respects_the_rules() {
        let header = "AddNo_Full,StNam_Full,Unit,Inc_Muni,Post_City,State,Zip_Code,Plus_4,UUID,Longitude,Latitude,Placement,Lifecycle";
        let mut rows: Vec<String> = vec![];
        for i in 0..400 {
            let unit = if i % 5 == 0 { "2A" } else { "" };
            let placement = if i % 7 == 0 {
                "Unknown"
            } else {
                "Structure - Rooftop"
            };
            // two houses on each street: only one may be sampled
            rows.push(format!(
                "{},MAIN{} Road,{unit},Town,Not stated,TX,{:05},,{{U{i}}},-97.5,30.5,{placement},",
                100 + i,
                i / 2,
                78000 + i % 30
            ));
        }
        let dir = std::env::temp_dir().join(format!("nad-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, rows: &[String]| {
            let path = dir.join(name);
            std::fs::write(&path, format!("{header}\n{}\n", rows.join("\n"))).unwrap();
            path
        };
        let forward = sample(&write("nad_r1.txt", &rows), "s").unwrap();
        let reversed: Vec<_> = rows.iter().rev().cloned().collect();
        let backward = sample(&write("nad_r1b.txt", &reversed), "s").unwrap();
        let refs = |lines: &[Value]| -> Vec<String> {
            lines
                .iter()
                .map(|l| l["ref"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(refs(&forward), refs(&backward));
        let base: Vec<_> = forward
            .iter()
            .filter(|l| l["cohort"] != "us_degraded")
            .collect();
        assert!(!base.is_empty());
        let streets: HashSet<_> = base
            .iter()
            .map(|l| {
                l["queries"][0]
                    .as_str()
                    .unwrap()
                    .split(',')
                    .next()
                    .unwrap()
                    .split(' ')
                    .nth(1)
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(streets.len(), base.len(), "one address per street");
        assert!(
            base.iter()
                .all(|l| !l["tags"].to_string().contains("unknown"))
        );
        let found = lookup(
            &write("nad_r1.txt", &rows),
            &refs(&forward).into_iter().collect(),
        )
        .unwrap();
        assert_eq!(
            found.len(),
            refs(&forward).into_iter().collect::<HashSet<_>>().len()
        );
        assert!(
            lookup(
                &write("nad_r1.txt", &rows),
                &BTreeSet::from(["{nope}".to_string()])
            )
            .is_err()
        );
        std::fs::remove_dir_all(dir).ok();
    }
}
