//! Reverse geocoding at each case's expected point, scored twice with the same rules for every
//! service: the gold suite's ReverseGeocode check (country, region codes, the locality's name),
//! and the forward rules applied to the answer (distance to the point, level, house number,
//! postcode), which is what tells reverse geocoders apart.

use super::{Entry, Options, Provider, fetch, key, read_entry, save_entry};
use crate::{
    model::{Case, Located, Record, load_corpus, load_suite},
    run::{place_name, rpc_json, sha256_files, utc_now},
    score::{expected_place, reverse_applies, reverse_class, reversed, score},
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    fmt::Write as _,
    fs::{File, OpenOptions},
    io::Write,
    time::Duration,
};

pub struct Reverse {
    pub request: fn(f64, f64) -> Value,
    /// Shapes the first answer like a FullAddress, with every locality and region name and ISO
    /// 3166-2 code it carries; `{}` when there is none.
    pub normalize: fn(&Value) -> Result<Value>,
}

/// The release under test, served locally and reached over its RPC (`--rpc`), so its answers
/// are cached and scored exactly like a provider's.
pub static OPENGEOCODING: Provider = Provider {
    name: "opengeocoding",
    label: "OpenGeocoding",
    credential_env: "",
    auth_parameter: "",
    query_error_statuses: &[],
    not_found_status: None,
    interval_ms: 0,
    storage_terms: None,
    request: |_| json!({}),
    normalize: |_| bail!("score OpenGeocoding's forward answers with evaluate run"),
    validate_status: None,
    reverse: Some(Reverse {
        request: |lat, lon| json!({"endpoint": "ReverseGeocode", "lat": lat, "lon": lon}),
        normalize: |response| match &response["results"][0]["address"] {
            Value::Null => Ok(json!({})),
            address => Ok(address.clone()),
        },
    }),
};

/// Copies the native names under `keys` (native, FullAddress) where the answer has none yet.
pub fn add_places(address: &mut Value, native: &Value, keys: &[(&str, &str)]) {
    let Some(details) = address.get_mut("addressDetails") else {
        return;
    };
    for (from, to) in keys {
        let name = native[*from].as_str().unwrap_or_default();
        if !name.is_empty() && details[*to].as_str().is_none_or(str::is_empty) {
            details[*to] = name.into();
        }
    }
}

/// Sets the answer's ISO 3166-2 codes where it has none yet.
pub fn add_codes<'a>(address: &mut Value, codes: impl IntoIterator<Item = &'a str>) {
    let Some(details) = address.get_mut("addressDetails") else {
        return;
    };
    let codes: Vec<_> = codes
        .into_iter()
        .filter(|c| !c.is_empty())
        .map(|c| json!({"code": c.to_uppercase()}))
        .collect();
    if !codes.is_empty() && details["regionCodes"].as_array().is_none_or(Vec::is_empty) {
        details["regionCodes"] = codes.into();
    }
}

/// Nominatim's reverse answer (LocationIQ, maps.co): one place with its whole address.
pub fn nominatim(response: &Value) -> Result<Value> {
    if response.get("error").is_some() || response.get("address").is_none() {
        ensure!(
            response["error"].as_str() == Some("Unable to geocode"),
            "unexpected Nominatim error"
        );
        return Ok(json!({}));
    }
    let native = &response["address"];
    let road = [
        "road",
        "pedestrian",
        "footway",
        "path",
        "cycleway",
        "square",
    ]
    .iter()
    .map(|k| native[*k].as_str().unwrap_or_default())
    .find(|r| !r.is_empty())
    .unwrap_or_default();
    let details = json!({
        "countryCode": native["country_code"].as_str().unwrap_or_default().to_lowercase(),
        "houseNumber": native["house_number"], "road": road, "postcode": native["postcode"],
    });
    let lat: f64 = response["lat"]
        .as_str()
        .context("missing latitude")?
        .parse()?;
    let lon: f64 = response["lon"]
        .as_str()
        .context("missing longitude")?
        .parse()?;
    let name = response["display_name"].as_str().unwrap_or_default();
    let mut address = super::full_address(name, &json!(lat), &json!(lon), details)?;
    add_places(&mut address, native, PLACES);
    let codes = native.as_object().into_iter().flatten();
    add_codes(
        &mut address,
        codes
            .filter(|(k, _)| k.starts_with("ISO3166-2"))
            .filter_map(|(_, v)| v.as_str()),
    );
    Ok(address)
}

/// Native locality and region keys shared by Nominatim-style and OpenCage-style answers.
pub const PLACES: &[(&str, &str)] = &[
    ("city", "city"),
    ("town", "town"),
    ("village", "village"),
    ("hamlet", "hamlet"),
    ("municipality", "municipality"),
    ("suburb", "suburb"),
    ("city_district", "cityDistrict"),
    ("district", "district"),
    ("borough", "borough"),
    ("neighbourhood", "neighbourhood"),
    ("quarter", "quarter"),
    ("county", "county"),
    ("state_district", "stateDistrict"),
    ("state", "state"),
    ("province", "province"),
    ("region", "region"),
];

/// Scores one answer (a FullAddress, `{}` for none) at a case's expected point. Region codes an
/// answer lacks are not held against it, as in forward scoring; an answer finer than the
/// case's place is what a reverse geocoder returns, so it is never too fine.
pub fn classify(case: &Case, address: &Value, record: &mut Record) {
    let expect = case
        .expect
        .as_ref()
        .and_then(expected_place)
        .expect("reverse cases have one expected place");
    let mut found = reversed(&json!({"results": [{"address": address}]}));
    let mut admin = expect.clone();
    if found.regions.is_empty() && !admin.region.is_empty() {
        admin.region.clear();
        record.flags.push("region_codes_missing".into());
    }
    // the forward rules below judge the house, with every alternative's number
    found.answer = None;
    found.class = reverse_class(&admin, place_name(case), &found, false).into();
    let mut forward = case.expect.clone().unwrap_or_default();
    forward.max_level = None;
    for alternative in forward.any_of.iter_mut().flatten() {
        alternative.max_level = None;
    }
    let scored = score(Some(&forward), address, false);
    record.class = if found.class == "pass" {
        scored.class.into()
    } else {
        found.class.clone()
    };
    record.level = Some(scored.level.into());
    record.distance_km = scored.distance_km;
    record.flags.extend(scored.flags);
    record.reverse = Some(found);
}

pub async fn run(options: Options) -> Result<i32> {
    let started_at = utc_now();
    let provider = options.provider;
    let reverse = provider
        .reverse
        .as_ref()
        .with_context(|| format!("{} has no reverse adapter", provider.name))?;
    ensure!(
        options.fetch || options.budget == 0,
        "--budget requires --fetch"
    );
    ensure!(
        !options.fetch || options.budget > 0,
        "--fetch requires an explicit positive --budget"
    );
    ensure!(
        options.compare.is_none(),
        "--reverse pairs services offline; --compare is forward only"
    );
    let cases = match &options.corpus {
        Some(path) => load_corpus(path)?,
        None => load_suite()?,
    };
    let cases: Vec<_> = cases
        .into_iter()
        .filter(|c| c.expect.as_ref().is_some_and(reverse_applies))
        .filter(|c| options.only.as_ref().is_none_or(|re| re.is_match(&c.id)))
        .collect();
    ensure!(!cases.is_empty(), "no reverse cases selected");
    let mut ids = HashSet::new();
    ensure!(
        cases.iter().all(|c| ids.insert(&c.id)),
        "duplicate case ids"
    );
    let request_of = |case: &Case| {
        let [lat, lon] = case
            .expect
            .as_ref()
            .and_then(expected_place)
            .and_then(|e| e.point)
            .unwrap_or_default();
        (reverse.request)(lat, lon)
    };
    let _lock = if options.fetch {
        std::fs::create_dir_all(&options.cache)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(options.cache.join(".fetch.lock"))?;
        file.try_lock()
            .context("another fetch holds this cache lock")?;
        Some(file)
    } else {
        None
    };
    let mut missing = HashSet::new();
    for case in &cases {
        let request = request_of(case);
        if read_entry(provider, &options.cache, &request)?.is_none() {
            missing.insert(key(&request));
        }
    }
    eprintln!(
        "{} reverse cases, {} distinct uncached requests; offline={}",
        cases.len(),
        missing.len(),
        !options.fetch
    );
    ensure!(
        missing.is_empty() || options.fetch,
        "offline cache misses: {}; use --fetch --budget N to authorize provider requests",
        missing.len()
    );
    ensure!(
        !options.fetch || missing.len() <= options.budget,
        "{} missing requests exceed --budget {}; no requests sent",
        missing.len(),
        options.budget
    );
    let local = provider.name == OPENGEOCODING.name;
    ensure!(
        !local || options.rpc.is_some() || missing.is_empty(),
        "OpenGeocoding answers come from a local service: pass --rpc URL"
    );
    let client = if missing.is_empty() {
        None
    } else {
        if let Some(terms) = provider.storage_terms {
            ensure!(
                options.storage_authorized,
                "{} response storage requires an applicable agreement authorizing this offline evaluation; verify {terms}, then pass --storage-authorized",
                provider.label
            );
        }
        let token = if local {
            String::new()
        } else {
            let token = std::env::var(provider.credential_env).with_context(|| {
                format!(
                    "set {} through your secret manager",
                    provider.credential_env
                )
            })?;
            ensure!(
                !token.trim().is_empty(),
                "{} is empty",
                provider.credential_env
            );
            token
        };
        Some((
            reqwest::Client::builder()
                .retry(reqwest::retry::never())
                .timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            token,
        ))
    };
    let mut fetched = 0;
    let mut records = vec![];
    let mut cache_files = BTreeMap::new();
    for case in &cases {
        let request = request_of(case);
        let cache_path = options.cache.join(format!("{}.json", key(&request)));
        let entry = match read_entry(provider, &options.cache, &request)? {
            Some(entry) => entry,
            None => {
                let (client, token) = client.as_ref().context("offline cache miss")?;
                if fetched > 0 {
                    tokio::time::sleep(Duration::from_millis(provider.interval_ms)).await;
                }
                let entry = if local {
                    let base = options.rpc.as_deref().context("--rpc")?;
                    let payload = json!({"lat": request["lat"], "lon": request["lon"]});
                    let response = rpc_json(client, base, "ReverseGeocode", &payload)
                        .await
                        .map_err(|e| anyhow::anyhow!("OpenGeocoding ReverseGeocode failed: {e}"))?;
                    Entry {
                        request,
                        fetched_at: utc_now(),
                        status: 200,
                        response,
                    }
                } else {
                    fetch(provider, client, token, request, false).await?
                };
                save_entry(&options.cache, &entry)?;
                fetched += 1;
                entry
            }
        };
        cache_files.insert(cache_path, ());
        let mut record = Record {
            id: case.id.clone(),
            query: case.query.clone(),
            ..Default::default()
        };
        if provider.query_error_statuses.contains(&entry.status) {
            record.class = "error".into();
            record.error = Some(format!("http_{}", entry.status));
            record.reverse = Some(Located {
                class: "error".into(),
                error: record.error.clone(),
                ..Default::default()
            });
        } else {
            if entry.status == 200
                && let Some(validate) = provider.validate_status
            {
                validate(&entry.response)?;
            }
            match (reverse.normalize)(&entry.response) {
                Ok(address) => {
                    classify(case, &address, &mut record);
                    record.response = Some(address);
                }
                Err(error) => {
                    record.class = "error".into();
                    record.error = Some("invalid_response".into());
                    record.flags.push(error.to_string());
                    record.reverse = Some(Located {
                        class: "error".into(),
                        error: record.error.clone(),
                        ..Default::default()
                    });
                    eprintln!("{}: malformed response counted as error (cached)", case.id);
                }
            }
        }
        records.push(record);
        if records.len() % 50 == 0 {
            eprintln!("{}/{} cases; {fetched} fetched", records.len(), cases.len());
        }
    }
    let files = options
        .corpus
        .clone()
        .map_or_else(crate::model::suite, |p| vec![p]);
    let manifest = json!({
        "suite": format!("{}-reverse", provider.name), "started_at": started_at, "cases": cases.len(),
        "corpus_sha256": sha256_files(&files)?,
        "cache_sha256": sha256_files(&cache_files.into_keys().collect::<Vec<_>>())?,
        "scoring": {"confidence": false, "missing_region_codes": "not held against the answer", "max_level": "not applied"},
        "request_settings": (reverse.request)(0., 0.), "fetched": fetched,
        "storage_authorized": options.storage_authorized, "interval_ms": provider.interval_ms,
        "backend": {"label": provider.label, "status": {"software_version": provider.name, "data_updated": "unknown"}},
    });
    let mut report = format!(
        "# {} reverse geocoding\n\nOne request per distinct expected point; each case scored by the gold suite's ReverseGeocode check (country, region codes, locality name) and by the forward rules on the answer (distance, level, house number, postcode). Confidence is not scored. Native responses are in the private cache.\n\n| Class | Gold reverse check | Reverse address check |\n|---|---:|---:|\n",
        provider.label
    );
    let mut classes: BTreeMap<&str, [usize; 2]> = BTreeMap::new();
    for record in &records {
        let admin = record
            .reverse
            .as_ref()
            .map_or("error", |r| r.class.as_str());
        classes.entry(admin).or_default()[0] += 1;
        classes.entry(&record.class).or_default()[1] += 1;
    }
    for (class, [admin, full]) in &classes {
        let _ = writeln!(report, "| {class} | {admin} | {full} |");
    }
    let _ = writeln!(
        report,
        "| **cases** | {} | {} |",
        records.len(),
        records.len()
    );
    std::fs::create_dir_all(&options.out)?;
    let mut output = File::create(options.out.join("results.jsonl"))?;
    for record in &records {
        serde_json::to_writer(&mut output, record)?;
        writeln!(output)?;
    }
    std::fs::write(
        options.out.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)? + "\n",
    )?;
    std::fs::write(options.out.join("report.md"), &report)?;
    println!(
        "{fetched} requests fetched; report: {}",
        options.out.join("report.md").display()
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::Expect;

    fn case(level: &str, region: &[&str], components: &[(&str, &str)]) -> Case {
        Case {
            id: "c".into(),
            query: "q".into(),
            expect: Some(Expect {
                level: level.into(),
                max_level: Some(level.into()),
                country: vec!["dk".into()],
                point: Some([55.6761, 12.5689]),
                radius_km: Some(0.15),
                region: region.iter().map(|r| (*r).into()).collect(),
                components: components
                    .iter()
                    .map(|(k, v)| ((*k).into(), (*v).into()))
                    .collect(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn answer(number: &str, codes: &[&str]) -> Value {
        let codes: Vec<_> = codes.iter().map(|c| json!({"code": c})).collect();
        json!({"displayName": "x", "geoCoordinates": {"lat": 55.6762, "lon": 12.5690},
            "addressDetails": {"countryCode": "dk", "houseNumber": number, "road": "Rådhuspladsen",
            "postcode": "1550", "city": "Copenhagen", "regionCodes": codes}})
    }

    fn class(case: &Case, address: &Value) -> (String, String, Vec<String>) {
        let mut record = Record::default();
        classify(case, address, &mut record);
        (record.reverse.unwrap().class, record.class, record.flags)
    }

    #[test]
    fn reverse_answers_are_scored_by_admin_and_address_rules() {
        let house = case(
            "house",
            &["DK-84"],
            &[("house_number", "9"), ("postcode", "1550")],
        );
        assert_eq!(class(&house, &answer("9", &["DK-84"])).1, "pass");
        assert_eq!(
            class(&house, &answer("11", &["DK-84"])).1,
            "wrong_component"
        );
        assert_eq!(class(&house, &answer("", &["DK-84"])).1, "too_coarse");
        assert_eq!(class(&house, &answer("9", &["DK-81"])).0, "wrong_region");
        let (admin, full, flags) = class(&house, &answer("9", &[]));
        assert_eq!((admin.as_str(), full.as_str()), ("pass", "pass"));
        assert!(flags.contains(&"region_codes_missing".into()));
        assert_eq!(class(&house, &json!({})).0, "empty");
        let mut far = answer("9", &["DK-84"]);
        far["geoCoordinates"]["lat"] = 55.70.into();
        assert_eq!(class(&house, &far).1, "near_miss");
        // a house is what a reverse geocoder answers at a postcode's point
        assert_eq!(
            class(&case("postcode", &[], &[]), &answer("9", &[])).1,
            "pass"
        );
        let mut town = case("locality", &[], &[]);
        town.suggest = Some(crate::model::SuggestExpect {
            text: "Aarhus".into(),
            ..Default::default()
        });
        town.expect.as_mut().unwrap().radius_km = Some(10.);
        assert_eq!(class(&town, &answer("9", &[])).0, "wrong_place");
        let mut unit = house.clone();
        let mut alternative = unit.expect.take().unwrap();
        let mut other = alternative.clone();
        other.components.insert("house_number".into(), "9A".into());
        alternative
            .components
            .insert("house_number".into(), "3/9".into());
        unit.expect = Some(Expect {
            any_of: Some(vec![alternative, other]),
            ..Default::default()
        });
        assert_eq!(class(&unit, &answer("9A", &["DK-84"])).1, "pass");
    }

    #[test]
    fn nominatim_and_tomtom_reverse_answers_are_normalized() {
        let native = json!({"lat": "55.6764", "lon": "12.5688", "display_name": "9 Rådhuspladsen",
            "address": {"house_number": "9", "road": "Rådhuspladsen", "suburb": "Indre By",
            "city": "Copenhagen", "state": "Capital Region of Denmark", "ISO3166-2-lvl4": "DK-84",
            "postcode": "1550", "country_code": "dk"}});
        let address = nominatim(&native).unwrap();
        let d = &address["addressDetails"];
        assert_eq!(
            (d["houseNumber"].as_str(), d["suburb"].as_str()),
            (Some("9"), Some("Indre By"))
        );
        assert_eq!(d["regionCodes"], json!([{"code": "DK-84"}]));
        assert_eq!(
            nominatim(&json!({"error": "Unable to geocode"})).unwrap(),
            json!({})
        );
        assert!(nominatim(&json!({"error": "Invalid key"})).is_err());
        let tomtom = super::super::named("tomtom")
            .unwrap()
            .reverse
            .as_ref()
            .unwrap();
        let native = json!({"addresses": [{"position": "55.676400,12.568800", "address": {
            "streetNumber": "9", "streetName": "Rådhuspladsen", "municipality": "Copenhagen",
            "countrySubdivisionCode": "84", "countrySubdivisionName": "Capital Region",
            "postalCode": "1550", "countryCode": "DK", "freeformAddress": "Rådhuspladsen 9"}}]});
        let address = (tomtom.normalize)(&native).unwrap();
        assert_eq!(
            address["addressDetails"]["regionCodes"],
            json!([{"code": "DK-84"}])
        );
        assert_eq!(address["addressDetails"]["state"], "Capital Region");
        assert_eq!(address["geoCoordinates"]["lat"], 55.6764);
        assert_eq!(
            (tomtom.normalize)(&json!({"addresses": []})).unwrap(),
            json!({})
        );
    }

    #[test]
    fn reverse_requests_carry_the_point_and_no_credentials() {
        let mut keys = HashSet::new();
        for provider in super::super::PROVIDERS.iter().chain([&OPENGEOCODING]) {
            let Some(reverse) = &provider.reverse else {
                continue;
            };
            let request = (reverse.request)(55.6761, -12.5689);
            let text = request.to_string();
            assert!(
                text.contains("55.6761") && text.contains("-12.5689"),
                "{text}"
            );
            assert!(
                provider.auth_parameter.is_empty()
                    || request.get(provider.auth_parameter).is_none()
            );
            assert!(keys.insert(key(&request)));
        }
        assert_eq!(keys.len(), 8);
    }

    #[tokio::test]
    async fn cached_reverse_answers_replay_offline() {
        let dir = std::env::temp_dir().join(format!("reverse-test-{}", std::process::id()));
        let cache = dir.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        let house = case("house", &["DK-84"], &[("house_number", "9")]);
        let mut country = house.clone();
        country.id = "country".into();
        country.expect.as_mut().unwrap().level = "country".into();
        let corpus = dir.join("cases.jsonl");
        std::fs::write(
            &corpus,
            [&house, &country]
                .map(|c| serde_json::to_string(c).unwrap() + "\n")
                .concat(),
        )
        .unwrap();
        let request = (OPENGEOCODING.reverse.as_ref().unwrap().request)(55.6761, 12.5689);
        save_entry(
            &cache,
            &Entry {
                request,
                fetched_at: String::new(),
                status: 200,
                response: json!({"results": [{"address": answer("9", &["DK-84"])}]}),
            },
        )
        .unwrap();
        let options = Options {
            provider: &OPENGEOCODING,
            storage_authorized: false,
            corpus: Some(corpus),
            out: dir.join("out"),
            cache,
            only: None,
            fetch: false,
            budget: 0,
            compare: None,
            reverse: true,
            rpc: None,
        };
        assert_eq!(super::super::run(options).await.unwrap(), 0);
        let records: Vec<Record> =
            crate::model::load_jsonl(&dir.join("out/results.jsonl")).unwrap();
        assert_eq!(records.len(), 1, "a country's point is not reverse-checked");
        assert_eq!(records[0].class, "pass");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
