use super::{Provider, full_address};
use crate::score::text;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

pub const PROVIDER: Provider = Provider {
    name: "tomtom",
    label: "TomTom Search v2 Geocode",
    credential_env: "TOMTOM_API_KEY",
    query_error_statuses: &[422],
    not_found_status: None,
    interval_ms: 200,
    auth_parameter: "key",
    storage_terms: Some("https://developer.tomtom.com/terms-and-conditions"),
    request,
    normalize,
    validate_status: None,
    reverse: Some(REVERSE),
};

fn request(query: &str) -> Value {
    let mut endpoint = reqwest::Url::parse("https://api.tomtom.com/search/2/geocode/").unwrap();
    endpoint
        .path_segments_mut()
        .unwrap()
        .pop_if_empty()
        .push(&format!("{query}.json"));
    json!({"endpoint": endpoint.as_str(), "language": "en-US", "limit": 1})
}

fn normalize(response: &Value) -> Result<Value> {
    let results = response["results"]
        .as_array()
        .context("missing TomTom results array")?;
    let Some(result) = results.first() else {
        return Ok(json!({}));
    };
    let native = &result["address"];
    let country = text(native, "countryCode").to_lowercase();
    let mut address = json!({"countryCode": country});
    match text(result, "type") {
        "Point Address" | "Address Range" | "Street" | "Cross Street" => {
            if matches!(text(result, "type"), "Point Address" | "Address Range") {
                address["houseNumber"] = native["streetNumber"].clone();
            }
            address["road"] = native["streetName"].clone();
            address["city"] = native["municipality"].clone();
            address["postcode"] = native["postalCode"].clone();
        }
        "Geography" => match text(result, "entityType") {
            "Country" => (),
            "CountrySubdivision" => address["state"] = native["countrySubdivision"].clone(),
            "CountrySecondarySubdivision" => {
                address["county"] = native["countrySecondarySubdivision"].clone()
            }
            "CountryTertiarySubdivision" => {
                address["county"] = native["countryTertiarySubdivision"].clone()
            }
            "Municipality" | "MunicipalitySubdivision" | "Neighbourhood" | "Neighborhood" => {
                address["city"] = native["municipality"].clone();
                address["suburb"] = native["municipalitySubdivision"].clone();
            }
            "PostalCodeArea" => address["postcode"] = native["postalCode"].clone(),
            _ => bail!("unsupported TomTom geography type"),
        },
        _ => bail!("unsupported TomTom result type"),
    }
    let subdivision = text(native, "countrySubdivisionCode");
    if !subdivision.is_empty() {
        address["regionCodes"] =
            json!([{"code": format!("{}-{subdivision}", country.to_uppercase())}]);
    }
    full_address(
        text(native, "freeformAddress"),
        &result["position"]["lat"],
        &result["position"]["lon"],
        address,
    )
}

const REVERSE: super::reverse::Reverse = super::reverse::Reverse {
    request: |lat, lon| json!({"endpoint": format!("https://api.tomtom.com/search/2/reverseGeocode/{lat},{lon}.json"), "language": "en-US"}),
    normalize: reverse_normalize,
};

fn reverse_normalize(response: &Value) -> Result<Value> {
    let addresses = response["addresses"]
        .as_array()
        .context("missing TomTom addresses array")?;
    let Some(result) = addresses.first() else {
        return Ok(json!({}));
    };
    let native = &result["address"];
    let country = text(native, "countryCode").to_lowercase();
    let details = json!({
        "countryCode": country, "houseNumber": native["streetNumber"], "road": native["streetName"],
        "postcode": native["postalCode"],
    });
    let (lat, lon) = text(result, "position")
        .split_once(',')
        .context("missing TomTom position")?;
    let mut address = full_address(
        text(native, "freeformAddress"),
        &json!(lat.trim().parse::<f64>()?),
        &json!(lon.trim().parse::<f64>()?),
        details,
    )?;
    super::reverse::add_places(
        &mut address,
        native,
        &[
            ("municipality", "city"),
            ("municipalitySubdivision", "suburb"),
            ("countryTertiarySubdivision", "district"),
            ("countrySecondarySubdivision", "county"),
            ("countrySubdivisionName", "state"),
            ("countrySubdivision", "state"),
        ],
    );
    let subdivision = text(native, "countrySubdivisionCode");
    if !subdivision.is_empty() {
        super::reverse::add_codes(
            &mut address,
            [format!("{}-{subdivision}", country.to_uppercase()).as_str()],
        );
    }
    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_query_paths_and_preserves_result_granularity() {
        let req = request("12 A/B? #é");
        let endpoint = reqwest::Url::parse(req["endpoint"].as_str().unwrap()).unwrap();
        assert_eq!(endpoint.host_str(), Some("api.tomtom.com"));
        assert!(endpoint.query().is_none());
        assert!(endpoint.fragment().is_none());
        assert!(endpoint.path().contains("A%2FB%3F%20%23%C3%A9"));
        let mut response = json!({"results": [{"type": "Point Address", "address": {"streetNumber": "12", "streetName": "Example Road", "municipality": "Exampletown", "postalCode": "12345", "countrySubdivision": "CA", "countrySubdivisionCode": "CA", "countryCode": "US", "freeformAddress": "12 Example Road"}, "position": {"lat": 37., "lon": -122.}}]});
        for (kind, entity, level) in [
            ("Point Address", "", "house"),
            ("Street", "", "street"),
            ("Geography", "Municipality", "locality"),
            ("Geography", "CountrySubdivision", "region"),
            ("Geography", "Country", "country"),
            ("Geography", "PostalCodeArea", "postcode"),
        ] {
            response["results"][0]["type"] = kind.into();
            response["results"][0]["entityType"] = entity.into();
            let normalized = normalize(&response).unwrap();
            assert_eq!(crate::score::level_of(&normalized), level);
            assert!(crate::score::region_codes(&normalized).contains("US-CA"));
        }
        assert_eq!(normalize(&json!({"results": []})).unwrap(), json!({}));
        assert!(normalize(&json!({"error": "denied"})).is_err());
    }
}
