use super::*;
use crate::*;

/// Kaya Diabaas is filed under Jan Kok; Tera Kora, a village 2.4 km away with houses of its own,
/// is what people write for it. Westpunt lies 14 km off.
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
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Kaya Diabaas",
        postcode: "",
        city: "Jan Kok",
        cc: "CW",
        lon: -69.095,
        lat: 12.255,
        numbers: &["575"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Kaya Granit",
        postcode: "",
        city: "Tera Kora",
        cc: "CW",
        lon: -69.078,
        lat: 12.261,
        numbers: &["1"],
    },
];

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
