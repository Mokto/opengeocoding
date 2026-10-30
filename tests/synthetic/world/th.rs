use super::*;
use crate::{index, metres, street};

pub const PLACES: &[Place] = &[
    place!(
        "th",
        "country",
        "",
        "TH",
        "",
        66_000_000,
        (100.5, 13.75),
        "",
        "ประเทศไทย",
        &[("th", "ประเทศไทย"), ("en", "Thailand")]
    ),
    place!(
        "th-ban-phaeo",
        "locality",
        "town",
        "TH",
        "",
        10_000,
        (100.1, 13.59),
        "th",
        "บ้านแพ้ว",
        &[("th", "บ้านแพ้ว")]
    ),
    place!(
        "th-ban-rai",
        "locality",
        "town",
        "TH",
        "",
        10_000,
        (99.46, 15.24),
        "th",
        "บ้านไร่",
        &[("th", "บ้านไร่")]
    ),
];

/// OpenStreetMap keys a Thai house with its village (หมู่) or its range of units, as written.
/// The second street is named, as many mapped there are, by its subdistrict, district and
/// province.
pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "พระประโทน-บ้านแพ้ว",
        postcode: "74120",
        city: "บ้านแพ้ว",
        cc: "TH",
        lon: 100.101,
        lat: 13.59,
        numbers: &["171 ม.8", "225 หมู่ที่ 2", "11 moo 1", "267/108-109"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "ต.คอกควาย อ.บ้านไร่ จ.อุทัยธานี",
        postcode: "61140",
        city: "บ้านไร่",
        cc: "TH",
        lon: 99.461,
        lat: 15.24,
        numbers: &["159 หมู่8", "บ้านเลขที่32 หมู่8"],
    },
];

#[test]
fn thai_houses_are_suggested_with_their_village_or_unit_range() {
    let road = "พระประโทน-บ้านแพ้ว";
    let admin = "ต.คอกควาย อ.บ้านไร่ จ.อุทัยธานี";
    for (typed, on, number) in [
        (
            "171 ม.8 พระประโทน-บ้านแพ้ว, บ้านแพ้ว, 74120, Thailand",
            road,
            "171 ม.8",
        ),
        (
            "พระประโทน-บ้านแพ้ว 171 ม.8, บ้านแพ้ว, 74120, Thailand",
            road,
            "171 ม.8",
        ),
        ("พระประโทน-บ้านแพ้ว 171 ม.8", road, "171 ม.8"),
        (
            "225 หมู่ที่ 2 พระประโทน-บ้านแพ้ว, 74120, Thailand",
            road,
            "225 หมู่ที่ 2",
        ),
        (
            "11 Moo 1 พระประโทน-บ้านแพ้ว, 74120, Thailand",
            road,
            "11 moo 1",
        ),
        (
            "267/108-109 พระประโทน-บ้านแพ้ว, 74120, Thailand",
            road,
            "267/108-109",
        ),
        (
            "159 หมู่8 ต.คอกควาย อ.บ้านไร่ จ.อุทัยธานี, 61140, Thailand",
            admin,
            "159 หมู่8",
        ),
        (
            "ต.คอกควาย อ.บ้านไร่ จ.อุทัยธานี บ้านเลขที่32 หมู่8, 61140, Thailand",
            admin,
            "บ้านเลขที่32 หมู่8",
        ),
    ] {
        let found = index().suggest(typed, 5, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{typed}");
        assert!(
            metres((found[0].x, found[0].y), house(street(on), number)) < 5.0,
            "{typed}"
        );
        assert!(
            index()
                .retrieve_suggestion(&found[0].retrieval_id)
                .is_some(),
            "{typed}"
        );
    }
    // a village no house of that number is in is no house
    assert!(
        index()
            .suggest(
                "171 ม.9 พระประโทน-บ้านแพ้ว, 74120, Thailand",
                5,
                None,
                None,
                false
            )
            .iter()
            .all(|s| s.kind != "address")
    );
}
