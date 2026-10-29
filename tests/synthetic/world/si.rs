use super::*;
use crate::{Case, case};

/// Two municipalities, each the index's region (ISO 3166-2 SI-xxx), and a bilingual one; the
/// villages' houses are numbered on the village's own name, which Slovenian addresses follow
/// with the municipality.
pub const PLACES: &[Place] = &[
    place!(
        "si",
        "country",
        "",
        "SI",
        "",
        2_100_000,
        (121.5, -45.8),
        "",
        "Slovenija",
        &[("sl", "Slovenija"), ("en", "Slovenia")]
    ),
    place!(
        "si-tolmin",
        "region",
        "",
        "SI",
        "SI-128",
        11_000,
        (120.0, -45.0),
        "si",
        "Tolmin",
        &[("sl", "Tolmin")]
    ),
    place!(
        "si-kanal",
        "region",
        "",
        "SI",
        "SI-044",
        5_000,
        (121.5, -45.0),
        "si",
        "Kanal",
        &[("sl", "Kanal")]
    ),
    place!(
        "si-koper",
        "region",
        "",
        "SI",
        "SI-050",
        53_000,
        (123.0, -45.0),
        "si",
        "Koper / Capodistria",
        &[("sl", "Koper"), ("it", "Capodistria")]
    ),
    place!(
        "si-tolmin-town",
        "locality",
        "town",
        "SI",
        "SI-128",
        3_000,
        (120.0, -45.0),
        "si-tolmin",
        "Tolmin",
        &[("sl", "Tolmin")]
    ),
    place!(
        "si-baca",
        "locality",
        "village",
        "SI",
        "SI-128",
        300,
        (120.3, -44.75),
        "si-tolmin",
        "Bača pri Modreju",
        &[("sl", "Bača pri Modreju")]
    ),
    place!(
        "si-tolmin-gorenja-vas",
        "locality",
        "village",
        "SI",
        "SI-128",
        100,
        (119.7, -45.25),
        "si-tolmin",
        "Gorenja vas",
        &[("sl", "Gorenja vas")]
    ),
    place!(
        "si-kanal-gorenja-vas",
        "locality",
        "village",
        "SI",
        "SI-044",
        100,
        (121.5, -45.0),
        "si-kanal",
        "Gorenja vas",
        &[("sl", "Gorenja vas")]
    ),
    place!(
        "si-cesta",
        "locality",
        "village",
        "SI",
        "SI-128",
        100,
        (119.7, -44.7),
        "si-tolmin",
        "Cesta",
        &[("sl", "Cesta")]
    ),
    // a village of Kanal whose area reaches past the municipality's border into Tolmin
    place!(
        "si-kanal-mejna",
        "locality",
        "village",
        "SI",
        "SI-044",
        100,
        (120.62, -45.0),
        "si-kanal",
        "Mejna",
        &[("sl", "Mejna")]
    ),
    place!(
        "si-vanganel",
        "locality",
        "village",
        "SI",
        "SI-050",
        800,
        (123.0, -45.0),
        "si-koper",
        "Vanganel (naselje)",
        &[("sl", "Vanganel (naselje)")]
    ),
];

const fn village(name: &'static str, city: &'static str, (lon, lat): (f64, f64)) -> Street {
    Street {
        class: "residential",
        length: 0.0,
        name,
        postcode: "",
        city,
        cc: "SI",
        lon,
        lat,
        numbers: &["10", "19a", "20"],
    }
}

/// The register's settlement Postaja, which the data files under the nearest village it knows
const POSTAJA: Street = village("Postaja", "Bača pri Modreju", (120.3, -44.75));
const TOLMIN_TABOR: Street = village("Tabor", "Gorenja vas", (119.7, -45.25));
const KANAL_TABOR: Street = village("Tabor", "Gorenja vas", (121.5, -45.0));
const VANGANEL: Street = village("Vanganel", "Vanganel (naselje)", (123.0, -45.0));

/// a Tolmin village's houses the data files under Mejna, the Kanal village nearest them
const PIVOLA: Street = village("Pivola", "Mejna", (120.45, -45.0));
/// the village Cesta, beside a street of the town whose name ends in the word
const CESTA: Street = village("Cesta", "Cesta", (119.7, -44.7));
const RIMSKA_CESTA: Street = village("Rimska cesta", "Tolmin", (120.0, -45.0));

pub const STREETS: &[Street] = &[
    POSTAJA,
    TOLMIN_TABOR,
    KANAL_TABOR,
    VANGANEL,
    PIVOLA,
    CESTA,
    RIMSKA_CESTA,
];

fn at(street: &Street, number: &str) -> crate::Want {
    crate::Want::At(house(street, number), 25.0, true)
}

pub fn cases() -> Vec<Case> {
    vec![
        // the village and number, then only the municipality
        case("Postaja 19a, Tolmin, Slovenia", at(&POSTAJA, "19a")),
        // the settlement's name is a village of another municipality too
        case(
            "Tabor 10, Gorenja vas, Tolmin, Slovenia",
            at(&TOLMIN_TABOR, "10"),
        ),
        case(
            "Tabor 10, Gorenja vas, Kanal, Slovenia",
            at(&KANAL_TABOR, "10"),
        ),
        // a bilingual municipality, named in Slovene
        case("Vanganel 20, Koper, Slovenia", at(&VANGANEL, "20")),
        // filed under the next municipality's village, inside this one
        case("Pivola 20, Tolmin, Slovenia", at(&PIVOLA, "20")),
        // the village Cesta, not Rimska cesta
        case("Cesta 10, Tolmin, Slovenia", at(&CESTA, "10")),
    ]
}
