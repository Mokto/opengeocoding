//! Candidate checks the service runs on every backend answer.
use divan::black_box;
use geolocation::{
    matching::*,
    proto::geolocation::{AddressDetails, FullAddress},
    resolver::format_response,
};
use serde_json::json;

fn main() {
    divan::main();
}

fn parsed(values: &[(&str, &str)]) -> ParsedAddress {
    values
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn candidate(country: &str, city: &str, state: &str, postcode: &str) -> FullAddress {
    FullAddress {
        address_details: AddressDetails {
            country_code: country.into(),
            city: city.into(),
            state: state.into(),
            postcode: postcode.into(),
            ..Default::default()
        }
        .into(),
        ..Default::default()
    }
}

#[divan::bench]
fn normalize_places() -> usize {
    [
        "München",
        "São-Paulo",
        "Łódź",
        "København",
        "Αθήνα",
        "Москва",
        "東京",
    ]
    .iter()
    .map(|p| normalize_place(black_box(p)).len())
    .sum()
}

#[divan::bench]
fn choose_among_candidates(bencher: divan::Bencher) {
    let address = parsed(&[
        ("road", "rue de rivoli"),
        ("house_number", "10"),
        ("postcode", "75004"),
        ("city", "paris"),
        ("country", "france"),
    ]);
    let candidates = vec![
        candidate("us", "Paris", "Texas", "75460"),
        candidate("fr", "Paris", "Île-de-France", "75004"),
        candidate("fr", "Paris", "Île-de-France", "75001"),
        candidate("ca", "Paris", "Ontario", "N3L"),
    ];
    bencher.bench(|| choose_candidate(black_box(&address), "fr", "75004", black_box(&candidates)));
}

#[divan::bench]
fn format_nominatim_response(bencher: divan::Bencher) {
    let hit = json!({
        "display_name": "10, Rue de Rivoli, Paris, Île-de-France, France",
        "lat": "48.8556", "lon": "2.3601", "addresstype": "place",
        "address": { "house_number": "10", "road": "Rue de Rivoli", "city": "Paris",
            "postcode": "75004", "state": "Île-de-France", "country": "France",
            "country_code": "fr", "ISO3166-2-lvl4": "FR-IDF", "ISO3166-2-lvl6": "FR-75C" }
    });
    bencher.bench(|| format_response(black_box(&hit)).unwrap());
}
