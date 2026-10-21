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
    // a markaz named in English alone, as Overture names Egypt's
    place!(
        "eg-ast-manfalut-markaz",
        "county",
        "",
        "EG",
        "EG-AST",
        300_000,
        (31.1, 27.31),
        "eg-ast",
        "Manfalut District",
        &[("en", "Manfalut District")],
    ),
    place!(
        "eg-ast-manfalut",
        "locality",
        "city",
        "EG",
        "EG-AST",
        80_000,
        (30.97, 27.31),
        "eg-ast-manfalut-markaz",
        "منفلوط",
        &[("ar", "منفلوط"), ("en", "Manfalut")],
    ),
    place!(
        "eg-ast-bani-rafie",
        "locality",
        "village",
        "EG",
        "EG-AST",
        8_000,
        (31.4, 27.31),
        "eg-ast-manfalut-markaz",
        "بني رافع",
        &[("ar", "بني رافع")],
    ),
    // made up: another town of the governorate, outside the markaz
    place!(
        "eg-ast-tanta",
        "locality",
        "city",
        "EG",
        "EG-AST",
        400_000,
        (32.6, 27.0),
        "eg-ast",
        "طنطا",
        &[("ar", "طنطا")],
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
    // a village's house, which its address gives the markaz's town ("مركز منفلوط")
    Street {
        class: "residential",
        length: 0.0,
        name: "شارع المدرسه",
        postcode: "71631",
        city: "بني رافع",
        cc: "EG",
        lon: 31.4,
        lat: 27.31,
        numbers: &["7"],
    },
];

#[test]
fn egyptian_houses_are_suggested_by_their_markaz_without_hamza_or_unit_words() {
    let first = |typed: &str| {
        index()
            .suggest(typed, 5, None, None, false)
            .into_iter()
            .next()
    };
    for (typed, road, number) in [
        ("شارع الكنيسه 5, منفلوط, اسيوط", "شارع الكنيسه", "5"),
        (
            "شارع الكنيسه 5, مركز منفلوط, اسيوط, 71621, Egypt",
            "شارع الكنيسه",
            "5",
        ),
        (
            "شارع الكنيسة 5, مركز منفلوط, أسيوط, Egypt",
            "شارع الكنيسه",
            "5",
        ),
        ("شارع الكنيسه 5, منفلوط, منفلوط, Egypt", "شارع الكنيسه", "5"),
        // a suburb no place is named for, with the postcode or two places after it
        (
            "شارع الكنيسه 5, الحي الغربي, منفلوط, اسيوط, Egypt",
            "شارع الكنيسه",
            "5",
        ),
        (
            "شارع الكنيسه 5, الحي الغربي, اسيوط, 71621, Egypt",
            "شارع الكنيسه",
            "5",
        ),
        (
            "شارع المدرسه 7, مركز منفلوط, اسيوط, 71631, Egypt",
            "شارع المدرسه",
            "7",
        ),
        ("شارع المدرسه 7, منفلوط, اسيوط", "شارع المدرسه", "7"),
    ] {
        let found = first(typed);
        assert_eq!(found.as_ref().map(|s| s.kind), Some("address"), "{typed}");
        let found = found.unwrap();
        assert!(
            metres((found.x, found.y), house(street(road), number)) < 5.0,
            "{typed}"
        );
    }
    // the unit's word is no wildcard, and a town that names a place is never dropped
    for typed in [
        "شارع الكنيسه 5, مركز طنطا, اسيوط, Egypt",
        "شارع الكنيسه 5, مركز طنطا, اسيوط, 71621, Egypt",
        "شارع المدرسه 7, طنطا, اسيوط, 71631, Egypt",
        // with only the governorate left, a suburb that names nothing still holds
        "شارع الكنيسه 5, الحي الغربي, اسيوط, Egypt",
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
