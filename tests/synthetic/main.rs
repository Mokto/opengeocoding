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
use world::{PLACES, STREETS, house};

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
fn a_typed_place_name_lists_every_place_by_size_and_a_country_narrows_it() {
    let paris = |cc| {
        index()
            .suggest("Par", 5, cc)
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
    let at = |text: &str, cc| index().suggest(text, 5, cc);
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
