use super::*;
use crate::{Case, Want, at_house, case, index, metres, named};

/// Rocha's farms lie far from any settlement: no locality within 5 km, no county. Camino del Indio
/// 40 is Overture's, which files it under Uruguay; Camino Los Ceibos 17 is OSM's, which names no
/// country. The region's area is in `AREAS`.
pub const PLACES: &[Place] = &[
    place!(
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
    ),
    place!(
        "uy-ro",
        "region",
        "",
        "UY",
        "UY-RO",
        70_000,
        (-53.9, -33.9),
        "uy",
        "Rocha",
        &[("en", "Rocha")],
    ),
];

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
const fn route() -> [Street; 24] {
    let mut streets = [DEDICATED; 24];
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
    streets[23] = INDIO;
    streets
}

const INDIO: Street = Street {
    class: "residential",
    length: 0.0,
    name: "Camino del Indio",
    postcode: "27000",
    city: "",
    cc: "UY",
    lon: -53.85,
    lat: -33.85,
    numbers: &["40"],
};

const CEIBOS: (f64, f64) = (-53.95, -33.95);

pub const FEATURES: &[Feature] = &[Feature {
    shape: Shape::Node,
    lon: CEIBOS.0,
    lat: CEIBOS.1,
    tags: &[
        ("addr:street", "Camino Los Ceibos"),
        ("addr:housenumber", "17"),
        ("addr:postcode", "27000"),
    ],
}];

pub const STREETS: &[Street] = &route();

pub fn cases() -> Vec<Case> {
    vec![
        // the street named as written, though the dedicated name holds the number nearer the
        // postcode's centre
        case("Ruta 31 13, 50000, Uruguay", at_house("Ruta 31", "13")),
        // libpostal reads the road's number as a second house number
        case("13 Ruta 31, 50000, Uruguay", at_house("Ruta 31", "13")),
        case(
            "17 Camino Los Ceibos, 27000, Uruguay",
            Want::At(CEIBOS, 25.0, true),
        ),
        named(
            "17 Camino Los Ceibos, 27000, Uruguay",
            "uy",
            "",
            &["UY-RO"],
            true,
        ),
        named(
            "40 Camino del Indio, 27000, Uruguay",
            "uy",
            "",
            &["UY-RO"],
            true,
        ),
    ]
}

/// The road is in no town: its houses are still in Uruguay, and each cell's house 13 is another
/// house, though all read alike.
#[test]
fn every_copy_of_a_rural_house_is_suggested_in_its_country() {
    let found = index().suggest(
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

#[test]
fn a_house_far_from_any_settlement_names_its_region_and_country() {
    let indio = house(&INDIO, "40");
    for (typed, number, point) in [
        ("Camino Los Ceibos 17", "17", CEIBOS),
        ("Camino del Indio 40", "40", indio),
    ] {
        let found = index().suggest(typed, 5, Some("UY"), None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{typed}");
        assert!(metres((found[0].x, found[0].y), point) < 5.0, "{typed}");
        assert_eq!(found[0].country_code, "uy", "{typed}");
        assert!(found[0].display_name.ends_with("Rocha, Uruguay"), "{typed}");
        let reverse = index().reverse_search(point.1, point.0, 50.0, 1);
        let address = &reverse[0].1["address"];
        assert_eq!(address["house_number"], number, "{typed}");
        assert_eq!(address["country_code"], "uy", "{typed}");
        assert_eq!(address["state"], "Rocha", "{typed}");
        assert_eq!(address["ISO3166-2-lvl4"], "UY-RO", "{typed}");
    }
}
