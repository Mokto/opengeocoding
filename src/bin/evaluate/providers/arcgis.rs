use super::{Provider, full_address};
use crate::score::text;
use anyhow::{Context, Result, bail, ensure};
use opengeocoding::matching::{ParsedAddress, expected_country};
use serde_json::{Value, json};

pub const PROVIDER: Provider = Provider {
    name: "arcgis",
    label: "Esri ArcGIS World Geocoding",
    credential_env: "ARCGIS_API_KEY",
    auth_parameter: "token",
    query_error_statuses: &[422],
    not_found_status: None,
    interval_ms: 200,
    storage_terms: Some(
        "https://developers.arcgis.com/rest/geocode/find-address-candidates/#forstorage",
    ),
    request,
    normalize,
    validate_status: None,
};

fn request(query: &str) -> Value {
    json!({"endpoint": "https://geocode-api.arcgis.com/arcgis/rest/services/World/GeocodeServer/findAddressCandidates", "SingleLine": query, "f": "json", "outFields": "*", "outSR": 4326, "langCode": "EN", "maxLocations": 1, "forStorage": true})
}

fn normalize(response: &Value) -> Result<Value> {
    ensure!(
        response.get("error").is_none(),
        "ArcGIS API error; check the token, stored-geocoding privilege and credits"
    );
    if let Some(sr) = response.get("spatialReference") {
        ensure!(
            sr["wkid"].as_u64() == Some(4326) || sr["latestWkid"].as_u64() == Some(4326),
            "ArcGIS returned an unexpected coordinate system"
        );
    }
    let candidates = response["candidates"]
        .as_array()
        .context("missing ArcGIS candidates array")?;
    let Some(candidate) = candidates.first() else {
        return Ok(json!({}));
    };
    let native = &candidate["attributes"];
    if text(native, "Status") == "U" {
        return Ok(json!({}));
    }
    let country = expected_country(&ParsedAddress::from_iter([(
        "country".into(),
        text(native, "Country").into(),
    )]));
    let mut address = json!({"countryCode": country});
    match text(native, "Addr_type") {
        "Subaddress" | "SubAddress" | "PointAddress" | "StreetAddress" | "StreetAddressExt"
        | "StreetName" | "StreetInt" | "POI" => {
            if matches!(
                text(native, "Addr_type"),
                "Subaddress" | "SubAddress" | "PointAddress" | "StreetAddress" | "POI"
            ) {
                address["houseNumber"] = native["AddNum"].clone();
            }
            address["road"] = ["StPreDir", "StPreType", "StName", "StType", "StDir"]
                .into_iter()
                .map(|key| text(native, key))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
                .into();
            address["city"] = native["City"].clone();
            address["postcode"] = native["Postal"].clone();
        }
        "Postal" | "PostalExt" | "PostalLoc" => address["postcode"] = native["Postal"].clone(),
        "Locality" => match text(native, "Type") {
            "Country" => (),
            "State or Province" => address["state"] = native["Region"].clone(),
            "Territory" => address["state"] = native["Territory"].clone(),
            "County" => address["county"] = native["Subregion"].clone(),
            "City" | "MetroArea" | "Neighborhood" | "District" | "Block" | "Sector" | "Zone" => {
                address["city"] = native
                    .get("City")
                    .filter(|v| v.as_str().is_some_and(|s| !s.is_empty()))
                    .unwrap_or(&candidate["address"])
                    .clone();
            }
            _ => bail!("unsupported ArcGIS locality type"),
        },
        _ => bail!("unsupported ArcGIS address type"),
    }
    full_address(
        text(candidate, "address"),
        &candidate["location"]["y"],
        &candidate["location"]["x"],
        address,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_stored_wgs84_requests_and_respects_native_fallback_types() {
        let req = request("Exampletown");
        assert_eq!(req["forStorage"], true);
        assert_eq!(req["outSR"], 4326);
        assert!(req.get("sourceCountry").is_none());
        let mut input = json!({"spatialReference": {"wkid": 4326}, "candidates": [{"address": "12 Example Road", "location": {"x": -122., "y": 37.}, "score": 100, "attributes": {"Country": "USA", "Addr_type": "PointAddress", "Type": "City", "AddNum": "12", "StName": "Example", "StType": "Road", "City": "Exampletown", "Region": "California", "Postal": "12345"}}]});
        for (kind, subtype, level) in [
            ("PointAddress", "", "house"),
            ("Subaddress", "", "house"),
            ("SubAddress", "", "house"),
            ("StreetAddressExt", "", "street"),
            ("StreetName", "", "street"),
            ("Locality", "City", "locality"),
            ("Locality", "State or Province", "region"),
            ("Locality", "Country", "country"),
            ("Postal", "", "postcode"),
        ] {
            input["candidates"][0]["attributes"]["Addr_type"] = kind.into();
            input["candidates"][0]["attributes"]["Type"] = subtype.into();
            let result = normalize(&input).unwrap();
            assert_eq!(crate::score::level_of(&result), level);
            assert_eq!(crate::score::coordinates(&result), Some([37., -122.]));
        }
        input["spatialReference"]["wkid"] = 3857.into();
        assert!(normalize(&input).is_err());
        assert_eq!(normalize(&json!({"candidates": []})).unwrap(), json!({}));
        assert!(normalize(&json!({"error": {"code": 498}, "candidates": []})).is_err());
    }
}
