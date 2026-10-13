use crate::{matching::*, metrics::METRICS, proto::opengeocoding::*};
use anyhow::{Context, Result, bail, ensure};
use reqwest::{Client, Url};
use serde_json::{Value, json};
use std::{borrow::Cow, sync::Arc, time::Duration};
use tokio::sync::Semaphore;

const ORDERS: [(&str, &[&str]); 4] = [
    (
        "default",
        &[
            "country",
            "state",
            "postcode",
            "city",
            "road",
            "house_number",
        ],
    ),
    ("postcode", &["postcode", "city", "country"]),
    ("loose", &["country", "state", "postcode", "city"]),
    ("just_city", &["country", "city"]),
];

pub struct Resolver {
    client: Client,
    base: String,
    postal: Arc<rustpostal::LibModules>,
    permits: Arc<Semaphore>,
}

impl Resolver {
    pub async fn new(base: String) -> Result<Self> {
        let url = Url::parse(&base)?;
        ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "INDEX_URL must be an HTTP(S) base URL without credentials, query or fragment"
        );
        let base = base.trim_end_matches('/').to_string();
        let client = Client::builder().timeout(Duration::from_secs(10)).build()?;
        let status = client
            .get(format!("{base}/status"))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        check_index_status(&status)?;
        let postal = rustpostal::LibModules::Address;
        postal
            .setup()
            .map_err(|_| anyhow::anyhow!("libpostal model setup failed"))?;
        Ok(Self {
            client,
            base,
            postal: Arc::new(postal),
            permits: Arc::new(Semaphore::new(1)),
        })
    }

    pub async fn parse(&self, query: &str) -> Result<ParsedAddress> {
        // ponytail: serialize libpostal calls until a native thread-safety stress test permits a wider parsing pool.
        let permit = self.permits.clone().acquire_owned().await?;
        let postal = self.postal.clone();
        let query = query.to_owned();
        tokio::task::spawn_blocking(move || {
            let (_permit, _postal) = (permit, postal);
            Ok(rustpostal::address::parse_address(&query, None, None)?
                .into_iter()
                .collect())
        })
        .await?
    }

    async fn get(
        &self,
        endpoint: &str,
        params: &[(&str, String)],
        language: Option<&str>,
    ) -> Result<Value> {
        for attempt in 0..=4 {
            let result = async {
                let mut request = self
                    .client
                    .get(format!("{}/{}", self.base, endpoint))
                    .query(params);
                if let Some(language) = language {
                    request = request.header("Accept-Language", language);
                }
                Ok(request
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await?)
            }
            .await;
            match result {
                Ok(value) => return Ok(value),
                Err(error) if attempt < 4 => {
                    tracing::warn!(attempt = attempt + 1, %error, "Retrying serve_index");
                    tokio::time::sleep(Duration::from_millis((attempt + 1) * 500)).await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!()
    }

    pub async fn candidates(&self, query: &str, language: Option<&str>) -> Result<Vec<Value>> {
        let query = query.trim_matches(whitespace);
        if query.is_empty() {
            return Ok(vec![]);
        }
        let result = self
            .get(
                "search",
                &[
                    ("q", query.into()),
                    ("format", "json".into()),
                    ("addressdetails", "true".into()),
                    ("namedetails", "true".into()),
                ],
                language,
            )
            .await?;
        match result {
            Value::Array(candidates) => Ok(candidates),
            _ => bail!("serve_index search response must be an array"),
        }
    }

    pub async fn first_acceptable(
        &self,
        query: &str,
        address: &ParsedAddress,
        country: &str,
        postcode: &str,
        tier: &str,
        other_names: bool,
    ) -> Result<Option<FullAddress>> {
        // A lone name the parser labels a state (California, 京都) still has to name the result.
        let lone_state = part(address, "city").is_empty()
            && !part(address, "state").is_empty()
            && address.keys().all(|key| key == "state" || key == "country");
        let mut named;
        let address = if lone_state {
            named = address.clone();
            named.insert("city".into(), part(address, "state").into());
            &named
        } else {
            address
        };
        let candidates = {
            let _timer = METRICS.search.with_label_values(&[tier]).start_timer();
            self.candidates(query, Some("en")).await?
        };
        let english = candidates
            .iter()
            .map(format_response)
            .collect::<Result<Vec<_>>>()?;
        let mut judged = Cow::Borrowed(english.as_slice());
        if !part(address, "city").is_empty()
            && !english
                .iter()
                .any(|f| city_agrees(address, f.address_details.as_option()))
        {
            let local = self.candidates(query, None).await?;
            for (index, candidate) in candidates.iter().enumerate() {
                if let Some(twin) = local.iter().rev().find(|hit| {
                    truthy(&hit["osm_id"])
                        && hit.get("osm_id") == candidate.get("osm_id")
                        && hit.get("osm_type") == candidate.get("osm_type")
                }) {
                    let formatted = format_response(twin)?;
                    if city_agrees(address, formatted.address_details.as_option()) {
                        judged.to_mut()[index] = formatted;
                    }
                }
            }
        }
        let city = part(address, "city");
        // Only a bare place name ("Budapest, Hungary") may be a same-named region; in a street
        // address a same-named county or province is a different place.
        let bare = address.keys().all(|key| {
            matches!(
                key.as_str(),
                "city" | "state" | "state_district" | "country_region" | "country"
            )
        });
        if other_names && !city.is_empty() {
            for (index, candidate) in candidates.iter().enumerate() {
                let eligible = match candidate["addresstype"].as_str() {
                    Some("city" | "town" | "village" | "municipality" | "hamlet") => true,
                    Some(
                        "region" | "state" | "province" | "state_district" | "county" | "country",
                    ) => bare,
                    _ => false,
                };
                if !eligible
                    || !matches!(candidate["class"].as_str(), Some("place" | "boundary"))
                    || city_agrees(address, judged[index].address_details.as_option())
                {
                    continue;
                }
                // A place's own names in every language (name:*, alt_name, short_name) corroborate
                // queries written in another language or script than the English response.
                if let Some(name) = own_name(candidate, city) {
                    let details = judged.to_mut()[index]
                        .address_details
                        .get_or_insert_default();
                    details.city = name.into();
                }
            }
        }
        let mut viable: Vec<_> = judged
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                let details = f.address_details.as_option();
                (postcode_matches(postcode, details) || house_road_city(address, details))
                    && (part(address, "city").is_empty()
                        || city_agrees(address, details)
                        || same_postcode_country(country, postcode, details))
            })
            .collect();
        if !lone_state
            && !city.is_empty()
            && ["road", "house_number", "postcode"]
                .iter()
                .all(|key| part(address, key).is_empty())
        {
            // A bare place name means the settlement, not a same-named quarter, county or region
            // that the index may rank higher.
            viable.sort_by_key(|(i, _)| match candidates[*i]["addresstype"].as_str() {
                Some("city" | "town") => 0,
                Some("municipality") => 1,
                Some("village") => 2,
                _ => 3,
            });
        }
        let values: Vec<_> = viable.iter().map(|(_, f)| (*f).clone()).collect();
        let Some((index, matched)) = choose_candidate(address, country, postcode, &values) else {
            if !candidates.is_empty() {
                tracing::warn!(count = candidates.len(), tier, "No acceptable candidate");
            }
            return Ok(None);
        };
        if matched != "country" {
            tracing::info!(
                stated_country = country,
                accepted_country = english[viable[index].0].address_details.country_code,
                matched,
                "Accepting corroborated country override"
            );
        }
        Ok(Some(english[viable[index].0].clone()))
    }

    pub async fn resolve(&self, query: &str) -> Result<FullAddress> {
        if query.is_empty() || query.chars().count() > 256 {
            return Ok(FullAddress::default());
        }
        // Tabs and ASCII record separators between fields read as spaces to both parsers.
        let query = &query.replace(|ch: char| whitespace(ch) && ch != ' ', " ");
        if PLACEHOLDERS.contains(&query.trim_matches(whitespace).to_lowercase().as_str()) {
            return Ok(FullAddress::default());
        }
        let mut address = self.parse(query).await?;
        if part(&address, "country").is_empty() {
            let parts: Vec<_> = query
                .split(',')
                .map(|s| s.trim_matches(whitespace))
                .collect();
            if parts.len() >= 3 {
                let rotated_query = parts[2..]
                    .iter()
                    .chain(parts[..2].iter())
                    .copied()
                    .collect::<Vec<_>>()
                    .join(", ");
                let rotated = self.parse(&rotated_query).await?;
                if !part(&rotated, "country").is_empty() {
                    address = rotated;
                }
            }
        }
        // Repeated fields ("Karnataka, India, India") reach the parser as one "india india".
        if let Some(stated) = address.get_mut("country") {
            let words: Vec<_> = stated.split(' ').collect();
            let half = words.len() / 2;
            if half > 0 && words.len() % 2 == 0 && words[..half] == words[half..] {
                *stated = words[..half].join(" ");
            }
        }
        let country = expected_country(&address);
        let postcode = if part(&address, "postcode").is_empty() {
            eircode(query)
        } else {
            part(&address, "postcode").into()
        };
        if let Some(found) = self
            .first_acceptable(query, &address, &country, &postcode, "raw", true)
            .await?
        {
            return Ok(accept(found, &address, &postcode, "raw", true));
        }
        for (tier, order) in ORDERS {
            let structured = create_query(&address, order);
            if !useful_query(&structured, &postcode, &country) {
                continue;
            }
            if let Some(found) = self
                .first_acceptable(&structured, &address, &country, &postcode, tier, true)
                .await?
            {
                return Ok(accept(found, &address, &postcode, tier, false));
            }
        }
        let stripped = without_country(query, &address);
        if !stripped.is_empty()
            && let Some(found) = self
                .first_acceptable(
                    &stripped,
                    &address,
                    &country,
                    &postcode,
                    "without_stated_country",
                    country.is_empty(),
                )
                .await?
        {
            return Ok(accept(
                found,
                &address,
                &postcode,
                "without_stated_country",
                true,
            ));
        }
        // Overriding a recognized stated country needs the place's primary fields to agree;
        // alternative names alone matched same-named regions abroad.
        if !part(&address, "country").is_empty() {
            let mut without = address.clone();
            without.shift_remove("country");
            for (tier, order) in ORDERS {
                let tier = format!("{tier}_without_stated_country");
                let structured = create_query(&without, order);
                if !useful_query(&structured, &postcode, "") {
                    continue;
                }
                if let Some(found) = self
                    .first_acceptable(
                        &structured,
                        &without,
                        "",
                        &postcode,
                        &tier,
                        country.is_empty(),
                    )
                    .await?
                {
                    return Ok(accept(found, &address, &postcode, &tier, true));
                }
            }
        }
        METRICS.tiers.with_label_values(&["unresolved"]).inc();
        Ok(FullAddress::default())
    }

    pub async fn reverse(&self, lat: f32, lon: f32) -> Result<Option<FullAddress>> {
        let value = self
            .get(
                "reverse",
                &[
                    ("lat", (lat as f64).to_string()),
                    ("lon", (lon as f64).to_string()),
                    ("format", "json".into()),
                    ("addressdetails", "true".into()),
                ],
                Some("en"),
            )
            .await?;
        if !truthy(&value["address"]) {
            return Ok(None);
        }
        let mut formatted = format_response(&value)?;
        formatted.good_match = true;
        Ok(Some(formatted))
    }

    pub async fn state(&self, query: &str) -> Result<Option<Region>> {
        let Some(result) = self.candidates(query, Some("en")).await?.into_iter().next() else {
            return Ok(None);
        };
        if !truthy(&result["place_id"]) {
            return Ok(None);
        }
        let place = result["place_id"].to_string();
        let details = self
            .get(
                "details",
                &[
                    ("format", "json".into()),
                    ("addressdetails", "1".into()),
                    ("place_id", place),
                ],
                None,
            )
            .await?;
        let names = &details["names"];
        let en_name = ["name:en", "_place_name:en", "name"]
            .iter()
            .find_map(|name| names[*name].as_str().filter(|s| !s.is_empty()))
            .unwrap_or("");
        Ok(Some(Region {
            country_code: string(&details["country_code"]).into(),
            country_name: string(&result["address"]["country"]).into(),
            admin_level: integer(&details["admin_level"])?,
            local_name: string(&details["localname"]).into(),
            en_name: en_name.into(),
            admin_code: query
                .split('-')
                .nth(1)
                .context("region code must contain a hyphen")?
                .into(),
            ..Default::default()
        }))
    }
}

pub fn check_index_status(status: &Value) -> Result<()> {
    ensure!(
        status["status"].as_u64() == Some(0)
            && status["software_version"].as_str() == Some("overture-index"),
        "INDEX_URL must point to a ready serve_index backend"
    );
    Ok(())
}

fn own_name<'a>(candidate: &'a Value, city: &str) -> Option<&'a str> {
    let names = candidate["namedetails"].as_object().into_iter().flatten();
    names
        .filter_map(|(_, v)| v.as_str())
        .flat_map(|v| v.split(';'))
        .chain(string(&candidate["display_name"]).split(',').next())
        .find(|name| names_agree(city, name))
}

/// Missing-data placeholders that would otherwise match to places named "n/a".
const PLACEHOLDERS: &[&str] = &[
    "n/a", "n.a.", "na", "none", "null", "nil", "unknown", "-", "?",
];

/// A structured query of one character, or of a bare postcode with no country to place it in,
/// matches arbitrary places worldwide.
fn useful_query(query: &str, postcode: &str, country: &str) -> bool {
    query.chars().filter(|ch| ch.is_alphanumeric()).count() > 1
        && (!country.is_empty() || normalize_postcode(query) != normalize_postcode(postcode))
}

fn accept(
    mut found: FullAddress,
    address: &ParsedAddress,
    postcode: &str,
    tier: &str,
    good: bool,
) -> FullAddress {
    // A street without the requested house number is a fallback, not a match.
    let house_found = part(address, "house_number").is_empty()
        || found
            .address_details
            .as_option()
            .is_some_and(|d| !d.house_number.is_empty());
    // Accepted although its postcode disagrees: the input contradicts itself.
    let postcode_found = postcode_matches(postcode, found.address_details.as_option());
    if !postcode.is_empty() {
        let details = found.address_details.get_or_insert_default();
        if details.postcode.is_empty() {
            details.postcode = postcode.into();
        }
    }
    found.good_match = good && house_found && postcode_found;
    METRICS.tiers.with_label_values(&[tier]).inc();
    found
}

pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::Number(n) => n.as_f64().is_some_and(|v| v != 0.),
    }
}

fn string(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}

fn integer(value: &Value) -> Result<u32> {
    match value {
        Value::String(s) => s.parse().context("integer field"),
        _ => value
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .context("integer field"),
    }
}

fn float(value: &Value) -> Result<f32> {
    match value {
        Value::String(s) => s.parse().context("coordinate field"),
        _ => value.as_f64().map(|v| v as f32).context("coordinate field"),
    }
}

pub fn format_response(value: &Value) -> Result<FullAddress> {
    let bounding = if truthy(&value["boundingbox"]) {
        Some(BoundingBox {
            min_lat: float(&value["boundingbox"][0])?,
            max_lat: float(&value["boundingbox"][1])?,
            min_lon: float(&value["boundingbox"][2])?,
            max_lon: float(&value["boundingbox"][3])?,
            ..Default::default()
        })
    } else {
        None
    };
    let mut formatted = FullAddress {
        display_name: string(&value["display_name"]).into(),
        ..Default::default()
    };
    if truthy(&value["lat"]) && truthy(&value["lon"]) {
        formatted.geo_coordinates = GeoCoordinates {
            lat: float(&value["lat"])?,
            lon: float(&value["lon"])?,
            bounding_box: bounding.into(),
            ..Default::default()
        }
        .into();
    }
    if !truthy(&value["address"]) {
        return Ok(formatted);
    }
    let address = value["address"]
        .as_object()
        .context("address metadata must be an object")?;
    let mut regular = address.clone();
    let mut codes = vec![];
    for (key, code) in address.iter().filter(|(key, _)| key.contains("ISO3166-2")) {
        regular.remove(key);
        if code == "RS-KM" {
            continue;
        }
        let level = key.replace("ISO3166-2-lvl", "");
        let level: u32 = if level.is_empty() { 2 } else { level.parse()? };
        if level > 10 {
            continue;
        }
        codes.push(json!({"admin_level": level, "code": code}));
    }
    regular.insert("region_codes".into(), json!(codes));
    let details: AddressDetails = serde_json::from_value(Value::Object(regular))?;
    formatted.address_details = details.into();
    Ok(formatted)
}
