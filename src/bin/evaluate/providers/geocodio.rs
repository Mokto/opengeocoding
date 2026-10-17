use super::{Provider, full_address};
use crate::score::text;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

pub const PROVIDER: Provider = Provider {
    name: "geocodio",
    label: "Geocodio API v2",
    credential_env: "GEOCODIO_API_KEY",
    auth_parameter: "api_key",
    not_found_status: None,
    interval_ms: 200,
    storage_terms: Some("https://www.geocod.io/terms-of-use/"),
    request,
    normalize,
};

fn request(query: &str) -> Value {
    json!({"endpoint": "https://api.geocod.io/v2/geocode", "q": query, "limit": 1})
}

fn normalize(response: &Value) -> Result<Value> {
    let results = response["results"]
        .as_array()
        .context("missing Geocodio results array")?;
    let Some(result) = results.first() else {
        return Ok(json!({}));
    };
    let native = &result["address_components"];
    let mut address = json!({"countryCode": text(native, "country").to_lowercase()});
    match text(result, "accuracy_type") {
        "rooftop"
        | "point"
        | "range_interpolation"
        | "intersection"
        | "street_center"
        | "nearest_rooftop_match" => {
            if matches!(
                text(result, "accuracy_type"),
                "rooftop" | "point" | "range_interpolation"
            ) {
                address["houseNumber"] = native["number"].clone();
            }
            address["road"] = native["formatted_street"].clone();
            address["city"] = native["city"].clone();
            address["postcode"] = native["postal_code"].clone();
        }
        // ponytail: "place" conflates ZIP and city centroids; require a distinct native type before crediting postcode precision.
        "place" => address["city"] = native["city"].clone(),
        "county" => address["county"] = native["county"].clone(),
        "state" => address["state"] = native["state_province"].clone(),
        _ => bail!("unsupported Geocodio accuracy type"),
    }
    full_address(
        text(result, "formatted_address"),
        &result["location"]["lat"],
        &result["location"]["lng"],
        address,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_accuracy_type_not_input_components_or_accuracy_score() {
        let mut input = json!({"results": [{"address_components": {"number": "12", "formatted_street": "Example Road", "city": "Exampletown", "county": "Example County", "state_province": "CA", "postal_code": "12345", "country": "US"}, "formatted_address": "12 Example Road", "location": {"lat": 37., "lng": -122.}, "accuracy_type": "rooftop", "accuracy": 1}]});
        for (kind, level) in [
            ("rooftop", "house"),
            ("range_interpolation", "house"),
            ("nearest_rooftop_match", "street"),
            ("street_center", "street"),
            ("place", "locality"),
            ("county", "region"),
            ("state", "region"),
        ] {
            input["results"][0]["accuracy_type"] = kind.into();
            let result = normalize(&input).unwrap();
            assert_eq!(crate::score::level_of(&result), level);
            assert_eq!(crate::score::coordinates(&result), Some([37., -122.]));
            assert!(result.get("goodMatch").is_none());
        }
        assert_eq!(normalize(&json!({"results": []})).unwrap(), json!({}));
        assert!(normalize(&json!({"error": "invalid key"})).is_err());
    }
}
