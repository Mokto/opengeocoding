use super::{Provider, full_address};
use crate::score::text;
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

pub const PROVIDER: Provider = Provider {
    name: "google",
    label: "Google Geocoding v3",
    credential_env: "GOOGLE_MAPS_API_KEY",
    auth_parameter: "key",
    storage_terms: Some("https://developers.google.com/maps/documentation/geocoding/policies"),
    request,
    normalize,
};

fn request(query: &str) -> Value {
    json!({"endpoint": "https://maps.googleapis.com/maps/api/geocode/json", "address": query, "language": "en"})
}

fn has_type(value: &Value, kind: &str) -> bool {
    value["types"]
        .as_array()
        .is_some_and(|types| types.iter().any(|t| t.as_str() == Some(kind)))
}

fn normalize(response: &Value) -> Result<Value> {
    match text(response, "status") {
        "ZERO_RESULTS" => return Ok(json!({})),
        "OK" => (),
        "OVER_DAILY_LIMIT" | "OVER_QUERY_LIMIT" => {
            bail!("Google quota exceeded; stopped without retrying")
        }
        "REQUEST_DENIED" => bail!("Google request denied; check the key, API access and billing"),
        "INVALID_REQUEST" => bail!("Google invalid request"),
        "UNKNOWN_ERROR" => bail!("Google server error; stopped without retrying"),
        _ => bail!("invalid Google API status"),
    }
    let result = response["results"]
        .as_array()
        .and_then(|r| r.first())
        .context("Google OK without results")?;
    let components = result["address_components"]
        .as_array()
        .context("missing address components")?;
    let component = |kind, field| {
        components
            .iter()
            .find(|c| has_type(c, kind))
            .map_or("", |c| text(c, field))
    };
    let country = component("country", "short_name").to_lowercase();
    let mut address = json!({"countryCode": country});
    let is_address = [
        "street_address",
        "premise",
        "subpremise",
        "establishment",
        "point_of_interest",
    ]
    .iter()
    .any(|t| has_type(result, t));
    if is_address || has_type(result, "route") || has_type(result, "intersection") {
        address["road"] = component("route", "long_name").into();
        if is_address {
            address["houseNumber"] = component("street_number", "long_name").into();
        }
        address["city"] = component("locality", "long_name").into();
        address["postcode"] = component("postal_code", "long_name").into();
        ensure!(
            !text(&address, "road").is_empty() || !text(&address, "city").is_empty(),
            "Google address lacks street and locality"
        );
    } else if has_type(result, "postal_code") || has_type(result, "postal_code_prefix") {
        address["postcode"] = components
            .iter()
            .find(|c| has_type(c, "postal_code") || has_type(c, "postal_code_prefix"))
            .map_or("", |c| text(c, "long_name"))
            .into();
        ensure!(
            !text(&address, "postcode").is_empty(),
            "Google postcode result lacks postcode"
        );
    } else if has_type(result, "country") {
    } else if let Some(kind) = (1..=7)
        .map(|n| format!("administrative_area_level_{n}"))
        .find(|t| has_type(result, t))
    {
        address["state"] = component(&kind, "long_name").into();
    } else {
        let kind = [
            "locality",
            "postal_town",
            "sublocality",
            "neighborhood",
            "sublocality_level_1",
            "sublocality_level_2",
        ]
        .into_iter()
        .find(|t| has_type(result, t))
        .context("unsupported Google result type")?;
        address["city"] = component(kind, "long_name").into();
    }
    full_address(
        text(result, "formatted_address"),
        &result["geometry"]["location"]["lat"],
        &result["geometry"]["location"]["lng"],
        address,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_native_types_not_ancestor_components_or_rooftop_labels() {
        let component =
            |kind, name| json!({"types": [kind], "short_name": name, "long_name": name});
        let mut response = json!({"status": "OK", "results": [{"types": ["street_address"], "formatted_address": "12 Example Road", "geometry": {"location": {"lat": 37., "lng": -122.}, "location_type": "ROOFTOP"}, "address_components": [component("country", "US"), component("route", "Example Road"), component("street_number", "12"), component("locality", "Exampletown"), component("postal_code", "12345"), component("administrative_area_level_1", "CA")]}]});
        for (kind, level) in [
            ("street_address", "house"),
            ("route", "street"),
            ("locality", "locality"),
            ("administrative_area_level_1", "region"),
            ("postal_code", "postcode"),
            ("country", "country"),
        ] {
            response["results"][0]["types"] = json!([kind]);
            let normalized = normalize(&response).unwrap();
            assert_eq!(crate::score::level_of(&normalized), level);
            assert_eq!(crate::score::coordinates(&normalized), Some([37., -122.]));
            assert!(normalized.get("goodMatch").is_none());
            assert!(crate::score::region_codes(&normalized).is_empty());
        }
        assert_eq!(
            normalize(&json!({"status": "ZERO_RESULTS", "results": []})).unwrap(),
            json!({})
        );
        assert!(normalize(&json!({"status": "REQUEST_DENIED", "results": []})).is_err());
        assert!(normalize(&json!({"status": "OVER_QUERY_LIMIT", "results": []})).is_err());
        assert!(normalize(&json!({"status": "OK", "results": []})).is_err());
    }
}
