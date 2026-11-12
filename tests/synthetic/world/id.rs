use super::*;
use crate::{Case, at_house, case, town};

/// Two villages called Pepe: a small one in Grobogan regency (Tegowanu district, Central Java)
/// and a larger one in Sleman (Yogyakarta). Two called Guntur: a kelurahan of South Jakarta and a
/// larger village in Grobogan.
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
        "Kabupaten Grobogan",
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
        "Tego Wanu",
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
    place!(
        "id-guntur-grobogan",
        "locality",
        "village",
        "ID",
        "ID-JT",
        9_000,
        (165.2, -60.05),
        "id-grobogan",
        "Guntur",
        &[],
    ),
    place!(
        "id-jk",
        "region",
        "",
        "ID",
        "ID-JK",
        10_000_000,
        (167.0, -60.0),
        "id",
        "Daerah Khusus Ibukota Jakarta",
        &[("en", "Special Capital Region of Jakarta")],
    ),
    place!(
        "id-jaksel",
        "county",
        "",
        "ID",
        "ID-JK",
        2_200_000,
        (167.0, -60.0),
        "id-jk",
        "Jakarta Selatan",
        &[("en", "South Jakarta")],
    ),
    place!(
        "id-setiabudi",
        "localadmin",
        "",
        "ID",
        "ID-JK",
        130_000,
        (167.02, -60.0),
        "id-jaksel",
        "Setiabudi",
        &[],
    ),
    place!(
        "id-guntur",
        "locality",
        "village",
        "ID",
        "ID-JK",
        2_000,
        (167.03, -60.01),
        "id-setiabudi",
        "Guntur",
        &[],
    ),
    place!(
        "id-blora",
        "county",
        "",
        "ID",
        "ID-JT",
        900_000,
        (165.0, -59.0),
        "id-jt",
        "Blora",
        &[],
    ),
    place!(
        "id-gubeng-kec",
        "localadmin",
        "",
        "ID",
        "ID-JT",
        120_000,
        (165.25, -60.15),
        "id-grobogan",
        "Gubeng",
        &[],
    ),
    place!(
        "id-sawang-kec",
        "localadmin",
        "",
        "ID",
        "ID-JT",
        60_000,
        (165.2, -60.1),
        "id-grobogan",
        "Sawang",
        &[],
    ),
    place!(
        "id-sawang",
        "locality",
        "village",
        "ID",
        "ID-JT",
        4_000,
        (165.0, -58.0),
        "id-jt",
        "Sawang",
        &[],
    ),
    place!(
        "id-kota-baru",
        "locality",
        "village",
        "ID",
        "ID-JT",
        2_000,
        (165.13, -60.02),
        "id-tegowanu",
        "Kota Baru",
        &[],
    ),
    place!(
        "id-melati-timur",
        "locality",
        "village",
        "ID",
        "ID-JT",
        2_000,
        (165.14, -60.03),
        "id-tegowanu",
        "Melati Timur",
        &[],
    ),
    place!(
        "id-mekar-harum",
        "locality",
        "village",
        "ID",
        "ID-JT",
        2_000,
        (165.15, -60.04),
        "id-grobogan",
        "Kecamatan Mekar Harum",
        &[],
    ),
    place!(
        "id-taman",
        "locality",
        "neighbourhood",
        "ID",
        "ID-JT",
        2_000,
        (165.14, -60.02),
        "id-kota-baru",
        "Taman",
        &[],
    ),
    // a village of Gubeng district as the data files it, but in Blora's polygon
    place!(
        "id-gubeng",
        "locality",
        "village",
        "ID",
        "ID-JT",
        4_000,
        (165.0, -59.05),
        "id-gubeng-kec",
        "Gubeng",
        &[],
    ),
];

/// Grobogan's and Blora's regency polygons.
pub const AREAS: &[(&str, &str, bool, bool, f64, f64, f64)] = &[
    ("id-grobogan", "county", true, true, 165.0, -60.0, 0.3),
    ("id-blora", "county", true, true, 165.0, -59.0, 0.3),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Jalan Merdeka",
        postcode: "",
        city: "Pepe",
        cc: "ID",
        lon: 165.12,
        lat: -60.02,
        numbers: &["5"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Jalan Mekar Harum",
        postcode: "",
        city: "Pepe",
        cc: "ID",
        lon: 165.16,
        lat: -60.04,
        numbers: &["2"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Jl. Sudirman",
        postcode: "",
        city: "Guntur",
        cc: "ID",
        lon: 167.03,
        lat: -60.01,
        numbers: &["7"],
    },
];

pub fn check_suggest(index: &opengeocoding::index::Index) {
    for (query, cc, place) in [
        (
            "Desa Pepe, Kec. Tegowanu, Kab. Grobogan, Jawa Tengah, Indonesia",
            None,
            "id-pepe",
        ),
        (
            "Desa Pepe, Kec. Tegowanu, Kab. Grobogan, Jateng",
            None,
            "id-pepe",
        ),
        ("Pepe, Grobogan, Jawa Tengah, Indonesia", None, "id-pepe"),
        ("Pepe, Tegowanu, Grobogan, Jateng", None, "id-pepe"),
        ("Kab. Grobogan", Some("id"), "id-grobogan"),
        ("Kab. Grobog", Some("id"), "id-grobogan"),
        ("Prov. Jawa Tengah", Some("id"), "id-jt"),
        ("Prov. Jawa Ten", Some("id"), "id-jt"),
        ("Mekar Harum, Grobogan, Jateng", None, "id-mekar-harum"),
        (
            "Desa Melati, Kec. Tegowanu, Kab. Grobogan, Jateng",
            None,
            "id-melati-timur",
        ),
        (
            "Kec. Mekar Harum, Kab. Grobogan, Jateng",
            None,
            "id-mekar-harum",
        ),
        ("Kec. Mekar Har", Some("id"), "id-mekar-harum"),
        ("Pepe, Grobogan ", Some("id"), "id-pepe"),
        ("Pepe, Grobog", Some("id"), "id-pepe"),
        (
            "Taman, Kota Baru, Grobogan, Jawa Tengah, Indonesia",
            None,
            "id-taman",
        ),
        ("Desa Pe", Some("ID"), "id-pepe-sleman"),
        ("Desa Pepe ", Some("id"), "id-pepe-sleman"),
        (
            "Desa Kota Baru, Kec. Tegowanu, Kab. Grobogan, Jateng",
            None,
            "id-kota-baru",
        ),
        (
            "Kel. Guntur, Kec. Setiabudi, Kota Adm. Jakarta Selatan, DKI Jakarta, Indonesia",
            None,
            "id-guntur",
        ),
        ("Guntur, Setiabudi, Jaksel", None, "id-guntur"),
    ] {
        let found = index.suggest(query, 5, cc, None, false);
        let wanted = crate::place(place);
        assert!(
            found
                .first()
                .is_some_and(|s| crate::metres((s.x, s.y), (wanted.lon, wanted.lat)) < 5.0),
            "{query}: {:?}",
            found.iter().map(|s| &s.display_name).collect::<Vec<_>>()
        );
    }
    for (query, wrong) in [
        ("Kota Grobogan", "id-grobogan"),
        ("Kab. Grobog ", "id-grobogan"),
        ("Prov. Jawa Ten ", "id-jt"),
        ("Desa Mekar Harum, Grobogan, Jateng", "id-mekar-harum"),
        ("Kec. Kota Baru, Grobogan, Jateng", "id-kota-baru"),
        ("Desa Baru, Grobogan, Jateng", "id-kota-baru"),
        ("Kec. Mekar Harum, Sleman, Yogyakarta", "id-mekar-harum"),
        ("Kec. Mekar Harum, Grobogan, France", "id-mekar-harum"),
    ] {
        let wrong = crate::place(wrong);
        assert!(
            index
                .suggest(query, 5, Some("id"), None, false)
                .iter()
                .all(|s| {
                    (matches!(wrong.subtype, "county" | "region")
                        && s.kind
                            != if wrong.subtype == "county" {
                                "county"
                            } else {
                                "region"
                            })
                        || crate::metres((s.x, s.y), (wrong.lon, wrong.lat)) > 5.0
                }),
            "{query}"
        );
    }
    let taman = crate::place("id-taman");
    assert!(
        index
            .suggest(
                "Taman, Baru, Grobogan, Jawa Tengah, Indonesia",
                5,
                None,
                None,
                false
            )
            .iter()
            .all(|s| crate::metres((s.x, s.y), (taman.lon, taman.lat)) > 5.0)
    );
    let district = crate::place("id-sawang-kec");
    assert!(
        index
            .suggest("Desa Sawang, Kab. Grobogan, Jateng", 5, None, None, false)
            .iter()
            .all(|s| crate::metres((s.x, s.y), (district.lon, district.lat)) > 5.0)
    );
    let pepe = crate::place("id-pepe");
    for (query, cc) in [
        ("Pepe, Kota Grobogan, Jawa Tengah, Indonesia", None),
        ("Pepe, Grobog ", Some("id")),
        ("Pepe, Bogor, Jawa Tengah, Indonesia", None),
        ("Desa Pepe, Kec. Setiabudi, Kab. Grobogan, Jateng", None),
        ("Desa Pepe, Kec. Tegowanu, Kab. Sleman, Jateng", None),
        (
            "Desa Pepe, Kec. Tegowanu, Kab. Grobogan, Jateng, France",
            None,
        ),
        (
            "Desa Pepe, Kec. Tegowanu, Kab. Grobogan, Jateng",
            Some("fr"),
        ),
    ] {
        assert!(
            index
                .suggest(query, 5, cc, None, false)
                .iter()
                .all(|s| crate::metres((s.x, s.y), (pepe.lon, pepe.lat)) > 5.0),
            "{query}"
        );
    }
}

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
        // the district, not its namesake village the data files under it but places in Blora
        case(
            "Gubeng, Grobogan, Jawa Tengah, Indonesia",
            town("id-gubeng-kec", true),
        ),
        case(
            "Sawang, Grobogan, Jawa Tengah, Indonesia",
            town("id-sawang-kec", true),
        ),
        // the province shortened, the country left out
        case(
            "Desa Pepe, Kec. Tegowanu, Kab. Grobogan, Jateng",
            town("id-pepe", true),
        ),
        case(
            "Pepe, Tegowanu, Grobogan, Jawa Tengah",
            town("id-pepe", true),
        ),
        case(
            "Kel. Guntur, Kec. Setiabudi, Kota Adm. Jakarta Selatan, DKI Jakarta, Indonesia",
            town("id-guntur", true),
        ),
        case(
            "Guntur, Setiabudi, Kota Jkt Selatan, DKI Jakarta",
            town("id-guntur", true),
        ),
        case("Guntur, Setiabudi, Jaksel", town("id-guntur", true)),
        // "Jl." and "Jalan" are one street type
        case(
            "Jalan Sudirman No. 7, Guntur, Setiabudi, Jaksel",
            at_house("Jl. Sudirman", "7"),
        ),
        case(
            "Jl. Merdeka No. 5, Desa Pepe, Kec. Tegowanu, Kab. Grobogan, Jateng",
            at_house("Jalan Merdeka", "5"),
        ),
    ]
}
