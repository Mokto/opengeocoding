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
    // Brazil's many towns and districts named Santa Cruz, all bigger than Aruba's
    place!(
        "br-santa-cruz-1",
        "locality",
        "town",
        "BR",
        "BR-PA",
        38_000,
        (-51.0, -3.0),
        "br-pa",
        "Santa Cruz",
        &[("pt", "Santa Cruz")]
    ),
    place!(
        "br-santa-cruz-2",
        "locality",
        "town",
        "BR",
        "BR-PA",
        29_000,
        (-51.1, -3.0),
        "br-pa",
        "Santa Cruz",
        &[("pt", "Santa Cruz")]
    ),
    place!(
        "br-santa-cruz-3",
        "locality",
        "town",
        "BR",
        "BR-PA",
        21_000,
        (-51.2, -3.0),
        "br-pa",
        "Santa Cruz",
        &[("pt", "Santa Cruz")]
    ),
    place!(
        "br-santa-cruz-do-sul",
        "locality",
        "town",
        "BR",
        "BR-PA",
        130_000,
        (-51.3, -3.0),
        "br-pa",
        "Santa Cruz do Sul",
        &[("pt", "Santa Cruz do Sul")]
    ),
    place!(
        "br-santa-cruz-do-capibaribe",
        "locality",
        "town",
        "BR",
        "BR-PA",
        110_000,
        (-51.4, -3.0),
        "br-pa",
        "Santa Cruz do Capibaribe",
        &[("pt", "Santa Cruz do Capibaribe")]
    ),
    place!(
        "br-santa-cruz-4",
        "locality",
        "town",
        "BR",
        "BR-PA",
        47_000,
        (-51.5, -3.0),
        "br-pa",
        "Santa Cruz",
        &[("pt", "Santa Cruz")]
    ),
    place!(
        "br-santa-cruz-5",
        "locality",
        "town",
        "BR",
        "BR-PA",
        30_000,
        (-51.6, -3.0),
        "br-pa",
        "Santa Cruz",
        &[("pt", "Santa Cruz")]
    ),
    place!(
        "br-santa-cruz-6",
        "locality",
        "town",
        "BR",
        "BR-PA",
        27_000,
        (-51.7, -3.0),
        "br-pa",
        "Santa Cruz",
        &[("pt", "Santa Cruz")]
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
