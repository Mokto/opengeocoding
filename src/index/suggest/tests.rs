use super::street::comma_number;
#[test]
fn a_house_number_part_is_short_and_may_carry_its_mark() {
    for (part, number) in [
        ("12", "12"),
        ("12A", "12A"),
        ("nº 12", "12"),
        ("1234", "1234"),
    ] {
        assert_eq!(comma_number(part), Some(number), "{part}");
    }
    for other in ["12345", "2955-243", "12AB", "Lisboa", "", "1. th"] {
        assert_eq!(comma_number(other), None, "{other}");
    }
}
use super::*;

#[test]
fn typed_words_and_digraphs() {
    // a hyphen ends a word like a space: "saint" is whole and abbreviated, as indexed
    assert_eq!(words_open("saint-etien", true), ["st", "etien"]);
    assert_eq!(words_open("saint-etienne", false), ["st", "etienne"]);
    assert_eq!(words_open("calle", true), ["calle"]);
    for name in ["東京都", "上海市", "서울특별시", "İstanbul", "İzmir"] {
        assert_eq!(words_open(name, true), words(name), "{name}");
    }
    assert_eq!(umlaut_form("koeln").as_deref(), Some("koln"));
    assert_eq!(umlaut_form("muenchen").as_deref(), Some("munchen"));
    assert_eq!(umlaut_form("paris"), None);
}

#[test]
fn a_place_added_from_geonames_weighs_a_localitys_unknown_size() {
    let place = |class: &str, population| Place {
        subtype: "locality".into(),
        class: class.into(),
        cc: "vn".into(),
        region: "".into(),
        population,
        x: 0.0,
        y: 0.0,
        bbox: [f32::NAN; 4],
        parent: NONE,
        primary: "Huyện Lâm Hà".into(),
        names: "".into(),
        en: Place::english(""),
        keys: Box::default(),
        city_like: false,
        agglomeration: false,
        settlement_twin: false,
    };
    assert_eq!(weight(&place("geonames", 90_000)), 100);
    assert_eq!(weight(&place("", 90_000)), 90_000);
    assert_eq!(weight(&place("", 0)), 100);
}

const SEP: char = '\u{1f}';

/// Paris (France), Paris (Texas), London, a hamlet called Londo, a street with houses 12 and
/// 120, and a postcode.
fn tiny() -> (Index, std::path::PathBuf) {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "suggest-index-{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    // subtype, class, cc, region, _, population, x, y, bbox, parent, primary, names, item
    let place = |subtype: &str,
                 class: &str,
                 cc: &str,
                 region: &str,
                 pop: u32,
                 x: f64,
                 y: f64,
                 parent: &str,
                 primary: &str,
                 names: &str| {
        format!(
            "{subtype}\t{class}\t{cc}\t{region}\t\t{pop}\t{x}\t{y}\t0\t0\t0\t0\t{parent}\t{primary}\t{names}\t\n"
        )
    };
    let en = |n: &str| format!("en={n}");
    let places = [
        place(
            "country",
            "",
            "FR",
            "",
            68_000_000,
            2.0,
            46.0,
            "",
            "France",
            &en("France"),
        ),
        place(
            "region",
            "",
            "FR",
            "FR-IDF",
            12_000_000,
            2.5,
            48.7,
            "0",
            "Île-de-France",
            &en("Ile-de-France"),
        ),
        place(
            "locality",
            "city",
            "FR",
            "FR-IDF",
            2_100_000,
            2.35,
            48.85,
            "1",
            "Paris",
            &format!("{}{SEP}fr=Paris", en("Paris")),
        ),
        place(
            "country",
            "",
            "US",
            "",
            330_000_000,
            -100.0,
            40.0,
            "",
            "United States",
            &en("United States"),
        ),
        place(
            "region",
            "",
            "US",
            "US-TX",
            29_000_000,
            -99.0,
            31.0,
            "3",
            "Texas",
            &en("Texas"),
        ),
        place(
            "locality",
            "town",
            "US",
            "US-TX",
            25_000,
            -95.55,
            33.66,
            "4",
            "Paris",
            &en("Paris"),
        ),
        place(
            "country",
            "",
            "GB",
            "",
            67_000_000,
            -2.0,
            54.0,
            "",
            "United Kingdom",
            &en("United Kingdom"),
        ),
        place(
            "locality",
            "city",
            "GB",
            "GB-ENG",
            8_900_000,
            -0.12,
            51.5,
            "6",
            "London",
            &en("London"),
        ),
        place(
            "locality",
            "hamlet",
            "NE",
            "",
            0,
            1.1,
            14.0,
            "",
            "Londo",
            &en("Londo"),
        ),
        place(
            "country",
            "",
            "DK",
            "",
            6_000_000,
            10.0,
            56.0,
            "",
            "Denmark",
            &en("Denmark"),
        ),
        place(
            "locality",
            "city",
            "DK",
            "DK-84",
            600_000,
            12.57,
            55.68,
            "9",
            "Copenhagen",
            &en("Copenhagen"),
        ),
        place(
            "locality",
            "village",
            "DK",
            "DK-84",
            6_000,
            12.0,
            55.0,
            "9",
            "4",
            &en("4"),
        ),
        place(
            "country",
            "",
            "LU",
            "",
            660_000,
            6.1,
            49.6,
            "",
            "Luxembourg",
            &en("Luxembourg"),
        ),
        // a Belgian province: its name is the country's, so the country is not set from it
        place(
            "region",
            "",
            "BE",
            "BE-WLX",
            300_000,
            5.6,
            49.8,
            "",
            "Luxembourg",
            &en("Luxembourg"),
        ),
        place(
            "locality",
            "village",
            "LU",
            "",
            6_000,
            6.01,
            49.83,
            "12",
            "Mertzig",
            &en("Mertzig"),
        ),
    ];
    std::fs::write(dir.join("places.tsv"), places.concat()).unwrap();

    let (schema, f) = schema::schema();
    let index = tantivy::Index::create_in_dir(&dir, schema).unwrap();
    index.tokenizers().register("fold", schema::analyzer());
    let mut writer: tantivy::IndexWriter = index.writer_with_num_threads(1, 15_000_000).unwrap();
    for (i, p) in places.iter().enumerate() {
        let c: Vec<&str> = p.split('\t').collect();
        let names = format!(
            "{} {}",
            c[13],
            c[14]
                .replace(SEP, " ")
                .replace("en=", "")
                .replace("fr=", "")
        );
        writer
            .add_document(tantivy::doc!(
                f.kind => "p", f.names => names, f.country => c[2], f.ids => i.to_string(),
                f.name => c[13], f.id => i as u64, f.importance => c[5].parse::<u64>().unwrap(),
                f.x => c[6].parse::<f64>().unwrap(), f.y => c[7].parse::<f64>().unwrap()
            ))
            .unwrap();
    }
    // Rue de Rivoli crosses a cell edge: a second part (10) holds house 26. Street 11 is
    // named for a date.
    for (sid, name, x) in [
        (7u64, "Rue de Rivoli", 2.34f64),
        (10, "Rue de Rivoli", 2.36),
        (11, "11 Listopada", 2.35),
    ] {
        let mut street = tantivy::doc!(
            f.kind => "s", f.names => name, f.country => "FR", f.name => name,
            f.id => sid, f.place => 2u64, f.importance => 50u64, f.x => x, f.y => 48.86f64
        );
        // house 12 has no postcode of its own; house 13 carries this other code
        if sid == 7 {
            street.add_text(f.pcode, "11111");
        }
        for id in ["2", "1", "0"] {
            street.add_text(f.ids, id);
        }
        writer.add_document(street).unwrap();
    }
    for (sid, name, place) in [(8u64, "Examplegade", 10u64), (9, "Scopegade", 11)] {
        writer
            .add_document(tantivy::doc!(
                f.kind => "s", f.names => name, f.country => "DK", f.name => name,
                f.id => sid, f.place => place, f.ids => place.to_string(), f.ids => "9",
                f.importance => 50u64, f.x => 12.57f64, f.y => 55.68f64
            ))
            .unwrap();
    }
    writer
        .add_document(tantivy::doc!(
            f.kind => "z", f.pcode => "75001", f.name => "75001", f.country => "FR",
            f.place => 2u64, f.importance => 10u64, f.x => 2.336f64, f.y => 48.862f64
        ))
        .unwrap();
    // house 6 has no postcode; another house on the street carries 11111
    writer
        .add_document(tantivy::doc!(
            f.kind => "s", f.names => "Rue de Colmar-Berg", f.country => "LU",
            f.name => "Rue de Colmar-Berg", f.pcode => "11111",
            f.id => 12u64, f.place => 14u64, f.ids => "14", f.ids => "12",
            f.importance => 50u64, f.x => 6.01f64, f.y => 49.83f64
        ))
        .unwrap();
    writer
        .add_document(tantivy::doc!(
            f.kind => "z", f.pcode => "9169", f.name => "9169", f.country => "LU",
            f.place => 14u64, f.importance => 10u64, f.x => 6.01f64, f.y => 49.83f64
        ))
        .unwrap();
    writer.commit().unwrap();
    drop(writer);

    let mut fst = tantivy_fst::MapBuilder::memory();
    for (sid, numbers) in [
        (7u32, &["12", "120", "12a", "13"][..]),
        (8, &["12", "120", "12a"][..]),
        (9, &["12"][..]),
        (10, &["26"][..]),
        (11, &["26"][..]),
        (12, &["6"][..]),
    ] {
        for n in numbers {
            let mut key = sid.to_be_bytes().to_vec();
            key.extend_from_slice(n.as_bytes());
            let (x, y) = match sid {
                7 => (2.3 + n.len() as f64 / 100.0, 48.86),
                8 => (12.57, 55.68),
                12 => (6.01, 49.83),
                _ => (12.0, 55.0),
            };
            let slot = u8::from(sid == 7 && *n == "13");
            let (x, y) = schema::pack(x, y, slot);
            fst.insert(key, (x as u64) << 32 | y as u64).unwrap();
        }
    }
    std::fs::write(dir.join("numbers.shards"), "1").unwrap();
    std::fs::write(dir.join("numbers-00.fst"), fst.into_inner().unwrap()).unwrap();
    let index = Index::open(&dir, Arc::new(Parser::unloaded(0))).unwrap();
    (index, dir)
}

fn names(found: &[Suggestion]) -> Vec<&str> {
    found.iter().map(|s| s.display_name.as_str()).collect()
}

#[test]
fn retrieval_ids_survive_reopening_and_reject_invalid_references() {
    let (index, dir) = tiny();
    let reopened = Index::open(&dir, Arc::new(Parser::unloaded(0))).unwrap();
    let suggestion = index.suggest("paris", 1, None, None, false).remove(0);
    let found = reopened
        .retrieve_suggestion(&suggestion.retrieval_id)
        .unwrap();
    assert_eq!(found.retrieval_id, suggestion.retrieval_id);
    assert_eq!((found.x, found.y), (suggestion.x, suggestion.y));
    for key in [
        "",
        "p4294967295",
        "l4294967295",
        "s4294967295:0",
        "s0:4294967295",
        "z0:4294967295",
        "h0:0",
        "s0:0:extra",
        "💥",
        "h0:0:",
        "h0:0:unknown",
        "x0",
    ] {
        let id = format!("{}/{key}", index.suggestion_scope);
        assert!(index.retrieve_suggestion(&id).is_none(), "{key}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn completes_places_by_name_start_and_weight() {
    let (index, dir) = tiny();
    let cold = names(&index.suggest("par", 5, None, None, false)).join("|");
    index.warm_suggest();
    assert_eq!(
        names(&index.suggest("par", 5, None, None, false)).join("|"),
        cold
    );
    let found = index.suggest("par", 5, None, None, false);
    assert_eq!(found[0].id, "p2");
    assert_eq!(
        (found[0].kind, found[0].country_code.as_str()),
        ("city", "fr")
    );
    assert_eq!(found[1].id, "p5");
    assert_eq!(found[0].display_name, "Paris, Ile-de-France, France");
    // the whole word typed, and then the town's country or region
    assert_eq!(index.suggest("paris tex", 5, None, None, false)[0].id, "p5");
    assert_eq!(index.suggest("paris, tx", 5, None, None, false)[0].id, "p5");
    assert_eq!(index.suggest("paris, fr", 5, None, None, false)[0].id, "p2");
    assert_eq!(
        index.suggest("paris, unit", 5, None, None, false)[0].id,
        "p5"
    );
    // a hamlet named exactly as typed does not outrank the city the text starts
    assert_eq!(index.suggest("londo", 5, None, None, false)[0].id, "p7");
    assert_eq!(index.suggest("londo ", 5, None, None, false)[0].id, "p8");
    assert_eq!(
        names(&index.suggest("PARIS", 1, Some("us"), None, false)),
        ["Paris, Texas, United States"]
    );
    assert!(index.suggest("p", 5, None, None, false).is_empty());
    assert!(index.suggest("  ,, ", 5, None, None, false).is_empty());
    assert!(index.suggest("zzzzz", 5, None, None, false).is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_name_one_edit_off_finds_the_best_known_place() {
    let (index, dir) = tiny();
    assert_eq!(
        index.similar_place("Pariss", None).as_deref(),
        Some("Paris")
    );
    assert_eq!(
        index.similar_place("Parris", Some("us")).as_deref(),
        Some("Paris")
    );
    // the name itself, a different one, a short one and one in another country
    assert_eq!(index.similar_place("Paris", None), None);
    assert_eq!(index.similar_place("Lyonn", None), None);
    assert_eq!(index.similar_place("Pari", None), None);
    assert_eq!(index.similar_place("Pariss", Some("gb")), None);
    // "kingdom" is a key of United Kingdom, but not one of its names
    assert_eq!(index.similar_place("Kingdon", None), None);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn typos_words_inside_names_and_the_users_position() {
    let (index, dir) = tiny();
    let ids = |text: &str, focus: Option<&Focus>| -> Vec<String> {
        index
            .suggest(text, 5, None, focus, false)
            .into_iter()
            .map(|s| s.id)
            .collect()
    };
    // one transposition, deletion or substitution, while typing and once typed
    for typo in [
        "lodnon",
        "lndon",
        "londn",
        "lodnon, united kingdom",
        "lomdon ",
    ] {
        assert_eq!(ids(typo, None)[..1], ["p7"], "{typo}");
    }
    // but not the first letter, not two edits and not a short text
    for far in ["ondon", "lnodno", "pariz, france", "lndn"] {
        assert!(!ids(far, None).contains(&"p7".into()), "{far}");
    }
    assert_eq!(ids("pariz ", None)[..2], ["p2", "p5"]);
    // a name typed exactly comes before one a letter off
    assert_eq!(ids("londo ", None), ["p8", "p7"]);
    // a later word of a name, from its third letter
    assert_eq!(ids("kingd", None), ["p6"]);
    assert_eq!(ids("states", None), ["p3"]);
    assert!(index.name_candidates("ki", true, false).is_empty());
    // Paris, Texas near Dallas; Paris, France near Lyon or with no position
    let dallas = Focus {
        lat: 32.78,
        lon: -96.8,
        scale_km: 100.0,
    };
    let lyon = Focus {
        lat: 45.76,
        lon: 4.84,
        scale_km: 100.0,
    };
    assert_eq!(ids("paris", Some(&dallas))[..2], ["p5", "p2"]);
    assert_eq!(ids("pari", Some(&lyon))[..2], ["p2", "p5"]);
    assert_eq!(ids("paris", None)[..2], ["p2", "p5"]);
    // and as the town that scopes a street
    assert_eq!(ids("rue de rivoli, paris", Some(&dallas))[0], "s7");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn focus_boost_falls_with_distance() {
    let f = Focus {
        lat: 0.0,
        lon: 0.0,
        scale_km: 100.0,
    };
    assert_eq!(f.boost(0.0, 0.0), 1001.0);
    // a degree of longitude at the equator is 111.2 km
    let at = |km: f64| f.boost(0.0, km / 111.195);
    assert!((at(100.0) - 501.0).abs() < 1.0);
    assert!((at(1000.0) - 10.9).abs() < 0.1);
    assert!(f.boost(0.0, 180.0) < 1.03);
}

#[test]
fn a_comma_can_separate_the_house_number_from_the_road() {
    let (index, dir) = tiny();
    for (query, id) in [
        ("12, rue de rivoli", "h7:12"),
        (" 12 , rue de rivoli ", "h7:12"),
        ("12, rue de riv", "h7:12"),
        ("12, rue de rivoli, par", "h7:12"),
        ("12, rue de rivoli, paris, france", "h7:12"),
        ("12A, rue de rivoli, paris", "h7:12a"),
        ("120, rue de rivoli, paris", "h7:120"),
    ] {
        let found = index.suggest(query, 5, None, None, false);
        assert_eq!(found.len(), 1, "{query}");
        let s = &found[0];
        assert_eq!(s.id, id, "{query}");
        assert_eq!(s.kind, "address");
        assert_eq!(s.country_code, "fr");
    }
    for query in [
        "1, rue de rivoli",
        "99, rue de rivoli",
        "12, rue de rivoli, london",
        "12, rue de rivoli, paris, united states",
        "12, rue de rivoli, paris, texas",
        "12,",
        "12345678901, rue de rivoli",
        "75001, london",
    ] {
        assert!(
            index.suggest(query, 5, None, None, false).is_empty(),
            "{query}"
        );
    }
    assert!(
        index
            .suggest("12, rue de rivoli", 5, Some("us"), None, false)
            .is_empty()
    );
    assert!(
        index
            .suggest("12, rue de rivoli", 0, None, None, false)
            .is_empty()
    );
    assert_eq!(
        index.suggest("75001, paris", 1, None, None, false)[0].id,
        "zfr:75001"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn floor_fallback_uses_the_buildings_country_and_preserves_constraints() {
    let (index, dir) = tiny();
    let base = index
        .suggest("Examplegade 12, Copenhagen", 1, None, None, false)
        .remove(0);
    for suffix in ["4", "4.", "4. t", "4. th.", "ST. TV.", "kl.", "12. mf."] {
        for tail in ["", ", Copen", ", Copenhagen, Denmark"] {
            let query = format!("Examplegade 12, {suffix}{tail}");
            let found = index.suggest(&query, 5, None, None, false);
            assert_eq!(found.len(), 1, "{query}");
            let s = &found[0];
            assert_eq!((&s.id, s.x, s.y), (&base.id, base.x, base.y), "{query}");
            assert_eq!(s.unverified_suffix, suffix);
            assert_eq!(
                s.display_name,
                format!("12, Examplegade, {suffix}, Copenhagen, Denmark")
            );
            assert!(s.id.ends_with(":12"));
        }
    }
    let letter = index.suggest("Examplegade 12A, 4", 1, Some("DK"), None, false);
    assert_eq!(letter[0].id, "h8:12a");
    assert_eq!(letter[0].unverified_suffix, "4");
    for query in [
        "Examplegade 1, 4",
        "Examplegade 99, 4",
        "Examplegade, 4",
        "Examplegade 12, 1550",
        "Examplegade 12, 4, London",
        "Examplegade 12, 4, United States",
        "Examplegade 12, 4, 9999",
        "Examplegade 12, not a floor",
        "12 rue de rivoli, 4",
    ] {
        assert!(
            index.suggest(query, 5, None, None, false).is_empty(),
            "{query}"
        );
    }
    assert!(
        index
            .suggest("Examplegade 12, 4", 5, Some("fr"), None, false)
            .is_empty()
    );
    assert!(
        index
            .suggest("Examplegade 12, 4", 0, None, None, false)
            .is_empty()
    );
    let literal = index.suggest("Scopegade 12, 4,", 1, None, None, false);
    assert_eq!(literal[0].id, "h9:12");
    assert!(literal[0].unverified_suffix.is_empty());
    assert!(base.unverified_suffix.is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn completes_streets_houses_and_postcodes() {
    let (index, dir) = tiny();
    let street = &index.suggest("rue de riv", 5, None, None, false)[0];
    assert_eq!((street.id.as_str(), street.kind), ("s7", "street"));
    assert_eq!(
        street.display_name,
        "Rue de Rivoli, Paris, Ile-de-France, France"
    );
    // the town after a comma, still being typed
    assert!(
        index
            .suggest("rue de rivoli, par", 5, None, None, false)
            .iter()
            .any(|s| s.id == "s7")
    );
    let house = &index.suggest("12 rue de rivoli, par", 5, None, None, false)[0];
    assert_eq!((house.id.as_str(), house.kind), ("h7:12", "address"));
    assert!(house.display_name.starts_with("12, Rue de Rivoli, Paris"));
    // a house number typed last is being typed: 12 and 120
    let ids: Vec<_> = index
        .suggest("rue de rivoli 12", 5, None, None, false)
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(ids[..2], ["h7:12", "h7:120"]);
    // "ul." (ulica), which the index leaves out
    assert_eq!(
        index.suggest("ul. rue de rivoli 12, paris", 5, None, None, false)[0].id,
        "h7:12"
    );
    // a house on the street's other part
    assert_eq!(
        index.suggest("26 rue de rivoli, paris", 5, None, None, false)[0].id,
        "h10:26"
    );
    // the number last, after a road named for a date
    assert_eq!(
        index.suggest("11 listopada 26, paris", 5, None, None, false)[0].id,
        "h11:26"
    );
    // no such number: the street
    assert_eq!(
        index.suggest("99 rue de rivoli", 5, None, None, false)[0].id,
        "s7"
    );
    // no place by that name in the scope
    assert!(
        index
            .suggest("rue de rivoli, texas", 5, None, None, false)
            .is_empty()
    );
    let code = &index.suggest("7500", 5, None, None, false)[0];
    assert_eq!((code.id.as_str(), code.kind), ("zfr:75001", "postcode"));
    assert!(index.suggest("75", 5, None, None, false).is_empty());
    assert!(
        index
            .suggest("rue de riv", 5, Some("us"), None, false)
            .is_empty()
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_house_without_its_own_postcode_still_answers_the_town() {
    let (index, dir) = tiny();
    // 75001 is a known postcode and is not stored on the street. House 12 has no postcode;
    // house 13 stores a different one. The street's other code must not hide house 12.
    for query in [
        "12 rue de rivoli, paris, 75001, france",
        "rue de rivoli 12, paris, 75001, france",
        "12, rue de rivoli, paris, 75001, france",
    ] {
        let found = index.suggest(query, 5, None, None, false);
        assert_eq!(found.len(), 1, "{query}");
        assert_eq!(found[0].id, "h7:12", "{query}");
        assert_eq!(found[0].kind, "address");
        assert_eq!(found[0].country_code, "fr");
    }
    // the street stores no postcode at all (its other part): the same recovery
    let other = index.suggest(
        "26 rue de rivoli, paris, 75001, france",
        5,
        None,
        None,
        false,
    );
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].id, "h10:26");
    assert!(
        index
            .suggest(
                "13 rue de rivoli, paris, 75001, france",
                5,
                None,
                None,
                false
            )
            .is_empty()
    );
    // "Luxembourg" is also a Belgian province, so the country is not set from the last part.
    // 9169 is still that country's postcode. House 6 does not carry it; the street's other
    // code must not hide the house.
    for query in [
        "6 Rue de Colmar-Berg, Mertzig, 9169, Luxembourg",
        "Rue de Colmar-Berg 6, Mertzig, 9169, Luxembourg",
    ] {
        let found = index.suggest(query, 5, None, None, false);
        assert_eq!(found.len(), 1, "{query}");
        assert_eq!(found[0].id, "h12:6", "{query}");
        assert_eq!(found[0].kind, "address");
        assert_eq!(found[0].country_code, "lu");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
