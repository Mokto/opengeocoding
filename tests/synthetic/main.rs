//! The service against the made-up world of `world.rs`, built by the real `build_index`. Needs no
//! download. Expectations are the facts authored in `world.rs`, never this service's answers.
//!
//! `cargo test` runs the checks that read no libpostal models (suggestions, reverse lookups).
//! `cargo test --test synthetic -- --ignored` also resolves addresses, which needs the models.
mod world;

use opengeocoding::{
    index::{Index, Parser},
    resolver::Resolver,
};
use std::{
    path::PathBuf,
    process::Command,
    sync::{Arc, OnceLock},
};
use world::{FEATURES, PLACES, STREETS, house, road_end};

fn dir() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("synthetic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        world::write_release(&root.join("release"));
        world::write_osm(&root.join("osm"));
        world::write_geonames(&root.join("cities.txt"));
        let out = root.join("index");
        let built = Command::new(env!("CARGO_BIN_EXE_build_index"))
            .arg(root.join("release"))
            .arg(&out)
            .arg("--osm")
            .arg(root.join("osm"))
            .arg("--geonames-places")
            .arg(root.join("cities.txt"))
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "build_index failed:\n{}",
            String::from_utf8_lossy(&built.stderr)
        );
        out
    })
}

fn index() -> &'static Index {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX.get_or_init(|| Index::open(dir(), Arc::new(Parser::unloaded(0))).unwrap())
}

fn place(id: &str) -> &'static world::Place {
    PLACES.iter().find(|p| p.id == id).unwrap()
}

fn street(name: &str) -> &'static world::Street {
    STREETS.iter().find(|s| s.name == name).unwrap()
}

fn metres(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0) * a.1.to_radians().cos()).hypot(a.1 - b.1) * 111_195.0
}

#[test]
fn poi_houses_need_a_known_road_and_never_override_addresses_or_move_streets() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("synthetic-poi-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    world::write_release(&root.join("release"));
    world::write_poi_addresses(&root.join("release"));
    let out = root.join("index");
    let built = Command::new(env!("CARGO_BIN_EXE_build_index"))
        .arg(root.join("release"))
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let licences = std::fs::read_to_string(out.join("PLACES-LICENSES.txt")).unwrap();
    assert!(licences.contains("Copyright 2024 Foursquare Labs, Inc."));
    assert!(licences.contains("Community Data License Agreement"));
    assert!(licences.contains("Apache License"));
    let poi = Index::open(&out, Arc::new(Parser::unloaded(0))).unwrap();
    let at = |text: &str| poi.suggest(text, 5, Some("GB"), None, false);
    let added = at("High Street 999, Beacon");
    assert_eq!((added.len(), added[0].kind), (1, "address"));
    assert!(metres((added[0].x, added[0].y), (30.011, 10.0)) < 5.);
    let original = at("High Street 1, Beacon");
    assert!(
        metres(
            (original[0].x, original[0].y),
            house(street("High Street"), "1")
        ) < 5.
    );
    for query in [
        "Missing Road",
        "High Street 997, Beacon",
        "High Street 12, Beacon",
    ] {
        assert!(!at(query).iter().any(|s| s.kind == "address"), "{query}");
    }
    let market = at("Market Street, Beacon");
    assert_eq!(market[0].kind, "street");
    assert!(metres((market[0].x, market[0].y), (30.012, 10.0)) < 5.);
    let market_house = at("Market Street 888, Beacon");
    assert_eq!(market_house[0].kind, "address");
    assert!(metres((market_house[0].x, market_house[0].y), (30.013, 10.0)) < 5.);
    assert!(
        !poi.suggest("Main Street 994, Paris", 5, Some("US"), None, false)
            .iter()
            .any(|s| s.kind == "address")
    );
    drop(poi);
    world::cover_address_countries(&root.join("release"));
    let covered = root.join("covered-index");
    let built = Command::new(env!("CARGO_BIN_EXE_build_index"))
        .arg(root.join("release"))
        .arg(&covered)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let covered = Index::open(&covered, Arc::new(Parser::unloaded(0))).unwrap();
    assert!(
        !covered
            .suggest("High Street 999, Beacon", 5, Some("GB"), None, false)
            .iter()
            .any(|s| s.kind == "address")
    );
    drop(covered);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn geonames_primary_names_survive_deduplication_without_exporting_other_labels() {
    for text in [
        "Северная Роща, Alpha",
        "Severnaya Roshcha, Alpha",
        "Severnaya Roshcha, Première",
        "Северная Роща, Francia, Austria",
    ] {
        let found = index().suggest(text, 5, Some("AT"), None, false);
        assert!(!found.is_empty(), "{text}");
        assert!(
            metres((found[0].x, found[0].y), (30.0, 0.21)) < 5.,
            "{text}"
        );
    }
    assert!(
        index()
            .suggest("Untrusted Alias", 5, None, None, false)
            .is_empty()
    );
    assert!(
        index()
            .suggest("Северная Роща, Francia", 5, None, None, false)
            .is_empty()
    );
    let bare = index().suggest("Severnaya Roshcha", 5, Some("AT"), None, false);
    assert!(metres((bare[0].x, bare[0].y), (80.0, -10.0)) < 5.);
}

#[test]
fn place_slugs_require_a_known_subdivision_and_an_exact_settlement_name() {
    for (query, id) in [
        ("us-tx-paris", "us-tx-paris"),
        ("FR-IDF-PARIS", "fr-paris"),
        ("at-1-severnaya-roshcha", "at-nordhain"),
        ("at-2-severnaya-roshcha", "at-northgrove"),
        ("at-1-северная-роща", "at-nordhain"),
    ] {
        let hits = index().place_slug(query).unwrap();
        let rendered = index().render(&hits[0], true);
        let want = place(id);
        assert!(
            metres(
                (
                    rendered["lon"].as_str().unwrap().parse().unwrap(),
                    rendered["lat"].as_str().unwrap().parse().unwrap()
                ),
                (want.lon, want.lat)
            ) < 5.,
            "{query}"
        );
    }
    for query in [
        "us-ga-paris",
        "us-tx-severnaya-roshcha",
        "de-nw-paris",
        "de-tx-paris",
        "us-zz-paris",
        "us-tx-missing",
        "at-1-roshcha",
    ] {
        assert!(index().place_slug(query).unwrap().is_empty(), "{query}");
    }
    for query in [
        "Paris",
        "Paris, France",
        "us-tx-",
        "us-tx--paris",
        "us-tx-paris, france",
        "102-main-street",
    ] {
        assert!(index().place_slug(query).is_none(), "{query}");
    }
}

#[test]
fn an_unreviewed_place_cannot_acquire_a_historical_qualifier() {
    for query in [
        "Paris, Former Coast, France",
        "Severnaya Roshcha, Former Coast, Austria",
        "Paris, Ile-de-France, France",
        "Paris, Former Coast, United States",
        "2 Rue de Rivoli, Former Coast, France",
        "Paris, Former Coast, France, Austria",
        "Paris, Former Coast",
        "Paris",
    ] {
        assert!(index().historical_place(query).is_none(), "{query}");
    }
}

#[test]
fn a_typed_place_name_lists_every_place_by_size_and_a_country_narrows_it() {
    let paris = |cc| {
        index()
            .suggest("Par", 5, cc, None, false)
            .iter()
            .map(|s| (s.country_code.clone(), s.kind))
            .collect::<Vec<_>>()
    };
    assert_eq!(paris(None), [("fr".into(), "city"), ("us".into(), "city")]);
    assert_eq!(paris(Some("US")), [("us".into(), "city")]);
    assert!(paris(Some("DE")).is_empty());
}

#[test]
fn suggestions_find_houses_postcodes_and_names_in_other_languages() {
    let at = |text: &str, cc| index().suggest(text, 5, cc, None, false);
    let rivoli = at("Rue de Rivoli 2", None);
    assert_eq!((rivoli.len(), rivoli[0].kind), (1, "address"));
    let want = house(street("Rue de Rivoli"), "2");
    assert!(metres((rivoli[0].x, rivoli[0].y), want) < 5.0);
    for typed in [
        "2 Rue de Rivoli, 75001 Paris, France",
        "Rue de Rivoli 2, Paris 75001, France",
        "2 Rue de Rivoli, 75001 Par",
    ] {
        let found = at(typed, None);
        assert!(!found.is_empty(), "{typed}");
        assert_eq!(found[0].kind, "address", "{typed}");
        assert!(metres((found[0].x, found[0].y), want) < 5.0, "{typed}");
    }
    for typed in [
        "2 Rue de Rivoli, 75460 Paris, France",
        "2 Rue de Rivoli, 75001 Paris, United States",
        "1 Long Road, 75460 Paris, United States",
    ] {
        assert!(
            at(typed, None).iter().all(|s| s.kind != "address"),
            "{typed}"
        );
    }
    let own_postcode = at("1 Long Road, 75462 Paris, United States", None);
    assert!(!own_postcode.is_empty());
    assert_eq!(own_postcode[0].kind, "address");
    assert!(
        metres(
            (own_postcode[0].x, own_postcode[0].y),
            house(street("Long Road"), "1")
        ) < 5.0
    );
    assert!(at("2 Rue de Rivoli, 75001 Paris, France", Some("US")).is_empty());
    for typed in [
        "7 bis Rue de Rivoli, 75001 Paris, France",
        "Rue de Rivoli 7 bis, 75001 Paris, France",
    ] {
        let found = at(typed, None);
        assert!(!found.is_empty(), "{typed}");
        assert_eq!(found[0].kind, "address", "{typed}");
        assert!(
            metres(
                (found[0].x, found[0].y),
                house(street("Rue de Rivoli"), "7bis")
            ) < 5.0,
            "{typed}"
        );
    }
    let found = at("14 Meadow Road, Bayport, IL", None);
    assert!(!found.is_empty());
    assert_eq!(found[0].kind, "address");
    assert_eq!(found[0].country_code, "us");
    assert!(metres((found[0].x, found[0].y), house(street("Meadow Road"), "14")) < 5.0);
    // the stated country excludes the French street
    assert!(at("Rue de Rivoli 2", Some("US")).is_empty());
    assert_eq!(at("75460", None)[0].kind, "postcode");
    // Cologne's English name and the digraph spelling of its German one
    for typed in ["Colo", "Koeln"] {
        let found = at(typed, None);
        assert_eq!(found[0].country_code, "de", "{typed}");
    }
    // Delhi, the capital of India, comes before the town in Ontario
    let delhi: Vec<_> = at("Delh", None)
        .iter()
        .map(|s| (s.country_code.clone(), s.kind))
        .collect();
    assert_eq!(delhi[0], ("in".into(), "city"));
    assert!(delhi.contains(&("ca".into(), "town")));
}

#[test]
fn joined_city_and_region_suggestions_keep_both_constraints() {
    let index = index();
    let want = house(street("Birch Completion Road"), "12");
    let base = index.suggest(
        "12 Birch Completion Road, Delhi, ON N4B 1A1, Canada",
        1,
        None,
        None,
        false,
    );
    assert_eq!(base[0].kind, "address");
    for (query, cc) in [
        ("12 Birch Completion Road, Delhi ON N4B 1A1, Canada", None),
        ("Birch Completion Road 12, Delhi ON N4B1A1, Canada", None),
        ("12 Birch Completion Road, Delhi ON, Canada", None),
        ("12 Birch Completion Road, Delhi ON N4B 1A1", Some("CA")),
        ("12 Birch Completion Road, Delhi ON N4B 1A1", None),
    ] {
        let found = index.suggest(query, 1, cc, None, false);
        assert_eq!(found.len(), 1, "{query}");
        assert_eq!(found[0].kind, "address", "{query}");
        assert_eq!(found[0].id, base[0].id, "{query}");
        assert_eq!(found[0].retrieval_id, base[0].retrieval_id);
        assert!(metres((found[0].x, found[0].y), want) < 2., "{query}");
        let retrieved = index.retrieve_suggestion(&found[0].retrieval_id).unwrap();
        assert_eq!((retrieved.x, retrieved.y), (found[0].x, found[0].y));
    }
    let suffix = index.suggest(
        "12A Birch Completion Road, Delhi ON N4B 1A1, Canada",
        1,
        None,
        None,
        false,
    );
    assert_eq!(suffix[0].kind, "address");
    assert!(suffix[0].id.ends_with(":12a"));
    assert!(
        metres(
            (suffix[0].x, suffix[0].y),
            house(street("Birch Completion Road"), "12a")
        ) < 2.
    );
    for (query, cc) in [
        ("12B Birch Completion Road, Delhi ON N4B 1A1, Canada", None),
        ("12 Birch Completion Road, Delhi QC N4B 1A1, Canada", None),
        ("12 Birch Completion Road, Delhi ON G1A 0A2, Canada", None),
        ("12 Birch Completion Road, Delhi ON N4B 1A1, France", None),
        (
            "12 Birch Completion Road, Delhi ON N4B 1A1, Canada",
            Some("US"),
        ),
        ("12 Birch Completion Road, Unknown ON N4B 1A1, Canada", None),
        ("999 Birch Completion Road, Delhi ON N4B 1A1, Canada", None),
        ("12 Missing Road, Delhi ON N4B 1A1, Canada", None),
        (
            "12 Birch Completion Road, Delhi ON, Quebec, N4B 1A1, Canada",
            None,
        ),
    ] {
        assert!(
            !index
                .suggest(query, 5, cc, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
    let query = "12 Birch Completion Road, Harbour ON, Canada";
    let found = index.suggest(query, 1, None, None, false);
    let literal = STREETS.iter().find(|s| s.city == "Harbour ON").unwrap();
    assert_eq!(found[0].kind, "address", "{query}");
    assert!(metres((found[0].x, found[0].y), house(literal, "12")) < 2.);
    assert!(
        index
            .suggest(
                "12 Birch Completion Road, Delhi ON, Canada",
                0,
                None,
                None,
                false
            )
            .is_empty()
    );
}

#[test]
fn a_full_address_ending_in_a_country_a_region_name_contains_is_suggested() {
    // "North Denmark Region" contains "Denmark" without being named so
    for (typed, road, number) in [
        ("Skibsgade 9, 8500 Havnholm, Denmark", "Skibsgade", "9"),
        (
            "Skibsgade 9, st. th, 8500 Havnholm, Denmark",
            "Skibsgade",
            "9",
        ),
        ("Kajgade 5, 8600 Postholm, Denmark", "Kajgade", "5"),
    ] {
        let want = house(street(road), number);
        let found = index().suggest(typed, 5, None, None, false);
        assert!(
            found
                .iter()
                .any(|s| s.kind == "address" && metres((s.x, s.y), want) < 5.0),
            "{typed}"
        );
    }
}

#[test]
fn native_and_numeric_place_names_work_while_the_last_word_is_unfinished() {
    for (text, id) in [
        ("桜森市", "gb-beacon"),
        ("松林市", "gb-beacon"),
        ("솔빛도시", "gb-beacon"),
        ("İncikent", "gb-beacon"),
        ("6th of Bramble", "gb-bramble"),
        ("Sixth of Bramble", "gb-bramble"),
        ("Bramble6", "gb-bramble"),
    ] {
        let expected = place(id);
        for query in [text.to_string(), format!("{text} "), format!("{text}, UK")] {
            let found = index().suggest(&query, 5, None, None, false);
            assert!(!found.is_empty(), "{query}");
            assert_eq!(
                (found[0].kind, found[0].country_code.as_str()),
                ("town", "gb"),
                "{query}"
            );
            assert!(
                metres((found[0].x, found[0].y), (expected.lon, expected.lat)) < 5.,
                "{query}"
            );
        }
    }
}

#[test]
fn a_postcodes_place_and_country_qualifiers_corroborate_it() {
    for (text, road) in [
        ("75001, Paris", "Rue de Rivoli"),
        ("75001, Par", "Rue de Rivoli"),
        ("75001, FR", "Rue de Rivoli"),
        ("75001, Paris, France", "Rue de Rivoli"),
        ("AA1 1AA, Beacon", "High Street"),
        ("AA1 1AA, UK", "High Street"),
    ] {
        let found = index().suggest(text, 5, None, None, false);
        assert!(!found.is_empty(), "{text}");
        assert_eq!(found[0].kind, "postcode", "{text}");
        assert_eq!(
            found[0].country_code,
            street(road).cc.to_lowercase(),
            "{text}"
        );
        assert!(
            metres((found[0].x, found[0].y), house(street(road), "1")) < 200.,
            "{text}"
        );
    }
    for text in [
        "75001, London",
        "75001, Paris, United States",
        "75001, Paris, France, Texas",
        "AA1 1AA, France",
    ] {
        assert!(
            index().suggest(text, 5, None, None, false).is_empty(),
            "{text}"
        );
    }
    assert!(
        index()
            .suggest("75001, Paris, France", 5, Some("US"), None, false)
            .is_empty()
    );
    for text in [
        "2 Rue de Rivoli, Paris, 75001",
        "2 Rue de Rivoli, Paris, 75001, France",
    ] {
        let found = index().suggest(text, 5, None, None, false);
        assert!(!found.is_empty(), "{text}");
        assert_eq!(found[0].kind, "address", "{text}");
        assert!(
            metres(
                (found[0].x, found[0].y),
                house(street("Rue de Rivoli"), "2")
            ) < 5.,
            "{text}"
        );
    }
}

#[test]
fn postal_towns_need_not_be_geographic_places_and_region_names_need_not_be_countries() {
    for (typed, road, number) in [
        (
            "15 Township Lane, 60001 Postalton, United States",
            "Township Lane",
            "15",
        ),
        ("15 Township Lane, 60001 Posta", "Township Lane", "15"),
        ("16 Cedar Road, Arbor, Georgia", "Cedar Road", "16"),
    ] {
        let found = index().suggest(typed, 5, None, None, false);
        assert!(!found.is_empty(), "{typed}");
        assert_eq!(found[0].kind, "address", "{typed}");
        assert_eq!(found[0].country_code, "us", "{typed}");
        assert!(
            metres((found[0].x, found[0].y), house(street(road), number)) < 5.0,
            "{typed}"
        );
    }
    for typed in [
        "15 Township Lane, 60001 Elsewhere, United States",
        "15 Township Lane, 60001 Postalton, France",
    ] {
        assert!(
            index()
                .suggest(typed, 5, None, None, false)
                .iter()
                .all(|s| s.kind != "address"),
            "{typed}"
        );
    }
}

#[test]
fn retrieves_exact_suggestions_without_a_search_or_session() {
    let index = index();
    let mut kinds = std::collections::BTreeSet::new();
    for query in [
        "Paris",
        "Rue de Rivoli",
        "2 Rue de Rivoli, Paris",
        "75001",
        "Paris Museum",
    ] {
        let suggestions = index.suggest(query, 15, None, None, true);
        assert!(!suggestions.is_empty(), "{query}");
        for suggestion in suggestions {
            let found = index.retrieve_suggestion(&suggestion.retrieval_id).unwrap();
            assert_eq!(found.id, suggestion.id);
            assert_eq!(found.display_name, suggestion.display_name);
            assert_eq!(
                (found.x, found.y, found.bbox),
                (suggestion.x, suggestion.y, suggestion.bbox)
            );
            assert_eq!(found.retrieval_id, suggestion.retrieval_id);
            kinds.insert(found.kind);
            let stale = format!(
                "stale/{}",
                suggestion.retrieval_id.split_once('/').unwrap().1
            );
            assert!(index.retrieve_suggestion(&stale).is_none());
        }
    }
    for kind in ["city", "street", "address", "postcode", "landmark"] {
        assert!(kinds.contains(kind), "missing {kind}: {kinds:?}");
    }
    for id in ["", "p1", "stale/p1", "💥"] {
        assert!(index.retrieve_suggestion(id).is_none());
    }
}

#[test]
fn landmark_flag_does_not_contaminate_default_suggestions() {
    let signature = |text, landmarks| {
        index()
            .suggest(text, 5, None, None, landmarks)
            .into_iter()
            .map(|s| {
                (
                    s.id,
                    s.display_name,
                    s.kind,
                    s.category,
                    s.country_code,
                    s.x,
                    s.y,
                    s.bbox,
                )
            })
            .collect::<Vec<_>>()
    };
    for text in ["PNV", "Paris", "Paris Gare", "Thingsmuseum"] {
        let before = signature(text, false);
        let on = signature(text, true);
        assert!(before.iter().all(|s| s.2 != "landmark" && s.3.is_empty()));
        assert!(on.iter().any(|s| s.2 == "landmark"));
        assert_eq!(before, signature(text, false));
        let towns: Vec<_> = before
            .iter()
            .filter(|s| s.2 == "city" || s.2 == "village")
            .collect();
        assert_eq!(towns, on.iter().take(towns.len()).collect::<Vec<_>>());
    }
}

#[test]
fn landmarks_come_only_when_asked_and_after_the_towns_of_their_name() {
    let at = |text: &str, cc, landmarks| index().suggest(text, 5, cc, None, landmarks);
    let landmarks = |text: &str, cc| -> Vec<_> {
        at(text, cc, true)
            .into_iter()
            .filter(|s| s.kind == "landmark")
            .collect()
    };
    for text in ["Paris", "PNV", "Paris Gare", "Paris Museum"] {
        assert!(at(text, None, false).iter().all(|s| s.kind != "landmark"));
    }
    let paris: Vec<_> = at("Paris", None, true)
        .iter()
        .map(|s| (s.country_code.clone(), s.kind, s.category))
        .collect();
    assert_eq!(
        paris,
        [
            ("fr".into(), "city", ""),
            ("us".into(), "city", ""),
            ("fr".into(), "landmark", "airport"),
            ("fr".into(), "landmark", "station"),
            ("fr".into(), "landmark", "museum"),
        ]
    );
    let kinds = |text: &str| -> Vec<_> {
        at(text, None, true)
            .iter()
            .map(|s| (s.kind, s.category))
            .collect()
    };
    assert_eq!(
        kinds("Thingsmuseum"),
        [("village", ""), ("landmark", "museum")]
    );
    assert_eq!(
        kinds("Nordville"),
        [
            ("village", ""),
            ("landmark", "airport"),
            ("landmark", "station")
        ]
    );
    assert_eq!(
        kinds("Twin Hall"),
        [("landmark", "station"), ("landmark", "museum")]
    );
    let focus = opengeocoding::index::Focus {
        lat: 33.66,
        lon: -95.56,
        scale_km: 100.0,
    };
    assert_eq!(
        index().suggest("Paris", 5, None, Some(&focus), true)[0].country_code,
        "us"
    );
    let airport = &FEATURES[0];
    let pnv = &at("PNV", None, true)[0];
    assert_eq!((pnv.kind, pnv.category), ("landmark", "airport"));
    assert_eq!(
        pnv.display_name,
        "Paris Nordville Airport, Paris, Ile-de-France, France"
    );
    // a way's and a relation's points are their nodes' mean
    assert!(metres((pnv.x, pnv.y), (airport.lon, airport.lat)) < 5.0);
    let museum = &landmarks("Paris Museum", None)[0];
    // a whole word starts a longer name too
    assert_eq!(landmarks("Paris Museum ", None)[0].id, museum.id);
    assert!(metres((museum.x, museum.y), (FEATURES[2].lon, FEATURES[2].lat)) < 5.0);
    // from a later word of its name
    assert_eq!(landmarks("gare de l'ouest", None)[0].category, "station");
    assert_eq!(landmarks("Paris Gare, France", None)[0].category, "station");
    assert_eq!(landmarks("Paris Gare, Paris", None)[0].category, "station");
    assert!(landmarks("Paris Gare, Texas", None).is_empty());
    assert!(landmarks("PNV", Some("US")).is_empty());
    // no Wikidata item; not a landmark's tag
    assert!(landmarks("Paris Unlisted", None).is_empty());
    assert!(landmarks("Paris Bakery", None).is_empty());
}

#[test]
fn reverse_names_the_nearest_locality_and_nothing_in_the_ocean() {
    let koeln = place("de-koeln");
    let found = index().reverse(koeln.lat + 0.01, koeln.lon);
    assert_eq!(found["address"]["country_code"], "de");
    // rendered in English
    assert_eq!(found["address"]["city"], "Cologne");
    assert_eq!(index().reverse(0.0, 0.0)["error"], "Unable to geocode");
}

#[test]
fn reverse_search_returns_the_nearest_houses_then_falls_back_to_the_locality() {
    let s = street("Main Street");
    let near = house(s, "102");
    let found = index().reverse_search(near.1, near.0, 100.0, 3);
    let mut numbers: Vec<_> = found
        .iter()
        .map(|(layer, v, _)| (*layer, v["address"]["house_number"].as_str().unwrap_or("")))
        .collect();
    // 100 and 104 stand the same distance away, so only the nearest is ordered
    assert_eq!(numbers[0], ("address", "102"));
    // house points are stored to a metre or so
    assert!(found[0].2 < 5.0, "{}", found[0].2);
    numbers.sort();
    assert_eq!(
        numbers,
        [("address", "100"), ("address", "102"), ("address", "104")]
    );

    let paris = place("fr-paris");
    let far = index().reverse_search(paris.lat + 0.02, paris.lon, 100.0, 3);
    assert_eq!(far.len(), 1);
    assert_eq!(far[0].0, "locality");
    assert_eq!(far[0].1["address"]["city"], "Paris");
}

/// A street's segment runs along its latitude from `lon` to `road_end`.
fn to_road(s: &world::Street, lon: f64, lat: f64) -> f64 {
    metres((lon, lat), (lon.clamp(s.lon, road_end(s)), s.lat))
}

fn coordinates(v: &serde_json::Value) -> (f64, f64) {
    let at = |k: &str| v[k].as_str().unwrap().parse::<f64>().unwrap();
    (at("lon"), at("lat"))
}

#[test]
fn reverse_search_names_the_nearest_street_when_no_house_is_in_range() {
    // 30 m north of the middle of a street that has no houses
    let s = street("Feldweg");
    let (lon, lat) = ((s.lon + road_end(s)) / 2.0, s.lat + 0.00027);
    let found = index().reverse_search(lat, lon, 500.0, 3);
    assert_eq!(found.len(), 1, "{found:?}");
    let (layer, v, m) = &found[0];
    assert_eq!(*layer, "street");
    assert_eq!(v["address"]["road"], "Feldweg");
    // Cologne's polygon holds the street, and the village point 330 m away lies in Cologne
    assert_eq!(v["address"]["village"], "Feldheim");
    assert_eq!(v["address"]["city"], "Cologne");
    assert!(v["address"].get("house_number").is_none());
    assert!(v["address"].get("postcode").is_none());
    // edges are stored to a metre or two
    assert!((m - to_road(s, lon, lat)).abs() < 3.0, "{m}");
    // at the street's point nearest the point
    assert!(metres(coordinates(v), (lon, s.lat)) < 3.0, "{v}");
    // a radius short of the street leaves the locality
    let short = index().reverse_search(lat, lon, 20.0, 1);
    assert_eq!(short[0].0, "locality");

    // 2.6 km from the houses of a long street, 33 m south of its line: the street, with the
    // postcode of its houses
    let s = street("Long Road");
    let (lon, lat) = (road_end(s) - 0.002, s.lat - 0.0003);
    let found = index().reverse_search(lat, lon, 500.0, 1);
    let (layer, v, m) = &found[0];
    assert_eq!(
        (*layer, &v["address"]["road"]),
        ("street", &"Long Road".into())
    );
    assert_eq!(v["address"]["postcode"], "75462");
    assert!((m - to_road(s, lon, lat)).abs() < 3.0, "{m}");
    // Paris's polygon holds the street; Reno, a city point beside it in the same state, names it
    assert_eq!(v["address"]["city"], "Reno");
    assert_eq!(v["address"]["suburb"], "Paris");
    // by its houses, the houses answer, as they did before streets did
    let (hx, hy) = house(s, "3");
    let found = index().reverse_search(hy + 0.0003, hx, 500.0, 1);
    assert_eq!(
        (found[0].0, found[0].1["address"]["house_number"].as_str()),
        ("address", Some("3"))
    );
    // 1.1 km from the street: the nearest locality
    let found = index().reverse_search(s.lat + 0.01, lon, 500.0, 1);
    assert_eq!(found[0].0, "locality");
    assert_eq!(found[0].1["address"]["city"], "Reno");

    // beside a path, with no street in range: the locality, not the path
    let p = street("Uferpfad");
    let found = index().reverse_search(p.lat + 0.0002, p.lon + 0.005, 500.0, 1);
    assert_eq!(found[0].0, "locality");
}

#[test]
fn reverse_streets_do_not_cross_the_containing_country_or_region() {
    for (lon, layer, cc, region) in [
        (60.499, "locality", "at", Some("AT-3")),
        (60.501, "street", "at", Some("AT-4")),
        (61.498, "street", "at", Some("AT-4")),
        (61.5005, "locality", "li", None),
    ] {
        let found = index().reverse_search(0.0, lon, 500.0, 3);
        assert!(!found.is_empty(), "{lon}");
        for (got_layer, v, _) in &found {
            assert_eq!(*got_layer, layer, "{lon}: {v}");
            assert_eq!(v["address"]["country_code"], cc, "{lon}: {v}");
            assert_eq!(
                v["address"]["ISO3166-2-lvl4"].as_str(),
                region,
                "{lon}: {v}"
            );
        }
    }
}

#[test]
fn resolve_region_prefers_a_house_then_a_region_coded_locality_and_not_a_faraway_guess() {
    let region = |lat: f64, lon: f64| {
        index().reverse_region(lat, lon)["address"]["ISO3166-2-lvl4"]
            .as_str()
            .map(String::from)
    };
    let (x, y) = house(street("Grenzgasse"), "1");
    // the nearest locality point (Nordhain) is across the line from the house's town
    assert_eq!(index().reverse(y, x)["address"]["village"], "Nordhain");
    assert_eq!(region(y, x).as_deref(), Some("AT-2"));
    // past the house's 100 m the locality answers
    assert_eq!(region(y + 0.002, x).as_deref(), Some("AT-1"));
    // Ohnecode's parents stop at the country, so its neighbour Mitcode names the region
    assert_eq!(index().reverse(0.0, 40.0)["address"]["village"], "Ohnecode");
    assert_eq!(region(0.0, 40.0).as_deref(), Some("AT-1"));
    // 51 km from the nearest locality, another region's 274 km away: it answers; between two
    // regions' localities it does not
    assert_eq!(region(-20.46, 100.0).as_deref(), Some("AT-2"));
    assert_eq!(region(-19.0, 100.0), None);
    // and no farther than 150 km
    assert_eq!(region(-23.0, 100.0), None);
}

#[test]
fn resolve_region_answers_the_polygon_that_holds_the_point() {
    let region = |lat: f64, lon: f64| {
        index().reverse_region(lat, lon)["address"]["ISO3166-2-lvl4"]
            .as_str()
            .map(String::from)
    };
    // Grenzdorf, across the border in Delta, is the nearest town
    let grenzdorf = place("at-grenzdorf");
    assert_eq!(region(0.0, 60.45).as_deref(), Some("AT-3"));
    assert_eq!(region(0.0, 60.55).as_deref(), Some("AT-4"));
    assert_eq!(region(0.0, grenzdorf.lon).as_deref(), Some("AT-4"));
    // and ReverseGeocode names a town of the point's own region, though farther
    assert_eq!(
        index().reverse(0.0, 60.45)["address"]["village"],
        "Gammadorf"
    );
    assert_eq!(
        index().reverse(0.0, 60.55)["address"]["village"],
        "Grenzdorf"
    );
    // offshore, in the territorial sea only
    assert_eq!(region(0.0, 59.55).as_deref(), Some("AT-3"));
    // Liechtenstein has no regions: Delta's Randdorf is nearer than Vaduz, but is in Austria
    assert_eq!(region(0.0, 61.52), None);
    assert_eq!(index().reverse(0.0, 61.52)["address"]["village"], "Vaduz");
}

#[test]
fn a_nearer_foreign_settlement_cannot_take_a_border_house() {
    let found = index().suggest("Uferweg 7, Eastufer", 5, Some("AT"), None, false);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].country_code, "at");
    assert_eq!(found[0].kind, "address");
    let (x, y) = house(street("Uferweg"), "7");
    let reverse = index().reverse_search(y, x, 100.0, 1);
    assert_eq!(reverse[0].1["address"]["country_code"], "at");
    assert_eq!(reverse[0].1["address"]["ISO3166-2-lvl4"], "AT-5");
}

#[test]
fn a_place_region_parent_agrees_with_its_containing_region() {
    let found = index().suggest("Falkenstadt", 5, Some("AT"), None, false);
    assert_eq!(found.len(), 1);
    assert!(
        found[0].display_name.contains("Epsilon"),
        "{}",
        found[0].display_name
    );
    let p = place("at-falkenstadt");
    assert_eq!(
        index().reverse_region(p.lat, p.lon)["address"]["ISO3166-2-lvl4"],
        "AT-5"
    );
}

#[test]
fn an_explicit_region_parent_beats_an_inconsistent_child_region_column() {
    let city = place("us-il-bayport");
    assert_eq!(
        index().reverse(city.lat, city.lon)["address"]["ISO3166-2-lvl4"],
        "US-IL"
    );
}

#[test]
fn a_county_cannot_override_the_containing_region_of_a_house() {
    let parent = index()
        .region_places()
        .find(|(code, _)| *code == "AT-4")
        .unwrap()
        .1;
    let hit = |number| opengeocoding::index::Hit::Street {
        doc: tantivy::DocAddress::new(0, 0),
        sid: 0,
        place: parent,
        name: "Border Road".into(),
        x: 60.45,
        y: 0.0,
        number,
        postcode: None,
        city: None,
        postal_town: None,
        cc: "at".into(),
    };
    let house = index().render(&hit(Some("1".into())), true);
    assert_eq!(house["address"]["ISO3166-2-lvl4"], "AT-3");
    let road = index().render(&hit(None), true);
    assert_eq!(road["address"]["ISO3166-2-lvl4"], "AT-4");
}

#[test]
fn a_county_with_an_iso_code_adds_it_below_its_region_and_is_listed() {
    let colmar = place("fr-colmar");
    let address = &index().reverse(colmar.lat, colmar.lon)["address"];
    assert_eq!(address["ISO3166-2-lvl4"], "FR-IDF");
    assert_eq!(address["ISO3166-2-lvl6"], "FR-68");
    // a place outside the county has no second code
    let paris = place("fr-paris");
    let address = &index().reverse(paris.lat, paris.lon)["address"];
    assert_eq!(address["ISO3166-2-lvl4"], "FR-IDF");
    assert!(address.get("ISO3166-2-lvl6").is_none());
    let county = index()
        .region_places()
        .find(|(code, _)| *code == "FR-68")
        .unwrap()
        .1;
    assert_eq!(index().parent_region_code(county), Some("FR-IDF"));
    // the code names the county, as a search for it does
    let found = index().search("fr, FR-68");
    assert_eq!(
        index().render(&found[0], true)["address"]["county"],
        "Haut-Rhin"
    );
    assert_eq!(
        index()
            .region_places()
            .filter(|(c, _)| c.starts_with("FR-"))
            .count(),
        2
    );
}

#[test]
fn an_irish_county_names_its_province_which_is_listed_and_found_by_its_code() {
    let dublin = place("ie-dublin");
    let address = &index().reverse(dublin.lat, dublin.lon)["address"];
    assert_eq!(address["ISO3166-2-lvl4"], "IE-D");
    assert_eq!(address["ISO3166-2-lvl5"], "IE-L");
    let mut listed: Vec<_> = index()
        .region_places()
        .map(|(code, _)| code)
        .filter(|code| code.starts_with("IE-"))
        .collect();
    listed.sort();
    // no county of the other provinces is in this world
    assert_eq!(listed, ["IE-D", "IE-L"]);
    let found = index().search("ie, IE-L");
    let province = &index().render(&found[0], true)["address"];
    assert_eq!(province["state"], "Leinster");
    assert_eq!(province["ISO3166-2-lvl5"], "IE-L");
    assert!(province.get("ISO3166-2-lvl4").is_none());
    let (_, place) = index().region_places().find(|(c, _)| *c == "IE-L").unwrap();
    assert_eq!(index().details(place.into())["admin_level"], 5);
}

enum Want {
    Abstain,
    /// A point, within some metres, and whether the match is a good one.
    At((f64, f64), f64, bool),
}

struct Case {
    query: &'static str,
    want: Want,
    /// Why the case is known to fail; it must keep failing until the label is removed.
    known_failure: &'static str,
}

fn cases() -> Vec<Case> {
    let town = |id: &str, good| {
        let p = place(id);
        Want::At((p.lon, p.lat), 1000.0, good)
    };
    let at_house = |name: &str, n: &str| Want::At(house(street(name), n), 25.0, true);
    let case = |query, want| Case {
        query,
        want,
        known_failure: "",
    };
    vec![
        case(
            "7 Acacia Straat, 9991 Veldhaven, South Africa",
            at_house("Acacia Street", "7"),
        ),
        case(
            "8 Literal Straat, 9991 Veldhaven, South Africa",
            at_house("Literal Straat", "8"),
        ),
        case(
            "25 Ulstr., 50667 Köln, Germany",
            at_house("Ulstrasse", "25"),
        ),
        case(
            "24 Birkenstarsse, 50667 Köln, Germany",
            at_house("Birkenstrasse", "24"),
        ),
        case(
            "24 Birkensrasse, 50667 Köln, Germany",
            at_house("Birkenstrasse", "24"),
        ),
        case("Paris, France", town("fr-paris", true)),
        case("Missing Road, Paris, France", town("fr-paris", false)),
        case("Île-de-France, Haut-Rhin", town("fr-haut-rhin", true)),
        case(
            "2 R. de Rivoli, 75001 Paris, France",
            at_house("Rue de Rivoli", "2"),
        ),
        case(
            "7 bis R. de Rivoli, 75001 Paris, France",
            at_house("Rue de Rivoli", "7bis"),
        ),
        case(
            "Granveien 5B, 1234 Fjordvik, Norway",
            at_house("Granveien", "5b"),
        ),
        case(
            "Granvn. 5B, 1234 Fjordvik, Norway",
            at_house("Granveien", "5b"),
        ),
        case("Granvn. 5B, 1234, Norway", at_house("Granveien", "5b")),
        case(
            "Bjørkvegen 12d, 1234 Fjordvik, Norway",
            at_house("Bjørkvegen", "12d"),
        ),
        case(
            "Bjørkvn. 12d, 1234 Fjordvik, Norway",
            at_house("Bjørkvegen", "12d"),
        ),
        case(
            "Willowlaan 4a, 1234 AB Polderdam, Netherlands",
            at_house("Willowlaan", "4a"),
        ),
        case(
            "Willowln. 4a, 1234 AB Polderdam, Netherlands",
            at_house("Willowlaan", "4a"),
        ),
        case("Furuvn. 5, 1234 Fjordvik, Norway", at_house("Furuvn.", "5")),
        case(
            "Birchveien - North 5B, 1234 Fjordvik, Norway",
            at_house("Birchveien - North", "5b"),
        ),
        case(
            "Birchvn. - North 5B, 1234 Fjordvik, Norway",
            at_house("Birchveien - North", "5b"),
        ),
        case(
            "Pinevn. - North 5, 1234 Fjordvik, Norway",
            at_house("Pinevn. - North", "5"),
        ),
        case(
            "Meadowlaan 4a, 1234 TJ Polderdam, Netherlands",
            at_house("Meadowlaan", "4a"),
        ),
        case(
            "Meadowln. 4a, 1234 TJ Polderdam, Netherlands",
            at_house("Meadowlaan", "4a"),
        ),
        // a Danish floor and door beside the house still resolve the building
        case(
            "Skibsgade 9, 8500 Havnholm, Denmark",
            at_house("Skibsgade", "9"),
        ),
        case(
            "Skibsgade 9, st. th, 8500 Havnholm, Denmark",
            at_house("Skibsgade", "9"),
        ),
        case(
            "Skibsgade 9, 1. tv, 8500 Havnholm, Denmark",
            at_house("Skibsgade", "9"),
        ),
        case(
            "Skibsgade 9, 2. 3, 8500 Havnholm, Denmark",
            at_house("Skibsgade", "9"),
        ),
        case("Świtały 12, 76-231, Poland", at_house("Świtały", "12")),
        case("Jaszcz 10, 76-231, Poland", at_house("Jaszcz", "10")),
        case(
            "ul. M. Kopernika 11, 76-231 Damnica, Poland",
            at_house("Mikołaja Kopernika", "11"),
        ),
        case(
            "ul. Mikołaja Kopernika 11, 76-231 Damnica",
            at_house("Mikołaja Kopernika", "11"),
        ),
        case(
            "Rakowice Małe 20BS, 76-231, Poland",
            at_house("Rakowice Małe", "20bs"),
        ),
        case("Paris, Texas", town("us-tx-paris", true)),
        case("Paris, TX, USA", town("us-tx-paris", true)),
        case("us-tx-paris", town("us-tx-paris", true)),
        case("FR-IDF-PARIS", town("fr-paris", true)),
        case("at-1-severnaya-roshcha", town("at-nordhain", true)),
        case("at-1-северная-роща", town("at-nordhain", true)),
        case("at-2-severnaya-roshcha", town("at-northgrove", true)),
        case("us-ga-paris", Want::Abstain),
        case("us-tx-severnaya-roshcha", Want::Abstain),
        case("de-nw-paris", Want::Abstain),
        case("de-tx-paris", Want::Abstain),
        case("us-zz-paris", Want::Abstain),
        case("us-tx-missing", Want::Abstain),
        case("at-1-roshcha", Want::Abstain),
        // the larger Paris, when nothing says otherwise
        case("Paris", town("fr-paris", true)),
        case("Delhi", town("in-dl-delhi", true)),
        case("Delhi, India", town("in-dl-delhi", true)),
        case("Delhi, Ontario", town("ca-on-delhi", true)),
        case("Delhi, ON, Canada", town("ca-on-delhi", true)),
        case("Delhi, Canada", town("ca-on-delhi", true)),
        case(
            "2 Rue de Rivoli, 75001 Paris, France",
            at_house("Rue de Rivoli", "2"),
        ),
        case("Rue de Rivoli 2, Paris", at_house("Rue de Rivoli", "2")),
        case(
            "14 Meadow Road, 60001 Bayport, Illinois, United States",
            at_house("Meadow Road", "14"),
        ),
        case(
            "7 bis Rue de Rivoli, 75001 Paris, France",
            at_house("Rue de Rivoli", "7bis"),
        ),
        case(
            "8 A Rue de Rivoli, 75001 Paris, France",
            at_house("Rue de Rivoli", "8a"),
        ),
        case(
            "102 Main Street, Paris, Texas",
            at_house("Main Street", "102"),
        ),
        case("Hohe Strasse 2, Köln", at_house("Hohe Straße", "2")),
        case(
            "2 Hohe Straße, Colonia, Germany",
            at_house("Hohe Straße", "2"),
        ),
        case("2 Hohe Straße, Colonia, France", Want::Abstain),
        // the street is there, house 99 is not: the street, not a good match
        case(
            "99 Rue de Rivoli, Paris, France",
            Want::At(house(street("Rue de Rivoli"), "3"), 200.0, false),
        ),
        // France has this street, Texas has not: the town in Texas, never the French street
        case("Rue de Rivoli, Paris, Texas", town("us-tx-paris", false)),
        case("Orchard, United Kingdom", town("gb-orchard", true)),
        case("Cologne, Germany", town("de-koeln", true)),
        case("Köln, Germany", town("de-koeln", true)),
        Case {
            query: "Koeln",
            want: town("de-koeln", true),
            known_failure: "index: search does not fold the oe digraph for ö, as suggest does",
        },
        case("Atlantis, France", Want::Abstain),
        case("Paris, Germany", Want::Abstain),
        case("75460", Want::At((-95.5559, 33.66), 1000.0, true)),
    ]
}

#[test]
fn canadian_unit_suggestions_identify_buildings_not_apartments_or_range_endpoints() {
    let index = index();
    for number in ["12", "12a"] {
        let query = format!("{number} Maple Scope Road, Delhi, ON N4B 1A1, Canada");
        let mut base = index.suggest(&query, 1, Some("ca"), None, false);
        assert_eq!(base.len(), 1, "building control: {query}");
        let base = base.remove(0);
        assert_eq!(base.kind, "address", "{query}");
        for unit in ["100", "A", "BSMT", "UPPR", "APT"] {
            let query = format!("{unit}-{number} Maple Scope Road, Delhi, ON N4B 1A1, Canada");
            for landmarks in [false, true] {
                let found = index.suggest(&query, 1, None, None, landmarks);
                assert_eq!(found.len(), 1, "{query}");
                let got = &found[0];
                assert_eq!((got.kind, got.country_code.as_str()), ("address", "ca"));
                assert_eq!(
                    (&got.id, &got.retrieval_id, got.x, got.y),
                    (&base.id, &base.retrieval_id, base.x, base.y),
                    "{query}"
                );
                assert_eq!(got.display_name, base.display_name);
                assert!(got.unverified_suffix.is_empty());
                assert!(metres((got.x, got.y), house(street("Maple Scope Road"), number)) < 2.);
            }
            assert!(index.suggest(&query, 1, Some("us"), None, false).is_empty());
            assert!(index.suggest(&query, 0, None, None, false).is_empty());
        }
    }
    let known_failure = "index: completion does not join a spaced fractional civic number";
    for query in [
        "12 1/2 Maple Scope Road, Delhi, ON N4B 1A1, Canada",
        "100-12 1/2 Maple Scope Road, Delhi, ON N4B 1A1, Canada",
    ] {
        assert!(
            index.suggest(query, 5, Some("ca"), None, false).is_empty(),
            "fixed, remove its known_failure ({known_failure}): {query}"
        );
    }
    let literal = index.suggest(
        "20-24 Maple Scope Road, Delhi, ON N4B 1A1, Canada",
        1,
        None,
        None,
        false,
    );
    assert_eq!(literal[0].kind, "address");
    assert!(
        metres(
            (literal[0].x, literal[0].y),
            house(street("Maple Scope Road"), "20-24")
        ) < 2.
    );
    for query in [
        "1-999 Maple Scope Road, Delhi, ON N4B 1A1, Canada",
        "1-12 Maple Scope Road, Delhi, ON M5T 0A8, Canada",
        "1-12 Maple Scope Road, Delhi, ON N4B 1A1, Australia",
        "1-12 Maple Scope Road, Delhi, ON N4B 1A1, Poland",
        "1-12 Maple Scope Road, Missingville, ON N4B 1A1, Canada",
        "1-12 Missing Road, Delhi, ON N4B 1A1, Canada",
    ] {
        assert!(
            !index
                .suggest(query, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
}

#[test]
fn korean_houses_keep_their_number_and_administrative_hierarchy() {
    for (province, number, lon, code) in [
        ("가람도", "13-34", 140.0, "KR-41"),
        ("가람도", "290", 140.0001, "KR-41"),
        ("나래도", "13-34", 142.0, "KR-43"),
    ] {
        let query = format!("{province} 솔빛시 새봄면 새로12길 {number}, South Korea");
        let found = index().suggest(&query, 5, Some("KR"), None, false);
        assert_eq!((found.len(), found[0].kind), (1, "address"), "{query}");
        let want = (lon, 35.0);
        assert!(metres((found[0].x, found[0].y), want) < 5., "{query}");
        assert!(
            index()
                .suggest(&query, 5, Some("JP"), None, false)
                .is_empty()
        );
        let reversed = index().reverse_search(want.1, want.0, 5., 1);
        assert_eq!(reversed[0].1["address"]["house_number"], number);
        assert_eq!(reversed[0].1["address"]["ISO3166-2-lvl4"], code);
        assert_eq!(
            index().reverse_region(want.1, want.0)["address"]["ISO3166-2-lvl4"],
            code
        );
    }
    for prefix in [
        "가람도 솔빛시 새봄면 새로1",
        "가람도 솔빛시 새봄면 새로12길 13",
        "가람도 솔빛시 새봄면 새로12길 13-",
    ] {
        let found = index().suggest(prefix, 1, None, None, false);
        assert_eq!(found.len(), 1, "{prefix}");
        assert!(
            metres((found[0].x, found[0].y), (140.0, 35.0)) < 200.,
            "{prefix}"
        );
        if prefix.ends_with(['3', '-']) {
            assert_eq!(found[0].kind, "address", "{prefix}");
        }
    }
    for (number, lon) in [
        ("45-6", 140.002),
        ("산45-6", 140.0021),
        ("산 45-6", 140.0021),
    ] {
        let query = format!("가람도 솔빛시 새봄면 옛리 {number}, 대한민국");
        let found = index().suggest(&query, 5, None, None, false);
        assert_eq!((found.len(), found[0].kind), (1, "address"), "{query}");
        assert!(metres((found[0].x, found[0].y), (lon, 35.)) < 5., "{query}");
    }
    for query in [
        "가람도 솔빛시 새봄면 새로12길 999, South Korea",
        "가람도 솔빛시 새봄면 새로12길 99, South Korea",
        "가람도 솔빛시 새봄면 새로12길 13-34, Japan",
        "가람도 없는시 새봄면 새로12길 13-34, South Korea",
    ] {
        assert!(
            !index()
                .suggest(query, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
}

#[test]
fn taiwanese_suggestions_keep_building_identity_and_unverified_floors() {
    for (road, number, stored) in [
        ("明光路三段389巷6弄", "11", "明光路3段389巷6弄"),
        ("明光路3段389巷6弄", "11之2", "明光路3段389巷6弄"),
        ("市民大道五段", "131", "市民大道五段"),
        ("市民大道5段", "7", "市民大道五段"),
    ] {
        let building = format!("青山市光華區{road}{number}號");
        let plain = index().suggest(&building, 1, None, None, false);
        assert_eq!(plain.len(), 1, "{building}");
        assert_eq!(plain[0].kind, "address", "{building}");
        assert!(plain[0].display_name.starts_with(&format!("{number}, ")));
        assert!(metres((plain[0].x, plain[0].y), house(street(stored), number)) < 5.);
        for floor in ["", "三樓", "7樓", "地下二樓"] {
            for country in ["", ", Taiwan", "，臺灣"] {
                let query = format!("{building}{floor}{country}");
                let fullwidth: String = query
                    .chars()
                    .map(|c| {
                        if c.is_ascii_digit() {
                            char::from_u32(c as u32 + 0xfee0).unwrap()
                        } else {
                            c
                        }
                    })
                    .collect();
                for text in [&query, &fullwidth] {
                    for cc in [None, Some("TW")] {
                        let found = index().suggest(text, 1, cc, None, false);
                        assert_eq!(found.len(), 1, "{text}");
                        let hit = &found[0];
                        assert_eq!(hit.kind, "address", "{text}");
                        assert_eq!(hit.country_code, "tw", "{text}");
                        assert_eq!(hit.retrieval_id, plain[0].retrieval_id, "{text}");
                        assert_eq!((hit.x, hit.y), (plain[0].x, plain[0].y), "{text}");
                        let suffix = text
                            .split('號')
                            .nth(1)
                            .unwrap()
                            .split([',', '，'])
                            .next()
                            .unwrap();
                        assert_eq!(hit.unverified_suffix, suffix, "{text}");
                        if !suffix.is_empty() {
                            assert!(hit.display_name.contains(suffix), "{text}");
                        }
                        let retrieved = index().retrieve_suggestion(&hit.retrieval_id).unwrap();
                        assert_eq!(retrieved.id, plain[0].id, "{text}");
                        assert_eq!((retrieved.x, retrieved.y), (hit.x, hit.y), "{text}");
                        assert!(retrieved.unverified_suffix.is_empty(), "{text}");
                    }
                }
            }
        }
    }
    for query in [
        "青山市光華區明光路三段389巷6弄999號三樓, Taiwan",
        "青山市光華區明光路三段389巷6弄11之9號三樓, Taiwan",
        "青山市光華區明光路四段389巷6弄11號三樓, Taiwan",
        "青山市光華區明光路三段388巷6弄11號三樓, Taiwan",
        "青山市光華區明光路三段389巷5弄11號三樓, Taiwan",
        "青山市光華區市民大道五段999號三樓, Taiwan",
        "碧海市光華區明光路三段389巷6弄11號三樓, Taiwan",
        "青山市未知區明光路三段389巷6弄11號三樓, Taiwan",
        "青山市光華區明光路三段389巷6弄11號三樓, France",
        "青山市光華區明光路三段389巷6弄11號三樓, Alpha, Taiwan",
        "青山市光華區明光路三段389巷6弄11號三樓, Missingland",
        "青山市光華區明光路三段389巷6弄11號三樓商店, Taiwan",
        "999青山市光華區明光路三段389巷6弄11號三樓, Taiwan",
        "９９９青山市光華區明光路三段389巷6弄11號三樓, Taiwan",
        "中國青山市光華區明光路三段389巷6弄11號三樓, Taiwan",
        "青山市光華區明光路三段389巷6弄12345678901號三樓, Taiwan",
        "青山市光華區明光路三段389巷6弄11-12號三樓, Taiwan",
        "青山市光華區明光路三段389巷6弄11至12號三樓, Taiwan",
        "青山市光華區明先路三段389巷6弄11號三樓, Taiwan",
        "青山市光華區明光路三段389巷6弄",
    ] {
        assert!(
            !index()
                .suggest(query, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
    let query = "青山市光華區明光路三段389巷6弄11號三樓, Taiwan";
    assert!(
        index()
            .suggest(query, 5, Some("JP"), None, false)
            .is_empty()
    );
    assert!(index().suggest(query, 0, None, None, false).is_empty());
}

#[test]
fn osm_site_boundaries_do_not_become_house_points() {
    for (number, point) in [
        ("702", (140.0199, 35.0199)),
        ("703", (140.0299, 35.0199)),
        ("704", (140.04, 35.02)),
        ("707", (140.06, 35.01)),
        ("708", (140.07, 35.01)),
    ] {
        let query = format!("가람도 솔빛시 새봄면 배움길 {number}, South Korea");
        let found = index().suggest(&query, 5, None, None, false);
        assert_eq!(found.len(), 1, "{query}");
        assert_eq!(found[0].kind, "address");
        assert!(metres((found[0].x, found[0].y), point) < 5.);
        let reverse = index().reverse_search(point.1, point.0, 5., 1);
        assert_eq!(reverse[0].1["address"]["house_number"], number);
    }
    let postcodes = index().postcode_hits("54399", Some("kr"));
    assert_eq!(postcodes.len(), 1);
    let postcode = index().render(&postcodes[0], true);
    assert!(
        metres(
            (
                postcode["lon"].as_str().unwrap().parse().unwrap(),
                postcode["lat"].as_str().unwrap().parse().unwrap()
            ),
            (140.045, 35.01)
        ) < 5.
    );
    for number in ["705", "706"] {
        let query = format!("가람도 솔빛시 새봄면 배움길 {number}, South Korea");
        assert!(
            index()
                .suggest(&query, 5, None, None, false)
                .iter()
                .all(|s| s.kind == "street"),
            "{query}"
        );
    }
}

#[test]
fn source_unit_columns_keep_each_units_point_and_the_building_key() {
    let unit_street = street("Courtyard Lane");
    for number in ["1/47", "2/47", "3A/49B", "4/51-53"] {
        let query = format!("{number} Courtyard Lane, Harbourton");
        let found = index().suggest(&query, 5, Some("NZ"), None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{query}");
        assert!(
            metres((found[0].x, found[0].y), house(unit_street, number)) < 5.,
            "{query}"
        );
    }
    let found = index().suggest("47 Courtyard Lane, Harbourton", 5, Some("NZ"), None, false);
    assert_eq!(found[0].kind, "address");
    assert!(metres((found[0].x, found[0].y), house(unit_street, "47")) < 5.);
}

#[test]
fn a_houses_postal_town_is_scoped_and_keeps_its_settlement_and_county() {
    for (number, postcode, town) in [
        ("31", "ZZ1 1AA", Some("Posthaven")),
        ("32", "ZZ1 1AB", Some("Farhaven")),
        ("33", "ZZ1 1AC", None),
        ("34", "ZZ1 1AC", None),
        ("35", "", Some("Posthaven")),
        ("36", "", Some("Farhaven")),
    ] {
        let street = STREETS
            .iter()
            .find(|s| s.name == "Courier Road" && s.numbers.contains(&number))
            .unwrap();
        let (lon, lat) = house(street, number);
        let found = index().reverse_search(lat, lon, 5.0, 1);
        assert_eq!(found.len(), 1, "{number}");
        let address = &found[0].1["address"];
        assert_eq!(address["house_number"], number);
        assert_eq!(address["village"], "Oakmere");
        assert_eq!(address["county"], "Posthaven");
        assert_eq!(address["state"], "Mailshire");
        assert_eq!(address["postal_town"].as_str(), town);
        assert_eq!(
            address["postcode"].as_str().unwrap_or(""),
            postcode.replace(' ', "")
        );
    }
}

#[tokio::test]
#[ignore = "needs the libpostal models; run with --ignored"]
async fn resolve_follows_the_authored_facts() {
    let resolver = Resolver::open(dir()).unwrap();
    for number in ["12", "12a", "12 1/2"] {
        for unit in ["100", "A", "BSMT", "UPPR", "APT"] {
            let query = format!("{unit}-{number} Maple Scope Road, Delhi, ON N4B 1A1, Canada");
            let found = resolver.resolve(&query).await.unwrap();
            assert!(found.good_match, "{query}: {found:?}");
            let details = found.address_details.as_option().unwrap();
            assert_eq!(details.country_code, "ca");
            assert_eq!(details.postcode, "N4B 1A1");
            assert_eq!(
                opengeocoding::index::schema::number_key(&details.house_number),
                opengeocoding::index::schema::number_key(number)
            );
            let point = found.geo_coordinates.as_option().unwrap();
            assert!(
                metres(
                    (point.lon as f64, point.lat as f64),
                    house(street("Maple Scope Road"), number)
                ) < 2.,
                "{query}: {found:?}"
            );
        }
    }
    let literal = resolver
        .resolve("20-24 Maple Scope Road, Delhi, ON N4B 1A1, Canada")
        .await
        .unwrap();
    assert!(literal.good_match, "{literal:?}");
    assert_eq!(
        literal.address_details.as_option().unwrap().house_number,
        "20-24"
    );
    for query in [
        "1-999 Maple Scope Road, Delhi, ON N4B 1A1, Canada",
        "APT-999 Maple Scope Road, Delhi, ON N4B 1A1, Canada",
        "1-12 Maple Scope Road, Delhi, ON M5T 0A8, Canada",
        "1-12 Missing Road, Delhi, ON N4B 1A1, Canada",
    ] {
        let found = resolver.resolve(query).await.unwrap();
        assert!(!found.good_match, "{query}: {found:?}");
    }
    for (road, number, want) in [
        ("明光路三段389巷6弄", "11", (123.0, 25.0)),
        ("明光路3段389巷6弄", "11之2", (123.0001, 25.0)),
        ("市民大道五段", "131", (123.01, 25.0)),
        ("市民大道五段", "7", (123.0101, 25.0)),
    ] {
        let fullwidth = |text: &str| {
            text.chars()
                .map(|c| {
                    if c.is_ascii_digit() {
                        char::from_u32(c as u32 + 0xfee0).unwrap()
                    } else {
                        c
                    }
                })
                .collect::<String>()
        };
        for (road, number) in [
            (road.to_owned(), number.to_owned()),
            (fullwidth(road), fullwidth(number)),
        ] {
            for floor in ["", "三樓", "7樓", "地下二樓"] {
                let query = format!("青山市光華區{road}{number}號{floor}, Taiwan");
                let got = resolver.resolve(&query).await.unwrap();
                let point = got.geo_coordinates.as_option().unwrap();
                let details = got.address_details.as_option().unwrap();
                assert!(got.good_match, "{query}: {got:?}");
                assert_eq!(details.country_code, "tw", "{query}");
                assert_eq!(
                    details.house_number,
                    opengeocoding::index::schema::number_key(&number),
                    "{query}"
                );
                assert!(
                    metres((point.lon as f64, point.lat as f64), want) < 5.,
                    "{query}: {got:?}"
                );
            }
        }
    }
    for query in [
        "光華社區明光路999號三樓, Taiwan",
        "青山市光華區市民大道五段999號三樓, Taiwan",
        "青山市光華區明光路三段389巷6弄999號三樓, Taiwan",
        "青山市光華區明光路三段389巷6弄11之9號三樓, Taiwan",
        "青山市光華區明光路四段389巷6弄11號三樓, Taiwan",
        "青山市光華區明光路三段388巷6弄11號三樓, Taiwan",
        "青山市光華區明光路三段389巷5弄11號三樓, Taiwan",
        "碧海市光華區明光路三段389巷6弄11號三樓, Taiwan",
        "青山市未知區明光路三段389巷6弄11號三樓, Taiwan",
        "青山市光華區明光路三段389巷6弄11號三樓, France",
        "青山市光華區明光路三段389巷6弄11號三樓, Alpha, Taiwan",
    ] {
        let got = resolver.resolve(query).await.unwrap();
        assert!(!got.good_match, "{query}: {got:?}");
        assert!(
            got.address_details.as_option().is_none_or(|d| {
                d.house_number.is_empty()
                    && (!d.city.is_empty() || !d.state.is_empty() || d.country.is_empty())
            }),
            "{query}: {got:?}"
        );
    }
    for query in ["Taiwan", "臺灣", "Taiwan, Taiwan"] {
        let got = resolver.resolve(query).await.unwrap();
        assert!(got.good_match, "{query}: {got:?}");
        let point = got.geo_coordinates.as_option().unwrap();
        assert!(metres((point.lon as f64, point.lat as f64), (125.0, 20.0)) < 5.);
    }
    for number in ["1/47", "2/47", "3A/49B", "4/51-53"] {
        let query = format!("{number} Courtyard Lane, Kōwhai Bay, Harbourton, New Zealand");
        let found = resolver.resolve(&query).await.unwrap();
        assert!(found.good_match, "{query}: {found:?}");
        assert_eq!(
            found.address_details.as_option().unwrap().house_number,
            number.to_lowercase()
        );
        let point = found.geo_coordinates.as_option().unwrap();
        assert!(
            metres(
                (point.lon as f64, point.lat as f64),
                house(street("Courtyard Lane"), number)
            ) < 5.,
            "{query}: {found:?}"
        );
    }
    for number in ["3/47", "4/51", "4/53"] {
        let query = format!("{number} Courtyard Lane, Kōwhai Bay, Harbourton, New Zealand");
        let found = resolver.resolve(&query).await.unwrap();
        assert!(!found.good_match, "{query}: {found:?}");
    }
    for road in ["Orchard Road", "Orchard Rd", "East Orchard Road"] {
        let expected = if road == "Orchard Rd" {
            "Orchard Road"
        } else {
            road
        };
        for number in ["5a", "10-12", "2/19"] {
            let query = format!("{number} {road}, Harbourton, New Zealand");
            let found = resolver.resolve(&query).await.unwrap();
            let details = found.address_details.as_option().unwrap();
            assert!(found.good_match, "{query}: {found:?}");
            assert_eq!(details.road, expected, "{query}: {found:?}");
            assert_eq!(details.house_number, number, "{query}: {found:?}");
            let point = found.geo_coordinates.as_option().unwrap();
            assert!(
                metres(
                    (point.lon as f64, point.lat as f64),
                    house(street(expected), number)
                ) < 5.,
                "{query}: {found:?}"
            );
        }
    }
    for number in ["99", "3/19", "99b", "20-22"] {
        let query = format!("{number} Orchard Road, Harbourton, New Zealand");
        let found = resolver.resolve(&query).await.unwrap();
        assert!(!found.good_match, "{query}: {found:?}");
        let details = found.address_details.as_option().unwrap();
        assert_eq!(details.road, "Orchard Road", "{query}: {found:?}");
        assert!(details.house_number.is_empty(), "{query}: {found:?}");
    }
    for suburb in ["Kōwhai Bay", "Kowhai Bay"] {
        for number in ["5a", "10-12", "2/19"] {
            let query = format!("{number} Orchard Road, {suburb}, Harbourton, New Zealand");
            let parsed = resolver.parse(&query).await.unwrap();
            assert_eq!(parsed["road"], "orchard road", "{query}: {parsed:?}");
            assert_eq!(parsed["city"], "harbourton", "{query}: {parsed:?}");
            assert_eq!(
                parsed["suburb"],
                suburb.to_lowercase(),
                "{query}: {parsed:?}"
            );
            let found = resolver.resolve(&query).await.unwrap();
            assert!(found.good_match, "{query}: {found:?}");
            let details = found.address_details.as_option().unwrap();
            assert_eq!(details.country_code, "nz", "{query}: {found:?}");
            assert_eq!(details.town, "Harbourton", "{query}: {found:?}");
            assert_eq!(details.house_number, number, "{query}: {found:?}");
            let point = found.geo_coordinates.as_option().unwrap();
            assert!(
                metres(
                    (point.lon as f64, point.lat as f64),
                    house(street("Orchard Road"), number)
                ) < 5.,
                "{query}: {found:?}"
            );
        }
    }
    let found = resolver
        .resolve("5a Orchard Road, Fern Bay, Harbourton, New Zealand")
        .await
        .unwrap();
    assert!(found.good_match, "{found:?}");
    let point = found.geo_coordinates.as_option().unwrap();
    assert!(
        metres((point.lon as f64, point.lat as f64), (170.15, -40.0)) < 5.,
        "{found:?}"
    );
    for query in [
        "99 Orchard Road, Kōwhai Bay, Harbourton, New Zealand",
        "3/19 Orchard Road, Kōwhai Bay, Harbourton, New Zealand",
        "5a Orchard Road, Missing Bay, Harbourton, New Zealand",
        "5a Orchard Road, Kōwhai Bay, Eastport, New Zealand",
        "5a Orchard Road, Kōwhai Bay, Harbourton, Australia",
        "5a Orchard Road, Kōwhai Bay, Harbourton, NZ-AUK, New Zealand",
    ] {
        let found = resolver.resolve(query).await.unwrap();
        assert!(!found.good_match, "{query}: {found:?}");
    }
    for (number, precise) in [
        ("702", true),
        ("703", true),
        ("704", true),
        ("705", false),
        ("706", false),
        ("707", true),
        ("708", true),
    ] {
        let query = format!("가람도 솔빛시 새봄면 배움길 {number}, South Korea");
        let found = resolver.resolve(&query).await.unwrap();
        assert_eq!(found.good_match, precise, "{query}: {found:?}");
        assert_eq!(
            found.address_details.as_option().unwrap().house_number,
            if precise { number } else { "" }
        );
    }
    for (province, number, lon) in [
        ("가람도", "13-34", 140.0),
        ("가람도", "290", 140.0001),
        ("나래도", "13-34", 142.0),
    ] {
        let query = format!("{province} 솔빛시 새봄면 새로12길 {number}, South Korea");
        let found = resolver.resolve(&query).await.unwrap();
        assert!(found.good_match, "{query}: {found:?}");
        assert_eq!(
            found.address_details.as_option().unwrap().house_number,
            number
        );
        let point = found.geo_coordinates.as_option().unwrap();
        assert!(
            metres((point.lon as f64, point.lat as f64), (lon, 35.)) < 5.,
            "{query}"
        );
    }
    for query in [
        "가람도 솔빛시 새봄면 새로12길 999, South Korea",
        "가람도 솔빛시 새봄면 새로12길 99, South Korea",
        "가람도 솔빛시 새봄면 새로12길 13-34, Japan",
        "가람도 없는시 새봄면 새로12길 13-34, South Korea",
    ] {
        let found = resolver.resolve(query).await.unwrap();
        assert!(!found.good_match, "{query}: {found:?}");
    }
    let region = resolver.resolve("Garam, South Korea").await.unwrap();
    assert!(region.good_match);
    assert_eq!(
        region.address_details.as_option().unwrap().region_codes[0].code,
        "KR-41"
    );
    let ambiguous = resolver
        .resolve("가람도 솔빛시 새봄면 새로12길 17, South Korea")
        .await
        .unwrap();
    assert!(!ambiguous.good_match);
    assert!(
        ambiguous
            .address_details
            .as_option()
            .is_none_or(|d| d.house_number.is_empty())
    );
    for (number, query) in [
        ("31", "31 Courier Road, Posthaven, United Kingdom"),
        ("35", "35 Courier Road, Posthaven, United Kingdom"),
        ("31", "31 Courier Road, Mailshire, United Kingdom"),
    ] {
        let got = resolver.resolve(query).await.unwrap();
        assert!(got.good_match, "{got:?}");
        let details = got.address_details.as_option().unwrap();
        assert_eq!(details.house_number, number);
        assert_eq!(details.village, "Oakmere");
        assert_eq!(details.county, "Posthaven");
        assert_eq!(details.state, "Mailshire");
    }
    let got = resolver
        .resolve("61 Quay Road, Harbor City, United Kingdom")
        .await
        .unwrap();
    let details = got.address_details.as_option().unwrap();
    assert_eq!(details.house_number, "61", "{got:?}");
    assert_eq!(details.village, "Dockward", "{got:?}");
    assert_eq!(details.state, "Harbor City", "{got:?}");
    assert!(details.postcode.is_empty(), "{got:?}");
    assert!(got.good_match, "{got:?}");
    let point = got.geo_coordinates.as_option().unwrap();
    assert!(metres((point.lon as f64, point.lat as f64), (35.001, 12.0)) < 5.);
    for postcode in ["6799", "6781"] {
        let query = format!("Dispatchveien 51, {postcode} Postalvik, Norway");
        let got = resolver.resolve(&query).await.unwrap();
        let details = got.address_details.as_option().unwrap();
        assert_eq!(details.house_number, "51", "{query}: {got:?}");
        assert_eq!(details.postcode, "6780", "{query}: {got:?}");
        assert_eq!(details.village, "Birchlet", "{query}: {got:?}");
        assert_eq!(details.county, "Postalvik", "{query}: {got:?}");
        assert!(!got.good_match, "{query}: {got:?}");
        let point = got.geo_coordinates.as_option().unwrap();
        assert!(metres((point.lon as f64, point.lat as f64), (18.001, 62.0)) < 5.);
    }
    for query in [
        "61 Quay Road, Harbor City, France",
        "61 Quay Road, Harbor City, England, United Kingdom",
        "61 Quay Road, ZZ2 2AA Harbor City, United Kingdom",
        "Dispatchveien 51, 6782 Postalvik, Norway",
        "Dispatchveien 51, 6791 Postalvik, Norway",
        "Dispatchveien 99, 6799 Postalvik, Norway",
        "Dispatchveien 51, 6799 Postalvik, France",
        "Dispatchveien 51, 6799 Postalvik, Sweden",
        "Dispatchveien 51, 6799 Postalvik, Vestland, Norway",
        "Twin Road 51, 6799 Postalvik, Norway",
        "7 Acacia Straat, 9992 Veldhaven, South Africa",
        "7 Acacia Straat, 9991 Veldhaven, France",
        "9 Acacia Straat, 9991 Veldhaven, South Africa",
        "25 Ulstr., 50668 Köln, Germany",
        "25 Ulstr., 50667 Köln, France",
        "99 Birkenstarsse, 50667 Köln, Germany",
        "24 Birkenstarsse, 50668 Köln, Germany",
        "24 Birkenstarsse, 50667 Köln, France",
        "32 Courier Road, Posthaven, United Kingdom",
        "36 Courier Road, Posthaven, United Kingdom",
        "33 Courier Road, Amber Reach, United Kingdom",
        "31 Courier Road, Posthaven, France",
        "31 Courier Road, Posthaven, Scotland, United Kingdom",
        "31 Courier Road, ZZ2 2AA Posthaven, United Kingdom",
        "999 Courier Road, Posthaven, United Kingdom",
    ] {
        let got = resolver.resolve(query).await.unwrap();
        assert!(!got.good_match, "{query}: {got:?}");
        assert!(
            got.address_details
                .as_option()
                .is_none_or(|d| d.house_number.is_empty()),
            "{query}: {got:?}"
        );
    }
    for query in [
        "Северная Роща, Alpha, Austria",
        "Severnaya Roshcha, Alpha, Austria",
        "Severnaya Roshcha, Première, Austria",
        "Nordhain Village, Alpha, Austria",
    ] {
        let found = resolver.resolve(query).await.unwrap();
        let point = found.geo_coordinates.as_option().unwrap();
        assert!(
            metres((point.lon as f64, point.lat as f64), (30.0, 0.21)) < 5.,
            "{query}: {found:?}"
        );
        assert!(found.good_match, "{query}: {found:?}");
    }
    let partial = resolver.resolve("Roshcha, Alpha, Austria").await.unwrap();
    assert!(
        !partial.geo_coordinates.as_option().is_some_and(|point| {
            metres((point.lon as f64, point.lat as f64), (85.0, -10.0)) < 5.
        }),
        "{partial:?}"
    );
    let bare = resolver
        .resolve("Severnaya Roshcha, Austria")
        .await
        .unwrap();
    let point = bare.geo_coordinates.as_option().unwrap();
    assert!(metres((point.lon as f64, point.lat as f64), (80.0, -10.0)) < 5.);
    for query in [
        "Северная Роща, Beta, Austria",
        "Северная Роща Village, Beta, Austria",
        "Severnaya Roshcha, Alpha, France",
    ] {
        let found = resolver.resolve(query).await.unwrap();
        assert!(
            !found.geo_coordinates.as_option().is_some_and(|point| {
                metres((point.lon as f64, point.lat as f64), (30.0, 0.21)) < 5.
            }),
            "{query}: {found:?}"
        );
    }
    let mut wrong = vec![];
    for case in cases() {
        let got = resolver.resolve(case.query).await.unwrap();
        let point = got
            .geo_coordinates
            .as_option()
            .map(|g| (g.lon as f64, g.lat as f64));
        let right = match (&case.want, point) {
            (Want::Abstain, p) => p.is_none() && !got.good_match,
            (Want::At(want, within, good), Some(p)) => {
                metres(p, *want) <= *within && got.good_match == *good
            }
            (Want::At(..), None) => false,
        };
        let line = format!(
            "{:?}: {:?}, good_match {}, at {point:?}",
            case.query, got.display_name, got.good_match
        );
        match (right, case.known_failure.is_empty()) {
            (false, true) => wrong.push(format!("wrong: {line}")),
            (true, false) => wrong.push(format!(
                "fixed, remove its known_failure ({}): {line}",
                case.known_failure
            )),
            _ => {}
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
    for country in ["Netherlands", "NL"] {
        let query = format!("Meadowln. 4a, 1234 TJ Polderdam, {country}");
        let parsed = resolver.parse(&query).await.unwrap();
        assert_eq!(parsed["house_number"], "4a", "{parsed:?}");
        assert_eq!(parsed["road"], "meadowln.", "{parsed:?}");
        assert_eq!(parsed["postcode"], "1234 tj", "{parsed:?}");
        let got = resolver.resolve(&query).await.unwrap();
        let details = got.address_details.as_option().unwrap();
        assert_eq!(details.country_code, "nl", "{got:?}");
        assert_eq!(details.postcode, "1234 TJ", "{got:?}");
        assert_eq!(details.house_number, "4a", "{got:?}");
        assert!(got.good_match, "{got:?}");
    }
    let separate_country = resolver
        .parse("Meadowln. 4a, 1234, TJ, Polderdam, Netherlands")
        .await
        .unwrap();
    assert_eq!(separate_country["postcode"], "1234", "{separate_country:?}");
    for query in ["7 Uferweg, 5555 Eastufer, Austria", "Falkenstadt, Austria"] {
        let got = resolver.resolve(query).await.unwrap();
        let details = got.address_details.as_option().unwrap();
        assert_eq!(details.country_code, "at", "{query}: {got:?}");
        assert!(
            details.region_codes.iter().any(|r| r.code == "AT-5"),
            "{query}: {got:?}"
        );
        assert!(got.good_match, "{query}: {got:?}");
    }
    for (query, cc) in [
        ("7 Uferweg, Eastufer, Liechtenstein", "li"),
        ("Falkenstadt, Germany", "de"),
    ] {
        let got = resolver.resolve(query).await.unwrap();
        if let Some(details) = got.address_details.as_option() {
            assert_eq!(details.country_code, cc, "{query}: {got:?}");
            assert!(details.house_number.is_empty(), "{query}: {got:?}");
        }
        assert!(!got.good_match, "{query}: {got:?}");
    }
    for query in ["Texas, Haut-Rhin", "Île-de-France, Haut-Rhin, Germany"] {
        let got = resolver.resolve(query).await.unwrap();
        assert!(
            got.address_details
                .as_option()
                .is_none_or(|d| d.county != "Haut-Rhin"),
            "{query}: {got:?}"
        );
    }
    for query in [
        "Askvn. 7, 1234 Fjordvik, Norway",
        "Elmvn. - North 7, 1234 Fjordvik, Norway",
        "Birchvn. - South 5B, 1234 Fjordvik, Norway",
        "Birchvn. - North 5B, Fjordvik, Sweden",
        "Birchvn. - North 99, 1234 Fjordvik, Norway",
        "Granvn. 5B, Norway",
        "Granvn. 99, 1234 Fjordvik, Norway",
        "2 R. de Rivoli, 75001 Paris, Germany",
        "Willowln. 4a, Polderdam, Norway",
    ] {
        let got = resolver.resolve(query).await.unwrap();
        assert!(
            got.address_details
                .as_option()
                .is_none_or(|d| d.house_number.is_empty()),
            "{query}: {got:?}"
        );
        assert!(!got.good_match, "{query}: {got:?}");
    }
    for (postcode, number, good) in [
        ("75461", "2", true),
        ("75461-1234", "2", true),
        ("75469", "2", false),
        ("75462", "2", false),
        ("75460", "", false),
    ] {
        let query = format!("2 Canal Street, {postcode} Paris, United States");
        let got = resolver.resolve(&query).await.unwrap();
        let details = got.address_details.as_option().unwrap();
        assert_eq!(details.country_code, "us", "{got:?}");
        assert_eq!(details.house_number, number, "{query}: {got:?}");
        assert_eq!(got.good_match, good, "{query}: {got:?}");
        if number.is_empty() {
            assert!(details.road.is_empty(), "{query}: {got:?}");
        } else {
            assert_eq!(details.postcode, "75461", "{query}: {got:?}");
            let street = STREETS
                .iter()
                .find(|s| s.name == "Canal Street" && s.numbers.contains(&number))
                .unwrap();
            let at = got.geo_coordinates.as_option().unwrap();
            assert!(
                metres((at.lon as f64, at.lat as f64), house(street, number)) < 5.0,
                "{query}: {got:?}"
            );
        }
    }
}
