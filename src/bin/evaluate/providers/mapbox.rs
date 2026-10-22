use super::Provider;
use crate::score::text;
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

const ENDPOINT: &str = "https://api.mapbox.com/search/geocode/v6/forward";

pub const PROVIDER: Provider = Provider {
    name: "mapbox",
    label: "Mapbox Geocoding v6 permanent",
    credential_env: "MAPBOX_ACCESS_TOKEN",
    query_error_statuses: &[422],
    not_found_status: None,
    interval_ms: 200,
    auth_parameter: "access_token",
    storage_terms: None,
    request,
    normalize,
    validate_status: None,
    reverse: Some(REVERSE),
};

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

// v6 reverse rejects `limit` without `types` and returns the finest feature first.
const REVERSE: super::reverse::Reverse = super::reverse::Reverse {
    request: |lat, lon| json!({"endpoint": "https://api.mapbox.com/search/geocode/v6/reverse", "longitude": lon, "latitude": lat, "permanent": true, "language": "en", "worldview": "us"}),
    normalize: reverse_normalize,
};

fn reverse_normalize(response: &Value) -> Result<Value> {
    let mut address = normalize(response)?;
    let context = &response["features"][0]["properties"]["context"];
    let names: serde_json::Map<_, _> = ["place", "locality", "neighborhood", "district", "region"]
        .iter()
        .map(|k| ((*k).to_string(), context[*k]["name"].clone()))
        .collect();
    super::reverse::add_places(
        &mut address,
        &names.into(),
        &[
            ("place", "city"),
            ("locality", "suburb"),
            ("neighborhood", "neighbourhood"),
            ("district", "county"),
            ("region", "state"),
        ],
    );
    Ok(address)
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
}
