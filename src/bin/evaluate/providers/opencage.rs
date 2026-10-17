use super::{Provider, full_address};
use crate::score::text;
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

pub const PROVIDER: Provider = Provider {
    name: "opencage",
    label: "OpenCage Geocoding v1",
    credential_env: "OPENCAGE_API_KEY",
    auth_parameter: "key",
    query_error_statuses: &[400, 422],
    not_found_status: None,
    interval_ms: 1000,
    storage_terms: None,
    request,
    normalize,
};

fn request(query: &str) -> Value {
    json!({"endpoint": "https://api.opencagedata.com/geocode/v1/json", "q": query, "language": "en", "limit": 1, "no_annotations": 1, "no_record": 1})
}

fn normalize(response: &Value) -> Result<Value> {
    ensure!(
        response["status"]["code"].as_u64() == Some(200),
        "OpenCage API error or missing status; stopped without retrying"
    );
    let results = response["results"]
        .as_array()
        .context("missing OpenCage results array")?;
    let Some(result) = results.first() else {
        return Ok(json!({}));
    };
    let native = &result["components"];
    let mut address = json!({"countryCode": text(native, "country_code").to_lowercase()});
    match text(native, "_type") {
        "country" => (),
        "state" | "province" | "region" => {
            address["state"] = native
                .get(text(native, "_type"))
                .unwrap_or(&native["state"])
                .clone()
        }
        "county" | "state_district" => address["county"] = native[text(native, "_type")].clone(),
        "postcode" | "postal_code" => address["postcode"] = native["postcode"].clone(),
        "road" | "pedestrian" | "footway" | "path" => {
            address["road"] = native
                .get(text(native, "_type"))
                .unwrap_or(&native["road"])
                .clone()
        }
        kind @ ("city" | "town" | "village" | "hamlet" | "municipality" | "suburb"
        | "neighbourhood" | "city_district") => {
            address["city"] = native
                .get(kind)
                .unwrap_or(&native["_normalized_city"])
                .clone();
        }
        _ if matches!(text(native, "_type"), "house" | "building")
            || [
                "agriculture",
                "building",
                "commerce",
                "construction",
                "education",
                "financial",
                "government",
                "health",
                "industrial",
                "military",
                "natural/water",
                "outdoors/recreation",
                "place_of_worship",
                "social",
                "transportation",
                "travel/tourism",
            ]
            .contains(&text(native, "_category")) =>
        {
            address["houseNumber"] = native["house_number"].clone();
            address["road"] = native["road"].clone();
            address["city"] = native["_normalized_city"].clone();
            address["postcode"] = native["postcode"].clone();
        }
        _ => bail!("unsupported OpenCage result type"),
    }
    if let Some(codes) = native["ISO_3166-2"].as_array() {
        address["regionCodes"] = codes
            .iter()
            .filter_map(Value::as_str)
            .map(|code| json!({"code": code.to_uppercase()}))
            .collect();
    }
    full_address(
        text(result, "formatted"),
        &result["geometry"]["lat"],
        &result["geometry"]["lng"],
        address,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_native_types_and_iso_codes_without_interpreting_confidence() {
        let mut input = json!({"status": {"code": 200}, "results": [{"components": {"_type": "house", "_category": "building", "house_number": "12", "road": "Example Road", "city": "Exampletown", "state": "California", "postcode": "12345", "country_code": "us", "ISO_3166-2": ["US-CA"]}, "formatted": "12 Example Road", "geometry": {"lat": 37., "lng": -122.}, "confidence": 10}]});
        for (kind, level) in [
            ("house", "house"),
            ("road", "street"),
            ("city", "locality"),
            ("state", "region"),
            ("postcode", "postcode"),
            ("country", "country"),
        ] {
            input["results"][0]["components"]["_type"] = kind.into();
            let result = normalize(&input).unwrap();
            assert_eq!(crate::score::level_of(&result), level);
            assert!(crate::score::region_codes(&result).contains("US-CA"));
            assert!(result.get("goodMatch").is_none());
        }
        input["results"][0]["components"]["_type"] = "charging_station".into();
        input["results"][0]["components"]["_category"] = "transportation".into();
        assert_eq!(crate::score::level_of(&normalize(&input).unwrap()), "house");
        input["results"][0]["components"]["_type"] = "museum".into();
        input["results"][0]["components"]["_category"] = "outdoors/recreation".into();
        assert_eq!(crate::score::level_of(&normalize(&input).unwrap()), "house");
        input["results"][0]["components"]["house_number"] = Value::Null;
        assert_eq!(
            crate::score::level_of(&normalize(&input).unwrap()),
            "street"
        );
        input["results"][0]["components"]["_type"] = "unknown_place".into();
        input["results"][0]["components"]["_category"] = "place".into();
        assert!(normalize(&input).is_err());
        assert_eq!(
            normalize(&json!({"status": {"code": 200}, "results": []})).unwrap(),
            json!({})
        );
        assert!(normalize(&json!({"status": {"code": 402}, "results": []})).is_err());
        assert!(normalize(&json!({"results": []})).is_err());
    }
}
