use super::*;
use crate::{Case, at_house, case, town};

/// Two villages called Pepe: a small one in Grobogan regency (Tegowanu district, Central Java)
/// and a larger one in Sleman (Yogyakarta). Two called Guntur: a kelurahan of South Jakarta and a
/// larger village in Grobogan.
pub const PLACES: &[Place] = &[
    place!(
        "id-rimba-town",
        "locality",
        "town",
        "ID",
        "ID-JT",
        3500,
        (165.27, -60.16),
        "id-grobogan",
        "Rimba",
        &[("id", "Kecamatan Rimba")],
    ),
    place!(
        "id-kenanga",
        "locality",
        "village",
        "ID",
        "ID-JT",
        700,
        (165.25, -60.18),
        "id-grobogan",
        "Kenanga",
        &[],
    ),
    place!(
        "id-bening",
        "locality",
        "village",
        "ID",
        "ID-JT",
        600,
        (165.26, -60.19),
        "id-grobogan",
        "Bening",
        &[],
    ),
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
        "id-serojawangi",
        "locality",
        "village",
        "ID",
        "ID-JT",
        250,
        (165.17, -60.02),
        "id-tegowanu",
        "Serojawangi",
        &[]
    ),
    place!(
        "id-serojawangi-other",
        "locality",
        "village",
        "ID",
        "ID-JT",
        500,
        (165.19, -60.03),
        "id-mekar-harum",
        "Serojawangi",
        &[]
    ),
    place!(
        "id-pesisirjalan",
        "locality",
        "village",
        "ID",
        "ID-JT",
        200,
        (165.17, -60.04),
        "id-tegowanu",
        "Pesisirjalan",
        &[]
    ),
    place!(
        "id-pesisir-jalan-other",
        "locality",
        "village",
        "ID",
        "ID-JT",
        400,
        (165.0, -59.1),
        "id-blora",
        "Pesisir Jalan",
        &[]
    ),
    place!(
        "id-rimbakuning-1",
        "locality",
        "village",
        "ID",
        "ID-JT",
        1000,
        (165.25, -60.13),
        "id-tegowanu",
        "Rimbakuning",
        &[]
    ),
    place!(
        "id-rimbakuning-2",
        "locality",
        "village",
        "ID",
        "ID-JT",
        500,
        (165.26, -60.14),
        "id-tegowanu",
        "Rimbakuning",
        &[]
    ),
    place!(
        "id-rimbakuning-3",
        "locality",
        "village",
        "ID",
        "ID-JT",
        4000,
        (165.23, -60.15),
        "id-mekar-harum",
        "Rimbakuning",
        &[]
    ),
    place!(
        "id-rimbakuning-4",
        "locality",
        "village",
        "ID",
        "ID-JT",
        4000,
        (165.231, -60.15),
        "id-mekar-harum",
        "Rimbakuning",
        &[]
    ),
    place!(
        "id-rimbakuning-5",
        "locality",
        "village",
        "ID",
        "ID-JT",
        4000,
        (165.232, -60.15),
        "id-mekar-harum",
        "Rimbakuning",
        &[]
    ),
    place!(
        "id-rimbakuning-6",
        "locality",
        "village",
        "ID",
        "ID-JT",
        4000,
        (165.233, -60.15),
        "id-mekar-harum",
        "Rimbakuning",
        &[]
    ),
    place!(
        "id-rimbakuning-7",
        "locality",
        "village",
        "ID",
        "ID-JT",
        4000,
        (165.234, -60.15),
        "id-mekar-harum",
        "Rimbakuning",
        &[]
    ),
    place!(
        "id-rimbakuning-8",
        "locality",
        "village",
        "ID",
        "ID-JT",
        4000,
        (165.235, -60.15),
        "id-mekar-harum",
        "Rimbakuning",
        &[]
    ),
    place!(
        "id-rimbakuning-9",
        "locality",
        "village",
        "ID",
        "ID-JT",
        4000,
        (165.236, -60.15),
        "id-mekar-harum",
        "Rimbakuning",
        &[]
    ),
    place!(
        "id-rimbakuning-10",
        "locality",
        "village",
        "ID",
        "ID-JT",
        4000,
        (165.237, -60.15),
        "id-mekar-harum",
        "Rimbakuning",
        &[]
    ),
    place!(
        "id-rimbakuning-11",
        "locality",
        "village",
        "ID",
        "ID-JT",
        4000,
        (165.238, -60.15),
        "id-mekar-harum",
        "Rimbakuning",
        &[]
    ),
    place!(
        "id-cendana",
        "locality",
        "village",
        "ID",
        "ID-JT",
        500,
        (165.28, -60.22),
        "id-grobogan",
        "Cendana",
        &[]
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
        name: "Kec. Mekar Harum Road",
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

pub fn write_hierarchy(path: &std::path::Path) {
    use std::io::Write;
    let rows = serde_json::json!([
        ["33", "Jawa Tengah"],
        ["33.15", "Kabupaten Grobogan"],
        ["33.15.90", "Rimba"],
        ["33.15.90.2001", "Kenanga"],
        ["33.15.01", "Tegowanu"],
        ["33.15.01.2001", "Bening"],
        ["33.15.91", "Sejuk"],
        ["33.15.92", "Gaya"],
        ["33.15.92.2001", "Cendana"],
        ["33.15.93", "Sejuk Rimba"],
        ["33.15.94", "Sejuk Rimba"],
        ["33.15.95", "Mekar Harum"]
    ]);
    let mut out = flate2::write::GzEncoder::new(
        std::fs::File::create(path).unwrap(),
        flate2::Compression::default(),
    );
    out.write_all(&serde_json::to_vec(&rows).unwrap()).unwrap();
    out.finish().unwrap();
}

const ADMIN_POINTS: &[(u64, &str, f64, f64, &str, &str)] = &[
    (123, "33.15.91", 165.27, -60.18, "Kecamatan Sejuk", "Sejuk"),
    (124, "33.15.92", 165.27, -60.22, "Kecamatan Gaya", "Gaya"),
    (
        126,
        "33.15.93",
        165.28,
        -60.24,
        "Sejuk Rimba",
        "Sejuk Rimba",
    ),
    (
        127,
        "33.15.94",
        165.29,
        -60.24,
        "Sejuk Rimba",
        "Sejuk Rimba",
    ),
    (
        128,
        "33.15.95",
        165.29,
        -60.26,
        "Kecamatan Mekar Harum",
        "Mekar Harum",
    ),
    (
        125,
        "33.15.01",
        165.29,
        -60.2,
        "Kecamatan Tegowanu",
        "Tegowanu",
    ),
];

pub fn write_admin_points(path: &std::path::Path) {
    use std::io::Write;
    let mut out = flate2::write::GzEncoder::new(
        std::fs::File::create(path).unwrap(),
        flate2::Compression::default(),
    );
    out.write_all(&serde_json::to_vec(ADMIN_POINTS).unwrap())
        .unwrap();
    out.finish().unwrap();
}

pub fn check_admin_points(index: &opengeocoding::index::Index, directory: &std::path::Path) {
    let text = std::fs::read_to_string(directory.join("places.tsv")).unwrap();
    let rows: Vec<Vec<_>> = text.lines().map(|l| l.split('\t').collect()).collect();
    for &(_, _, x, y, _, name) in &ADMIN_POINTS[..2] {
        let id = rows
            .iter()
            .position(|r| {
                r[13] == name && r[1] == opengeocoding::index::country::id::ADMIN_POINT_CLASS
            })
            .unwrap();
        let rendered = index.render(&opengeocoding::index::Hit::Place(id as u32), true);
        assert_eq!(rendered["address"]["district"], name);
        assert!(rendered["address"].get("municipality").is_none());
        assert!(
            index.search(name).iter().all(
                |hit| !matches!(hit, opengeocoding::index::Hit::Place(p) if *p as usize == id)
            )
        );
        let found = index.suggest(
            &format!("Kec. {name}, Grobogan, Jawa Tengah, Indonesia"),
            5,
            Some("id"),
            None,
            false,
        );
        let suggestion = found.iter().find(|s| s.id == format!("p{id}")).unwrap();
        assert_eq!(suggestion.kind, "district");
        assert!((suggestion.x - x).abs() < 1e-5 && (suggestion.y - y).abs() < 1e-5);
        let selected = index.retrieve_suggestion(&suggestion.retrieval_id).unwrap();
        assert_eq!(selected.kind, "district");
    }
    let original = rows.iter().find(|r| r[13] == "Tego Wanu").unwrap();
    assert_eq!(original[6].parse::<f64>().unwrap(), 165.1);
    assert_eq!(original[7].parse::<f64>().unwrap(), -60.0);
}

pub async fn check_admin_search(resolver: &opengeocoding::resolver::Resolver) {
    for name in ["Sejuk", "Gaya"] {
        for query in [
            format!("Kec. {name}, Kab. Grobogan, Jateng"),
            format!("{name}, Grobogan, Jawa Tengah, Indonesia"),
        ] {
            let got = resolver.resolve(&query).await.unwrap();
            let details = got.address_details.as_option().unwrap();
            assert_eq!(details.country_code, "id", "{query}: {got:?}");
            assert_eq!(details.county, "Kabupaten Grobogan", "{query}: {got:?}");
            assert!(
                details.district.is_empty() && details.municipality.is_empty(),
                "{query}: {got:?}"
            );
            assert!(!got.good_match, "{query}: {got:?}");
            let at = got.geo_coordinates.as_option().unwrap();
            assert!(
                crate::metres((at.lon as f64, at.lat as f64), (165.0, -60.0)) < 5.0,
                "{query}: {got:?}"
            );
        }
    }
}

pub fn check_hierarchy(index: &opengeocoding::index::Index, directory: &std::path::Path) {
    let text = std::fs::read_to_string(directory.join("places.tsv")).unwrap();
    let rows: Vec<Vec<_>> = text
        .lines()
        .map(|line| line.split('\t').collect())
        .collect();
    let district = rows
        .iter()
        .position(|r| {
            r[1] == opengeocoding::index::country::id::HIERARCHY_CLASS && r[13] == "Rimba"
        })
        .unwrap();
    assert!(rows[district][6].parse::<f64>().unwrap().is_nan());
    assert!(rows[district][7].parse::<f64>().unwrap().is_nan());
    let village = rows.iter().position(|r| r[13] == "Kenanga").unwrap();
    assert_eq!(rows[village][12].parse::<usize>().unwrap(), district);
    let rendered = index.render(&opengeocoding::index::Hit::Place(village as u32), true);
    assert_eq!(rendered["address"]["district"], "Rimba");
    assert!(rendered["address"].get("municipality").is_none());
    let parsed = [("city".into(), "Rimba".into())].into();
    let details = opengeocoding::proto::opengeocoding::AddressDetails {
        country_code: "id".into(),
        village: "Kenanga".into(),
        district: "Rimba".into(),
        ..Default::default()
    };
    assert!(!opengeocoding::matching::city_agrees(
        &parsed,
        Some(&details)
    ));
    let found = index.suggest(
        "Kenanga, Rimba, Grobogan, Jawa Tengah, Indonesia",
        5,
        None,
        None,
        false,
    );
    let suggestion = found.first().unwrap();
    let prefix = suggestion.retrieval_id.split_once('/').unwrap().0;
    assert!(
        index
            .retrieve_suggestion(&format!("{prefix}/p{district}"))
            .is_none()
    );
    for query in ["Rimba", "Kec. Rimba", "Kecamatan Rimba"] {
        assert!(
            index
                .suggest(query, 10, Some("id"), None, false)
                .iter()
                .all(|s| s.id != format!("p{district}") && s.x.is_finite() && s.y.is_finite())
        );
        assert!(index.search(query).iter().all(
            |hit| !matches!(hit, opengeocoding::index::Hit::Place(p) if *p as usize == district)
        ));
    }
    assert!(
        text.lines()
            .filter(|line| line.split('\t').nth(13) == Some("Kenanga"))
            .all(|line| {
                let r: Vec<_> = line.split('\t').collect();
                (r[6].parse::<f64>().unwrap() - 165.25).abs() < 1e-8
                    && (r[7].parse::<f64>().unwrap() + 60.18).abs() < 1e-8
            })
    );
}

pub fn check_suggest(index: &opengeocoding::index::Index) {
    let district = crate::place("id-mekar-harum");
    let found = index.suggest("Kec. Mekar Harum, Grobogan, Jateng", 1, None, None, false);
    assert!(
        found
            .first()
            .is_some_and(|s| crate::metres((s.x, s.y), (district.lon, district.lat)) < 5.0)
    );
    for (query, cc, place) in [
        (
            "Seroja Wangi, Tego Wanu, Grobogan, Jawa Tengah, Indonesia",
            None,
            "id-serojawangi",
        ),
        (
            "Desa Seroja Wangi, Kec. Tego Wanu, Kab. Grobogan, Jateng",
            None,
            "id-serojawangi",
        ),
        (
            "Pesisir Jalan, Tego Wanu, Grobogan, Jawa Tengah, Indonesia",
            None,
            "id-pesisirjalan",
        ),
        (
            "Desa Pesisir Jalan, Kec. Tego Wanu, Kab. Grobogan, Jateng",
            None,
            "id-pesisirjalan",
        ),
        ("Seroja Wangi", Some("id"), "id-serojawangi-other"),
        (
            "Kenanga, Rimba, Grobogan, Jawa Tengah, Indonesia",
            None,
            "id-kenanga",
        ),
        (
            "Desa Kenanga, Kec. Rimba, Kab. Grobogan, Jateng",
            None,
            "id-kenanga",
        ),
        (
            "Bening, Tegowanu, Grobogan, Jawa Tengah, Indonesia",
            None,
            "id-bening",
        ),
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
        (
            "Seroja Wangi, Setiabudi, Grobogan, Jawa Tengah, Indonesia",
            "id-serojawangi",
        ),
        (
            "Desa Seroja Wangi, Kec. Tego Wanu, Kab. Sleman, Yogyakarta, Indonesia",
            "id-serojawangi",
        ),
        (
            "Seroja Wangi, Tego Wanu, Grobogan, Jawa Tengah, France",
            "id-serojawangi",
        ),
        (
            "Seroja Wan, Tego Wanu, Grobogan, Jawa Tengah, Indonesia",
            "id-serojawangi",
        ),
        (
            "Seroja Wangi, Tego Wanu, Kota Grobogan, Jawa Tengah, Indonesia",
            "id-serojawangi",
        ),
        (
            "Kenanga, Tegowanu, Grobogan, Jawa Tengah, Indonesia",
            "id-kenanga",
        ),
        (
            "Kenanga, Rimba, Sleman, Yogyakarta, Indonesia",
            "id-kenanga",
        ),
        (
            "Kenanga, Rimba, Grobogan, Jawa Tengah, France",
            "id-kenanga",
        ),
        (
            "Kenanga, Rimba, Kota Grobogan, Jawa Tengah, Indonesia",
            "id-kenanga",
        ),
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

#[test]
fn locality_labels_match_only_whole_indonesian_names() {
    use opengeocoding::{
        matching::{ParsedAddress, city_agrees},
        proto::opengeocoding::AddressDetails,
    };

    for (stated, name, cc, expected) in [
        ("Mekar Harum", "Kecamatan Mekar Harum", "id", true),
        ("Mekar Harum", "Desa Mekar Harum", "id", true),
        ("Lubuk Linggau", "Kecamatan Lubuklinggau", "id", true),
        ("Kota Baru", "Kecamatan Kota Baru", "id", true),
        ("Baru", "Kota Baru", "id", false),
        ("Baru", "Kecamatan Kota Baru", "id", false),
        ("Mekar", "Kecamatan Mekarsari", "id", false),
        ("Mekar Harum", "Kecamatan Mekar Harum", "fr", false),
    ] {
        let parsed: ParsedAddress = [("city".into(), stated.into())].into();
        let details = AddressDetails {
            country_code: cc.into(),
            village: name.into(),
            ..Default::default()
        };
        assert_eq!(
            city_agrees(&parsed, Some(&details)),
            expected,
            "{stated}, {name}, {cc}"
        );
    }
}

pub fn cases() -> Vec<Case> {
    vec![
        case(
            "Seroja Wangi, Tego Wanu, Grobogan, Jawa Tengah, Indonesia",
            town("id-serojawangi", true),
        ),
        case(
            "Desa Seroja Wangi, Kec. Tego Wanu, Kab. Grobogan, Jateng",
            town("id-serojawangi", true),
        ),
        case(
            "Pesisir Jalan, Tego Wanu, Grobogan, Jawa Tengah, Indonesia",
            town("id-pesisirjalan", true),
        ),
        case(
            "Desa Pesisir Jalan, Kec. Tego Wanu, Kab. Grobogan, Jateng",
            town("id-pesisirjalan", true),
        ),
        case(
            "Desa Rimba Kuning, Kec. Tego Wanu, Kab. Grobogan, Jateng",
            town("id-tegowanu", false),
        ),
        case(
            "Pesisir Jalan, Kab. Grobogan, Indonesia",
            town("id-pesisirjalan", true),
        ),
        case(
            "Kec. Pesisir Jalan, Kab. Grobogan, Indonesia",
            town("id-grobogan", false),
        ),
        case(
            "Seroja Wangi, Grobogan, Jawa Tengah, Indonesia",
            town("id-grobogan", false),
        ),
        case(
            "Desa Seroja Wangi, Kec. Setiabudi, Kab. Grobogan, Jateng",
            town("id-grobogan", false),
        ),
        case(
            "Kenanga, Rimba, Grobogan, Jawa Tengah, Indonesia",
            town("id-kenanga", true),
        ),
        case(
            "Desa Kenanga, Kec. Rimba, Kab. Grobogan, Jateng",
            town("id-kenanga", true),
        ),
        case(
            "Bening, Tegowanu, Grobogan, Jawa Tengah, Indonesia",
            town("id-bening", true),
        ),
        case(
            "Mekar Harum, Grobogan, Jawa Tengah, Indonesia",
            town("id-mekar-harum", true),
        ),
        case(
            "Mekar Harum, Grobogan, Indonesia",
            town("id-mekar-harum", true),
        ),
        case(
            "Kec. Mekar Harum, Kab. Grobogan, Jateng",
            town("id-mekar-harum", true),
        ),
        case(
            "Kec. Mekar Harum, Kab. Sleman, Yogyakarta, Indonesia",
            town("id-sleman", false),
        ),
        case(
            "Kota Baru, Grobogan, Jawa Tengah, Indonesia",
            town("id-kota-baru", true),
        ),
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
