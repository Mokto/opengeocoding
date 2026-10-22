//! Builds the gold corpus from licensed authoritative sources, never from a geocoder's answers.
//! Local registry snapshots are prepared by the adapters in `registry/`.

use crate::{registry, report::sorted, run::utc_now, score::SUGGEST_LIMIT};
use anyhow::{Context, Result, bail, ensure};
use indexmap::IndexMap;
use opengeocoding::eval::{holdout, sha256_hex};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
    sync::LazyLock,
    time::Duration,
};

const UA: &str =
    "opengeocoding-eval/0.1 (https://github.com/Mokto/opengeocoding; test corpus curation)";
static NON_LATIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^\x00-\x{024f}\s]").unwrap());
static NUMBERS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-?[\d.]+").unwrap());

fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

/// One curated line of `tests/gold.seeds.jsonl`; `queries` share every other field.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Seed {
    kind: String,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    cohort: Option<String>,
    #[serde(default)]
    qid: Option<String>,
    queries: Vec<String>,
    #[serde(default)]
    radius_km: Option<f64>,
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    max_level: Option<String>,
    #[serde(default)]
    must_pass: bool,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    known_failure: Option<String>,
    #[serde(default)]
    country: Option<Vec<String>>,
    #[serde(default)]
    extra_regions: Vec<String>,
    #[serde(default)]
    region_qid: Option<String>,
    #[serde(default)]
    review: Option<Value>,
    /// Absent means true; null means either is acceptable.
    #[serde(default, deserialize_with = "present")]
    good_match: Option<Value>,
    #[serde(default)]
    admin_level: Option<u64>,
    #[serde(default)]
    house_number: Option<String>,
    #[serde(default)]
    postcode: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default, rename = "ref")]
    reference: Option<String>,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    any_of: Option<Vec<Value>>,
    #[serde(default)]
    suggest_address: Option<crate::model::SuggestAddressExpect>,
}

#[derive(Default)]
struct Item {
    point: Option<[f64; 2]>,
    iso: BTreeSet<String>,
    codes: BTreeSet<String>,
    label: String,
    subdivision: bool,
}

struct Found {
    point: [f64; 2],
    components: IndexMap<&'static str, String>,
    radius_floor_km: f64,
}

fn license(source: &str) -> &'static str {
    if let Some(registry) = registry::get(source) {
        return registry.license;
    }
    match source {
        "ban" => "etalab-2.0",
        "census" => "US-Government-Public-Domain",
        "postcodes.io" => "OGL-UK-3.0",
        "kartverket" | "cartociudad" => "CC-BY-4.0",
        "swisstopo" => "swisstopo-OGD",
        _ => "CC0-1.0",
    }
}

fn source_url(source: &str, reference: &str) -> String {
    if let Some(registry) = registry::get(source) {
        return (registry.url)(reference);
    }
    match source {
        "wikidata" => format!("https://www.wikidata.org/wiki/{reference}"),
        "gleif" => format!("https://search.gleif.org/#/record/{reference}"),
        "ban" => format!("https://adresse.data.gouv.fr/base-adresse-nationale/{reference}"),
        "pdok" => {
            format!("https://api.pdok.nl/bzk/locatieserver/search/v3_1/lookup?id={reference}")
        }
        "census" => format!("https://geocoding.geo.census.gov/ (TIGER line {reference})"),
        "postcodes.io" => format!("https://api.postcodes.io/postcodes/{reference}"),
        "kartverket" => format!("https://ws.geonorge.no/adresser/v1/sok?sok={reference}"),
        "swisstopo" => format!(
            "https://api3.geo.admin.ch/rest/services/api/SearchServer?type=locations&searchText={reference}"
        ),
        "cartociudad" => {
            format!("https://www.cartociudad.es/geocoder/api/geocoder/findJsonp?q={reference}")
        }
        _ => String::new(),
    }
}

pub fn slug(text: &str) -> String {
    static NON_ALNUM: LazyLock<Regex> = LazyLock::new(|| Regex::new("[^a-z0-9]+").unwrap());
    let slug: String = NON_ALNUM
        .replace_all(&text.to_lowercase(), "-")
        .trim_matches('-')
        .chars()
        .take(48)
        .collect();
    if slug.is_empty() {
        "blank".into()
    } else {
        slug
    }
}

/// "Q1748" from a SPARQL binding of an entity URI.
fn entity(binding: &Value) -> String {
    binding["value"]
        .as_str()
        .unwrap_or("")
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string()
}

/// The postcode of a "number street, postcode town" reference: the last five-digit number, since
/// a house number can have five digits too.
fn last_five_digits(text: &str) -> Result<String> {
    static FIVE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\d{5}\b").unwrap());
    Ok(FIVE
        .find_iter(text)
        .last()
        .context("no five-digit postcode")?
        .as_str()
        .into())
}

fn round6(value: f64) -> f64 {
    crate::score::round(value, 6)
}

#[derive(Clone)]
struct Sources {
    client: Client,
}

impl Sources {
    async fn get(&self, url: &str, params: &[(&str, &str)]) -> Result<Value> {
        let mut last = None;
        // Public reference APIs occasionally time out; this is curation, not the evaluated service.
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            // Wikidata cuts a body short when a query hits its time limit: retry that too.
            let response = self
                .client
                .get(url)
                .query(params)
                .send()
                .await
                .and_then(|r| r.error_for_status());
            match response {
                Ok(response) => match response.json().await {
                    Ok(value) => return Ok(value),
                    Err(error) => last = Some(error),
                },
                Err(error) => last = Some(error),
            }
        }
        Err(last.expect("attempted").into())
    }

    async fn sparql(&self, query: &str) -> Result<Vec<Value>> {
        let result = self
            .get(
                "https://query.wikidata.org/sparql",
                &[("format", "json"), ("query", query)],
            )
            .await?;
        Ok(result["results"]["bindings"]
            .as_array()
            .cloned()
            .unwrap_or_default())
    }

    /// Coordinates, country and every ISO 3166-2 subdivision on the located-in (P131) chain.
    async fn wikidata(&self, qids: &BTreeSet<String>) -> Result<BTreeMap<String, Item>> {
        let mut found: BTreeMap<String, Item> = BTreeMap::new();
        let ordered: Vec<_> = qids.iter().collect();
        for batch in ordered.chunks(50) {
            let values = batch
                .iter()
                .map(|q| format!("wd:{q}"))
                .collect::<Vec<_>>()
                .join(" ");
            let qid = |row: &Value| entity(&row["item"]);
            let own = format!(
                r#"SELECT ?item ?coord ?iso ?label ?own WHERE {{ VALUES ?item {{ {values} }}
                OPTIONAL {{ ?item wdt:P625 ?coord }} OPTIONAL {{ ?item wdt:P17/wdt:P297 ?iso }} OPTIONAL {{ ?item wdt:P300 ?own }}
                OPTIONAL {{ ?item rdfs:label ?label FILTER(lang(?label) = "en") }} }}"#
            );
            // Each item's located-in (P131) chain, then those areas' codes: one transitive query
            // that also returns the codes repeats them once per path and takes Wikidata ~30 s.
            let chain = format!(
                "SELECT DISTINCT ?item ?area WHERE {{ VALUES ?item {{ {values} }} ?item wdt:P131* ?area }}"
            );
            let (own, chain) = tokio::try_join!(self.sparql(&own), self.sparql(&chain))?;
            for row in own {
                let item = found.entry(qid(&row)).or_default();
                if let Some(coord) = row["coord"]["value"].as_str()
                    && item.point.is_none()
                {
                    let numbers: Vec<f64> = NUMBERS
                        .find_iter(coord)
                        .filter_map(|m| m.as_str().parse().ok())
                        .collect();
                    ensure!(numbers.len() == 2, "coordinate {coord}");
                    item.point = Some([round6(numbers[1]), round6(numbers[0])]);
                }
                if let Some(iso) = row["iso"]["value"].as_str() {
                    item.iso.insert(iso.to_lowercase());
                }
                item.label = row["label"]["value"].as_str().unwrap_or("").into();
                item.subdivision |= row.get("own").is_some();
            }
            let chain: Vec<(String, String)> = chain
                .iter()
                .map(|row| (qid(row), entity(&row["area"])))
                .collect();
            let areas: BTreeSet<&str> = chain.iter().map(|(_, a)| a.as_str()).collect();
            // area -> (ISO 3166-2 code if it isn't a country, ISO 3166-1 code if it is)
            let mut codes: BTreeMap<String, (Vec<String>, Vec<String>)> = BTreeMap::new();
            for group in areas.into_iter().collect::<Vec<_>>().chunks(200) {
                let values = group
                    .iter()
                    .map(|a| format!("wd:{a}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                for row in self
                    .sparql(&format!(
                        "SELECT ?area ?code ?iso WHERE {{ VALUES ?area {{ {values} }}
                        OPTIONAL {{ ?area wdt:P300 ?code }} OPTIONAL {{ ?area wdt:P297 ?iso }} }}"
                    ))
                    .await?
                {
                    let entry = codes.entry(entity(&row["area"])).or_default();
                    match (row["code"]["value"].as_str(), row["iso"]["value"].as_str()) {
                        (_, Some(iso)) => entry.1.push(iso.to_lowercase()),
                        (Some(code), None) => entry.0.push(code.to_uppercase()),
                        _ => {}
                    }
                }
            }
            for (item, area) in &chain {
                let Some((subdivisions, countries)) = codes.get(area) else {
                    continue;
                };
                let item = found.entry(item.clone()).or_default();
                item.codes.extend(subdivisions.iter().cloned());
                // Territories with their own ISO code (American Samoa, Åland, Réunion) answer
                // as either that code or the sovereign country's.
                if !item.iso.is_empty() {
                    item.iso.extend(countries.iter().cloned());
                }
            }
        }
        let missing: Vec<_> = qids.iter().filter(|q| !found.contains_key(*q)).collect();
        ensure!(missing.is_empty(), "Wikidata items not found: {missing:?}");
        for item in found.values_mut() {
            // Dutch places sit in the Kingdom of the Netherlands, which has no ISO alpha-2 code.
            if item.iso.is_empty() {
                item.iso = item.codes.iter().map(|c| c[..2].to_lowercase()).collect();
            }
        }
        Ok(found)
    }

    /// Every located item with a population whose English label is one of `labels`: label ->
    /// (item, population), most populous first.
    async fn namesakes(
        &self,
        labels: &BTreeSet<String>,
    ) -> Result<BTreeMap<String, Vec<(String, f64)>>> {
        let mut found: BTreeMap<String, Vec<(String, f64)>> = BTreeMap::new();
        for batch in labels.iter().collect::<Vec<_>>().chunks(50) {
            let values = batch
                .iter()
                .map(|l| Ok(format!("{}@en", serde_json::to_string(l)?)))
                .collect::<Result<Vec<_>>>()?
                .join(" ");
            for row in self
                .sparql(&format!(
                    "SELECT ?label ?item (MAX(?pop) AS ?population) WHERE {{ VALUES ?label {{ {values} }}
                    ?item rdfs:label ?label; wdt:P625 ?coord; wdt:P1082 ?pop }} GROUP BY ?label ?item"
                ))
                .await?
            {
                let population = row["population"]["value"]
                    .as_str()
                    .and_then(|p| p.parse().ok())
                    .context("population")?;
                found
                    .entry(row["label"]["value"].as_str().unwrap_or("").into())
                    .or_default()
                    .push((entity(&row["item"]), population));
            }
        }
        for items in found.values_mut() {
            items.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        }
        Ok(found)
    }

    async fn lookup(&self, source: &str, query: &str) -> Result<(Found, String)> {
        let first = |re: &str, text: &str| -> Result<String> {
            Ok(Regex::new(re)?
                .find(text)
                .with_context(|| format!("{re} in {text:?}"))?
                .as_str()
                .to_string())
        };
        let found = |point, components: &[(&'static str, String)]| Found {
            point,
            components: components.iter().cloned().collect(),
            radius_floor_km: 0.,
        };
        match source {
            "ban" => {
                if let Some(code) = query.strip_prefix("municipality:") {
                    let result = self
                        .get(
                            "https://api-adresse.data.gouv.fr/search/",
                            &[("q", code), ("type", "municipality"), ("limit", "1")],
                        )
                        .await?;
                    let feature = &result["features"][0];
                    let city = feature["properties"]["citycode"].as_str().unwrap_or("");
                    ensure!(
                        city.ends_with(&code[code.len() - 2..]),
                        "BAN municipality {feature}"
                    );
                    let [lon, lat] = [0, 1].map(|i| {
                        feature["geometry"]["coordinates"][i]
                            .as_f64()
                            .unwrap_or(f64::NAN)
                    });
                    let id = feature["properties"]["id"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    return Ok((found([lat, lon], &[("postcode", code.into())]), id));
                }
                let result = self
                    .get(
                        "https://api-adresse.data.gouv.fr/search/",
                        &[("q", query), ("limit", "1")],
                    )
                    .await?;
                let feature = &result["features"][0];
                let props = &feature["properties"];
                let house = query.split_whitespace().next().unwrap_or("").to_string();
                let postcode = last_five_digits(query)?;
                ensure!(
                    props["type"] == "housenumber" && props["score"].as_f64().unwrap_or(0.) > 0.8,
                    "BAN match {props}"
                );
                ensure!(
                    props["housenumber"] == house.as_str()
                        && props["postcode"] == postcode.as_str(),
                    "BAN components {props}"
                );
                let [lon, lat] = [0, 1].map(|i| {
                    feature["geometry"]["coordinates"][i]
                        .as_f64()
                        .unwrap_or(f64::NAN)
                });
                let road = props["street"].as_str().unwrap_or("").to_string();
                let id = props["id"].as_str().unwrap_or("").to_string();
                Ok((
                    found(
                        [lat, lon],
                        &[
                            ("house_number", house),
                            ("road", road),
                            ("postcode", postcode),
                        ],
                    ),
                    id,
                ))
            }
            "pdok" => {
                let (kind, text) = query
                    .split_once(':')
                    .filter(|(k, _)| *k == "postcode")
                    .unwrap_or(("adres", query));
                let result = self
                    .get(
                        "https://api.pdok.nl/bzk/locatieserver/search/v3_1/free",
                        &[("q", text), ("fq", &format!("type:{kind}")), ("rows", "1")],
                    )
                    .await?;
                let doc = &result["response"]["docs"][0];
                let numbers: Vec<f64> = NUMBERS
                    .find_iter(doc["centroide_ll"].as_str().unwrap_or(""))
                    .filter_map(|m| m.as_str().parse().ok())
                    .collect();
                ensure!(numbers.len() == 2, "PDOK centroid {doc}");
                let postcode = first(r"\b\d{4}[A-Z]{2}\b", text)?;
                ensure!(doc["postcode"] == postcode.as_str(), "PDOK postcode {doc}");
                let mut components = vec![("postcode", postcode)];
                if kind == "adres" {
                    let house = Regex::new(r" (\d+\w*) \d{4}")?
                        .captures(text)
                        .context("PDOK house number")?[1]
                        .to_string();
                    let number: u64 = first(r"\d+", &house)?.parse()?;
                    ensure!(
                        doc["huisnummer"].as_u64() == Some(number),
                        "PDOK house {doc}"
                    );
                    components.push(("house_number", house));
                    components.push(("road", doc["straatnaam"].as_str().unwrap_or("").into()));
                }
                let id = doc["id"].as_str().unwrap_or("").to_string();
                Ok((found([numbers[1], numbers[0]], &components), id))
            }
            "census" => {
                let result = self
                    .get(
                        "https://geocoding.geo.census.gov/geocoder/locations/onelineaddress",
                        &[
                            ("address", query),
                            ("benchmark", "Public_AR_Current"),
                            ("format", "json"),
                        ],
                    )
                    .await?;
                let matches = result["result"]["addressMatches"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                ensure!(matches.len() == 1, "Census matches {matches:?}");
                let matched = &matches[0];
                let house = query.split_whitespace().next().unwrap_or("").to_string();
                let postcode = last_five_digits(query)?;
                let address = matched["matchedAddress"].as_str().unwrap_or("");
                ensure!(
                    address.starts_with(&format!("{house} ")) && address.contains(&postcode),
                    "Census match {matched}"
                );
                let point = ["y", "x"]
                    .map(|axis| round6(matched["coordinates"][axis].as_f64().unwrap_or(f64::NAN)));
                let mut result = found(point, &[("house_number", house), ("postcode", postcode)]);
                // Census coordinates are interpolated along TIGER street ranges, not rooftop points.
                result.radius_floor_km = 0.3;
                let id = match &matched["tigerLine"]["tigerLineId"] {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                Ok((result, id))
            }
            "postcodes.io" => {
                if let Some(code) = query.strip_prefix("outcode:") {
                    let result = self
                        .get(&format!("https://api.postcodes.io/outcodes/{code}"), &[])
                        .await?;
                    let r = &result["result"];
                    let point = [
                        r["latitude"].as_f64().unwrap_or(f64::NAN),
                        r["longitude"].as_f64().unwrap_or(f64::NAN),
                    ];
                    return Ok((found(point, &[("postcode", code.into())]), code.into()));
                }
                let result = self
                    .get(&format!("https://api.postcodes.io/postcodes/{query}"), &[])
                    .await?;
                let r = &result["result"];
                let point = [
                    r["latitude"].as_f64().unwrap_or(f64::NAN),
                    r["longitude"].as_f64().unwrap_or(f64::NAN),
                ];
                let postcode = r["postcode"].as_str().unwrap_or("").to_string();
                Ok((found(point, &[("postcode", postcode.clone())]), postcode))
            }
            "kartverket" => {
                let (text, postcode) = query
                    .split_once('|')
                    .context("kartverket ref is ADDRESS|POSTCODE")?;
                let result = self
                    .get(
                        "https://ws.geonorge.no/adresser/v1/sok",
                        &[
                            ("sok", text),
                            ("postnummer", postcode),
                            ("treffPerSide", "2"),
                        ],
                    )
                    .await?;
                let hits = result["adresser"].as_array().cloned().unwrap_or_default();
                ensure!(hits.len() == 1, "Kartverket matches {hits:?}");
                let a = &hits[0];
                ensure!(
                    a["adressetekst"].as_str().unwrap_or("").to_lowercase() == text.to_lowercase()
                        && a["postnummer"] == postcode,
                    "Kartverket match {a}"
                );
                let house = text.rsplit(' ').next().unwrap_or("").to_string();
                let point = ["lat", "lon"]
                    .map(|k| round6(a["representasjonspunkt"][k].as_f64().unwrap_or(f64::NAN)));
                let id = format!(
                    "{}-{}",
                    a["kommunenummer"].as_str().unwrap_or(""),
                    a["adressekode"]
                );
                let road = a["adressenavn"].as_str().unwrap_or("").to_string();
                Ok((
                    found(
                        point,
                        &[
                            ("house_number", house),
                            ("road", road),
                            ("postcode", postcode.into()),
                        ],
                    ),
                    format!("{id}-{}", a["nummer"]),
                ))
            }
            "swisstopo" => {
                let result = self
                    .get(
                        "https://api3.geo.admin.ch/rest/services/api/SearchServer",
                        &[
                            ("searchText", query),
                            ("type", "locations"),
                            ("origins", "address"),
                            ("limit", "1"),
                        ],
                    )
                    .await?;
                let attrs = &result["results"][0]["attrs"];
                let label = Regex::new("<[^>]+>")?
                    .replace_all(attrs["label"].as_str().unwrap_or(""), "")
                    .to_lowercase();
                let words: Vec<&str> = query.split(' ').collect();
                // the last four digits: a house number can have four too ("Chieberg 1526 3855")
                let postcode = Regex::new(r"\b\d{4}\b")?
                    .find_iter(query)
                    .last()
                    .context("swisstopo postcode")?
                    .as_str()
                    .to_string();
                let at = words
                    .iter()
                    .position(|w| *w == postcode)
                    .context("swisstopo postcode")?;
                ensure!(
                    label.starts_with(&(words[..=at].join(" ").to_lowercase() + " ")),
                    "swisstopo match {attrs}"
                );
                let point = ["lat", "lon"].map(|k| round6(attrs[k].as_f64().unwrap_or(f64::NAN)));
                let id = attrs["featureId"].as_str().unwrap_or("").to_string();
                Ok((
                    found(
                        point,
                        &[
                            ("house_number", words[at - 1].into()),
                            ("postcode", postcode),
                        ],
                    ),
                    id,
                ))
            }
            "cartociudad" => {
                // the postcode stays out of the search text: it makes CartoCiudad miss
                let (text, postcode) = query
                    .split_once('|')
                    .context("cartociudad ref is ADDRESS, TOWN|POSTCODE")?;
                let result = self
                    .client
                    .get("https://www.cartociudad.es/geocoder/api/geocoder/findJsonp")
                    .query(&[("q", text)])
                    .send()
                    .await?
                    .error_for_status()?
                    .text()
                    .await?;
                let body = result
                    .trim()
                    .trim_start_matches("callback(")
                    .trim_end_matches(')');
                let r: Value = serde_json::from_str(body)?;
                let house = first(r"\d+\w*,", text)?.trim_end_matches(',').to_string();
                let postcode = postcode.to_string();
                ensure!(
                    r["type"] == "portal"
                        && r["portalNumber"]
                            .as_u64()
                            .is_some_and(|n| n.to_string() == house)
                        && r["postalCode"] == postcode.as_str(),
                    "CartoCiudad match {r}"
                );
                let point = ["lat", "lng"].map(|k| round6(r[k].as_f64().unwrap_or(f64::NAN)));
                let id = r["id"].as_str().unwrap_or("").to_string();
                Ok((
                    found(point, &[("house_number", house), ("postcode", postcode)]),
                    id,
                ))
            }
            other => bail!("unknown source {other}"),
        }
    }
}

/// (source, ref) of a seed answered by an address registry rather than Wikidata.
fn registry_ref(seed: &Seed) -> Option<(String, String)> {
    (matches!(seed.kind.as_str(), "address" | "postcode")
        && seed.source.as_deref() != Some("wikidata"))
    .then(|| Some((seed.source.clone()?, seed.reference.clone()?)))
    .flatten()
}

/// Every distinct registry reference, looked up once: one task per registry, so the slow ones
/// (the Census geocoder) don't hold up the rest, and requests to each stay sequential and spaced.
async fn lookups(
    sources: &Sources,
    seeds: &[Seed],
) -> Result<BTreeMap<(String, String), (Found, String)>> {
    let mut by_source: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for seed in seeds {
        if matches!(seed.kind.as_str(), "address" | "postcode")
            && !seed
                .source
                .as_deref()
                .is_some_and(|s| s == "wikidata" || registry::get(s).is_some())
        {
            let (source, reference) =
                registry_ref(seed).context("address seeds need a source and ref")?;
            by_source.entry(source).or_default().insert(reference);
        }
    }
    let mut tasks = tokio::task::JoinSet::new();
    for (source, refs) in by_source {
        let sources = sources.clone();
        tasks.spawn(async move {
            let mut out = vec![];
            for reference in refs {
                let found = sources
                    .lookup(&source, &reference)
                    .await
                    .with_context(|| format!("{source} {reference}"))?;
                out.push(((source.clone(), reference), found));
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            anyhow::Ok(out)
        });
    }
    let mut all = BTreeMap::new();
    while let Some(done) = tasks.join_next().await {
        all.extend(done??);
    }
    Ok(all)
}

pub async fn build(seeds_path: &Path, out: &Path, files: &HashMap<String, String>) -> Result<()> {
    let seeds: Vec<Seed> = crate::model::load_jsonl::<Value>(seeds_path)?
        .into_iter()
        .enumerate()
        .map(|(n, v)| {
            serde_json::from_value(v).with_context(|| format!("{}:{}", seeds_path.display(), n + 1))
        })
        .collect::<Result<_>>()?;
    let today = utc_now()[..10].to_string();
    let sources = Sources {
        client: Client::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(90))
            .build()?,
    };
    let mut qids = BTreeSet::new();
    for seed in &seeds {
        qids.extend(seed.qid.clone());
        qids.extend(seed.city.clone());
        qids.extend(seed.region_qid.clone());
        if seed.source.as_deref() == Some("wikidata") {
            qids.extend(seed.reference.clone());
        }
        for alt in seed.any_of.iter().flatten() {
            qids.extend(alt["qid"].as_str().map(String::from));
        }
    }
    let (wd, mut looked_up) = tokio::try_join!(sources.wikidata(&qids), lookups(&sources, &seeds))?;
    let mut local_entries = BTreeMap::new();
    for registry in registry::ALL {
        let refs: BTreeSet<String> = seeds
            .iter()
            .filter(|s| s.source.as_deref() == Some(registry.source))
            .filter_map(|s| s.reference.clone())
            .collect();
        if refs.is_empty() {
            continue;
        }
        let path = files.get(registry.input).with_context(|| {
            format!(
                "seeds cite {}: pass --{} FILE",
                registry.source, registry.input
            )
        })?;
        let entries = (registry.prepare)(Path::new(path), &refs)?;
        for (n, (id, entry)) in entries.into_iter().enumerate() {
            let point = match entry.point {
                registry::Point::Wgs84(point) => point,
                registry::Point::Lv95([easting, northing]) => {
                    let response = sources
                        .get(
                            registry::REFRAME,
                            &[
                                ("easting", &easting.to_string()),
                                ("northing", &northing.to_string()),
                                ("altitude", "0"),
                                ("format", "json"),
                            ],
                        )
                        .await
                        .with_context(|| format!("REFRAME EGAID {id}"))?;
                    let point = registry::reframed_point(&response)?;
                    if (n + 1).is_multiple_of(50) {
                        eprintln!(
                            "swisstopo: converted {} of {} LV95 points with REFRAME",
                            n + 1,
                            refs.len()
                        );
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    point
                }
            };
            let key = (registry.source.to_string(), id.clone());
            looked_up.insert(
                key.clone(),
                (
                    Found {
                        point: point.map(round6),
                        components: entry.components.clone(),
                        radius_floor_km: entry.radius_floor_km,
                    },
                    id,
                ),
            );
            local_entries.insert(key, entry);
        }
    }
    let typed_name = |seed: &Seed, query: &str| -> Option<(String, String)> {
        let qid = seed.qid.clone()?;
        let name = query.split(',').next()?.trim();
        (matches!(seed.kind.as_str(), "place" | "region" | "country")
            && !name.is_empty()
            && name == wd[&qid].label)
            .then(|| (qid, name.to_string()))
    };
    let labels: BTreeSet<String> = seeds
        .iter()
        .flat_map(|s| s.queries.iter().filter_map(|q| typed_name(s, q)))
        .map(|(_, name)| name)
        .collect();
    let namesakes = sources.namesakes(&labels).await?;
    let mut named = BTreeSet::new();
    // Any listed code is acceptable: OSM tags different levels per country (FR-IDF and FR-75C, IE-L and IE-D).
    let region = |qid: Option<&str>, extra: &[String]| -> Vec<String> {
        // Only ISO 3166-2 codes: Somaliland's "SL" sits on Sanaag's chain too.
        let mut codes: BTreeSet<String> = qid
            .map(|q| {
                wd[q]
                    .codes
                    .iter()
                    .filter(|c| c.contains('-'))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        codes.extend(extra.iter().cloned());
        codes.into_iter().collect()
    };
    let iso = |qid: &str| -> Vec<String> { wd[qid].iso.iter().cloned().collect() };
    let source = |name: &str, reference: &str| {
        let mut source = json!({ "name": name, "ref": reference, "url": source_url(name, reference), "license": license(name), "retrieved": today });
        if let Some(entry) = local_entries.get(&(name.to_string(), reference.to_string())) {
            for (key, value) in &entry.provenance {
                source[key] = value.clone();
            }
        }
        source
    };
    let constructed = json!({ "name": "constructed", "license": "CC0-1.0", "retrieved": today });
    let mut cases: Vec<Value> = vec![];
    for seed in &seeds {
        let good_match = seed.good_match.clone().unwrap_or(json!(true));
        let item = seed.qid.as_deref().map(|q| &wd[q]);
        let found = registry_ref(seed)
            .map(|key| looked_up.get(&key).context("looked up"))
            .transpose()?;
        for query in &seed.queries {
            let cohort = seed.cohort.clone().unwrap_or_else(|| seed.kind.clone());
            let (expect, location, label, case_source) = match seed.kind.as_str() {
                "place" => {
                    let qid = seed.qid.as_deref().context("place seeds need a qid")?;
                    let item = item.context("qid")?;
                    // Cities that are themselves ISO 3166-2 subdivisions (Tokyo, Budapest, Berlin) are correctly
                    // answered by that subdivision; the region-code check still rejects a different one.
                    let level = seed.level.clone().unwrap_or_else(|| {
                        if item.subdivision {
                            "region"
                        } else {
                            "locality"
                        }
                        .into()
                    });
                    let mut expect = json!({
                        "country": seed.country.clone().unwrap_or_else(|| iso(qid)),
                        "level": level,
                        "max_level": seed.max_level.as_deref().unwrap_or("house"),
                        "point": item.point.with_context(|| format!("{qid} has no coordinates"))?,
                        "radius_km": seed.radius_km.unwrap_or(15.),
                        "region": region(Some(seed.region_qid.as_deref().unwrap_or(qid)), &seed.extra_regions),
                        "good_match": good_match,
                    });
                    if let Some(postcode) = &seed.postcode {
                        expect["components"] = json!({ "postcode": postcode });
                    }
                    let case_source = match (&seed.source, &seed.reference) {
                        (Some(name), Some(reference)) => source(name, reference),
                        _ => source("wikidata", qid),
                    };
                    let location = seed
                        .location
                        .clone()
                        .unwrap_or_else(|| format!("wikidata:{qid}"));
                    (
                        expect,
                        location,
                        seed.label.clone().unwrap_or_else(|| item.label.clone()),
                        case_source,
                    )
                }
                "region" => {
                    let qid = seed.qid.as_deref().context("region seeds need a qid")?;
                    let item = item.context("qid")?;
                    let expect = json!({
                        "country": iso(qid), "level": "region", "max_level": "region", "area": "region",
                        "point": item.point, "radius_km": 400.0, "region": region(Some(qid), &[]),
                        "admin_level": seed.admin_level.context("region seeds need admin_level")?, "good_match": true,
                    });
                    (
                        expect,
                        format!("wikidata:{qid}"),
                        item.label.clone(),
                        source("wikidata", qid),
                    )
                }
                "country" => {
                    let qid = seed.qid.as_deref().context("country seeds need a qid")?;
                    let expect = json!({
                        "country": seed.country.clone().context("country seeds need country")?,
                        "level": "country", "max_level": seed.max_level.as_deref().unwrap_or("country"), "good_match": true,
                    });
                    (
                        expect,
                        format!("wikidata:{qid}"),
                        wd[qid].label.clone(),
                        source("wikidata", qid),
                    )
                }
                "landmark" => {
                    let qid = seed.qid.as_deref().context("landmark seeds need a qid")?;
                    let item = item.context("qid")?;
                    let mut components = serde_json::Map::new();
                    if let Some(house) = &seed.house_number {
                        components.insert("house_number".into(), json!(house));
                    }
                    if let Some(postcode) = &seed.postcode {
                        components.insert("postcode".into(), json!(postcode));
                    }
                    let expect = json!({
                        "country": iso(qid), "level": seed.level.as_deref().unwrap_or("house"), "max_level": "house",
                        "point": item.point, "radius_km": seed.radius_km.unwrap_or(0.25), "region": region(Some(qid), &[]),
                        "components": components, "good_match": good_match,
                    });
                    (
                        expect,
                        format!("wikidata:{qid}"),
                        item.label.clone(),
                        source("wikidata", qid),
                    )
                }
                // a landmark's name or code typed into SuggestAddress with `includeLandmarks`
                "suggest_landmark" => {
                    let qid = seed
                        .qid
                        .as_deref()
                        .context("suggest_landmark seeds need a qid")?;
                    let item = item.context("qid")?;
                    let expect = json!({
                        "country": seed.country.clone().unwrap_or_else(|| iso(qid)), "level": "landmark",
                        "max_level": "house",
                        "point": item.point.with_context(|| format!("{qid} has no coordinates"))?,
                        "radius_km": seed.radius_km.unwrap_or(1.), "region": [], "good_match": good_match,
                        "components": { "category": seed.category.as_deref().context("suggest_landmark seeds need a category")? },
                    });
                    (
                        expect,
                        format!("wikidata:{qid}"),
                        seed.label.clone().unwrap_or_else(|| item.label.clone()),
                        source("wikidata", qid),
                    )
                }
                "address" | "postcode" => {
                    let name = seed
                        .source
                        .as_deref()
                        .context("address seeds need a source")?;
                    let reference = seed
                        .reference
                        .as_deref()
                        .context("address seeds need a ref")?;
                    let (point, components, floor, country, regions, id) = match found {
                        None => {
                            let postcode =
                                seed.queries[0].split(',').next().unwrap_or("").to_string();
                            let point = wd[reference].point.context("coordinates")?;
                            (
                                point,
                                json!({ "postcode": postcode }),
                                0.,
                                iso(reference),
                                region(Some(reference), &[]),
                                reference.to_string(),
                            )
                        }
                        Some((found, id)) => {
                            let country = registry::get(name).map(|r| r.country).unwrap_or_else(
                                || match name {
                                    "ban" => "fr",
                                    "pdok" => "nl",
                                    "census" => "us",
                                    "kartverket" => "no",
                                    "swisstopo" => "ch",
                                    "cartociudad" => "es",
                                    _ => "gb",
                                },
                            );
                            (
                                found.point,
                                json!(found.components),
                                found.radius_floor_km,
                                // overseas departments answer with their own ISO code too
                                std::iter::once(country)
                                    .chain(
                                        found
                                            .components
                                            .get("postcode")
                                            .filter(|_| matches!(name, "ban" | "ban-csv"))
                                            .and_then(|pc| {
                                                Some(match pc.get(..3)? {
                                                    "971" => "gp",
                                                    "972" => "mq",
                                                    "973" => "gf",
                                                    "974" => "re",
                                                    "976" => "yt",
                                                    _ => return None,
                                                })
                                            }),
                                    )
                                    .map(String::from)
                                    .collect(),
                                region(seed.city.as_deref(), &seed.extra_regions),
                                id.clone(),
                            )
                        }
                    };
                    let house = seed.kind == "address";
                    let expect = json!({
                        "country": country,
                        "level": if house { "house" } else { "postcode" },
                        "max_level": if house { "house" } else { "postcode" },
                        "point": point,
                        "radius_km": seed.radius_km.unwrap_or(0.15).max(floor),
                        "region": regions,
                        "components": components,
                        "good_match": good_match,
                    });
                    let expect = match local_entries.get(&(name.to_string(), reference.to_string()))
                    {
                        Some(entry) => registry::with_number_aliases(expect, &entry.number_aliases),
                        None => expect,
                    };
                    (
                        expect,
                        format!("{}:{id}", registry::get(name).map_or(name, |r| r.location)),
                        seed.label.clone().unwrap_or_else(|| reference.to_string()),
                        source(name, &id),
                    )
                }
                "none" => (
                    json!({ "level": "none" }),
                    format!("none:{}", slug(query)),
                    "no geographic referent".into(),
                    constructed.clone(),
                ),
                "error" => (
                    json!({ "error": seed.error.clone().context("error seeds need error")? }),
                    seed.location
                        .clone()
                        .unwrap_or_else(|| format!("none:{}", slug(query))),
                    seed.label.clone().unwrap_or_default(),
                    constructed.clone(),
                ),
                "any_of" => {
                    let mut qids = vec![];
                    let alternatives = seed
                        .any_of
                        .clone()
                        .context("any_of seeds need alternatives")?
                        .into_iter()
                        .map(|mut alt| {
                            if let Some(qid) = alt.as_object_mut().and_then(|o| o.remove("qid")) {
                                let qid = qid.as_str().unwrap_or("").to_string();
                                alt["country"] = json!(iso(&qid));
                                alt["point"] = json!(wd[&qid].point);
                                alt["region"] = json!(region(Some(&qid), &[]));
                                qids.push(qid);
                            }
                            alt
                        })
                        .collect::<Vec<_>>();
                    let case_source = if seed.source.as_deref() == Some("constructed") {
                        constructed.clone()
                    } else {
                        source(
                            "wikidata",
                            seed.reference.as_deref().unwrap_or(&qids.join(",")),
                        )
                    };
                    let location = seed
                        .location
                        .clone()
                        .unwrap_or_else(|| format!("{cohort}:{}", slug(query)));
                    (
                        json!({ "any_of": alternatives }),
                        location,
                        seed.label.clone().unwrap_or_default(),
                        case_source,
                    )
                }
                "unscored" => (
                    Value::Null,
                    format!("{cohort}:{}", slug(query)),
                    seed.label.clone().unwrap_or_default(),
                    constructed.clone(),
                ),
                other => bail!("unknown seed kind {other}"),
            };
            let mut tags = seed.tags.clone();
            let non_latin = NON_LATIN.is_match(query);
            if non_latin {
                tags.push("non_latin".into());
            }
            let mut base = format!("{cohort}-{}", slug(query));
            if non_latin || base.ends_with("-blank") {
                // Non-Latin and blank queries slug poorly; name them by place plus a stable query hash.
                base = format!(
                    "{cohort}-{}-{}",
                    slug(&label),
                    &sha256_hex(query.as_bytes())[..6]
                );
            }
            let mut id = base.clone();
            let mut n = 2;
            while cases.iter().any(|c| c["id"] == id.as_str()) {
                id = format!("{base}-{n}");
                n += 1;
            }
            let mut case = json!({
                "id": id, "query": query, "cohort": cohort, "location": location, "label": label,
                "expect": expect, "must_pass": seed.must_pass, "tags": tags, "source": case_source,
                "holdout": holdout(&location),
                "review": { "status": "source-verified",
                            "note": "Expectation copied from the cited source by `evaluate build-gold`; human review pending." },
            });
            if let Some(review) = &seed.review {
                case["review"] = review.clone();
            }
            if let Some(known) = &seed.known_failure {
                case["known_failure"] = json!(known);
            }
            if seed.kind == "suggest_landmark" {
                case["include_landmarks"] = json!(true);
            }
            if let Some(address) = &seed.suggest_address {
                case["suggest_address"] = json!(address);
            }
            if let Some((qid, name)) = typed_name(seed, query)
                && named.insert((qid.clone(), name.clone()))
            {
                let all = namesakes.get(&name).map(Vec::as_slice).unwrap_or_default();
                let own = all.iter().find(|(q, _)| *q == qid).map_or(0., |(_, p)| *p);
                let larger: Vec<&str> = all
                    .iter()
                    .filter(|(q, p)| *q != qid && *p > own)
                    .map(|(q, _)| q.as_str())
                    .collect();
                if larger.len() < SUGGEST_LIMIT {
                    case["suggest"] =
                        json!({ "text": name, "rank": larger.len() + 1, "larger": larger });
                }
            }
            cases.push(case);
        }
    }
    apply_labels(&mut cases, seeds_path)?;
    let lines: Vec<_> = cases
        .into_iter()
        .map(|c| serde_json::to_string(&sorted(c)))
        .collect::<Result<_, _>>()?;
    std::fs::write(out, lines.join("\n") + "\n")?;
    let locations: BTreeSet<_> = lines
        .iter()
        .filter_map(|l| {
            serde_json::from_str::<Value>(l).ok()?["location"]
                .as_str()
                .map(String::from)
        })
        .collect();
    eprintln!(
        "{} cases, {} locations -> {}",
        lines.len(),
        locations.len(),
        out.display()
    );
    Ok(())
}

/// Triage labels for generated seeds, which are rewritten on every resample: `NAME.labels.jsonl`
/// beside `NAME.seeds.jsonl`, one `{"ref", "query", "known_failure"}` per case. A label that no
/// longer matches a case is an error, so labels cannot silently rot.
fn apply_labels(cases: &mut [Value], seeds_path: &Path) -> Result<()> {
    let Some(name) = seeds_path
        .to_str()
        .and_then(|p| p.strip_suffix(".seeds.jsonl"))
    else {
        return Ok(());
    };
    let path = std::path::PathBuf::from(format!("{name}.labels.jsonl"));
    if !path.exists() {
        return Ok(());
    }
    for (n, label) in crate::model::load_jsonl::<Value>(&path)?.iter().enumerate() {
        let (reference, query) = (label["ref"].as_str(), label["query"].as_str());
        let known = label["known_failure"].as_str().filter(|k| !k.is_empty());
        let (Some(reference), Some(query), Some(known)) = (reference, query, known) else {
            bail!(
                "{}:{}: needs ref, query and known_failure",
                path.display(),
                n + 1
            );
        };
        let case = cases
            .iter_mut()
            .find(|c| c["source"]["ref"] == reference && c["query"] == query)
            .with_context(|| {
                format!(
                    "{}:{}: no case for {reference} {query:?}",
                    path.display(),
                    n + 1
                )
            })?;
        case["known_failure"] = json!(known);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjudicated_isa_town_replaces_the_region_without_weakening_the_case() {
        let seeds: Vec<Seed> =
            crate::model::load_jsonl(Path::new("tests/gold.seeds.jsonl")).unwrap();
        let seed = seeds
            .iter()
            .find(|s| s.qid.as_deref() == Some("Q1017673"))
            .unwrap();
        assert_eq!(seed.region_qid.as_deref(), Some("Q838532"));
        let cases: Vec<crate::model::Case> =
            crate::model::load_jsonl(Path::new("tests/gold.jsonl")).unwrap();
        let case = cases
            .iter()
            .find(|c| c.id == "city-isa-town-4ab705")
            .unwrap();
        let expect = case.expect.as_ref().unwrap();
        assert_eq!(expect.region, ["BH-14"]);
        assert_eq!(expect.point, Some([26.173611, 50.547778]));
        assert_eq!(expect.radius_km, Some(6.0));
        assert_eq!(case.review, seed.review);
        let mut response = json!({
            "geoCoordinates": {"lat": 26.173611, "lon": 50.547778},
            "addressDetails": {"countryCode": "bh", "town": "Isa Town", "regionCodes": [{"code": "BH-14"}]}
        });
        assert_eq!(
            crate::score::score(Some(expect), &response, true).class,
            "confidence_mismatch"
        );
        response["goodMatch"] = json!(true);
        assert_eq!(
            crate::score::score(Some(expect), &response, true).class,
            "pass"
        );
        response["addressDetails"]["regionCodes"][0]["code"] = json!("BH-16");
        assert_eq!(
            crate::score::score(Some(expect), &response, true).class,
            "wrong_region"
        );
    }

    #[test]
    fn adjudicated_zallaq_keeps_the_point_and_requires_southern_governorate() {
        let seeds: Vec<Seed> =
            crate::model::load_jsonl(Path::new("tests/gold.seeds.jsonl")).unwrap();
        let seed = seeds
            .iter()
            .find(|s| s.qid.as_deref() == Some("Q4117781"))
            .unwrap();
        assert_eq!(seed.region_qid.as_deref(), Some("Q838532"));
        let cases: Vec<crate::model::Case> =
            crate::model::load_jsonl(Path::new("tests/gold.jsonl")).unwrap();
        let case = cases
            .iter()
            .find(|c| c.id == "rural-az-zall-q-southern-governorate-bahrain")
            .unwrap();
        let expect = case.expect.as_ref().unwrap();
        assert_eq!(expect.region, ["BH-14"]);
        assert_eq!(expect.point, Some([26.05, 50.4833]));
        assert_eq!(expect.radius_km, Some(6.0));
        assert_eq!(case.review, seed.review);
        let mut response = json!({
            "geoCoordinates": {"lat": 26.05, "lon": 50.4833},
            "addressDetails": {"countryCode": "bh", "village": "Zallaq", "regionCodes": [{"code": "BH-14"}]},
            "goodMatch": true
        });
        assert_eq!(
            crate::score::score(Some(expect), &response, true).class,
            "pass"
        );
        response["addressDetails"]["regionCodes"][0]["code"] = json!("BH-17");
        assert_eq!(
            crate::score::score(Some(expect), &response, true).class,
            "wrong_region"
        );
    }

    #[test]
    fn five_digit_house_numbers_are_not_postcodes() {
        assert_eq!(
            last_five_digits("11626 Old Statesville Rd, Huntersville, NC 28078").unwrap(),
            "28078"
        );
        assert_eq!(
            last_five_digits("12 Rue de la Paix, 75002 Paris").unwrap(),
            "75002"
        );
    }

    #[test]
    fn slugs_match_the_existing_ids() {
        assert_eq!(
            slug("Rådhuspladsen 999, 1550 København V, Denmark"),
            "r-dhuspladsen-999-1550-k-benhavn-v-denmark"
        );
        assert_eq!(slug("   "), "blank");
        assert!(NON_LATIN.is_match("京都, Japan") && !NON_LATIN.is_match("Łódź, Poland"));
    }

    #[test]
    fn labels_attach_by_ref_and_query_and_stale_ones_fail() {
        let dir = std::env::temp_dir().join(format!("labels-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let seeds = dir.join("x.seeds.jsonl");
        let mut cases = vec![
            json!({"source": {"ref": "{A}"}, "query": "1 Main St"}),
            json!({"source": {"ref": "{A}"}, "query": "1 MAIN ST"}),
        ];
        let labels = dir.join("x.labels.jsonl");
        std::fs::write(
            &labels,
            "{\"ref\":\"{A}\",\"query\":\"1 MAIN ST\",\"known_failure\":\"why\"}\n",
        )
        .unwrap();
        apply_labels(&mut cases, &seeds).unwrap();
        assert!(cases[0].get("known_failure").is_none());
        assert_eq!(cases[1]["known_failure"], "why");
        std::fs::write(
            &labels,
            "{\"ref\":\"{B}\",\"query\":\"1 MAIN ST\",\"known_failure\":\"why\"}\n",
        )
        .unwrap();
        assert!(apply_labels(&mut cases, &seeds).is_err());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn seeds_parse() {
        for name in ["gold", "gold.suggest"] {
            let seeds = crate::model::load_jsonl::<Value>(
                &crate::model::root().join(format!("tests/{name}.seeds.jsonl")),
            )
            .unwrap();
            for seed in seeds {
                serde_json::from_value::<Seed>(seed.clone())
                    .unwrap_or_else(|e| panic!("{seed}: {e}"));
            }
        }
    }
}
