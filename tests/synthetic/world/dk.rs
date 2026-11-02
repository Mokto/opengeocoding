use super::*;
use crate::*;

pub const POINT_ONLY: &[&str] = &[
    "dk-close-village",
    "dk-confirmed-west",
    "dk-confirmed-east",
    "dk-wrong-village",
    "dk-overlap-town",
    "dk-overlap-village",
];

pub const AREAS: &[(&str, &str, bool, bool, f64, f64, f64)] = &[
    ("dk-north", "region", true, true, 12.2, 57.0, 0.6),
    ("dk-wood-west", "county", true, true, 12.0, 57.0, 0.2),
    ("dk-wood-east", "county", true, true, 12.4, 57.0, 0.2),
];

pub fn write_localities(path: &std::path::Path) {
    use std::io::Write;
    let records: Vec<_> = [
        (1, "Vestskov", 11.8, 12.2, 56.8, 57.2),
        (2, "Østerskov", 12.2, 12.6, 56.8, 57.2),
        (3, "Tvistskov", 12.09, 12.11, 57.04, 57.06),
    ]
    .into_iter()
    .map(|(id, name, left, right, bottom, top)| {
        let mut previous = [0i32; 2];
        let ring: Vec<_> = [
            [left, bottom],
            [right, bottom],
            [right, top],
            [left, top],
            [left, bottom],
        ]
        .map(|point| {
            let point = point.map(|v: f64| (v * 1e7).round() as i32);
            let delta = [point[0] - previous[0], point[1] - previous[1]];
            previous = point;
            delta
        })
        .into_iter()
        .collect();
        (id, name, vec![ring])
    })
    .collect();
    let mut out = flate2::write::GzEncoder::new(
        std::fs::File::create(path).unwrap(),
        flate2::Compression::default(),
    );
    out.write_all(&serde_json::to_vec(&records).unwrap())
        .unwrap();
    out.finish().unwrap();
}

pub const PLACES: &[Place] = &[
    place!(
        "dk",
        "country",
        "",
        "DK",
        "",
        5_000_000,
        (10.0, 56.0),
        "",
        "Denmark",
        &[("en", "Denmark")]
    ),
    place!(
        "dk-north",
        "region",
        "",
        "DK",
        "DK-81",
        600_000,
        (10.0, 57.0),
        "dk",
        "Region Nordjylland",
        &[("en", "North Denmark Region")]
    ),
    place!(
        "dk-border-west",
        "locality",
        "town",
        "DK",
        "DK-81",
        20_000,
        (11.0, 57.0),
        "dk-north",
        "Vesterhavn",
        &[]
    ),
    place!(
        "dk-border-east",
        "locality",
        "town",
        "DK",
        "DK-81",
        20_000,
        (11.4, 57.0),
        "dk-north",
        "Østerhavn",
        &[]
    ),
    place!(
        "dk-close-village",
        "locality",
        "village",
        "DK",
        "DK-81",
        100,
        (11.19975, 57.0),
        "dk-north",
        "Nærby",
        &[]
    ),
    place!(
        "dk-wood-west",
        "county",
        "",
        "DK",
        "DK-81",
        30_000,
        (12.0, 57.0),
        "dk-north",
        "Vestskov Kommune",
        &[("en", "Westwood Municipality")]
    ),
    place!(
        "dk-wood-east",
        "county",
        "",
        "DK",
        "DK-81",
        30_000,
        (12.4, 57.0),
        "dk-north",
        "Østerskov Kommune",
        &[("en", "Eastwood Municipality")]
    ),
    place!(
        "dk-confirmed-west",
        "locality",
        "town",
        "DK",
        "DK-81",
        20_000,
        (12.0, 57.0),
        "dk-wood-west",
        "Vestskov",
        &[]
    ),
    place!(
        "dk-confirmed-east",
        "locality",
        "town",
        "DK",
        "DK-81",
        20_000,
        (12.4, 57.0),
        "dk-wood-east",
        "Østerskov",
        &[]
    ),
    place!(
        "dk-wrong-village",
        "locality",
        "village",
        "DK",
        "DK-81",
        100,
        (12.19975, 57.0),
        "dk-wood-west",
        "Fristby",
        &[]
    ),
    place!(
        "dk-overlap-town",
        "locality",
        "town",
        "DK",
        "DK-81",
        10_000,
        (12.1, 57.05),
        "dk-wood-west",
        "Tvistskov",
        &[]
    ),
    place!(
        "dk-overlap-village",
        "locality",
        "village",
        "DK",
        "DK-81",
        100,
        (12.10005, 57.05),
        "dk-wood-west",
        "Tvistby",
        &[]
    ),
    place!(
        "dk-havnholm",
        "locality",
        "town",
        "DK",
        "",
        20_000,
        (10.4, 56.4),
        "dk",
        "Havnholm",
        &[("en", "Havnholm")]
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Vestskovgrænsen",
        postcode: "9810",
        city: "Vestskov C",
        cc: "DK",
        lon: 12.1998,
        lat: 57.0,
        numbers: &["12"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Østerskovgrænsen",
        postcode: "9810",
        city: "Østerskov",
        cc: "DK",
        lon: 12.2002,
        lat: 57.0,
        numbers: &["14"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Tvistvej",
        postcode: "9810",
        city: "Tvistskov",
        cc: "DK",
        lon: 12.1001,
        lat: 57.05,
        numbers: &["18"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Ubekræftetvej",
        postcode: "9810",
        city: "Postholm",
        cc: "DK",
        lon: 12.1998,
        lat: 57.001,
        numbers: &["16"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Vestergrænsevej",
        postcode: "9800",
        city: "Postholm",
        cc: "DK",
        lon: 11.1998,
        lat: 57.0,
        numbers: &["12"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Østergrænsevej",
        postcode: "9800",
        city: "Postholm",
        cc: "DK",
        lon: 11.2002,
        lat: 57.0,
        numbers: &["14"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Kajgade",
        postcode: "8600",
        city: "Postholm",
        cc: "DK",
        lon: 10.45,
        lat: 56.4,
        numbers: &["5"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Skibsgade",
        postcode: "8500",
        city: "Havnholm",
        cc: "DK",
        lon: 10.4,
        lat: 56.4,
        numbers: &["9"],
    },
];

#[test]
fn danish_reverse_keeps_geographic_membership_separate_from_postal_and_nearest_names() {
    let west = house(street("Vestergrænsevej"), "12");
    let nearest = index().reverse(west.1, west.0);
    assert_eq!(nearest["address"]["village"], "Nærby");
    for (point, road, number, town) in [
        (west, "Vestergrænsevej", "12", "Vesterhavn"),
        (
            house(street("Østergrænsevej"), "14"),
            "Østergrænsevej",
            "14",
            "Østerhavn",
        ),
    ] {
        let found = index().reverse_search(point.1, point.0, 100.0, 1);
        assert_eq!(found.len(), 1);
        let (layer, hit, distance) = &found[0];
        let address = &hit["address"];
        assert_eq!(*layer, "address");
        assert_eq!(address["house_number"], number);
        assert_eq!(address["road"], road);
        assert_eq!(address["town"], town);
        assert_eq!(address["postal_town"], "Postholm");
        assert_eq!(address["postcode"], "9800");
        assert_eq!(address["country_code"], "dk");
        assert_eq!(address["ISO3166-2-lvl4"], "DK-81");
        assert!(address.get("village").is_none());
        let returned = (
            hit["lon"].as_str().unwrap().parse().unwrap(),
            hit["lat"].as_str().unwrap().parse().unwrap(),
        );
        assert!(metres(returned, point) < 0.2);
        assert!(*distance < 0.2);
    }
}

#[test]
fn danish_reverse_offset_can_select_a_house_across_a_locality_boundary() {
    let west = house(street("Vestergrænsevej"), "12");
    let east = house(street("Østergrænsevej"), "14");
    let offset = (
        west.0 + 25.0 / (111_195.0 * west.1.to_radians().cos()),
        west.1,
    );
    assert!(offset.0 > 11.2);
    let found = index().reverse_search(offset.1, offset.0, 100.0, 1);
    let address = &found[0].1["address"];
    assert_eq!(address["house_number"], "14");
    assert_eq!(address["town"], "Østerhavn");
    assert_eq!(address["postal_town"], "Postholm");
    assert_eq!(address["postcode"], "9800");
    assert_eq!(address["ISO3166-2-lvl4"], "DK-81");
    let at_east = index().reverse_search(east.1, east.0, 100.0, 1);
    assert_eq!(found[0].1, at_east[0].1);
}

#[test]
fn official_membership_repairs_the_locality_without_changing_the_selected_house() {
    for (road, number, town, postal, county) in [
        (
            "Vestskovgrænsen",
            "12",
            "Vestskov",
            "Vestskov C",
            "Westwood Municipality",
        ),
        (
            "Østerskovgrænsen",
            "14",
            "Østerskov",
            "Østerskov",
            "Eastwood Municipality",
        ),
    ] {
        let point = house(street(road), number);
        assert_eq!(
            index().reverse(point.1, point.0)["address"]["village"],
            "Fristby"
        );
        let found = index().reverse_search(point.1, point.0, 100.0, 1);
        let (layer, hit, distance) = &found[0];
        let address = &hit["address"];
        assert_eq!(*layer, "address");
        assert_eq!(address["house_number"], number);
        assert_eq!(address["road"], road);
        assert_eq!(address["town"], town);
        assert!(address.get("village").is_none());
        assert_eq!(address["postal_town"], postal);
        assert_eq!(address["postcode"], "9810");
        assert_eq!(address["county"], county);
        assert_eq!(address["country_code"], "dk");
        assert_eq!(address["ISO3166-2-lvl4"], "DK-81");
        assert!(!hit["display_name"].as_str().unwrap().contains("Fristby"));
        let returned = (
            hit["lon"].as_str().unwrap().parse().unwrap(),
            hit["lat"].as_str().unwrap().parse().unwrap(),
        );
        assert!(metres(returned, point) < 0.2);
        assert!(*distance < 0.2);
    }
    let west = house(street("Vestskovgrænsen"), "12");
    let offset = west.0 + 25.0 / (111_195.0 * west.1.to_radians().cos());
    let found = index().reverse_search(west.1, offset, 100.0, 1);
    assert_eq!(found[0].1["address"]["house_number"], "14");
    assert_eq!(found[0].1["address"]["town"], "Østerskov");
    let unconfirmed = house(street("Ubekræftetvej"), "16");
    let found = index().reverse_search(unconfirmed.1, unconfirmed.0, 5.0, 1);
    assert_eq!(found[0].1["address"]["village"], "Fristby");
    assert_eq!(found[0].1["address"]["postal_town"], "Postholm");
    let disputed = house(street("Tvistvej"), "18");
    let found = index().reverse_search(disputed.1, disputed.0, 5.0, 1);
    assert_eq!(found[0].1["address"]["village"], "Tvistby");
    assert!(found[0].1["address"].get("town").is_none());
    assert_eq!(found[0].1["address"]["postal_town"], "Tvistskov");
}

pub fn cases() -> Vec<Case> {
    vec![
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
    ]
}
