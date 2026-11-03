use super::*;
use crate::*;

/// Hamilton, Bermuda's capital, is a town of about a thousand people with a namesake in Ontario
/// six hundred times its size. The index holds no Bermudian postcode ("HM 11"), as Overture and
/// OpenStreetMap mostly do not.
pub const PLACES: &[Place] = &[
    place!(
        "bm",
        "country",
        "",
        "BM",
        "",
        64_000,
        (-64.75, 32.3),
        "",
        "Bermuda",
        &[("en", "Bermuda")],
    ),
    place!(
        "bm-hamilton",
        "locality",
        "town",
        "BM",
        "",
        1_000,
        (-64.7814, 32.2949),
        "bm",
        "Hamilton",
        &[("en", "Hamilton")],
    ),
];

pub const STREETS: &[Street] = &[Street {
    class: "residential",
    length: 0.0,
    name: "Front Street",
    postcode: "",
    city: "Hamilton",
    cc: "BM",
    lon: -64.783,
    lat: 32.2925,
    numbers: &["25"],
}];

/// A registered office as company registers write it: building, floor, street, the town with a
/// postcode the index lacks, the country. It suggests its house as the house is typed, and else
/// the town, never the bigger Hamilton abroad.
#[test]
fn a_registered_address_suggests_its_house_past_an_unknown_postcode_and_else_its_town() {
    let at = |text: &str, cc: Option<&str>| index().suggest(text, 5, cc, None, false);
    let front = house(street("Front Street"), "25");
    for typed in [
        "25 Front Street, Hamilton, Bermuda",
        "25 Front Street, Hamilton HM 11, Bermuda",
    ] {
        let found = at(typed, None);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{typed}");
        assert!(metres((found[0].x, found[0].y), front) < 5.0, "{typed}");
    }
    let hamilton = place("bm-hamilton");
    for (typed, cc) in [
        (
            "Cedar House, 3rd Floor, 41 Cedar Avenue, Hamilton HM 12, Bermuda",
            None,
        ),
        (
            "Cedar House, 3rd Floor, 41 Cedar Avenue, Hamilton, Bermuda",
            None,
        ),
        ("Cedar House, P.O. Box 624, Hamilton, Bermuda", Some("bm")),
    ] {
        let found = at(typed, cc);
        assert!(!found.is_empty(), "{typed}");
        assert!(
            found
                .iter()
                .all(|s| s.country_code == "bm" && s.kind == "town"),
            "{typed}"
        );
        assert!(
            metres((found[0].x, found[0].y), (hamilton.lon, hamilton.lat)) < 5.0,
            "{typed}"
        );
    }
    // a region or country filter the town is not in leaves nothing to suggest
    for (typed, cc) in [
        (
            "Cedar House, 41 Cedar Avenue, Hamilton, Ontario, Bermuda",
            None,
        ),
        (
            "Cedar House, 41 Cedar Avenue, Hamilton HM 12, Bermuda",
            Some("ca"),
        ),
    ] {
        assert!(at(typed, cc).is_empty(), "{typed}");
    }
    // the country stated is the one whose Hamilton it is
    let found = at("Cedar House, 41 Cedar Avenue, Hamilton, Canada", None);
    assert_eq!(found.first().map(|s| s.country_code.as_str()), Some("ca"));
    // typed alone, the bigger town still comes first, and the street is still a street
    let found = at("Hamilton", None);
    let first: Vec<_> = found
        .iter()
        .take(2)
        .map(|s| s.country_code.as_str())
        .collect();
    assert_eq!(first, ["ca", "bm"]);
    let found = at("Front Street, Hamilton, Bermuda", None);
    assert_eq!(found.first().map(|s| s.kind), Some("street"));
}
