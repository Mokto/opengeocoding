use super::{Provider, full_address};
use crate::score::text;
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

pub const PROVIDER: Provider = Provider {
    name: "google",
    label: "Google Geocoding v3",
    credential_env: "GOOGLE_MAPS_API_KEY",
    query_error_statuses: &[422],
    not_found_status: None,
    interval_ms: 200,
    auth_parameter: "key",
    storage_terms: Some("https://developers.google.com/maps/documentation/geocoding/policies"),
    request,
    normalize,
    validate_status: Some(validate_status),
    reverse: None,
};

fn request(query: &str) -> Value {
    json!({"endpoint": "https://maps.googleapis.com/maps/api/geocode/json", "address": query, "language": "en"})
}

fn has_type(value: &Value, kind: &str) -> bool {
    value["types"]
        .as_array()
        .is_some_and(|types| types.iter().any(|t| t.as_str() == Some(kind)))
}

fn validate_status(response: &Value) -> Result<()> {
    match text(response, "status") {
        "ZERO_RESULTS" | "OK" => (),
        "OVER_DAILY_LIMIT" | "OVER_QUERY_LIMIT" => {
            bail!("Google quota exceeded; stopped without retrying")
        }
        "REQUEST_DENIED" => bail!("Google request denied; check the key, API access and billing"),
        "INVALID_REQUEST" => bail!("Google invalid request"),
        "UNKNOWN_ERROR" => bail!("Google server error; stopped without retrying"),
        _ => bail!("invalid Google API status"),
    }
    Ok(())
}

fn normalize(response: &Value) -> Result<Value> {
    validate_status(response)?;
    if text(response, "status") == "ZERO_RESULTS" {
        return Ok(json!({}));
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
    let mut country = component("country", "short_name").to_lowercase();
    if country.len() != 2 || !country.bytes().all(|b| b.is_ascii_alphabetic()) {
        country = opengeocoding::matching::expected_country(&indexmap::IndexMap::from([(
            "country".into(),
            component("country", "long_name").into(),
        )]));
    }
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
        address["city"] = ["locality", "postal_town", "sublocality", "neighborhood"]
            .into_iter()
            .map(|kind| component(kind, "long_name"))
            .find(|name| !name.is_empty())
            .unwrap_or_default()
            .into();
        address["postcode"] = component("postal_code", "long_name").into();
        ensure!(
            ["houseNumber", "road", "city"]
                .iter()
                .any(|key| !text(&address, key).is_empty()),
            "Google address lacks number, street and locality"
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
            "sublocality_level_3",
            "sublocality_level_4",
            "sublocality_level_5",
        ]
        .into_iter()
        .find(|t| has_type(result, t));
        if let Some(kind) = kind {
            address["city"] = component(kind, "long_name").into();
        } else if has_type(result, "colloquial_area") && has_type(result, "political") {
            let name = component("colloquial_area", "long_name");
            ensure!(!name.is_empty(), "Google area lacks a primary name");
            address["state"] = name.into();
        } else {
            bail!("unsupported Google result type");
        }
    }
    full_address(
        text(result, "formatted_address"),
        &result["geometry"]["location"]["lat"],
        &result["geometry"]["location"]["lng"],
        address,
    )
}

pub(super) fn normalization_failure(error: &anyhow::Error, response: &Value) -> &'static str {
    match error.to_string().as_str() {
        "Google OK without results" => "missing_results",
        "missing address components" => "missing_components",
        "invalid country code" => {
            if response["results"][0]["address_components"]
                .as_array()
                .is_some_and(|components| components.iter().any(|c| has_type(c, "country")))
            {
                "invalid_country"
            } else {
                "missing_country"
            }
        }
        "missing latitude" | "missing longitude" => "missing_coordinates",
        "invalid point" => "invalid_coordinates",
        "Google address lacks number, street and locality" => "missing_address_context",
        "Google area lacks a primary name" => "missing_primary_name",
        "Google postcode result lacks postcode" => "missing_postcode",
        "unsupported Google result type" => [
            "natural_feature",
            "park",
            "airport",
            "plus_code",
            "colloquial_area",
            "sublocality_level_3",
            "sublocality_level_4",
            "sublocality_level_5",
            "political",
        ]
        .into_iter()
        .find(|kind| has_type(&response["results"][0], kind))
        .unwrap_or("unsupported_type"),
        _ => "other",
    }
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

    #[test]
    fn handles_native_locality_variants_without_inventing_country_or_precision() {
        let component = |kind, name| json!({"types":[kind],"short_name":name,"long_name":name});
        let response = |types: Vec<&str>, parts: Vec<Value>| {
            json!({"status":"OK","results":[{
                "types":types,"formatted_address":"Example","geometry":{"location":{"lat":37.,"lng":-122.}},"address_components":parts
            }]})
        };
        let country = component("country", "US");
        let context = vec![
            country.clone(),
            component("street_number", "12"),
            component("route", "Parent Road"),
            component("locality", "Parent City"),
            component("postal_code", "12345"),
        ];
        for kind in [
            "sublocality_level_3",
            "sublocality_level_4",
            "sublocality_level_5",
        ] {
            let mut parts = context.clone();
            parts.push(component(kind, "Example District"));
            let normalized = normalize(&response(vec![kind, "political"], parts)).unwrap();
            assert_eq!(crate::score::level_of(&normalized), "locality");
            assert_eq!(normalized["addressDetails"]["city"], "Example District");
        }
        let mut parts = context.clone();
        parts.push(component("colloquial_area", "Example Area"));
        let normalized = normalize(&response(
            vec!["colloquial_area", "political"],
            parts.clone(),
        ))
        .unwrap();
        assert_eq!(crate::score::level_of(&normalized), "region");
        assert_eq!(normalized["addressDetails"]["state"], "Example Area");
        let normalized = normalize(&response(
            vec!["colloquial_area", "political", "locality"],
            parts,
        ))
        .unwrap();
        assert_eq!(crate::score::level_of(&normalized), "locality");
        let normalized = normalize(&response(
            vec!["premise"],
            vec![country.clone(), component("postal_town", "Postal City")],
        ))
        .unwrap();
        assert_eq!(normalized["addressDetails"]["city"], "Postal City");
        assert_eq!(crate::score::level_of(&normalized), "locality");
        let normalized = normalize(&response(
            vec!["street_address"],
            vec![country, component("street_number", "12")],
        ))
        .unwrap();
        assert_eq!(crate::score::level_of(&normalized), "house");
        let full_country = json!({"types":["country"],"long_name":"United States","short_name":""});
        let normalized = normalize(&response(vec!["country"], vec![full_country])).unwrap();
        assert_eq!(normalized["addressDetails"]["countryCode"], "us");
        let conflicting =
            json!({"types":["country"],"long_name":"United States","short_name":"CA"});
        let normalized = normalize(&response(vec!["country"], vec![conflicting])).unwrap();
        assert_eq!(normalized["addressDetails"]["countryCode"], "ca");
        let missing = response(vec!["country"], vec![]);
        let error = normalize(&missing).unwrap_err();
        assert_eq!(normalization_failure(&error, &missing), "missing_country");
        assert_eq!(
            normalization_failure(&anyhow::anyhow!("PRIVATE_RESPONSE_MARKER"), &missing),
            "other"
        );
        let unsupported = response(vec!["natural_feature"], context);
        let error = normalize(&unsupported).unwrap_err();
        assert_eq!(
            normalization_failure(&error, &unsupported),
            "natural_feature"
        );
    }
}
