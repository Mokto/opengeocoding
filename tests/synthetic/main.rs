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
use world::{PLACES, STREETS, house, road_end};

fn dir() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("synthetic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        world::write_release(&root.join("release"));
        let out = root.join("index");
        let built = Command::new(env!("CARGO_BIN_EXE_build_index"))
            .arg(root.join("release"))
            .arg(&out)
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
    let at = |text: &str| poi.suggest(text, 5, Some("GB"), None);
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
        !poi.suggest("Main Street 994, Paris", 5, Some("US"), None)
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
            .suggest("High Street 999, Beacon", 5, Some("GB"), None)
            .iter()
            .any(|s| s.kind == "address")
    );
    drop(covered);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_typed_place_name_lists_every_place_by_size_and_a_country_narrows_it() {
    let paris = |cc| {
        index()
            .suggest("Par", 5, cc, None)
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
    let at = |text: &str, cc| index().suggest(text, 5, cc, None);
    let rivoli = at("Rue de Rivoli 2", None);
    assert_eq!((rivoli.len(), rivoli[0].kind), (1, "address"));
    let want = house(street("Rue de Rivoli"), "2");
    assert!(metres((rivoli[0].x, rivoli[0].y), want) < 5.0);
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
        case("Paris, France", town("fr-paris", true)),
        case("Paris, Texas", town("us-tx-paris", true)),
        case("Paris, TX, USA", town("us-tx-paris", true)),
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
            "102 Main Street, Paris, Texas",
            at_house("Main Street", "102"),
        ),
        case("Hohe Strasse 2, Köln", at_house("Hohe Straße", "2")),
        // the street is there, house 99 is not: the street, not a good match
        case(
            "99 Rue de Rivoli, Paris, France",
            Want::At(house(street("Rue de Rivoli"), "3"), 200.0, false),
        ),
        // France has this street, Texas has not: the town in Texas, never the French street
        case("Rue de Rivoli, Paris, Texas", town("us-tx-paris", false)),
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

#[tokio::test]
#[ignore = "needs the libpostal models; run with --ignored"]
async fn resolve_follows_the_authored_facts() {
    let resolver = Resolver::open(dir()).unwrap();
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
}
