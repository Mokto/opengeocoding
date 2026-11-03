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
    place!(
        "ie-swords",
        "locality",
        "town",
        "IE",
        "IE-D",
        40_000,
        (-8.62, 52.66),
        "ie-d",
        "Swords",
        &[("en", "Swords")],
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Mallow Street",
        postcode: "D02 X285",
        city: "Dublin",
        cc: "IE",
        lon: -6.259,
        lat: 53.3498,
        numbers: &["18"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Mallow Street",
        postcode: "K67 Y2E3",
        city: "Swords",
        cc: "IE",
        lon: -8.619,
        lat: 52.66,
        numbers: &["18"],
    },
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

/// Eircodes fused with their town ("Dublin D02 Y123") garble libpostal's parse. One the data lacks
/// is dropped unless the house's own contradicts its routing key; one the data holds stays.
pub fn cases() -> Vec<Case> {
    let dublin = house(&STREETS[0], "18");
    vec![
        case(
            "18 Mallow Street, Dublin D02 Y123, Ireland",
            Want::At(dublin, 25.0, false),
        ),
        // the house is not taken, the stated town still is
        case(
            "18 Mallow Street, Dublin D04 Y123, Ireland",
            town("ie-dublin", false),
        ),
        // Dublin's Eircode, not the Mallow Street of Swords
        case(
            "18 Mallow Street, Swords D02 X285, Ireland",
            Want::At(dublin, 25.0, false),
        ),
        // an Eircode of its own part, whose routing key libpostal runs into the road
        case(
            "18 Mallow Street, Swords, K67 Y2E3, Ireland",
            Want::At(house(&STREETS[1], "18"), 25.0, true),
        ),
        Case {
            query: "18 Mallow Street, Swords K67 Y2E3, Ireland",
            want: Want::At(house(&STREETS[1], "18"), 25.0, true),
            known_failure: "parser: a known Eircode fused with its town garbles the parse, and only an unknown one is dropped",
        },
    ]
}

/// An Eircode the data lacks, fused with its town, still lets the house be suggested; where no
/// line names a street the data holds, the town is, whether its Eircode is known or not.
#[test]
fn a_registered_address_suggests_its_house_or_town_past_an_eircode() {
    let at = |text: &str| index().suggest(text, 5, None, None, false);
    let found = at("18 Mallow Street, Dublin D02 Y123, Ireland");
    assert_eq!(found.first().map(|s| s.kind), Some("address"));
    assert!(metres((found[0].x, found[0].y), house(&STREETS[0], "18")) < 5.0);
    let dublin = place("ie-dublin");
    for typed in [
        "Office Suite 27, The Osprey Business Centre, Dublin D02 Y123, Ireland",
        "Unit 1A, Cleaboy Business Park, Dublin D02 X285, Ireland",
    ] {
        let found = at(typed);
        assert_eq!(found.first().map(|s| s.kind), Some("city"), "{typed}");
        assert!(
            metres((found[0].x, found[0].y), (dublin.lon, dublin.lat)) < 5.0,
            "{typed}"
        );
    }
    // a town an earlier part names rules out a namesake far from it: Swords lies 170 km from
    // this world's Dublin, so "Dublin, Swords" names no place
    assert!(at("Unit 4, Dublin, Swords, Ireland").is_empty());
    // and so does an Eircode the data holds, Dublin's here
    assert!(at("Unit 4, Business Park, Swords D02 X285, Ireland").is_empty());
    let swords = at("Unit 4, Swords, Ireland");
    assert_eq!(swords.first().map(|s| s.kind), Some("town"));
}
