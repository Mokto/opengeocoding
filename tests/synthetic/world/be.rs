use super::*;
use crate::{index, metres, street};

pub const PLACES: &[Place] = &[
    place!(
        "be",
        "country",
        "",
        "BE",
        "",
        11_000_000,
        (4.5, 50.6),
        "",
        "Belgium",
        &[("en", "Belgium")]
    ),
    place!(
        "be-ukkelaar",
        "locality",
        "town",
        "BE",
        "",
        80_000,
        (4.33, 50.78),
        "be",
        "Ukkelaar",
        &[("nl", "Ukkelaar")]
    ),
];

pub const STREETS: &[Street] = &[Street {
    class: "residential",
    length: 0.0,
    name: "Linkebeekstraat",
    postcode: "1180",
    city: "Ukkelaar",
    cc: "BE",
    lon: 4.331,
    lat: 50.78,
    numbers: &["33", "34"],
}];

#[test]
fn a_belgian_flats_box_is_left_out_of_its_house() {
    let want = house(street("Linkebeekstraat"), "34");
    for typed in [
        "Linkebeekstraat 34 bus 1, 1180 Ukkelaar, Belgium",
        "Linkebeekstraat 34 bus 0M/H, 1180 Ukkelaar, Belgium",
        "Linkebeekstraat 34 boîte 0022, 1180 Ukkelaar, Belgium",
    ] {
        let found = index().suggest(typed, 5, None, None, false);
        assert!(!found.is_empty(), "{typed}");
        assert_eq!(found[0].kind, "address", "{typed}");
        assert!(metres((found[0].x, found[0].y), want) < 5.0, "{typed}");
    }
}
