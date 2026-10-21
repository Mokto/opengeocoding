use opengeocoding::eval::holdout;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[test]
fn global_address_benchmark_preserves_its_source_records_and_scope() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/global");
    let manifest: Value =
        serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let text = fs::read_to_string(root.join("global-addresses.jsonl")).unwrap();
    let adjudications: Value =
        serde_json::from_slice(&fs::read(root.join("adjudications.json")).unwrap()).unwrap();
    let mut alternatives: BTreeMap<(u64, u64), &Value> = adjudications["au"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            let key = (a["node"].as_u64().unwrap(), a["version"].as_u64().unwrap());
            (key, a)
        })
        .collect();
    assert_eq!(
        adjudications["counts"]["adjudicated"].as_u64().unwrap() as usize,
        alternatives.len()
    );
    let cases: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        manifest["evaluation_role"],
        "source-consistency-not-independent-accuracy"
    );
    assert!(manifest["errors"].as_object().unwrap().is_empty());
    assert_eq!(manifest["coverage"].as_object().unwrap().len(), 130);
    let by_id: BTreeMap<_, _> = cases
        .iter()
        .map(|case| (case["id"].as_str().unwrap(), case))
        .collect();
    let mut ids = BTreeSet::new();
    let mut expected = BTreeSet::new();
    for snapshot in manifest["snapshots"].as_array().unwrap() {
        let cc = snapshot["country"].as_str().unwrap().to_ascii_lowercase();
        let records = snapshot["records"].as_array().unwrap();
        assert!(!records.is_empty(), "no addresses for {cc}");
        let coverage = &manifest["coverage"][cc.to_ascii_uppercase()];
        assert_eq!(
            coverage["locations"].as_u64().unwrap() as usize,
            records.len()
        );
        for record in records {
            let node = record["id"].as_u64().unwrap();
            let version = record["version"].as_u64().unwrap();
            let alternative = (cc == "au")
                .then(|| alternatives.remove(&(node, version)))
                .flatten();
            let queries = record["queries"].as_array().unwrap();
            for (variant, query) in queries.iter().enumerate() {
                let id = format!("global-osm-{cc}-{node}-{variant}");
                assert!(expected.insert(id.clone()));
                let case = by_id[id.as_str()];
                assert_eq!(&case["query"], query);
                assert!(query.as_str().unwrap().chars().count() <= 200);
                // an adjudicated record also accepts G-NAF's point, and nothing else
                let expect = match alternative {
                    Some(a) => {
                        let [osm, gnaf] = case["expect"]["any_of"].as_array().unwrap().as_slice()
                        else {
                            panic!("{id}: two alternatives");
                        };
                        let mut moved = osm.clone();
                        moved["point"] = a["point"].clone();
                        assert_eq!(gnaf, &moved);
                        assert_eq!(case["adjudication"]["ref"], a["pid"]);
                        assert_eq!(case["adjudication"]["name"], "G-NAF");
                        osm
                    }
                    None => {
                        assert!(case.get("adjudication").is_none());
                        &case["expect"]
                    }
                };
                // a territory also answers as its sovereign country
                let mut countries = vec![cc.as_str()];
                countries.extend(match cc.as_str() {
                    "pr" => Some("us"),
                    "nc" => Some("fr"),
                    _ => None,
                });
                countries.sort();
                assert_eq!(expect["country"], serde_json::json!(countries));
                assert_eq!(expect["point"], record["point"]);
                assert_eq!(expect["level"], "house");
                assert_eq!(expect["radius_km"], 0.15);
                assert_eq!(
                    expect["components"]["house_number"],
                    record["tags"]["addr:housenumber"]
                );
                assert_eq!(case["location"], record["location"]);
                assert_eq!(
                    case["holdout"],
                    holdout(record["location"].as_str().unwrap())
                );
                assert_eq!(case["source"]["name"], "openstreetmap-address-node");
                assert_eq!(case["source"]["license"], "ODbL-1.0");
                assert_eq!(
                    case["source"]["url"],
                    format!(
                        "https://www.openstreetmap.org/api/0.6/node/{node}/{}",
                        record["version"].as_u64().unwrap()
                    )
                );
                assert_eq!(
                    case["source"]["snapshot_sha256"],
                    snapshot["proof"]["source_sha256"]
                );
                assert_eq!(
                    case["source"]["evaluation_role"],
                    manifest["evaluation_role"]
                );
                assert_eq!(case["source"]["version"], record["version"]);
                assert!(case.get("known_failure").is_none());
                assert!(case.get("stored").is_none());
            }
            for key in record["tags"].as_object().unwrap().keys() {
                assert!(key.starts_with("addr:"));
                assert_ne!(key, "addr:full");
            }
        }
    }
    assert!(alternatives.is_empty(), "unused adjudications");
    for case in &cases {
        assert!(ids.insert(case["id"].as_str().unwrap().to_owned()));
    }
    assert_eq!(ids, expected);
}
