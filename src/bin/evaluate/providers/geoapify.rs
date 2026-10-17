use super::{Provider, full_address};
use crate::score::text;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

pub const PROVIDER: Provider = Provider {
    name: "geoapify",
    label: "Geoapify Geocoding v1",
    credential_env: "GEOAPIFY_API_KEY",
    auth_parameter: "apiKey",
    query_error_statuses: &[422],
    not_found_status: None,
    interval_ms: 200,
    storage_terms: None,
    request,
    normalize,
    validate_status: None,
};

fn request(query: &str) -> Value {
    json!({"endpoint": "https://api.geoapify.com/v1/geocode/search", "text": query, "format": "json", "lang": "en", "limit": 1})
}

fn normalize(response: &Value) -> Result<Value> {
    let results = response["results"]
        .as_array()
        .context("missing Geoapify results array")?;
    let Some(result) = results.first() else {
        return Ok(json!({}));
    };
    let mut address = json!({"countryCode": text(result, "country_code").to_lowercase()});
    match text(result, "result_type") {
        "building" | "amenity" | "street" => {
            if text(result, "result_type") != "street" {
                address["houseNumber"] = result["housenumber"].clone();
            }
            address["road"] = result["street"].clone();
            address["city"] = result["city"].clone();
            address["postcode"] = result["postcode"].clone();
        }
        "postcode" => address["postcode"] = result["postcode"].clone(),
        "country" => (),
        "state" => address["state"] = result["state"].clone(),
        "county" => address["county"] = result["county"].clone(),
        kind @ ("city" | "town" | "village" | "hamlet" | "district" | "suburb" | "locality") => {
            address["city"] = result
                .get(kind)
                .filter(|v| v.as_str().is_some_and(|s| !s.is_empty()))
                .unwrap_or(&result["city"])
                .clone();
        }
        _ => bail!("unsupported Geoapify result type"),
    }
    full_address(
        text(result, "formatted"),
        &result["lat"],
        &result["lon"],
        address,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_resolution_overrides_address_context_and_confidence() {
        let mut input = json!({"results": [{"result_type": "building", "country_code": "US", "housenumber": "12", "street": "Example Road", "city": "Exampletown", "state": "California", "postcode": "12345", "formatted": "12 Example Road", "lat": 37., "lon": -122., "rank": {"confidence": 1}}]});
        for (kind, level) in [
            ("building", "house"),
            ("street", "street"),
            ("city", "locality"),
            ("state", "region"),
            ("postcode", "postcode"),
            ("country", "country"),
        ] {
            input["results"][0]["result_type"] = kind.into();
            let result = normalize(&input).unwrap();
            assert_eq!(crate::score::level_of(&result), level);
            assert_eq!(crate::score::coordinates(&result), Some([37., -122.]));
            assert!(result.get("goodMatch").is_none());
        }
        input["results"][0]["lat"] = 95.into();
        assert!(normalize(&input).is_err());
        assert_eq!(normalize(&json!({"results": []})).unwrap(), json!({}));
        assert!(normalize(&json!({"error": "denied"})).is_err());
    }
}
