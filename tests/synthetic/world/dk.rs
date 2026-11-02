use super::*;
use crate::*;

pub const POINT_ONLY: &[&str] = &["dk-close-village"];

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
