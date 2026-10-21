use super::*;

#[test]
fn refuses_another_builders_index() {
    let dir = std::env::temp_dir().join(format!("builder-check-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    check_builder(&dir).unwrap();
    let write = |hash: &str| {
        std::fs::write(
            dir.join("build.json"),
            format!(r#"{{"builder_hash":"{hash}"}}"#),
        )
        .unwrap()
    };
    write(env!("BUILDER_HASH"));
    check_builder(&dir).unwrap();
    write("0000000000000000");
    assert!(check_builder(&dir).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_exact_road_alias_beats_extra_words() {
    let (_, f) = schema::schema();
    let doc = tantivy::doc!(
        f.name => "East Kōwhai Road",
        f.name => "Kōwhai Road"
    );
    for query in ["Kōwhai Road", "Kowhai Rd"] {
        assert_eq!(best_name(&doc, f.name, query), "Kōwhai Road");
    }
    assert_eq!(
        best_name(&doc, f.name, "East Kowhai Rd"),
        "East Kōwhai Road"
    );
}

#[test]
fn a_number_after_a_typed_road_is_turned_first() {
    for (q, turned, number) in [
        (
            "Church Street 4, Amberley, 7410, New Zealand",
            "4 Church Street, Amberley, 7410, New Zealand",
            "4",
        ),
        (
            "Guppy Road 8/233, Taradale, Napier, New Zealand",
            "8/233 Guppy Road, Taradale, Napier, New Zealand",
            "8/233",
        ),
        (
            "York Street 15E, Rotorua",
            "15E York Street, Rotorua",
            "15E",
        ),
    ] {
        assert_eq!(
            number_last(q),
            Some((turned.to_string(), number.to_string())),
            "{q}"
        );
    }
    // a road without its type, a number already first, no number, no place after it
    for q in [
        "Calle Centenario 505, Calvillo, Mexico",
        "4 Church Street, Amberley",
        "Church Street, Amberley",
        "Church Street 4",
    ] {
        assert_eq!(number_last(q), None, "{q}");
    }
}

#[test]
fn postcode_repair_preserves_requested_fields_for_corroboration() {
    let dir = std::env::temp_dir().join(format!("parse-postcode-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (schema, f) = schema::schema();
    let tantivy_index = tantivy::Index::create_in_dir(&dir, schema).unwrap();
    tantivy_index
        .tokenizers()
        .register("fold", schema::analyzer());
    let mut writer: tantivy::IndexWriter = tantivy_index
        .writer_with_num_threads(1, 15_000_000)
        .unwrap();
    writer
        .add_document(tantivy::doc!(
            f.kind => "z", f.pcode => "12345", f.pcode => "12345ST",
            f.country => "US", f.place => 0u64, f.importance => 1u64
        ))
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    std::fs::write(dir.join("numbers.shards"), "1").unwrap();
    std::fs::write(
        dir.join("numbers-00.fst"),
        tantivy_fst::MapBuilder::memory().into_inner().unwrap(),
    )
    .unwrap();
    std::fs::write(
        dir.join("places.tsv"),
        "country\t\tUS\t\t\t0\t0\t0\t0\t0\t0\t0\t\tUnited States\ten=United States\n",
    )
    .unwrap();
    let parser = Arc::new(Parser::unloaded(16));
    let pairs = |p: &[(&str, &str)]| -> Parsed {
        p.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    let street = pairs(&[
        ("house_number", "20"),
        ("road", "oak"),
        ("city", "springfeld"),
        ("country", "usa"),
    ]);
    let mut garbled = street.to_vec();
    garbled.insert(2, ("house_number".into(), "12345".into()));
    let house = pairs(&[("house_number", "12345"), ("road", "oak"), ("state", "ny")]);
    let protected = [
        (
            "20 Ash, 12345 Springfeld, USA",
            pairs(&[
                ("house_number", "20"),
                ("road", "ash"),
                ("city", "springfeld"),
                ("country", "usa"),
            ]),
        ),
        (
            "21 Oak, 12345 Springfeld, USA",
            pairs(&[
                ("house_number", "21"),
                ("road", "oak"),
                ("city", "springfeld"),
                ("country", "usa"),
            ]),
        ),
        (
            "12345 Oak, USA",
            pairs(&[
                ("house_number", "12345"),
                ("road", "oak"),
                ("country", "usa"),
            ]),
        ),
        (
            "20 Oak 12345, Springfeld, USA",
            pairs(&[
                ("house_number", "20"),
                ("road", "oak 12345"),
                ("city", "springfeld"),
                ("country", "usa"),
            ]),
        ),
        (
            "P.O. Box 12345, Springfeld, USA",
            pairs(&[
                ("po_box", "p.o. box 12345"),
                ("city", "springfeld"),
                ("country", "usa"),
            ]),
        ),
        (
            "20 Oak, 12345 St. Johns, USA",
            pairs(&[
                ("house_number", "20"),
                ("road", "oak"),
                ("postcode", "12345"),
                ("city", "st. johns"),
                ("country", "usa"),
            ]),
        ),
    ];
    let mut duplicate = street.to_vec();
    duplicate.insert(1, ("road".into(), "ash".into()));
    let mut cache = parser.cache.lock().unwrap();
    cache.extend([
        ("20 Oak, 12345 Springfeld, USA".into(), garbled.into()),
        (
            "20 Oak, Springfeld, USA".into(),
            street
                .iter()
                .filter(|(k, _)| k != "country")
                .cloned()
                .collect(),
        ),
        ("12345 Oak, NY".into(), house.clone()),
        ("Oak, USA".into(), street.clone()),
        ("P.O. Box, Springfeld, USA".into(), street.clone()),
        ("20 Oak, Johns, USA".into(), street.clone()),
        ("20 Ash, Springfeld, USA".into(), duplicate.into()),
        ("21 Oak, Springfeld, USA".into(), street.clone()),
    ]);
    cache.extend(protected.iter().map(|(q, p)| ((*q).into(), p.clone())));
    drop(cache);
    let index = Index::open(&dir, parser).unwrap();
    let mut expected = street.to_vec();
    expected.push(("postcode".into(), "12345".into()));
    assert_eq!(
        &*index
            .parse_address("20 Oak, 12345 Springfeld, USA", true)
            .unwrap()
            .unwrap(),
        expected
    );
    assert_eq!(
        index.parse_address("12345 Oak, NY", true).unwrap().unwrap(),
        house
    );
    assert_eq!(
        index
            .parse_address("12345 Oak, NY", false)
            .unwrap()
            .unwrap(),
        house
    );
    for (query, parsed) in protected {
        assert_eq!(
            index.parse_address(query, true).unwrap().unwrap(),
            parsed,
            "{query}"
        );
    }
    assert_eq!(
        &*index
            .parse_address("20 Oak 12345, Springfeld, USA", false)
            .unwrap()
            .unwrap(),
        expected
            .iter()
            .filter(|(k, _)| k != "country")
            .cloned()
            .collect::<Vec<_>>()
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Builds a tiny index through the builder's own number and reverse writers, then reads houses
/// back by distance: nearest first, only within the radius, across streets, with postcodes.
#[test]
fn native_roads_are_filtered_before_the_ranked_children_hide_a_house() {
    let dir = std::env::temp_dir().join(format!("native-roads-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (_, mut writer, f) = schema::create(dir.to_str().unwrap(), 1, 15_000_000);
    for sid in 0..32u64 {
        let name = if sid == 31 {
            "明光路3段".to_string()
        } else {
            format!("明光路3段{sid}巷")
        };
        writer
            .add_document(tantivy::doc!(
                f.kind => "s", f.names => name.clone(), f.name => name,
                f.country => "TW", f.id => sid, f.place => 0u64,
                f.importance => 32 - sid, f.x => 123.0, f.y => 25.0
            ))
            .unwrap();
    }
    writer.commit().unwrap();
    drop(writer);
    let (x, y) = schema::pack(123.0, 25.0, 0);
    let spill = schema::NumberSpill::new(dir.join(".numbers-spill"), 1);
    spill.add(vec![schema::Bucket {
        short: vec![schema::Num::new(31, "237", x, y).ok().unwrap()],
        long: vec![],
    }]);
    spill.write_fsts(dir.to_str().unwrap(), 1, &|sid, slot| (sid, slot));
    std::fs::write(
        dir.join("places.tsv"),
        "country\t\tTW\t\t\t0\t123\t25\t123\t25\t123\t25\t\t臺灣\ten=Taiwan\n",
    )
    .unwrap();
    let index = Index::open(&dir, Arc::new(Parser::unloaded(0))).unwrap();
    let found = index.streets("明光路三段", Some("237"), None, None, Some("tw"), false);
    assert!(matches!(
        found.as_slice(),
        [Hit::Street { sid: 31, number: Some(number), .. }] if number == "237"
    ));
    let missing = index.streets("明光路三段", Some("999"), None, None, Some("tw"), false);
    assert!(matches!(
        missing.as_slice(),
        [Hit::Street {
            sid: 31,
            number: None,
            ..
        }]
    ));
    drop(index);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reverse_finds_the_nearest_houses() {
    let dir = std::env::temp_dir().join(format!("reverse-houses-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (schema, f) = schema::schema();
    let tantivy_index = tantivy::Index::create_in_dir(&dir, schema).unwrap();
    tantivy_index
        .tokenizers()
        .register("fold", schema::analyzer());
    let mut writer: tantivy::IndexWriter = tantivy_index
        .writer_with_num_threads(1, 15_000_000)
        .unwrap();
    for (sid, name, pcode) in [
        (1000u64, "Oak Street", "12345"),
        (1001, "Elm Street", ""),
        (1003, "Oak Street", "67890"),
        (1002, "Oak Street", "67890"),
    ] {
        let mut d = tantivy::doc!(
            f.kind => "s", f.names => name.to_lowercase(), f.country => "US", f.id => sid,
            f.place => 0u64, f.importance => 1u64, f.x => 10.0, f.y => 50.0, f.name => name,
            f.city => if sid == 1000 { "Wrongtown" } else { "Righttown" }
        );
        if !pcode.is_empty() {
            d.add_text(f.pcode, pcode);
        }
        writer.add_document(d).unwrap();
    }
    writer.commit().unwrap();
    drop(writer);
    let at = |sid, number: &str, lon, lat, own| {
        let (x, y) = schema::pack(lon, lat, own);
        schema::Num::new(sid, number, x, y).ok().unwrap()
    };
    let spill = schema::NumberSpill::new(dir.join(".numbers-spill"), 1);
    spill.add(vec![schema::Bucket {
        short: vec![
            at(1000, "1", 10.0, 50.0, 1),
            at(1000, "2", 10.0002, 50.0, 0),
            at(1001, "7", 10.0003, 50.0004, 0),
            at(1001, "9", 10.05, 50.0, 0),
            at(1002, "1", 11.0, 50.0, 1),
            at(1003, "1", 12.0, 50.0, 1),
        ],
        long: vec![],
    }]);
    spill.write_fsts(dir.to_str().unwrap(), 1, &|sid, slot| (sid, slot));
    assert_eq!(
        reverse::build(&dir, &tantivy_index, f.kind, |_| None).unwrap(),
        (5, 0)
    );
    std::fs::write(
        dir.join("places.tsv"),
        "country\t\tUS\t\t\t0\t0\t0\t0\t0\t0\t0\t\tUnited States\ten=United States\n",
    )
    .unwrap();
    let index = Index::open(&dir, Arc::new(Parser::unloaded(0))).unwrap();
    for _ in 0..32 {
        let found = index.streets(
            "Oak Street",
            Some("1"),
            Some("Wrongtown"),
            Some("67890"),
            Some("US"),
            false,
        );
        assert!(matches!(found.as_slice(), [Hit::Street { sid: 1002, .. }]));
    }
    let find = |lat, lon, radius, size| {
        index
            .reverse_search(lat, lon, radius, size)
            .into_iter()
            .map(|(layer, v, m)| {
                let a = &v["address"];
                let number = a["house_number"].as_str().unwrap_or_default().to_string();
                (
                    layer,
                    a["road"].as_str().unwrap_or_default().to_string(),
                    number,
                    m,
                )
            })
            .collect::<Vec<_>>()
    };
    let hits = find(50.0, 10.00005, 500.0, 2);
    let named: Vec<_> = hits
        .iter()
        .map(|h| (h.0, h.1.as_str(), h.2.as_str()))
        .collect();
    assert_eq!(
        named,
        [
            ("address", "Oak Street", "1"),
            ("address", "Oak Street", "2")
        ]
    );
    assert!(hits[0].3 < 5.0 && hits[0].3 < hits[1].3, "{hits:?}");
    let postcode = index.reverse_search(50.0, 10.0, 500.0, 1)[0].1["address"]["postcode"].clone();
    assert_eq!(postcode, "12345");
    // 100 m takes in the first three houses of two streets, not the one 3 km away
    let numbers: Vec<_> = find(50.0, 10.0, 100.0, 10)
        .into_iter()
        .map(|h| h.2)
        .collect();
    assert_eq!(numbers, ["1", "2", "7"]);
    // beyond a ring of cells the nearest house is still found, from another street
    let far = find(50.0, 10.05, 500.0, 1);
    assert_eq!((far[0].1.as_str(), far[0].2.as_str()), ("Elm Street", "9"));
    // no house and no locality in range
    assert!(find(51.0, 11.0, 500.0, 1).is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Documents of each kind share terms; `top_of` must return only the asked kind and country,
/// without a `kind` clause, across segments and with the kinds interleaved in one.
#[test]
fn equally_important_hits_come_in_the_same_order_whatever_the_document_order() {
    let found = |name: &str, order: [u64; 4], commit_after: usize| {
        let dir = std::env::temp_dir().join(format!("ties-index-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (schema, f) = schema::schema();
        let tantivy_index = tantivy::Index::create_in_dir(&dir, schema).unwrap();
        tantivy_index
            .tokenizers()
            .register("fold", schema::analyzer());
        let mut writer: tantivy::IndexWriter = tantivy_index
            .writer_with_num_threads(1, 15_000_000)
            .unwrap();
        for (i, id) in order.into_iter().enumerate() {
            writer
                .add_document(tantivy::doc!(
                    f.kind => "s", f.names => "oak", f.country => "US", f.id => id,
                    f.place => 7u64, f.importance => 5u64, f.x => id as f64, f.y => 0.0
                ))
                .unwrap();
            if i == commit_after {
                writer.commit().unwrap();
            }
        }
        writer.commit().unwrap();
        drop(writer);
        std::fs::write(dir.join("numbers.shards"), "1").unwrap();
        std::fs::write(
            dir.join("numbers-00.fst"),
            tantivy_fst::MapBuilder::memory().into_inner().unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("places.tsv"),
            "country\t\tUS\t\t\t0\t0\t0\t0\t0\t0\t0\t\tUnited States\ten=United States\n",
        )
        .unwrap();
        let index = Index::open(&dir, Arc::new(Parser::unloaded(0))).unwrap();
        let names = index.musts(index.f.names, "oak");
        let top = |n| -> Vec<u64> {
            index
                .top_of(
                    Kind::Street,
                    None,
                    names.iter().map(|(o, q)| (*o, q.box_clone())).collect(),
                    n,
                )
                .into_iter()
                .map(|a| index.fast(a, "id"))
                .collect()
        };
        let all = (top(10), top(2));
        drop(index);
        std::fs::remove_dir_all(&dir).unwrap();
        all
    };
    let expected = (vec![1, 2, 3, 4], vec![1, 2]);
    assert_eq!(found("a", [3, 1, 4, 2], 1), expected);
    assert_eq!(found("b", [2, 4, 1, 3], 0), expected);
    assert_eq!(found("c", [1, 2, 3, 4], 2), expected);
}

#[test]
fn top_of_separates_kinds_and_countries() {
    let dir = std::env::temp_dir().join(format!("kinds-index-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (schema, f) = schema::schema();
    let tantivy_index = tantivy::Index::create_in_dir(&dir, schema).unwrap();
    tantivy_index
        .tokenizers()
        .register("fold", schema::analyzer());
    let mut writer: tantivy::IndexWriter = tantivy_index
        .writer_with_num_threads(1, 15_000_000)
        .unwrap();
    let docs = [
        tantivy::doc!(f.kind => "p", f.names => "oak", f.country => "US", f.id => 7u64, f.importance => 1u64),
        tantivy::doc!(f.kind => "s", f.names => "oak", f.country => "US", f.pcode => "12345", f.id => 8u64, f.place => 7u64, f.importance => 2u64),
        tantivy::doc!(f.kind => "s", f.names => "oak", f.country => "DK", f.id => 9u64, f.place => 7u64, f.importance => 3u64),
        tantivy::doc!(f.kind => "z", f.pcode => "12345", f.country => "US", f.place => 7u64, f.importance => 4u64),
        tantivy::doc!(f.kind => "s", f.names => "oak", f.country => "DK", f.id => 11u64, f.place => 7u64, f.importance => 1u64),
        tantivy::doc!(f.kind => "p", f.names => "oak", f.country => "DK", f.id => 10u64, f.importance => 5u64),
    ];
    for (i, doc) in docs.into_iter().enumerate() {
        writer.add_document(doc).unwrap();
        if i == 3 || i == 5 {
            writer.commit().unwrap();
        }
    }
    drop(writer);
    std::fs::write(dir.join("numbers.shards"), "1").unwrap();
    std::fs::write(
        dir.join("numbers-00.fst"),
        tantivy_fst::MapBuilder::memory().into_inner().unwrap(),
    )
    .unwrap();
    std::fs::write(
        dir.join("places.tsv"),
        "country\t\tUS\t\t\t0\t0\t0\t0\t0\t0\t0\t\tUnited States\ten=United States\n",
    )
    .unwrap();
    let index = Index::open(&dir, Arc::new(Parser::unloaded(0))).unwrap();
    let names = || index.musts(index.f.names, "oak");
    let ids = |hits: Vec<DocAddress>| {
        let mut ids: Vec<u64> = hits.into_iter().map(|a| index.fast(a, "id")).collect();
        ids.sort();
        ids
    };
    assert_eq!(index.columns.len(), 2);
    assert_eq!(ids(index.top_of(Kind::Place, None, names(), 10)), [7, 10]);
    assert_eq!(ids(index.top_of(Kind::Place, None, names(), 1)), [10]);
    assert_eq!(ids(index.top_of(Kind::Place, Some("us"), names(), 10)), [7]);
    assert_eq!(
        ids(index.top_of(Kind::Street, None, names(), 10)),
        [8, 9, 11]
    );
    assert_eq!(
        ids(index.top_of(Kind::Street, Some("us"), names(), 10)),
        [8]
    );
    assert_eq!(
        ids(index.top_of(Kind::Street, Some("dk"), names(), 10)),
        [9, 11]
    );
    assert_eq!(ids(index.top_of(Kind::Street, Some("dk"), names(), 1)), [9]);
    assert!(
        index
            .top_of(Kind::Street, Some("fr"), names(), 10)
            .is_empty()
    );
    assert_eq!(
        ids(index.top_of(Kind::Street, None, vec![], 10)),
        [8, 9, 11]
    );
    assert!(index.any_of(Kind::Street, Some("dk"), names()));
    assert!(!index.any_of(Kind::Place, Some("fr"), names()));
    assert!(!index.any_of(Kind::Postcode, None, names()));
    let twice =
        index.memoized(|| [0, 1].map(|_| ids(index.top_of(Kind::Street, Some("dk"), names(), 10))));
    assert_eq!(twice, [vec![9, 11], vec![9, 11]]);
    let pcode = || vec![must(term(index.f.pcode, "12345"))];
    let hits = index.top_of(Kind::Postcode, None, pcode(), 10);
    assert_eq!(hits.len(), 1);
    assert!(
        index.columns[hits[0].segment_ord as usize]
            .id
            .first(hits[0].doc_id)
            .is_none()
    );
    assert_eq!(ids(index.top_of(Kind::Street, None, names(), 1)), [9]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn english_name_is_found_among_the_pairs() {
    let at = |names: &str| match Place::english(names) {
        NONE => None,
        at => names[at as usize..].split(SEP).next().map(str::to_owned),
    };
    assert_eq!(at("en=London").as_deref(), Some("London"));
    assert_eq!(
        at("fr=Londres\u{1f}en=London\u{1f}de=London").as_deref(),
        Some("London")
    );
    assert_eq!(at("fr=Londres\u{1f}en_x=London"), None);
    assert_eq!(
        at("alt_name=en=x\u{1f}en=London").as_deref(),
        Some("London")
    );
    assert_eq!(at(""), None);
}

#[test]
fn reverse_returns_the_nearest_locality_within_50_km() {
    let dir = std::env::temp_dir().join(format!("reverse-index-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (schema, f) = schema::schema();
    let tantivy_index = tantivy::Index::create_in_dir(&dir, schema).unwrap();
    tantivy_index
        .tokenizers()
        .register("fold", schema::analyzer());
    let mut writer: tantivy::IndexWriter = tantivy_index
        .writer_with_num_threads(1, 15_000_000)
        .unwrap();
    // id, subtype, country, region, x, y, parent, name: a place's row number is its id
    let places = [
        (0, "country", "DK", "", 0.0, 0.0, "", "Denmark"),
        (1, "region", "DK", "DK-84", 0.0, 0.0, "0", "Hovedstaden"),
        (2, "locality", "DK", "", 12.5683, 55.6761, "1", "Copenhagen"),
        (3, "locality", "DK", "", 12.0803, 55.6415, "1", "Roskilde"),
        (4, "neighborhood", "DK", "", 12.55, 55.69, "2", "Norrebro"),
        (5, "region", "DK", "DK-82", 0.0, 0.0, "0", "Midtjylland"),
        (6, "locality", "DK", "", 10.2039, 56.1629, "5", "Aarhus"),
        (7, "locality", "DK", "", 12.569, 55.67, "2", "Small Hamlet"),
    ];
    let mut tsv = String::new();
    for (id, subtype, cc, region, x, y, parent, name) in places {
        let class = match (id, subtype) {
            (7, _) => "hamlet",
            (_, "locality") => "city",
            _ => "",
        };
        tsv += &format!(
            "{subtype}\t{class}\t{cc}\t{region}\t\t0\t{x}\t{y}\t0\t0\t0\t0\t{parent}\t{name}\t\n"
        );
        if x != 0.0 {
            writer
                .add_document(tantivy::doc!(
                    f.kind => "p", f.country => cc, f.id => id as u64, f.importance => 1u64,
                    f.x => x, f.y => y
                ))
                .unwrap();
        }
    }
    writer.commit().unwrap();
    drop(writer);
    std::fs::write(dir.join("numbers.shards"), "1").unwrap();
    std::fs::write(
        dir.join("numbers-00.fst"),
        tantivy_fst::MapBuilder::memory().into_inner().unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("places.tsv"), tsv).unwrap();
    let index = Index::open(&dir, Arc::new(Parser::unloaded(0))).unwrap();
    let mut house = Hit::Street {
        doc: DocAddress::new(0, 0),
        sid: 0,
        place: 2,
        name: "Test Road".into(),
        x: 12.5683,
        y: 55.6761,
        number: Some("1".into()),
        postcode: None,
        city: None,
        postal_town: None,
        cc: "DK".into(),
    };
    assert!(index.house_locality_named(&house, "Copenhagen"));
    for name in ["Hovedstaden", "Denmark", "Cop", "DK"] {
        assert!(!index.house_locality_named(&house, name), "{name}");
    }
    if let Hit::Street { number, .. } = &mut house {
        *number = None;
    }
    assert!(!index.house_locality_named(&house, "Copenhagen"));
    let city_and_region = |lat: f64, lon: f64| {
        let found = index.reverse(lat, lon);
        let address = &found["address"];
        (
            address["city"].as_str().map(String::from),
            address["ISO3166-2-lvl4"].as_str().map(String::from),
        )
    };
    let found = |city: &str, region: &str| (Some(city.to_string()), Some(region.to_string()));
    assert_eq!(
        city_and_region(55.6761, 12.5683),
        found("Copenhagen", "DK-84")
    );
    // a neighbourhood is never the answer; its town is
    assert_eq!(city_and_region(55.69, 12.55), found("Copenhagen", "DK-84"));
    assert_eq!(city_and_region(55.64, 12.09), found("Roskilde", "DK-84"));
    // no locality within 5 km: the nearest within 50
    assert_eq!(city_and_region(55.65, 12.3), found("Roskilde", "DK-84"));
    assert_eq!(city_and_region(56.1, 10.5), found("Aarhus", "DK-82"));
    assert_eq!(index.street_place(1, 55.6761, 12.5683), (2, None));
    assert_eq!(index.street_place(4, 55.6761, 12.5683), (4, None));
    assert_eq!(
        index.street_place(2, 55.6415, 12.0803),
        (2, Some("Roskilde".into()))
    );
    assert_eq!(index.street_place(2, 56.1629, 10.2039), (2, None));
    assert_eq!(index.street_place(2, 55.65, 12.3), (2, None));
    assert_eq!(index.street_place(2, 55.67, 12.569), (2, None));
    assert_eq!(index.street_place(NONE, 55.6761, 12.5683), (NONE, None));
    // beyond 50 km (Roskilde is 61 km away), or at sea
    for (lat, lon) in [(55.64, 11.1), (56.0, 3.0), (0.0, 0.0), (-90.0, 180.0)] {
        let found = index.reverse(lat, lon);
        assert_eq!(found["error"], "Unable to geocode", "{lat} {lon}: {found}");
        assert!(found.get("address").is_none());
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn opens_without_writing_a_lock() {
    let dir = std::env::temp_dir().join(format!("read-only-index-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let index = tantivy::Index::create_in_dir(&dir, schema::schema().0).unwrap();
    index
        .writer::<tantivy::TantivyDocument>(15_000_000)
        .unwrap()
        .commit()
        .unwrap();
    drop(index);
    let locks = || {
        std::fs::read_dir(&dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".lock")
            })
            .count()
    };
    for e in std::fs::read_dir(&dir).unwrap() {
        let path = e.unwrap().path();
        if path.to_string_lossy().ends_with(".lock") {
            std::fs::remove_file(path).unwrap();
        }
    }
    let mmap = tantivy::directory::MmapDirectory::open(&dir).unwrap();
    let reader = tantivy::Index::open(ReadOnly(mmap))
        .unwrap()
        .reader_builder()
        .reload_policy(tantivy::ReloadPolicy::Manual)
        .try_into()
        .unwrap();
    assert_eq!(reader.searcher().num_docs(), 0);
    assert_eq!(locks(), 0);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn reversed_addresses_are_turned_and_units_leave_the_road() {
    let pairs = |p: &[(&str, &str)]| -> Vec<(String, String)> {
        p.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    let country = |text: &str| -> Result<Vec<(String, String)>> {
        Ok(if text == "United States" {
            pairs(&[("country", "united states")])
        } else {
            pairs(&[("road", "x")])
        })
    };
    let q = "United States, WY 82636, Evansville, 498 Hat Six Road";
    let two_roads = pairs(&[
        ("road", "united states wy"),
        ("postcode", "82636"),
        ("road", "evansville"),
        ("house_number", "498"),
        ("road", "hat six road"),
    ]);
    assert_eq!(
        reversed(q, &two_roads, country).unwrap().as_deref(),
        Some("498 Hat Six Road, Evansville, WY 82636, United States")
    );
    let labelled = pairs(&[("country", "france"), ("postcode", "76160")]);
    assert!(
        reversed("France, 76160 Darnétal, 2 Rue X", &labelled, country)
            .unwrap()
            .is_some()
    );
    // repeated roads after a street, not a country, are left alone
    let q = "Nieuwe Bosscheweg 6, 5017 JM Tilburg, Netherlands";
    let repeats = pairs(&[("road", "nieuwe bosscheweg"), ("road", "jm")]);
    assert!(reversed(q, &repeats, country).unwrap().is_none());
    assert!(one_house(&pairs(&[("house_number", "1"), ("road", "a")])));
    assert!(!one_house(&two_roads));

    for (road, kept, unit) in [
        ("south fillmore street #b", "south fillmore street", "#b"),
        (
            "201st avenue souhtwest apt 10a",
            "201st avenue souhtwest",
            "apt 10a",
        ),
        ("main street suite 200", "main street", "suite 200"),
        ("rue ste foy", "rue ste foy", ""),
        ("avenue unit", "avenue unit", ""),
    ] {
        let mut parsed = pairs(&[("house_number", "1"), ("road", road)]);
        unit_from_road(&mut parsed);
        assert_eq!(field(&parsed, "road"), kept, "{road}");
        assert_eq!(field(&parsed, "unit"), unit, "{road}");
    }
    for (road, kept, number) in [
        ("中正路63巷 22號", "中正路63巷", "22號"),
        ("8之3號 中正路", "中正路", "8之3號"),
        ("中正路 22", "中正路 22", ""),
        ("route 66", "route 66", ""),
    ] {
        let mut parsed = pairs(&[("road", road)]);
        cn_tw::number_from_cjk_road(&mut parsed);
        assert_eq!(field(&parsed, "road"), kept, "{road}");
        assert_eq!(field(&parsed, "house_number"), number, "{road}");
    }
    let mut parsed = pairs(&[("house", "佳崙"), ("unit", "386號")]);
    cn_tw::number_from_cjk_road(&mut parsed);
    assert_eq!(field(&parsed, "house_number"), "386號");
}

#[test]
fn one_edit_apart() {
    for (a, b) in [
        ("srteet", "street"),
        ("stret", "street"),
        ("streeet", "street"),
        ("strete", "street"),
        ("street", "street"),
        ("snta", "santa"),
    ] {
        assert!(one_edit(a, b), "{a} {b}");
    }
    for (a, b) in [
        ("sretet", "street"),
        ("str", "street"),
        ("avenue", "street"),
        ("tsreet", "streets"),
    ] {
        assert!(!one_edit(a, b), "{a} {b}");
    }
    assert_eq!(misspelt_kind("southewst"), ["se", "sw"]);
    assert_eq!(misspelt_kind("srteet"), ["st"]);
    assert!(misspelt_kind("sw").is_empty());
    assert!(postcode_not_contradicted(Some("04103"), Some("04103")));
    assert!(postcode_not_contradicted(None, Some("04103")));
    assert!(postcode_not_contradicted(Some("04101"), None));
    assert!(!postcode_not_contradicted(Some("04101"), Some("04103")));
    use tantivy_fst::Automaton;
    let run = |word: &str, input: &str| {
        let a = OneEdit(word.as_bytes());
        let state = input.bytes().fold(a.start(), |s, c| a.accept(&s, c));
        a.is_match(&state)
    };
    for word in ["street", "santa", "jacinthes", "trailwood"] {
        for input in [
            "street",
            "srteet",
            "stret",
            "strete",
            "streeet",
            "sretet",
            "snta",
            "santa",
            "jaicnthes",
            "tarilwood",
            "trailwod",
            "avenue",
            "tsreet",
            "atreet",
        ] {
            let same_start = input.as_bytes().first() == word.as_bytes().first();
            assert_eq!(
                run(word, input),
                same_start && one_edit(input, word),
                "{word} {input}"
            );
        }
    }
}

#[test]
fn admin_words_and_number_prefixes() {
    assert_eq!(without_admin_word("Vas County").as_deref(), Some("Vas"));
    assert_eq!(
        without_admin_word("Departamento de Santa Cruz").as_deref(),
        Some("Santa Cruz")
    );
    assert_eq!(
        without_admin_word("Kreisfreie Stadt München").as_deref(),
        Some("München")
    );
    assert_eq!(without_admin_word("County"), None);
    assert_eq!(
        without_admin_word("Maplewood Township").as_deref(),
        Some("Maplewood")
    );
    assert_eq!(
        without_admin_word("City and Borough of Juneau").as_deref(),
        Some("Juneau")
    );
    assert_eq!(without_admin_word("Leinster"), None);
    assert_eq!(house_number("No:5"), "5");
    assert_eq!(without_number_mark("köprüsü sk. no"), "köprüsü sk.");
    assert_eq!(without_number_mark("nord"), "nord");
    assert_eq!(house_number("nº 1824"), "1824");
    assert_eq!(house_number("№ 43"), "43");
    assert_eq!(house_number("12b"), "12b");
    assert_eq!(house_number("Nordweg"), "Nordweg");
}

#[test]
fn spaced_house_suffixes_and_postcode_city_parts() {
    assert_eq!(compact_house_suffix("7 bis").as_deref(), Some("7bis"));
    assert_eq!(compact_house_suffix("40 C").as_deref(), Some("40C"));
    assert_eq!(compact_house_suffix("63 P2").as_deref(), Some("63P2"));
    for n in [
        "7bis",
        "12 34",
        "12 bis apt 4",
        "12/3 A",
        "7 rue",
        "12 avenue",
        "20 route",
        "5 apt",
        "10 north",
    ] {
        assert!(compact_house_suffix(n).is_none(), "{n}");
    }
    let head = vec![
        ("house_number".into(), "28".into()),
        ("road".into(), "rue des cygnes".into()),
    ];
    let country = ("country".into(), "france".into());
    let wrong = vec![
        ("house_number".into(), "89116".into()),
        ("road".into(), "belle-rive".into()),
        ("city".into(), "vallee".into()),
    ];
    let aligned =
        postal_city_aligned(&head, "89116 Belle-Rive la Vallée", &country, &wrong).unwrap();
    assert_eq!(field(&aligned, "house_number"), "28");
    assert_eq!(field(&aligned, "postcode"), "89116");
    assert_eq!(field(&aligned, "city"), "belle-rive la vallée");
    let mut duplicate = head.clone();
    duplicate.extend(wrong.clone());
    duplicate.push(country.clone());
    let unseparated = postal_without_commas(
        "28 rue des cygnes 89116 Belle-Rive la Vallée France",
        &duplicate,
        |q| {
            assert_eq!(q, "28 rue des cygnes");
            Ok(head.clone())
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(field(&unseparated, "house_number"), "28");
    assert_eq!(field(&unseparated, "postcode"), "89116");
    assert_eq!(field(&unseparated, "city"), "belle-rive la vallée");
    let dutch = postal_city_aligned(
        &head,
        "6571 AD Berg en Dal",
        &("country".into(), "netherlands".into()),
        &wrong,
    )
    .unwrap();
    assert_eq!(field(&dutch, "postcode"), "6571 ad");
    assert_eq!(field(&dutch, "city"), "berg en dal");
    assert!(
        postal_city_aligned(
            &head,
            "9406 KX Assen",
            &("country".into(), "belgium".into()),
            &wrong
        )
        .is_none()
    );
    assert!(postal_city_aligned(&head, "8911 Belle-Rive", &country, &wrong).is_none());
    assert!(postal_city_aligned(&head, "89116 Belle-Rive 2", &country, &wrong).is_none());
    assert!(
        postal_city_aligned(
            &head,
            "89116 Belle-Rive",
            &country,
            &[("state".into(), "bourgogne".into())]
        )
        .is_none()
    );
}

#[test]
fn postcode_suffix_country_label_needs_an_intact_dutch_code() {
    let head = vec![
        ("house_number".into(), "4a".into()),
        ("road".into(), "meadowln. - east".into()),
    ];
    let country = ("country".into(), "netherlands".into());
    let mut whole = head.clone();
    whole.extend([
        ("postcode".into(), "1234".into()),
        ("country".into(), "tj".into()),
        ("city".into(), "polderdam".into()),
        country.clone(),
    ]);
    assert!(postcode_country_suffix("1234 TJ Polderdam", &whole));
    let aligned = postal_city_aligned(&head, "1234 TJ Polderdam", &country, &whole).unwrap();
    for key in ["house_number", "road"] {
        assert_eq!(field(&aligned, key), field(&whole, key));
    }
    assert_eq!(field(&aligned, "postcode"), "1234 tj");
    assert_eq!(field(&aligned, "city"), "polderdam");
    for (slot, value) in [(0, "4b"), (1, "other road")] {
        let mut changed = head.clone();
        changed[slot].1 = value.into();
        assert!(postal_city_aligned(&changed, "1234 TJ Polderdam", &country, &whole).is_none());
    }
    for tail in [
        "1234 TJ",
        "1234, TJ Polderdam",
        "5678 TJ Polderdam",
        "12345 TJ Polderdam",
        "1234 Tajikistan Polderdam",
    ] {
        assert!(!postcode_country_suffix(tail, &whole), "{tail}");
    }
    for suffix in ["中国", "ΤЈ", "T-J", "TJ.", "T"] {
        whole[3].1 = suffix.to_lowercase();
        assert!(
            !postcode_country_suffix(&format!("1234 {suffix} Polderdam"), &whole),
            "{suffix}"
        );
    }
    whole[3].1 = "tj".into();
    for cc in ["belgium", "germany", "norway", "france"] {
        assert!(
            postal_city_aligned(
                &head,
                "1234 TJ Polderdam",
                &("country".into(), cc.into()),
                &whole
            )
            .is_none(),
            "{cc}"
        );
    }
    whole.pop();
    assert!(!postcode_country_suffix("1234 TJ Polderdam", &whole));
    whole.push(country);
    for key in [
        "country",
        "house_number",
        "road",
        "postcode",
        "country_region",
    ] {
        whole.push((key.into(), "germany".into()));
        assert!(!postcode_country_suffix("1234 TJ Polderdam", &whole));
        whole.pop();
    }
    whole[3].1 = "tajikistan".into();
    assert!(!postcode_country_suffix("1234 TJ Polderdam", &whole));
}

#[test]
fn names_and_codes() {
    assert_eq!(cjk_stem("東京都"), Some("東京"));
    assert_eq!(cjk_stem("서울특별시"), Some("서울"));
    assert_eq!(cjk_stem("Kyoto"), None);
    assert_eq!(exact_key("上海市"), exact_key("上海"));
    assert_eq!(ie::irish_province("IE-D"), Some("IE-L"));
    assert_eq!(ie::irish_province("GB-ENG"), None);
    assert_eq!(title("københavn v"), "København V");
}

#[test]
fn an_administrative_settlement_twin_needs_the_same_source_identity_and_geography() {
    let rows = [
        ("locality", "GB", "GB-WLS", "Q999900001", false),
        ("region", "GB", "GB-WLS", "Q999900001", true),
        ("county", "GB", "GB-WLS", "Q999900002", false),
        ("region", "FR", "GB-WLS", "Q999900001", false),
        ("region", "GB", "GB-ENG", "Q999900001", false),
        ("region", "GB", "GB-WLS", "", false),
        ("locality", "GB", "GB-WLS", "", false),
        ("county", "GB", "GB-WLS", "Q999900001", true),
    ];
    let text: String = rows
        .iter()
        .map(|(kind, cc, region, item, _)| {
            format!("{kind}\t\t{cc}\t{region}\t0\t100\t1\t2\t1\t2\t1\t2\t\tTwinport\t\t{item}\n")
        })
        .collect();
    let path = std::env::temp_dir().join(format!("settlement-twins-{}.tsv", std::process::id()));
    std::fs::write(&path, text).unwrap();
    let (places, _, _) = load_places(&path, &HashMap::new()).unwrap();
    std::fs::remove_file(path).unwrap();
    for (place, row) in places.iter().zip(rows) {
        assert_eq!(place.settlement_twin, row.4, "{row:?}");
    }
}

#[test]
fn subdivisions_are_named_by_the_item_in_the_last_column() {
    let reference: Value = serde_json::from_str(include_str!("../../data/reference.json")).unwrap();
    let iso = iso_by_item(&reference, "iso3166_2");
    let listed: usize = reference["iso3166_2"]
        .as_object()
        .unwrap()
        .values()
        .map(|items| items.as_array().unwrap().len())
        .sum();
    assert_eq!(iso.len(), listed, "an item names two codes");
    for (item, code) in &iso {
        assert!(
            item.strip_prefix('Q')
                .is_some_and(|n| n.parse::<u64>().is_ok())
        );
        assert!(
            code.split_once('-')
                .is_some_and(|(cc, rest)| cc.len() == 2 && !rest.is_empty())
        );
    }
    let path = std::env::temp_dir().join(format!("places-test-{}.tsv", std::process::id()));
    let (item, code) = iso.iter().next().unwrap();
    // 15 columns, then the item; a place with no names still has its names column
    std::fs::write(
        &path,
        format!(
            "county\t\tUG\tUG-C\t2\t0\t1\t2\t1\t2\t1\t2\t\tKampala\t\t{item}\n\
                 county\t\tUG\tUG-C\t2\t0\t1\t2\t1\t2\t1\t2\t\tMpigi\ten=Mpigi\tQ0\n"
        ),
    )
    .unwrap();
    let (places, own, _) = load_places(&path, &iso).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(places.len(), 2);
    assert_eq!(own, [(0, code.to_string())]);
}
