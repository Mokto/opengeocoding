use super::{Provider, full_address};
use crate::score::text;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

pub const PROVIDER: Provider = Provider {
    name: "mapsco",
    label: "geocode.maps.co Search",
    credential_env: "MAPSCO_API_KEY",
    auth_parameter: "api_key",
    query_error_statuses: &[400, 422],
    not_found_status: None,
    interval_ms: 1000,
    storage_terms: None,
    request,
    normalize,
    validate_status: None,
};

fn request(query: &str) -> Value {
    json!({"endpoint": "https://geocode.maps.co/search", "q": query, "format": "geocodejson", "addressdetails": 1, "accept-language": "en", "limit": 1, "extratags": 0, "namedetails": 0})
}

fn normalize(response: &Value) -> Result<Value> {
    let features = response["features"]
        .as_array()
        .context("missing maps.co features array")?;
    let Some(feature) = features.first() else {
        return Ok(json!({}));
    };
    let native = &feature["properties"]["geocoding"];
    // Synthetic postcodes can have an administrative GeocodeJSON type.
    let kind = if matches!(text(native, "osm_key"), "place" | "boundary")
        && matches!(text(native, "osm_value"), "postcode" | "postal_code")
    {
        "postcode"
    } else {
        text(native, "type")
    };
    let name = [text(native, "name"), text(native, kind)]
        .into_iter()
        .find(|name| !name.is_empty())
        .unwrap_or_default();
    let mut address = json!({"countryCode": text(native, "country_code").to_lowercase()});
    match kind {
        "country" => (),
        "state" | "county" => address[kind] = json!(name),
        "postcode" => {
            address["postcode"] = json!(if text(native, "postcode").is_empty() {
                name
            } else {
                text(native, "postcode")
            });
        }
        "city" | "district" | "locality" => address["city"] = json!(name),
        "house" | "street" => {
            if kind == "house" {
                address["houseNumber"] = native["housenumber"].clone();
            }
            address["road"] = json!(if kind == "street" {
                name
            } else {
                text(native, "street")
            });
            address["city"] = native["city"].clone();
            address["postcode"] = native["postcode"].clone();
        }
        _ => bail!("unsupported maps.co result type"),
    }
    let point = &feature["geometry"]["coordinates"];
    full_address(text(native, "label"), &point[1], &point[0], address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_native_levels_and_explicit_postcodes_without_promoting_context() {
        let mut input = json!({"features": [{"properties": {"geocoding": {
            "type": "house", "osm_key": "building", "osm_value": "yes", "name": "Example", "label": "12 Example Road",
            "housenumber": "12", "street": "Example Road", "city": "Parent City", "district": "Parent District",
            "county": "Parent County", "state": "Parent State", "postcode": "12345", "country_code": "US"
        }}, "geometry": {"type": "Point", "coordinates": [-122.0, 37.0]}}]});
        for (kind, level) in [
            ("house", "house"),
            ("street", "street"),
            ("city", "locality"),
            ("district", "locality"),
            ("locality", "locality"),
            ("county", "region"),
            ("state", "region"),
            ("postcode", "postcode"),
            ("country", "country"),
        ] {
            input["features"][0]["properties"]["geocoding"]["type"] = kind.into();
            let result = normalize(&input).unwrap();
            assert_eq!(crate::score::level_of(&result), level, "{kind}");
            assert_eq!(crate::score::coordinates(&result), Some([37., -122.]));
            assert!(result.get("goodMatch").is_none());
            if level == "locality" {
                assert_eq!(result["addressDetails"]["city"], "Example");
            }
        }
        input["features"][0]["properties"]["geocoding"]["type"] = "county".into();
        input["features"][0]["properties"]["geocoding"]["osm_key"] = "place".into();
        input["features"][0]["properties"]["geocoding"]["osm_value"] = "postcode".into();
        let result = normalize(&input).unwrap();
        assert_eq!(crate::score::level_of(&result), "postcode");
        assert_eq!(result["addressDetails"]["postcode"], "12345");
        input["features"][0]["properties"]["geocoding"]["osm_key"] = "building".into();
        input["features"][0]["properties"]["geocoding"]["type"] = "unknown".into();
        assert!(normalize(&input).is_err());
        input["features"][0]["properties"]["geocoding"]["type"] = "house".into();
        input["features"][0]["geometry"]["coordinates"] = json!([-122., 91.]);
        assert!(normalize(&input).is_err());
        input["features"][0]["geometry"]["coordinates"] = json!([-122., 37.]);
        input["features"][0]["properties"]["geocoding"]["country_code"] = Value::Null;
        assert!(normalize(&input).is_err());
        assert_eq!(normalize(&json!({"features": []})).unwrap(), json!({}));
        assert!(normalize(&json!({"error": "Invalid key"})).is_err());
    }
}
