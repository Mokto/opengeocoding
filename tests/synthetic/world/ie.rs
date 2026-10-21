use super::*;
use crate::*;

pub const PLACES: &[Place] = &[
    // ISO 3166-2:IE puts County Dublin in the province of Leinster, IE-L.
    place!(
        "ie",
        "country",
        "",
        "IE",
        "",
        5_000_000,
        (-8.0, 53.0),
        "",
        "Ireland",
        &[("en", "Ireland")],
    ),
    place!(
        "ie-d",
        "region",
        "",
        "IE",
        "IE-D",
        1_400_000,
        (-6.3, 53.4),
        "ie",
        "County Dublin",
        &[("en", "County Dublin")],
    ),
    place!(
        "ie-dublin",
        "locality",
        "city",
        "IE",
        "IE-D",
        590_000,
        (-6.2603, 53.3498),
        "ie-d",
        "Dublin",
        &[("en", "Dublin"), ("ga", "Baile Átha Cliath")],
    ),
];

#[test]
fn an_irish_county_names_its_province_which_is_listed_and_found_by_its_code() {
    let dublin = place("ie-dublin");
    let address = &index().reverse(dublin.lat, dublin.lon)["address"];
    assert_eq!(address["ISO3166-2-lvl4"], "IE-D");
    assert_eq!(address["ISO3166-2-lvl5"], "IE-L");
    let mut listed: Vec<_> = index()
        .region_places()
        .map(|(code, _)| code)
        .filter(|code| code.starts_with("IE-"))
        .collect();
    listed.sort();
    // no county of the other provinces is in this world
    assert_eq!(listed, ["IE-D", "IE-L"]);
    let found = index().search("ie, IE-L");
    let province = &index().render(&found[0], true)["address"];
    assert_eq!(province["state"], "Leinster");
    assert_eq!(province["ISO3166-2-lvl5"], "IE-L");
    assert!(province.get("ISO3166-2-lvl4").is_none());
    let (_, place) = index().region_places().find(|(c, _)| *c == "IE-L").unwrap();
    assert_eq!(index().details(place.into())["admin_level"], 5);
}
