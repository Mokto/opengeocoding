use super::*;
use crate::*;

/// Mahuma is filed under Oranjestad, 4.4 km off; Santa Cruz, 2.1 km off, is what people write for
/// it, and Brazil has eight bigger towns of that name. Noord lies 8 km off and San Nicolaas 12 km:
/// both have houses of their own, and no town names the whole island.
pub const PLACES: &[Place] = &[
    place!(
        "aw",
        "country",
        "",
        "AW",
        "",
        107_000,
        (-69.97, 12.52),
        "",
        "Aruba",
        &[("en", "Aruba")]
    ),
    place!(
        "aw-oranjestad",
        "locality",
        "town",
        "AW",
        "",
        35_000,
        (-70.037, 12.52),
        "aw",
        "Oranjestad",
        &[("en", "Oranjestad")]
    ),
    place!(
        "aw-san-nicolaas",
        "locality",
        "city",
        "AW",
        "",
        0,
        (-69.888, 12.4295),
        "aw",
        "San Nicolaas",
        &[("en", "San Nicolaas")]
    ),
    place!(
        "aw-santa-cruz",
        "locality",
        "town",
        "AW",
        "",
        15_236,
        (-69.98, 12.511),
        "aw",
        "Santa Cruz",
        &[("en", "Santa Cruz")]
    ),
    place!(
        "aw-noord",
        "neighborhood",
        "",
        "AW",
        "",
        0,
        (-70.032, 12.5645),
        "aw",
        "Noord",
        &[("en", "Noord")]
    ),
];

pub const STREETS: &[Street] = &[Street {
    class: "residential",
    length: 0.0,
    name: "Mahuma",
    postcode: "",
    city: "Oranjestad",
    cc: "AW",
    lon: -69.9956,
    lat: 12.4995,
    numbers: &["36B"],
}];

#[test]
fn a_town_near_the_one_a_house_is_filed_under_still_suggests_it_past_namesakes_abroad() {
    let at = |text: &str| index().suggest(text, 5, None, None, false);
    let want = house(street("Mahuma"), "36B");
    for typed in [
        "36B Mahuma, Santa Cruz, Aruba",
        "Mahuma 36B, Santa Cruz, Aruba",
    ] {
        let found = at(typed);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{typed}");
        assert!(metres((found[0].x, found[0].y), want) < 5.0, "{typed}");
    }
    // like Tai Wai for a Kwai Chung house: a place too far to be the house's, the biggest one
    // included
    for typed in [
        "36B Mahuma, Noord, Aruba",
        "36B Mahuma, San Nicolaas, Aruba",
    ] {
        assert!(at(typed).iter().all(|s| s.kind != "address"), "{typed}");
    }
}
