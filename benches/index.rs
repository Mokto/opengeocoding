//! The index analyzer and house-number keys: run per token at build time (about a billion rows)
//! and per query by the shim. No libpostal or index files needed.
// The included modules' test blocks compile here without their #[test] functions.
#![allow(unused_imports)]
#[allow(dead_code)]
#[path = "../src/bin/build_index/canon.rs"]
mod canon;
#[allow(dead_code)]
#[path = "../src/bin/build_index/index.rs"]
mod index;

use divan::black_box;

const NAMES: [&str; 12] = [
    "Rue de Rivoli",
    "S VERMONT AVE",
    "Hauptstraße",
    "Drottninggatan",
    "Avenida Doutor João Pessoa",
    "Atatürk Caddesi",
    "улица Ленина",
    "芝公園四丁目",
    "濱海路三段",
    "서오릉로",
    "Sheikh Mohammed bin Rashid Boulevard",
    "350 Fifth Avenue",
];

fn main() {
    divan::main();
}

#[divan::bench]
fn fold_street_names() -> usize {
    NAMES.iter().map(|n| index::fold(black_box(n)).len()).sum()
}

#[divan::bench]
fn number_keys() -> usize {
    ["12", "12B", "１２６號", "4-2-8", " 7a "]
        .iter()
        .map(|n| index::number_key(black_box(n)).len())
        .sum()
}
