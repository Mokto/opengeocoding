use super::*;
use crate::{Case, at_house, case};

pub const PLACES: &[Place] = &[place!(
    "uy",
    "country",
    "",
    "UY",
    "",
    3_400_000,
    (-56.0, -32.5),
    "",
    "Uruguay",
    &[("es", "Uruguay"), ("en", "Uruguay")]
)];

/// Uruguay's register names a national road with its dedication in every 0.1° cell it crosses, each
/// cell with its own house 13; OSM's houses on it say only "Ruta 31".
const DEDICATED: Street = Street {
    class: "primary",
    length: 0.0,
    name: "Ruta 31 - Coronel Gorgonio Aguiar",
    postcode: "50000",
    city: "Salto",
    cc: "UY",
    lon: -57.95,
    lat: -31.35,
    numbers: &["12", "13", "14"],
};

/// More cells of the dedicated name than a street lookup's first candidates, then the short name.
const fn route() -> [Street; 23] {
    let mut streets = [DEDICATED; 23];
    let mut i = 0;
    while i < 22 {
        streets[i].lon += 0.1 * i as f64;
        i += 1;
    }
    streets[22] = Street {
        name: "Ruta 31",
        lon: -57.395,
        lat: -31.142,
        numbers: &["13"],
        ..DEDICATED
    };
    streets
}

pub const STREETS: &[Street] = &route();

pub fn cases() -> Vec<Case> {
    vec![
        // the street named as written, though the dedicated name holds the number nearer the
        // postcode's centre
        case("Ruta 31 13, 50000, Uruguay", at_house("Ruta 31", "13")),
        // libpostal reads the road's number as a second house number
        case("13 Ruta 31, 50000, Uruguay", at_house("Ruta 31", "13")),
    ]
}

/// The road is in no town: its houses are still in Uruguay, and each cell's house 13 is another
/// house, though all read alike.
#[test]
fn every_copy_of_a_rural_house_is_suggested_in_its_country() {
    let found = crate::index().suggest(
        "Ruta 31 - Coronel Gorgonio Aguiar 13, 50000, Uruguay",
        5,
        None,
        None,
        false,
    );
    let houses: Vec<_> = found.iter().filter(|s| s.kind == "address").collect();
    let names: Vec<_> = found
        .iter()
        .map(|s| (&s.display_name, &s.country_code))
        .collect();
    assert!(houses.len() > 1, "{names:?}");
    assert!(houses.iter().all(|s| s.country_code == "uy"), "{names:?}");
}
