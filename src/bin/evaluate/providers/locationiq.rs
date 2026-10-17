use super::{Provider, full_address};
use crate::score::text;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

pub const PROVIDER: Provider = Provider {
    name: "locationiq",
    label: "LocationIQ Search v1",
    credential_env: "LOCATIONIQ_ACCESS_TOKEN",
    auth_parameter: "key",
    query_error_statuses: &[422],
    not_found_status: Some(404),
    interval_ms: 1000,
    storage_terms: Some("https://locationiq.com/terms"),
    request,
    normalize,
};

fn request(query: &str) -> Value {
    json!({"endpoint": "https://us1.locationiq.com/v1/search", "q": query, "format": "json", "addressdetails": 1, "normalizeaddress": 1, "accept-language": "en", "limit": 1})
}

fn normalize(response: &Value) -> Result<Value> {
    if text(response, "error") == "Unable to geocode" {
        return Ok(json!({}));
    }
    let results = response
        .as_array()
        .context("missing LocationIQ results array")?;
    let Some(result) = results.first() else {
        return Ok(json!({}));
    };
    let native = &result["address"];
    let mut address = json!({"countryCode": text(native, "country_code").to_lowercase()});
    match text(result, "type") {
        "country" => (),
        "state" | "province" | "region" | "state_district" => {
            address["state"] = native["state"].clone()
        }
        "county" => address["county"] = native["county"].clone(),
        "postcode" | "postal_code" => address["postcode"] = native["postcode"].clone(),
        "city" | "town" | "village" | "hamlet" | "municipality" | "suburb" | "neighbourhood"
        | "locality" | "borough" | "district" | "isolated_dwelling" => {
            address["city"] = native["city"].clone()
        }
        _ if text(result, "class") == "boundary" => {
            let field = ["city", "county", "state"]
                .into_iter()
                .find(|key| !text(native, key).is_empty());
            if let Some(field) = field {
                address[field] = native[field].clone();
            }
        }
        _ if text(result, "class") == "place"
            && !matches!(text(result, "type"), "house" | "building") =>
        {
            bail!("unsupported LocationIQ place type")
        }
        _ => {
            if text(result, "class") != "highway" {
                address["houseNumber"] = native["house_number"].clone();
            }
            address["road"] = native["road"].clone();
            address["city"] = native["city"].clone();
            address["postcode"] = native["postcode"].clone();
        }
    }
    let lat: f64 = text(result, "lat")
        .parse()
        .context("invalid LocationIQ latitude")?;
    let lon: f64 = text(result, "lon")
        .parse()
        .context("invalid LocationIQ longitude")?;
    full_address(
        text(result, "display_name"),
        &json!(lat),
        &json!(lon),
        address,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_string_coordinates_and_does_not_promote_ancestors_or_http_errors() {
        let mut input = json!([{"class": "place", "type": "house", "lat": "37.0", "lon": "-122.0", "display_name": "12 Example Road", "address": {"house_number": "12", "road": "Example Road", "city": "Exampletown", "state": "California", "postcode": "12345", "country_code": "us"}}]);
        for (kind, level) in [
            ("house", "house"),
            ("city", "locality"),
            ("state", "region"),
            ("state_district", "region"),
            ("postcode", "postcode"),
            ("country", "country"),
        ] {
            input[0]["type"] = kind.into();
            let result = normalize(&input).unwrap();
            assert_eq!(crate::score::level_of(&result), level);
            assert_eq!(crate::score::coordinates(&result), Some([37., -122.]));
        }
        input[0]["type"] = "unknown_place".into();
        assert!(normalize(&input).is_err());
        input[0]["type"] = "residential".into();
        input[0]["class"] = "highway".into();
        assert_eq!(
            crate::score::level_of(&normalize(&input).unwrap()),
            "street"
        );
        input[0]["lat"] = "invalid".into();
        assert!(normalize(&input).is_err());
        assert_eq!(normalize(&json!([])).unwrap(), json!({}));
        assert_eq!(
            normalize(&json!({"error": "Unable to geocode"})).unwrap(),
            json!({})
        );
        assert!(normalize(&json!({"error": "Invalid key"})).is_err());
    }
}
