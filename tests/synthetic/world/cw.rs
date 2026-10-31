use super::*;
use crate::*;

/// Kaya Diabaas is filed under Jan Kok; Tera Kora, a village 2.4 km away with houses of its own,
/// is what people write for it. Westpunt lies 14 km off. Kaya Irena M. Schoop is filed under
/// Soto, 25 km from Willemstad, whose name Curaçao's addresses use for the whole island.
pub const PLACES: &[Place] = &[
    place!(
        "cw",
        "country",
        "",
        "CW",
        "",
        150_000,
        (-68.95, 12.15),
        "",
        "Curaçao",
        &[("en", "Curaçao")]
    ),
    place!(
        "cw-jan-kok",
        "locality",
        "village",
        "CW",
        "",
        1_000,
        (-69.1, 12.25),
        "cw",
        "Jan Kok",
        &[("en", "Jan Kok")]
    ),
    place!(
        "cw-tera-kora",
        "locality",
        "village",
        "CW",
        "",
        1_000,
        (-69.078, 12.26),
        "cw",
        "Tera Kora",
        &[("en", "Tera Kora")]
    ),
    place!(
        "cw-westpunt",
        "locality",
        "village",
        "CW",
        "",
        1_000,
        (-69.15, 12.37),
        "cw",
        "Westpunt",
        &[("en", "Westpunt")]
    ),
    place!(
        "cw-willemstad",
        "locality",
        "town",
        "CW",
        "",
        140_000,
        (-68.935, 12.107),
        "cw",
        "Willemstad",
        &[("en", "Willemstad")]
    ),
    place!(
        "cw-sint-willibrordus",
        "locality",
        "village",
        "CW",
        "",
        600,
        (-69.065, 12.217),
        "cw",
        "Sint Willibrordus",
        &[("en", "Sint Willibrordus")]
    ),
    place!(
        "cw-soto",
        "locality",
        "village",
        "CW",
        "",
        2_233,
        (-69.11, 12.279),
        "cw",
        "Soto",
        &[("en", "Soto")]
    ),
];

const fn curacaoan(
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
        cc: "CW",
        lon,
        lat,
        numbers,
    }
}

/// Papaya bij Westpunt ("near Westpunt") runs 1.6 km from Weg naar Westpunt, which has a house
/// 16 too.
pub const STREETS: &[Street] = &[
    curacaoan("Kaya Diabaas", "Jan Kok", (-69.095, 12.255), &["575"]),
    curacaoan("Kaya Granit", "Tera Kora", (-69.078, 12.261), &["1"]),
    curacaoan("Kaya Irena M. Schoop", "Soto", (-69.0743, 12.2876), &["1"]),
    curacaoan(
        "Papaya bij Westpunt",
        "Westpunt",
        (-69.153, 12.364),
        &["16"],
    ),
    curacaoan("Weg naar Westpunt", "Westpunt", (-69.143, 12.374), &["16"]),
    curacaoan(
        "Kaminda San Sebastian",
        "Sint Willibrordus",
        (-69.061, 12.223),
        &["A20"],
    ),
    curacaoan("Weg naar Santa Cruz", "Soto", (-69.118, 12.283), &["2"]),
    curacaoan("Hoffi Abou", "Soto", (-69.101, 12.295), &["9"]),
];

/// libpostal reads a number before the street as a venue (`house`) with it, a number after it
/// into the road, "bij" (near) as a `near` field, and a street as a suburb.
pub fn cases() -> Vec<Case> {
    vec![
        case(
            "16 Papaya bij Westpunt, Westpunt, Curacao",
            at_house("Papaya bij Westpunt", "16"),
        ),
        case(
            "Papaya bij Westpunt 16, Westpunt, Curacao",
            at_house("Papaya bij Westpunt", "16"),
        ),
        case(
            "Kaminda San Sebastian A20, Sint Willibrordus, Curacao",
            at_house("Kaminda San Sebastian", "A20"),
        ),
        case(
            "2 Weg naar Santa Cruz, Soto, Curacao",
            at_house("Weg naar Santa Cruz", "2"),
        ),
        case("9 Hoffi Abou, Soto, Curacao", at_house("Hoffi Abou", "9")),
    ]
}

#[test]
fn a_village_next_to_the_one_a_house_is_filed_under_still_suggests_it() {
    let at = |text: &str| index().suggest(text, 5, None, None, false);
    let want = house(street("Kaya Diabaas"), "575");
    for typed in [
        "575 Kaya Diabaas, Tera Kora, Curacao",
        "Kaya Diabaas 575, Tera Kora, Curaçao",
    ] {
        let found = at(typed);
        assert!(!found.is_empty(), "{typed}");
        assert_eq!(found[0].kind, "address", "{typed}");
        assert!(metres((found[0].x, found[0].y), want) < 5.0, "{typed}");
    }
    // not another number, and not a village too far to be the house's
    for typed in [
        "576 Kaya Diabaas, Tera Kora, Curacao",
        "575 Kaya Diabaas, Westpunt, Curacao",
    ] {
        assert!(at(typed).iter().all(|s| s.kind != "address"), "{typed}");
    }
}

#[test]
fn the_town_curacao_names_its_whole_island_after_still_suggests_a_house_far_from_it() {
    let at = |text: &str| index().suggest(text, 5, None, None, false);
    let want = house(street("Kaya Irena M. Schoop"), "1");
    for typed in [
        "1 Kaya Irena M. Schoop, Willemstad, Curacao",
        "Kaya Irena M. Schoop 1, Willemstad, Curaçao",
    ] {
        let found = at(typed);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{typed}");
        assert!(metres((found[0].x, found[0].y), want) < 5.0, "{typed}");
    }
    // not another village of the island, nor another number
    for typed in [
        "1 Kaya Irena M. Schoop, Westpunt, Curacao",
        "2 Kaya Irena M. Schoop, Willemstad, Curacao",
    ] {
        assert!(at(typed).iter().all(|s| s.kind != "address"), "{typed}");
    }
}
