use super::{Provider, full_address};
use crate::score::text;
use anyhow::{Context, Result, bail};
use opengeocoding::matching::{ParsedAddress, expected_country};
use serde_json::{Value, json};

pub const PROVIDER: Provider = Provider {
    name: "here",
    label: "HERE Geocoding and Search v7",
    credential_env: "HERE_API_KEY",
    query_error_statuses: &[422],
    not_found_status: None,
    interval_ms: 200,
    auth_parameter: "apiKey",
    storage_terms: Some("https://www.here.com/terms/here-platform-terms"),
    request,
    normalize,
};

fn request(query: &str) -> Value {
    json!({"endpoint": "https://geocode.search.hereapi.com/v1/geocode", "q": query, "lang": "en", "limit": 1})
}

fn normalize(response: &Value) -> Result<Value> {
    let items = response["items"]
        .as_array()
        .context("missing HERE items array")?;
    let Some(item) = items.first() else {
        return Ok(json!({}));
    };
    let native = &item["address"];
    let country = expected_country(&ParsedAddress::from_iter([(
        "country".into(),
        text(native, "countryCode").into(),
    )]));
    let mut address = json!({"countryCode": country});
    let postal = text(item, "resultType") == "postalCodePoint"
        || (text(item, "resultType") == "locality" && text(item, "localityType") == "postalCode");
    if postal {
        address["postcode"] = native["postalCode"].clone();
    } else {
        match text(item, "resultType") {
            "houseNumber" | "place" | "street" | "intersection" => {
                if matches!(text(item, "resultType"), "houseNumber" | "place") {
                    address["houseNumber"] = native["houseNumber"].clone();
                }
                address["road"] = native["street"].clone();
                address["city"] = native["city"].clone();
                address["postcode"] = native["postalCode"].clone();
            }
            "administrativeArea" => match text(item, "administrativeAreaType") {
                "country" => (),
                "state" => address["state"] = native["state"].clone(),
                "county" => address["county"] = native["county"].clone(),
                _ => bail!("unsupported HERE administrative area type"),
            },
            "locality" | "addressBlock" => {
                address["city"] = native["city"].clone();
                address["suburb"] = native["district"].clone();
            }
            _ => bail!("unsupported HERE result type"),
        }
    }
    full_address(
        text(item, "title"),
        &item["position"]["lat"],
        &item["position"]["lng"],
        address,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_iso3_and_keeps_postcodes_and_administrative_areas_coarse() {
        let mut response = json!({"items": [{"resultType": "houseNumber", "title": "12 Example Road", "address": {"countryCode": "USA", "houseNumber": "12", "street": "Example Road", "city": "Exampletown", "postalCode": "12345", "state": "California"}, "position": {"lat": 37., "lng": -122.}}]});
        for (kind, subtype, level) in [
            ("houseNumber", "", "house"),
            ("street", "", "street"),
            ("locality", "city", "locality"),
            ("locality", "postalCode", "postcode"),
            ("postalCodePoint", "", "postcode"),
            ("administrativeArea", "state", "region"),
            ("administrativeArea", "country", "country"),
        ] {
            response["items"][0]["resultType"] = kind.into();
            response["items"][0]["localityType"] = subtype.into();
            response["items"][0]["administrativeAreaType"] = subtype.into();
            let normalized = normalize(&response).unwrap();
            assert_eq!(crate::score::level_of(&normalized), level);
            assert_eq!(normalized["addressDetails"]["countryCode"], "us");
        }
        response["items"][0]["address"]["countryCode"] = "FRA".into();
        assert_eq!(
            normalize(&response).unwrap()["addressDetails"]["countryCode"],
            "fr"
        );
        response["items"][0]["address"]["countryCode"] = "ZZZ".into();
        assert!(normalize(&response).is_err());
        assert_eq!(normalize(&json!({"items": []})).unwrap(), json!({}));
        assert!(normalize(&json!({"error": "denied"})).is_err());
    }
}
