use super::*;
use crate::{Case, at_house, case, index, metres, street};

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
    place!(
        "th-suphan-buri",
        "region",
        "",
        "TH",
        "TH-72",
        850_000,
        (100.1, 14.6),
        "th",
        "จังหวัดสุพรรณบุรี",
        &[("th", "จังหวัดสุพรรณบุรี"), ("en", "Suphan Buri Province")]
    ),
    place!(
        "th-u-thong",
        "locality",
        "town",
        "TH",
        "TH-72",
        12_000,
        (99.87, 14.33),
        "th-suphan-buri",
        "อู่ทอง",
        &[("th", "อู่ทอง"), ("en", "U Thong")]
    ),
    place!(
        "th-ubon",
        "region",
        "",
        "TH",
        "TH-34",
        1_800_000,
        (104.6, 15.0),
        "th",
        "จังหวัดอุบลราชธานี",
        &[
            ("th", "จังหวัดอุบลราชธานี"),
            ("en", "Ubon Ratchathani Province")
        ]
    ),
    place!(
        "th-bangkok",
        "region",
        "",
        "TH",
        "TH-10",
        10_000_000,
        (100.5, 13.9),
        "th",
        "กรุงเทพมหานคร",
        &[("th", "กรุงเทพมหานคร"), ("en", "Bangkok")]
    ),
    place!(
        "th-sathon",
        "locality",
        "city",
        "TH",
        "TH-10",
        80_000,
        (100.53, 13.72),
        "th-bangkok",
        "สาทร",
        &[("th", "สาทร"), ("en", "Sathon")]
    ),
    place!(
        "th-bueng-kan",
        "region",
        "",
        "TH",
        "TH-38",
        420_000,
        (103.6, 18.4),
        "th",
        "จังหวัดบึงกาฬ",
        &[("th", "จังหวัดบึงกาฬ"), ("en", "Bueng Kan Province")]
    ),
    place!(
        "th-chanthaburi",
        "region",
        "",
        "TH",
        "TH-22",
        530_000,
        (102.1, 12.6),
        "th",
        "จังหวัดจันทบุรี",
        &[("th", "จังหวัดจันทบุรี"), ("en", "Chanthaburi Province")]
    ),
    place!(
        "th-chanthaburi-town",
        "locality",
        "city",
        "TH",
        "TH-22",
        100_000,
        (102.1, 12.61),
        "th-chanthaburi",
        "จันทบุรี",
        &[("th", "จันทบุรี"), ("en", "Chanthaburi")]
    ),
    place!(
        "th-soi-dao",
        "county",
        "",
        "TH",
        "TH-22",
        70_000,
        (102.25, 13.2),
        "th-chanthaburi",
        "อำเภอสอยดาว",
        &[("th", "อำเภอสอยดาว"), ("en", "Soi Dao")]
    ),
    place!(
        "th-thung-khanan",
        "locality",
        "village",
        "TH",
        "TH-22",
        2_000,
        (102.28, 13.24),
        "th-soi-dao",
        "บ้านทุ่งขนาน",
        &[("th", "บ้านทุ่งขนาน"), ("en", "Ban Thung Khanan")]
    ),
    place!(
        "th-trakan",
        "county",
        "",
        "TH",
        "TH-34",
        120_000,
        (105.04, 15.55),
        "th-ubon",
        "อำเภอตระการพืชผล",
        &[("th", "อำเภอตระการพืชผล"), ("en", "Trakan Phuet Phon")]
    ),
    place!(
        "th-don-tai",
        "locality",
        "village",
        "TH",
        "TH-34",
        1_500,
        (105.05, 15.56),
        "th-trakan",
        "บ้านดอนใต้",
        &[("th", "บ้านดอนใต้"), ("en", "Ban Don Tai")]
    ),
    place!(
        "th-seka",
        "county",
        "",
        "TH",
        "TH-38",
        80_000,
        (103.99, 18.05),
        "th-bueng-kan",
        "อำเภอเซกา",
        &[("th", "อำเภอเซกา"), ("en", "Seka")]
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
    Street {
        class: "residential",
        length: 0.0,
        name: "1",
        postcode: "22180",
        city: "บ้านทุ่งขนาน",
        cc: "TH",
        lon: 102.279,
        lat: 13.2436,
        numbers: &["108/2"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "ถนนมาลัยแมน",
        postcode: "72160",
        city: "อู่ทอง",
        cc: "TH",
        lon: 99.871,
        lat: 14.33,
        numbers: &["47"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "ชลประทาน",
        postcode: "34130",
        city: "บ้านดอนใต้",
        cc: "TH",
        lon: 105.041,
        lat: 15.553,
        numbers: &["132 หมู่ที่ 2"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "ซอยเกอเธ่",
        postcode: "10120",
        city: "สาทร",
        cc: "TH",
        lon: 100.546,
        lat: 13.723,
        numbers: &["18/8"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "ถนนสาทรใต้",
        postcode: "10120",
        city: "สาทร",
        cc: "TH",
        lon: 100.535,
        lat: 13.722,
        numbers: &["1"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "หมู่9",
        postcode: "38150",
        city: "",
        cc: "TH",
        lon: 103.995,
        lat: 18.128,
        numbers: &["35"],
    },
];

/// OpenStreetMap's Thai addresses as written: a village number (หมู่) or a bare number as the
/// street ("1"), the district and province, then the postcode. libpostal joins the bare street to
/// the house number, takes the village word for the town, cuts อู่ทอง before its vowel mark,
/// reads the lane's number (ซอยสาทร 1) as the house's and the province (Bueng Kan) as the town.
pub fn cases() -> Vec<Case> {
    vec![
        case(
            "108/2 1, สอยดาว, จันทบุรี, 22180, Thailand",
            at_house("1", "108/2"),
        ),
        case(
            "1 108/2, สอยดาว, จันทบุรี, 22180, Thailand",
            at_house("1", "108/2"),
        ),
        case(
            "47 ถนนมาลัยแมน, อำเภออู่ทอง, จังหวัดสุพรรณบุรี, 72160, Thailand",
            at_house("ถนนมาลัยแมน", "47"),
        ),
        case(
            "132 หมู่ที่ 2 ชลประทาน, นาพิน,ตระการพืชผล, 34130, Thailand",
            at_house("ชลประทาน", "132 หมู่ที่ 2"),
        ),
        case(
            "18/8 ซอยเกอเธ่ ซอยสาทร 1 ถนนสาทรใต้ แขวงทุ่งมหาเมฆ เขตสาทร กรุงเทพมหานคร 10120, Thailand",
            at_house("ซอยเกอเธ่", "18/8"),
        ),
        case(
            "35 หมู่9, Seka, Bueng Kan, 38150, Thailand",
            at_house("หมู่9", "35"),
        ),
    ]
}

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
