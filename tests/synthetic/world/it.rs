use super::*;
use crate::*;

/// Rho lies in the Metropolitan City of Milan, whose Wikidata item gives it the ISO code IT-MI;
/// Pordenone's former province has no code, only its sigla PN.
pub const PLACES: &[Place] = &[
    place!(
        "it",
        "country",
        "",
        "IT",
        "",
        59_000_000,
        (-20.0, -40.0),
        "",
        "Italy",
        &[("en", "Italy"), ("it", "Italia")]
    ),
    place!(
        "it-lombardia",
        "region",
        "",
        "IT",
        "IT-25",
        10_000_000,
        (-20.0, -40.0),
        "it",
        "Lombardia",
        &[("en", "Lombardy")]
    ),
    place!(
        "it-milano",
        "county",
        "",
        "IT",
        "IT-25",
        3_000_000,
        (-20.0, -40.0),
        "it-lombardia",
        "Città metropolitana di Milano",
        &[("en", "Milan")]
    ),
    place!(
        "it-rho",
        "locality",
        "town",
        "IT",
        "IT-25",
        50_000,
        (-20.0, -40.0),
        "it-milano",
        "Rho",
        &[("en", "Rho")]
    ),
    place!(
        "it-fvg",
        "region",
        "",
        "IT",
        "IT-36",
        1_200_000,
        (-21.0, -40.0),
        "it",
        "Friuli-Venezia Giulia",
        &[("en", "Friuli – Venezia Giulia")]
    ),
    place!(
        "it-pordenone",
        "county",
        "",
        "IT",
        "IT-36",
        300_000,
        (-21.0, -40.0),
        "it-fvg",
        "Pordenone",
        &[("en", "Pordenone")]
    ),
    place!(
        "it-cordovado",
        "locality",
        "village",
        "IT",
        "IT-36",
        2_700,
        (-21.0, -40.0),
        "it-pordenone",
        "Cordovado",
        &[("en", "Cordovado")]
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Via Bolzano",
        postcode: "",
        city: "Rho",
        cc: "IT",
        lon: -20.001,
        lat: -40.0,
        numbers: &["15", "17"],
    },
    // Possidente is a frazione the houses do not name
    Street {
        class: "residential",
        length: 0.0,
        name: "Via Praga",
        postcode: "",
        city: "Rho",
        cc: "IT",
        lon: -20.003,
        lat: -40.001,
        numbers: &["3"],
    },
    // the houses name a frazione no place is called
    Street {
        class: "residential",
        length: 0.0,
        name: "LOCALITA' CHANTEL",
        postcode: "",
        city: "Borgonuovo",
        cc: "IT",
        lon: -20.005,
        lat: -40.002,
        numbers: &["29"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Via Ligugnana",
        postcode: "",
        city: "Cordovado",
        cc: "IT",
        lon: -21.001,
        lat: -40.0,
        numbers: &["9"],
    },
];

pub fn cases() -> Vec<Case> {
    vec![
        case(
            "VIA BOLZANO 17, Rho (MI), Italy",
            at_house("Via Bolzano", "17"),
        ),
        case(
            "VIA PRAGA 3, POSSIDENTE, Rho (MI), Italy",
            at_house("Via Praga", "3"),
        ),
    ]
}

#[test]
fn italian_houses_are_suggested_with_province_sigle_and_frazioni() {
    let at = |text: &str| index().suggest(text, 5, None, None, false);
    for (typed, road, number) in [
        ("VIA BOLZANO 17, Rho (MI), Italy", "Via Bolzano", "17"),
        ("Via Bolzano 17, Rho, MI", "Via Bolzano", "17"),
        (
            "VIA LIGUGNANA 9, Cordovado (PN), Italy",
            "Via Ligugnana",
            "9",
        ),
        ("Via Ligugnana 9, Cordovado, PN", "Via Ligugnana", "9"),
        ("VIA PRAGA 3, POSSIDENTE, Rho (MI), Italy", "Via Praga", "3"),
        (
            "LOCALITA' CHANTEL 29, Borgonuovo (MI), Italy",
            "LOCALITA' CHANTEL",
            "29",
        ),
    ] {
        let found = at(typed);
        assert!(!found.is_empty(), "{typed}");
        assert_eq!(found[0].kind, "address", "{typed}");
        assert!(
            metres((found[0].x, found[0].y), house(street(road), number)) < 5.0,
            "{typed}"
        );
    }
    // another province's code rules the town out; a sigla with no code cannot
    for typed in [
        "VIA LIGUGNANA 9, Cordovado (MI), Italy",
        "Via Ligugnana 9, Cordovado, MI",
    ] {
        assert!(at(typed).iter().all(|s| s.kind != "address"), "{typed}");
    }
}
