use super::*;
use crate::*;

/// Ouagadougou numbers its streets by sector and order ("Rue 9.78"), so a house written after
/// its street ("9.78 129") can also read as house 9.78 of a street whose name ends in 129.
pub const PLACES: &[Place] = &[
    place!(
        "bf",
        "country",
        "",
        "BF",
        "",
        22_000_000,
        (-1.6, 12.3),
        "",
        "Burkina Faso",
        &[("en", "Burkina Faso")]
    ),
    place!(
        "bf-ouagadougou",
        "locality",
        "city",
        "BF",
        "",
        2_400_000,
        (-1.53, 12.37),
        "bf",
        "Ouagadougou",
        &[("fr", "Ouagadougou")]
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Rue 9.78",
        postcode: "",
        city: "Ouagadougou",
        cc: "BF",
        lon: -1.5467,
        lat: 12.3638,
        numbers: &["129"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Rue 27.129",
        postcode: "",
        city: "Ouagadougou",
        cc: "BF",
        lon: -1.4657,
        lat: 12.4067,
        numbers: &["5"],
    },
];

#[test]
fn a_number_that_finds_no_house_gives_way_to_the_reading_that_does() {
    for (typed, road, number) in [
        ("9.78 129, Ouagadougou, Burkina Faso", "Rue 9.78", "129"),
        ("27.129 5, Ouagadougou, Burkina Faso", "Rue 27.129", "5"),
    ] {
        let found = index().suggest(typed, 5, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{typed}");
        assert!(
            metres((found[0].x, found[0].y), house(street(road), number)) < 2.,
            "{typed}"
        );
    }
    // no house of either reading: the streets, not a made-up house
    let found = index().suggest("9.78 130, Ouagadougou, Burkina Faso", 5, None, None, false);
    assert!(!found.iter().any(|s| s.kind == "address"));
}
