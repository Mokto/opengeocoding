use crate::{
    model::{Case, Record, load_corpus, load_suite},
    report::{load_side, summarize},
    run::{classify, sha256_files, utc_now},
    score::{score, summary_of, text},
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    fmt::Write as _,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

const ENDPOINT: &str = "https://api.mapbox.com/search/geocode/v6/forward";

pub struct Options {
    pub corpus: Option<PathBuf>,
    pub out: PathBuf,
    pub cache: PathBuf,
    pub only: Option<regex::Regex>,
    pub fetch: bool,
    pub budget: usize,
    pub compare: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    request: Value,
    fetched_at: String,
    status: u16,
    response: Value,
}

fn request(query: &str) -> Value {
    json!({
        "endpoint": ENDPOINT,
        "q": query,
        "permanent": true,
        "autocomplete": false,
        "limit": 1,
        "language": "en",
        "worldview": "us",
    })
}

fn key(request: &Value) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, request.to_string().as_bytes());
    digest.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

fn read_entry(cache: &Path, request: &Value) -> Result<Option<Entry>> {
    let path = cache.join(format!("{}.json", key(request)));
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    let entry: Entry = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid cache entry {}", path.display()))?;
    ensure!(
        entry.request == *request,
        "cache request mismatch: {}",
        path.display()
    );
    ensure!(matches!(entry.status, 200 | 422), "invalid cached status");
    Ok(Some(entry))
}

fn save_entry(cache: &Path, entry: &Entry) -> Result<()> {
    let path = cache.join(format!("{}.json", key(&entry.request)));
    let pending = path.with_extension("json.tmp");
    let mut file = File::create(&pending)?;
    serde_json::to_writer(&mut file, entry)?;
    file.sync_all()?;
    std::fs::rename(pending, path)?;
    Ok(())
}

fn normalize(response: &Value) -> Result<Value> {
    let features = response["features"]
        .as_array()
        .context("missing features array")?;
    let Some(feature) = features.first() else {
        return Ok(json!({}));
    };
    let props = &feature["properties"];
    let context = &props["context"];
    let coordinates = feature["geometry"]["coordinates"]
        .as_array()
        .context("missing coordinates")?;
    ensure!(coordinates.len() == 2, "expected a point");
    let lon = coordinates[0].as_f64().context("invalid longitude")?;
    let lat = coordinates[1].as_f64().context("invalid latitude")?;
    ensure!(
        (-180.0..=180.0).contains(&lon) && (-90.0..=90.0).contains(&lat),
        "invalid point"
    );
    let country = text(&context["country"], "country_code").to_lowercase();
    ensure!(!country.is_empty(), "missing country code");
    let mut address = json!({"countryCode": country});
    let kind = text(props, "feature_type");
    let field = match kind {
        "address" | "secondary_address" => {
            address["houseNumber"] = context["address"]["address_number"].clone();
            address["road"] = context["street"]["name"]
                .as_str()
                .or_else(|| context["address"]["street_name"].as_str())
                .unwrap_or("")
                .into();
            ensure!(!text(&address, "road").is_empty(), "address without street");
            None
        }
        "street" => Some("road"),
        "postcode" => Some("postcode"),
        "country" => None,
        "region" => Some("state"),
        "district" => Some("county"),
        "place" => Some("city"),
        "locality" | "neighborhood" | "block" => Some("suburb"),
        _ => bail!("unsupported Mapbox feature type: {kind}"),
    };
    if let Some(field) = field {
        address[field] = props["name"].clone();
    }
    if matches!(kind, "address" | "secondary_address" | "street") {
        address["postcode"] = context["postcode"]["name"].clone();
        address["city"] = context["place"]["name"].clone();
    }
    let region = text(&context["region"], "region_code_full");
    address["regionCodes"] = if region.is_empty() {
        json!([])
    } else {
        json!([{"code": region.to_uppercase()}])
    };
    Ok(json!({
        "displayName": props["full_address"].as_str().unwrap_or(text(props, "name")),
        "geoCoordinates": {"lat": lat, "lon": lon},
        "addressDetails": address,
    }))
}

async fn fetch(client: &reqwest::Client, token: &str, request: Value) -> Result<Entry> {
    let params: Vec<_> = request
        .as_object()
        .context("request object")?
        .iter()
        .filter(|(k, _)| k.as_str() != "endpoint")
        .map(|(k, v)| {
            (
                k.as_str(),
                v.as_str().map_or_else(|| v.to_string(), String::from),
            )
        })
        .chain(std::iter::once(("access_token", token.to_owned())))
        .collect();
    let response = client
        .get(ENDPOINT)
        .query(&params)
        .send()
        .await
        .map_err(|error| error.without_url())
        .context("Mapbox request failed (not retried)")?;
    let status = response.status().as_u16();
    ensure!(
        matches!(status, 200 | 422),
        "Mapbox HTTP {status}; stopped without retrying"
    );
    let response: Value = response
        .json()
        .await
        .map_err(|error| error.without_url())
        .context("invalid Mapbox JSON")?;
    Ok(Entry {
        request,
        fetched_at: utc_now(),
        status,
        response,
    })
}

fn paired(cases: &[Case], records: &[Record], baseline: &Path) -> Result<String> {
    let (_, base) = load_side(baseline)?;
    let mut counts = [0usize; 4];
    let mut differences = String::new();
    for (case, record) in cases.iter().zip(records) {
        if case.expect.is_none() {
            continue;
        }
        let Some(before) = base.get(&case.id) else {
            continue;
        };
        ensure!(
            before.query == case.query,
            "baseline query differs for {}",
            case.id
        );
        let class = if let Some(response) = &before.response {
            score(case.expect.as_ref(), response, false).class
        } else {
            ensure!(
                before.error.is_some(),
                "baseline {} has no full response",
                case.id
            );
            "error"
        };
        if class == "unscored" {
            continue;
        }
        let index = match (class == "pass", record.class == "pass") {
            (true, true) => 0,
            (true, false) => 1,
            (false, true) => 2,
            (false, false) => 3,
        };
        counts[index] += 1;
        if class != record.class {
            let _ = writeln!(
                differences,
                "- `{}`: baseline **{class}**, Mapbox **{}**",
                case.id, record.class
            );
        }
    }
    ensure!(
        counts.iter().sum::<usize>() > 0,
        "no paired scored cases with the baseline"
    );
    Ok(format!(
        "\n## Paired forward comparison\n\nBoth sides re-scored against the current expectations, ignoring confidence. Only identical queries with saved responses/errors are paired.\n\n| Outcome | Cases |\n|---|---:|\n| Both pass | {} |\n| Baseline only passes | {} |\n| Mapbox only passes | {} |\n| Neither passes | {} |\n\n### Different classes\n\n{}",
        counts[0], counts[1], counts[2], counts[3], differences,
    ))
}

pub async fn run(options: Options) -> Result<i32> {
    let started_at = utc_now();
    ensure!(
        options.fetch || options.budget == 0,
        "--budget requires --fetch"
    );
    ensure!(
        !options.fetch || options.budget > 0,
        "--fetch requires an explicit positive --budget"
    );
    let cases = match &options.corpus {
        Some(path) => load_corpus(path)?,
        None => load_suite()?,
    };
    let excluded = cases
        .iter()
        .filter(|c| c.expect.as_ref().is_some_and(|e| e.error.is_some()))
        .count();
    let cases: Vec<_> = cases
        .into_iter()
        .filter(|c| c.expect.as_ref().is_none_or(|e| e.error.is_none()))
        .filter(|c| options.only.as_ref().is_none_or(|re| re.is_match(&c.id)))
        .collect();
    ensure!(!cases.is_empty(), "no cases selected");
    let mut ids = HashSet::new();
    ensure!(
        cases.iter().all(|c| ids.insert(&c.id)),
        "duplicate case ids"
    );
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
        let req = request(&case.query);
        if read_entry(&options.cache, &req)?.is_none() {
            missing.insert(key(&req));
        }
    }
    eprintln!(
        "{} cases, {} distinct uncached requests; offline={}",
        cases.len(),
        missing.len(),
        !options.fetch
    );
    ensure!(
        missing.is_empty() || options.fetch,
        "offline cache misses: {}; use --fetch --budget N to authorize permanent requests",
        missing.len()
    );
    ensure!(
        !options.fetch || missing.len() <= options.budget,
        "{} missing requests exceed --budget {}; no requests sent",
        missing.len(),
        options.budget
    );
    let client = if missing.is_empty() {
        None
    } else {
        let token = std::env::var("MAPBOX_ACCESS_TOKEN")
            .context("set MAPBOX_ACCESS_TOKEN through your secret manager")?;
        ensure!(!token.trim().is_empty(), "MAPBOX_ACCESS_TOKEN is empty");
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
        let req = request(&case.query);
        let cache_path = options.cache.join(format!("{}.json", key(&req)));
        let entry = match read_entry(&options.cache, &req)? {
            Some(entry) => entry,
            None => {
                let (client, token) = client.as_ref().context("offline cache miss")?;
                if fetched > 0 {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                let entry = fetch(client, token, req).await?;
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
        if entry.status == 200 {
            let response = normalize(&entry.response)?;
            classify(case, Ok(&response), &mut record, false);
            record.summary = Some(summary_of(&response));
            record.response = Some(response);
        } else {
            classify(case, Err("http_422"), &mut record, false);
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
        "suite": "mapbox-forward", "started_at": started_at, "cases": cases.len(),
        "corpus_sha256": sha256_files(&files)?,
        "cache_sha256": sha256_files(&cache_files.into_keys().collect::<Vec<_>>())?,
        "scoring": {"confidence": false},
        "request_settings": request(""), "fetched": fetched,
        "excluded_contract_cases": excluded,
        "backend": {"label": "Mapbox Geocoding v6 permanent", "status": {"software_version": "Mapbox hosted API", "data_updated": "unknown"}},
    });
    let (report, _) = summarize(&cases, &records, &manifest);
    let mut report = format!(
        "Forward geocoding only; confidence is not scored. Known failures remain failures. This existing corpus is diagnostic, not an unbiased accuracy estimate. Native responses and collection dates are in the private cache. No request-latency comparison is made.\n\n{report}"
    );
    if let Some(baseline) = &options.compare {
        report += &paired(&cases, &records, baseline)?;
    }
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

    fn response(kind: &str) -> Value {
        json!({"features": [{"properties": {
            "feature_type": kind, "name": "12 Example Road", "full_address": "12 Example Road, Exampletown",
            "context": {
                "address": {"address_number": "12"}, "street": {"name": "Example Road"},
                "postcode": {"name": "12345"}, "place": {"name": "Exampletown"},
                "region": {"region_code_full": "US-CA"}, "country": {"country_code": "US"}
            }
        }, "geometry": {"coordinates": [-122.0, 37.0]}}]})
    }

    #[test]
    fn normalization_preserves_granularity_and_coordinates() {
        for (kind, level) in [
            ("address", "house"),
            ("secondary_address", "house"),
            ("street", "street"),
            ("postcode", "postcode"),
            ("place", "locality"),
            ("region", "region"),
            ("country", "country"),
        ] {
            let mut input = response(kind);
            if kind == "postcode" {
                input["features"][0]["properties"]["name"] = "12345".into();
                input["features"][0]["properties"]["full_address"] = "12345 Exampletown".into();
            }
            let normalized = normalize(&input).unwrap();
            assert_eq!(crate::score::level_of(&normalized), level);
            assert_eq!(crate::score::coordinates(&normalized), Some([37., -122.]));
            assert_eq!(normalized["addressDetails"]["countryCode"], "us");
            assert!(normalized.get("goodMatch").is_none());
        }
        assert_eq!(normalize(&json!({"features": []})).unwrap(), json!({}));
        assert!(normalize(&json!({})).is_err());
        let mut invalid = response("address");
        invalid["features"][0]["geometry"]["coordinates"][1] = 100.into();
        assert!(normalize(&invalid).is_err());
    }

    #[tokio::test]
    async fn offline_cache_replay_deduplicates_and_never_requires_a_token() {
        let dir = std::env::temp_dir().join(format!("mapbox-test-{}", std::process::id()));
        let cache = dir.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        let corpus = dir.join("cases.jsonl");
        let case = Case {
            id: "one".into(),
            query: "Exampletown".into(),
            expect: Some(crate::score::Expect {
                level: "locality".into(),
                country: vec!["us".into()],
                good_match: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let other = Case {
            id: "two".into(),
            query: "Other town".into(),
            ..case.clone()
        };
        let write_cases = |second: &Case| {
            std::fs::write(
                &corpus,
                format!(
                    "{}\n{}\n",
                    serde_json::to_string(&case).unwrap(),
                    serde_json::to_string(second).unwrap()
                ),
            )
            .unwrap()
        };
        write_cases(&other);
        let options = |fetch, budget| Options {
            corpus: Some(corpus.clone()),
            cache: cache.clone(),
            out: dir.join("out"),
            only: None,
            fetch,
            budget,
            compare: None,
        };
        assert!(
            run(options(false, 0))
                .await
                .unwrap_err()
                .to_string()
                .contains("offline cache misses")
        );
        assert!(run(options(true, 0)).await.is_err());
        assert!(
            run(options(true, 1))
                .await
                .unwrap_err()
                .to_string()
                .contains("exceed --budget")
        );
        let duplicate = Case {
            query: case.query.clone(),
            ..other
        };
        write_cases(&duplicate);
        let req = request(&case.query);
        assert_ne!(key(&req), key(&request("Other town")));
        assert_eq!(req["permanent"], true);
        assert_eq!(req["autocomplete"], false);
        save_entry(
            &cache,
            &Entry {
                request: req.clone(),
                fetched_at: "2026-01-01T00:00:00Z".into(),
                status: 200,
                response: response("place"),
            },
        )
        .unwrap();
        assert_eq!(run(options(false, 0)).await.unwrap(), 0);
        assert_eq!(run(options(true, 1)).await.unwrap(), 0);
        let manifest: Value =
            serde_json::from_slice(&std::fs::read(dir.join("out/manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["fetched"], 0);
        assert_eq!(manifest["scoring"]["confidence"], false);
        let mut records: Vec<Record> =
            crate::model::load_jsonl(&dir.join("out/results.jsonl")).unwrap();
        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|r| r.class == "pass"));
        crate::rescore(&[case.clone(), duplicate.clone()], &mut records, false);
        assert!(records.iter().all(|r| r.class == "pass"));
        let baseline = dir.join("baseline.json");
        std::fs::write(
            &baseline,
            json!({"manifest": {}, "results": records}).to_string(),
        )
        .unwrap();
        let paired_report = paired(&[case.clone(), duplicate], &records, &baseline).unwrap();
        assert!(paired_report.contains("| Both pass | 2 |"));
        let mut changed = req.clone();
        changed["language"] = "da".into();
        assert_ne!(key(&req), key(&changed));
        assert!(read_entry(&cache, &changed).unwrap().is_none());
        save_entry(
            &cache,
            &Entry {
                request: req.clone(),
                fetched_at: String::new(),
                status: 422,
                response: json!({"message": "Query too long"}),
            },
        )
        .unwrap();
        assert_eq!(run(options(false, 0)).await.unwrap(), 0);
        let records: Vec<Record> =
            crate::model::load_jsonl(&dir.join("out/results.jsonl")).unwrap();
        assert!(
            records
                .iter()
                .all(|r| r.error.as_deref() == Some("http_422"))
        );
        std::fs::write(cache.join(format!("{}.json", key(&req))), "broken").unwrap();
        assert!(
            run(options(true, 1))
                .await
                .unwrap_err()
                .to_string()
                .contains("invalid cache entry")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
