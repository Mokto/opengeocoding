use super::*;
use crate::{Case, at_house, case, town};

/// Two villages called Pepe: a small one in Grobogan regency (Tegowanu district, Central Java)
/// and a larger one in Sleman (Yogyakarta).
pub const PLACES: &[Place] = &[
    place!(
        "id",
        "country",
        "",
        "ID",
        "",
        280_000_000,
        (165.0, -60.0),
        "",
        "Indonesia",
        &[("en", "Indonesia")],
    ),
    place!(
        "id-jt",
        "region",
        "",
        "ID",
        "ID-JT",
        37_000_000,
        (165.0, -60.0),
        "id",
        "Jawa Tengah",
        &[("en", "Central Java"), ("id", "Jawa Tengah")],
    ),
    place!(
        "id-yo",
        "region",
        "",
        "ID",
        "ID-YO",
        3_700_000,
        (166.0, -60.0),
        "id",
        "Yogyakarta",
        &[("id", "Yogyakarta")],
    ),
    place!(
        "id-grobogan",
        "county",
        "",
        "ID",
        "ID-JT",
        1_400_000,
        (165.0, -60.0),
        "id-jt",
        "Grobogan",
        &[],
    ),
    place!(
        "id-tegowanu",
        "localadmin",
        "",
        "ID",
        "ID-JT",
        50_000,
        (165.1, -60.0),
        "id-grobogan",
        "Tegowanu",
        &[],
    ),
    place!(
        "id-pepe",
        "locality",
        "village",
        "ID",
        "ID-JT",
        3_000,
        (165.12, -60.02),
        "id-tegowanu",
        "Pepe",
        &[],
    ),
    place!(
        "id-sleman",
        "county",
        "",
        "ID",
        "ID-YO",
        1_100_000,
        (166.0, -60.0),
        "id-yo",
        "Sleman",
        &[],
    ),
    place!(
        "id-pepe-sleman",
        "locality",
        "village",
        "ID",
        "ID-YO",
        8_000,
        (166.05, -60.0),
        "id-sleman",
        "Pepe",
        &[],
    ),
];

pub const STREETS: &[Street] = &[Street {
    class: "residential",
    length: 0.0,
    name: "Jalan Merdeka",
    postcode: "",
    city: "Pepe",
    cc: "ID",
    lon: 165.12,
    lat: -60.02,
    numbers: &["5"],
}];

pub fn cases() -> Vec<Case> {
    vec![
        case("Pepe, Grobogan, Indonesia", town("id-pepe", true)),
        case(
            "Pepe, Tegowanu, Grobogan, Jawa Tengah, Indonesia",
            town("id-pepe", true),
        ),
        case(
            "Desa Pepe, Kec. Tegowanu, Kab. Grobogan, Jawa Tengah, Indonesia",
            town("id-pepe", true),
        ),
        case(
            "Pepe, Sleman, Yogyakarta, Indonesia",
            town("id-pepe-sleman", true),
        ),
        case(
            "Jalan Merdeka No. 5, RT 02/RW 03, Desa Pepe, Kec. Tegowanu, Kab. Grobogan, Jawa Tengah, Indonesia",
            at_house("Jalan Merdeka", "5"),
        ),
        case(
            "Desa Tidakada, Kec. Tegowanu, Kab. Grobogan, Jawa Tengah, Indonesia",
            town("id-grobogan", false),
        ),
    ]
}
