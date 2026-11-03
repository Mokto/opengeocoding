use super::*;
use crate::*;

pub const PLACES: &[Place] = &[
    place!(
        "il",
        "country",
        "",
        "IL",
        "",
        1_000_000,
        (35.2, 31.8),
        "",
        "Israel",
        &[("en", "Israel")]
    ),
    place!(
        "il-bnei-brak",
        "locality",
        "town",
        "IL",
        "",
        200_000,
        (34.8338, 32.0807),
        "il",
        "Bnei Brak",
        &[("en", "Bnei Brak"), ("he", "בני ברק")]
    ),
];

/// Israel's seven-digit postcodes are not in the data, and a house number may stand in a part of
/// its own before a street the data lacks: the town is still suggested.
#[test]
fn a_house_number_part_and_an_unknown_postcode_still_suggest_the_town() {
    let town = place("il-bnei-brak");
    for typed in [
        "3, Masada, Bnei Brak 5126404, Israel",
        "30, ששת הימים, בני ברק 5120261, Israel",
    ] {
        let found = index().suggest(typed, 5, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("town"), "{typed}");
        assert!(
            metres((found[0].x, found[0].y), (town.lon, town.lat)) < 5.0,
            "{typed}"
        );
    }
}
