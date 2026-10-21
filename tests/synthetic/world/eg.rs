use super::*;
use crate::*;

pub const PLACES: &[Place] = &[
    place!(
        "eg",
        "country",
        "",
        "EG",
        "",
        110_000_000,
        (30.0, 27.0),
        "",
        "مصر",
        &[("ar", "مصر"), ("en", "Egypt")],
    ),
    place!(
        "eg-ast",
        "region",
        "",
        "EG",
        "EG-AST",
        4_400_000,
        (31.55, 27.21),
        "eg",
        "أسيوط",
        &[("ar", "أسيوط"), ("en", "Asyut")],
    ),
    place!(
        "eg-ast-manfalut",
        "locality",
        "city",
        "EG",
        "EG-AST",
        80_000,
        (30.97, 27.31),
        "eg-ast",
        "منفلوط",
        &[("ar", "منفلوط"), ("en", "Manfalut")],
    ),
];

pub const STREETS: &[Street] = &[
    // libpostal reads a number written before an Arabic street as part of the road.
    Street {
        class: "residential",
        length: 0.0,
        name: "شارع الكنيسه",
        postcode: "71621",
        city: "منفلوط",
        cc: "EG",
        lon: 30.97,
        lat: 27.31,
        numbers: &["5"],
    },
];

#[test]
fn egyptian_houses_are_suggested_without_hamza_or_the_towns_unit() {
    for typed in [
        "شارع الكنيسه 5, منفلوط, اسيوط",
        "شارع الكنيسه 5, مركز منفلوط, اسيوط, 71621, Egypt",
        "شارع الكنيسة 5, مركز منفلوط, أسيوط, Egypt",
    ] {
        let found = index().suggest(typed, 5, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{typed}");
        let want = house(street("شارع الكنيسه"), "5");
        assert!(metres((found[0].x, found[0].y), want) < 5.0, "{typed}");
    }
    // the unit's word is no wildcard: it names a town all the same
    let typed = "شارع الكنيسه 5, مركز طنطا, اسيوط, Egypt";
    assert!(
        index()
            .suggest(typed, 5, None, None, false)
            .iter()
            .all(|s| s.kind != "address"),
        "{typed}"
    );
}
