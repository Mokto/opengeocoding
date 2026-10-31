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

const fn aruban(
    name: &'static str,
    city: &'static str,
    (lon, lat): (f64, f64),
    numbers: &'static [&'static str],
) -> Street {
    Street {
        class: "residential",
        length: 0.0,
        name,
        postcode: "",
        city,
        cc: "AW",
        lon,
        lat,
        numbers,
    }
}

/// Noord and Santa Cruz are streets too, with a house of the number Palm Beach and Papillon have.
pub const STREETS: &[Street] = &[
    aruban("Mahuma", "Oranjestad", (-69.9956, 12.4995), &["36B"]),
    aruban("Primavera", "Oranjestad", (-70.012, 12.513), &["211H"]),
    aruban("Macuarima", "Santa Cruz", (-69.976, 12.503), &["119B"]),
    aruban("Palm Beach", "Noord", (-70.04, 12.566), &["21"]),
    aruban("Papillon", "Santa Cruz", (-69.984, 12.506), &["64"]),
    aruban("Noord", "Noord", (-70.036, 12.564), &["21"]),
    aruban("Santa Cruz", "Santa Cruz", (-69.981, 12.507), &["64"]),
    aruban(
        "St. Theresiaplein",
        "San Nicolaas",
        (-69.91, 12.436),
        &["8"],
    ),
    // OpenStreetMap writes 00000 as Aruba's postcode
    Street {
        postcode: "00000",
        ..aruban(
            "Caya G.F. Betico Croes",
            "Oranjestad",
            (-70.02, 12.521),
            &["211-T lok 6"],
        )
    },
];

/// libpostal reads a street of one or two words beside its number as a venue (`house`), a
/// suburb or one road with the number in it.
pub fn cases() -> Vec<Case> {
    vec![
        case(
            "211H Primavera, Oranjestad, Aruba",
            at_house("Primavera", "211H"),
        ),
        case(
            "119B Macuarima, Santa Cruz, Aruba",
            at_house("Macuarima", "119B"),
        ),
        case("Palm Beach 21, Noord, Aruba", at_house("Palm Beach", "21")),
        case("Papillon 64, Santa Cruz, Aruba", at_house("Papillon", "64")),
        case(
            "8 St. Theresiaplein, San Nicolaas, Aruba",
            at_house("St. Theresiaplein", "8"),
        ),
        // OpenStreetMap numbers a shop with its unit
        case(
            "211-T lok 6 Caya G.F. Betico Croes, 00000, Aruba",
            at_house("Caya G.F. Betico Croes", "211-T lok 6"),
        ),
        case(
            "Caya G.F. Betico Croes 211-T lok 6, 00000, Aruba",
            at_house("Caya G.F. Betico Croes", "211-T lok 6"),
        ),
    ]
}

#[test]
fn a_shop_numbered_with_its_unit_is_suggested_before_or_after_the_road() {
    let want = house(street("Caya G.F. Betico Croes"), "211-T lok 6");
    for typed in [
        "211-T lok 6 Caya G.F. Betico Croes, 00000, Aruba",
        "Caya G.F. Betico Croes 211-T lok 6, 00000, Aruba",
    ] {
        let found = index().suggest(typed, 5, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{typed}");
        assert!(metres((found[0].x, found[0].y), want) < 5.0, "{typed}");
    }
}

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
