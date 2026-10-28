use super::*;
use crate::{Case, at_house, case};

pub const PLACES: &[Place] = &[
    place!(
        "rs",
        "country",
        "",
        "RS",
        "",
        6_600_000,
        (20.4, 44.66),
        "",
        "Србија",
        &[("sr", "Србија"), ("en", "Serbia")]
    ),
    place!(
        "rs-sremcica",
        "locality",
        "town",
        "RS",
        "",
        20_000,
        (20.4, 44.66),
        "rs",
        "Сремчица",
        &[("sr", "Сремчица"), ("sr-Latn", "Sremčica")]
    ),
];

pub const STREETS: &[Street] = &[Street {
    class: "residential",
    length: 0.0,
    name: "Капетана Дерока",
    postcode: "",
    city: "Сремчица",
    cc: "RS",
    lon: 20.401,
    lat: 44.66,
    numbers: &["10", "12", "14", "10a"],
}];

pub fn cases() -> Vec<Case> {
    // Serbia's sources key the register's Cyrillic house letter in Latin ("10a" for "10А").
    vec![case(
        "Капетана Дерока 10А, Сремчица, Србија",
        at_house("Капетана Дерока", "10a"),
    )]
}
