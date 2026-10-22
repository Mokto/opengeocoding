use super::*;
use crate::{index, metres, street};

pub const PLACES: &[Place] = &[
    place!(
        "br",
        "country",
        "",
        "BR",
        "",
        200_000_000,
        (-50.0, -10.0),
        "",
        "Brazil",
        &[("en", "Brazil"), ("pt", "Brasil")]
    ),
    place!(
        "br-pa",
        "region",
        "",
        "BR",
        "BR-PA",
        8_000_000,
        (-52.0, -4.0),
        "br",
        "Grão-Norte",
        &[("pt", "Grão-Norte")]
    ),
    place!(
        "br-pedralva",
        "locality",
        "town",
        "BR",
        "BR-PA",
        50_000,
        (-49.3, -5.1),
        "br-pa",
        "Pedralva",
        &[("pt", "Pedralva")]
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Rua Alagoas",
        postcode: "68580-000",
        city: "Pedralva",
        cc: "BR",
        lon: -49.301,
        lat: -5.1,
        numbers: &["127"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Avenida 2204",
        postcode: "68580-000",
        city: "Pedralva",
        cc: "BR",
        lon: -49.302,
        lat: -5.102,
        numbers: &["1370"],
    },
];

#[test]
fn brazilian_houses_are_suggested_past_the_bairro_and_the_state_and_after_a_numbered_road() {
    for (typed, road, number) in [
        (
            "RUA ALAGOAS, 127, MUTIRAO, Pedralva - PA, 68580-000, Brazil",
            "Rua Alagoas",
            "127",
        ),
        (
            "Rua Alagoas 127, Pedralva - PA, 68580-000, Brazil",
            "Rua Alagoas",
            "127",
        ),
        (
            "AVENIDA 2204, 1370, Pedralva - PA, Brazil",
            "Avenida 2204",
            "1370",
        ),
    ] {
        let found = index().suggest(typed, 5, None, None, false);
        assert!(!found.is_empty(), "{typed}");
        assert_eq!(found[0].kind, "address", "{typed}");
        assert!(
            metres((found[0].x, found[0].y), house(street(road), number)) < 5.0,
            "{typed}"
        );
    }
}
