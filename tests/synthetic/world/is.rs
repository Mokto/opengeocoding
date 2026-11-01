use super::*;
use crate::{Case, Want, at_house, case, index, metres, street, town};
use std::path::Path;

pub const PLACES: &[Place] = &[
    place!(
        "is",
        "country",
        "",
        "IS",
        "",
        400_000,
        (-19.0, 65.0),
        "",
        "Iceland",
        &[("is", "Ísland"), ("en", "Iceland")]
    ),
    place!(
        "is-ljosvik",
        "locality",
        "town",
        "IS",
        "",
        20_000,
        (-20.0, 64.0),
        "is",
        "Ljósvík",
        &[]
    ),
    place!(
        "is-skuggavik",
        "locality",
        "town",
        "IS",
        "",
        10_000,
        (-21.0, 65.0),
        "is",
        "Skuggavík",
        &[]
    ),
];

const fn road(
    name: &'static str,
    lon: f64,
    lat: f64,
    postcode: &'static str,
    city: &'static str,
    numbers: &'static [&'static str],
) -> Street {
    Street {
        name,
        lon,
        lat,
        postcode,
        city,
        numbers,
        cc: "IS",
        class: "residential",
        length: 0.0,
    }
}

pub const STREETS: &[Street] = &[
    road("Ljósagarður", -20.001, 64.0, "121", "Ljósvík", &["13B"]),
    road("Ljósagarðar", -21.001, 65.0, "421", "Skuggavík", &["13B"]),
    road("Bjartaholt", -20.002, 64.0, "121", "Postalvík", &[""]),
    road("Óskráðholt", -20.003, 64.0, "121", "Ljósvík", &["8"]),
    road("Sólholt", -20.004, 64.0, "121", "Ljósvík", &[""]),
    road("Sólhóll", -20.005, 64.0, "121", "Ljósvík", &[""]),
    road("Dagskot", -20.006, 64.0, "121", "Ljósvík", &[""]),
    road("Dagsskot", -20.006, 64.0, "121", "Ljósvík", &[""]),
];

const ALIASES: &[(&str, &str)] = &[
    ("Ljósagarður", "Ljósagörðum"),
    ("Ljósagarðar", "Ljósagörðum"),
    ("Bjartaholt", "Bjartaholti"),
    ("Sólholt", "Sólhólum"),
    ("Sólhóll", "Sólhólum"),
    ("Dagskot", "Dagskoti"),
    ("Dagsskot", "Dagskoti"),
];

pub fn write(path: &Path) -> String {
    let mut text =
        "HNITNUM,HEITI_NF,HEITI_TGF,HUSNR,BOKST,VIDSK,POSTNR,N_HNIT_WGS84,E_HNIT_WGS84,YFIRFARID\n"
            .to_string();
    for (i, &(nf, tgf)) in ALIASES.iter().enumerate() {
        let street = STREETS.iter().find(|s| s.name == nf).unwrap();
        let (number, letter) = if street.numbers == ["13B"] {
            ("13", "B")
        } else {
            ("", "")
        };
        text += &format!(
            "{i},{nf},{tgf},{number},{letter},,{},{},{},0\n",
            street.postcode, street.lat, street.lon
        );
    }
    let sha = opengeocoding::eval::sha256_hex(text.as_bytes());
    std::fs::write(path, text).unwrap();
    sha
}

pub fn cases() -> Vec<Case> {
    vec![
        case(
            "Ljósagarður 13B, 121 Ljósvík, Iceland",
            at_house("Ljósagarður", "13B"),
        ),
        case(
            "Ljósagörðum 13B, 121 Ljósvík, Iceland",
            at_house("Ljósagarður", "13B"),
        ),
        case(
            "Ljósagörðum 13B, 421 Skuggavík, Iceland",
            at_house("Ljósagarðar", "13B"),
        ),
        case(
            "Bjartaholt, 121 Postalvík, Iceland",
            at_house("Bjartaholt", ""),
        ),
        case(
            "Bjartaholti, 121 Postalvík, Iceland",
            at_house("Bjartaholt", ""),
        ),
        case("Sólhólum, 121 Ljósvík, Iceland", Want::Abstain),
        case("Dagskoti, 121 Ljósvík, Iceland", Want::Abstain),
        case("Ljósagörðum 13B, Iceland", Want::Abstain),
        case(
            "Óþekktaholt, 121 Ljósvík, Iceland",
            town("is-ljosvik", false),
        ),
        case(
            "Ljósagörðum 99, 121 Ljósvík, Iceland",
            town("is-ljosvik", false),
        ),
    ]
}

#[test]
fn an_official_capture_must_match_its_checksum_and_coordinate_order() {
    let root = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("iceland-invalid-{}", std::process::id()));
    super::write_release(&root.join("release"));
    let csv = root.join("iceland.csv");
    let sha = write(&csv);
    for swapped in [false, true] {
        let expected = if swapped {
            let text = std::fs::read_to_string(&csv)
                .unwrap()
                .replace(",64,-20.001,", ",-20.001,64,");
            assert!(text.contains(",-20.001,64,"));
            std::fs::write(&csv, &text).unwrap();
            opengeocoding::eval::sha256_hex(text.as_bytes())
        } else {
            assert_ne!(sha, "0".repeat(64));
            "0".repeat(64)
        };
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_build_index"))
            .arg(root.join("release"))
            .arg(root.join("index"))
            .arg("--iceland-addresses")
            .arg(&csv)
            .arg("--iceland-addresses-sha256")
            .arg(expected)
            .output()
            .unwrap();
        assert!(!result.status.success());
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(
            error.contains(if swapped {
                "Iceland coordinate outside WGS84 bounds"
            } else {
                "Iceland capture SHA-256 mismatch"
            }),
            "{error}"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

pub async fn check_safeguards(resolver: &opengeocoding::resolver::Resolver) {
    for query in [
        "Bjartaholt, 421 Postalvík, Iceland",
        "Bjartaholti, 121 Postalvík, Norway",
        "Bjartaholti, 121 Skuggavík, Iceland",
        "Ljósagörðum 13B, 421 Ljósvík, Iceland",
        "Ljósagörðum 13B, 121 Ljósvík, Norway",
    ] {
        let got = resolver.resolve(query).await.unwrap();
        assert!(!got.good_match, "{query}: {got:?}");
    }
}

#[test]
fn official_aliases_keep_each_local_streets_houses() {
    for (query, name) in [
        ("Ljósagörðum 13B, 121 Ljósvík, Iceland", "Ljósagarður"),
        ("Ljósagörðum 13B, 421 Skuggavík, Iceland", "Ljósagarðar"),
    ] {
        let found = index().suggest(query, 5, None, None, false);
        let first = &found[0];
        assert_eq!(first.kind, "address", "{query}");
        assert_eq!(
            index().suggested_house(first).unwrap()["address"]["house_number"],
            "13b",
            "{query}"
        );
        assert!(
            metres((first.x, first.y), house(street(name), "13B")) < 5.0,
            "{query}"
        );
    }
}
