use super::*;
use crate::{Case, at_house, case, index, metres, street};

pub const PLACES: &[Place] = &[
    place!(
        "no",
        "country",
        "",
        "NO",
        "",
        5_000_000,
        (9.0, 62.0),
        "",
        "Norway",
        &[("en", "Norway")]
    ),
    place!(
        "no-fjordvik",
        "locality",
        "town",
        "NO",
        "",
        20_000,
        (10.8, 61.2),
        "no",
        "Fjordvik",
        &[("en", "Fjordvik")]
    ),
    place!(
        "no-postalvik",
        "county",
        "",
        "NO",
        "",
        1_000,
        (18.0, 62.0),
        "no",
        "Postalvik",
        &[("en", "Postalvik")]
    ),
    place!(
        "no-birchlet",
        "locality",
        "village",
        "NO",
        "",
        100,
        (18.0, 62.0),
        "no-postalvik",
        "Birchlet",
        &[("en", "Birchlet")]
    ),
];

pub const STREETS: &[Street] = &[
    RECOVERY_STREET,
    Street {
        postcode: "6782",
        numbers: &["52"],
        lon: 18.06,
        ..RECOVERY_STREET
    },
    Street {
        name: "Delivery Road",
        postcode: "6799",
        lon: 18.003,
        ..RECOVERY_STREET
    },
    Street {
        name: "Twin Road",
        lon: 18.004,
        ..RECOVERY_STREET
    },
    Street {
        name: "Twin Road",
        lon: 18.15,
        ..RECOVERY_STREET
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Birchveien - North",
        postcode: "1234",
        city: "Fjordvik",
        cc: "NO",
        lon: 10.805,
        lat: 61.20,
        numbers: &["5b"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Elmveien - North",
        postcode: "1234",
        city: "Fjordvik",
        cc: "NO",
        lon: 10.806,
        lat: 61.20,
        numbers: &["7"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Elmvegen - North",
        postcode: "1234",
        city: "Fjordvik",
        cc: "NO",
        lon: 10.807,
        lat: 61.20,
        numbers: &["7"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Pinevn. - North",
        postcode: "1234",
        city: "Fjordvik",
        cc: "NO",
        lon: 10.808,
        lat: 61.20,
        numbers: &["5"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Pineveien - North",
        postcode: "1234",
        city: "Fjordvik",
        cc: "NO",
        lon: 10.809,
        lat: 61.20,
        numbers: &["5"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Granveien",
        postcode: "1234",
        city: "Fjordvik",
        cc: "NO",
        lon: 10.80,
        lat: 61.20,
        numbers: &["5b"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Bjørkvegen",
        postcode: "1234",
        city: "Fjordvik",
        cc: "NO",
        lon: 10.81,
        lat: 61.20,
        numbers: &["12d"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Askveien",
        postcode: "1234",
        city: "Fjordvik",
        cc: "NO",
        lon: 10.82,
        lat: 61.20,
        numbers: &["7"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Askvegen",
        postcode: "1234",
        city: "Fjordvik",
        cc: "NO",
        lon: 10.83,
        lat: 61.20,
        numbers: &["7"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Furuvn.",
        postcode: "1234",
        city: "Fjordvik",
        cc: "NO",
        lon: 10.84,
        lat: 61.20,
        numbers: &["5"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Furuveien",
        postcode: "1234",
        city: "Fjordvik",
        cc: "NO",
        lon: 10.85,
        lat: 61.20,
        numbers: &["5"],
    },
];

pub fn cases() -> Vec<Case> {
    vec![
        case(
            "Granveien 5B, 1234 Fjordvik, Norway",
            at_house("Granveien", "5b"),
        ),
        case(
            "Granvn. 5B, 1234 Fjordvik, Norway",
            at_house("Granveien", "5b"),
        ),
        case("Granvn. 5B, 1234, Norway", at_house("Granveien", "5b")),
        case(
            "Bjørkvegen 12d, 1234 Fjordvik, Norway",
            at_house("Bjørkvegen", "12d"),
        ),
        case(
            "Bjørkvn. 12d, 1234 Fjordvik, Norway",
            at_house("Bjørkvegen", "12d"),
        ),
        case("Furuvn. 5, 1234 Fjordvik, Norway", at_house("Furuvn.", "5")),
        case(
            "Birchveien - North 5B, 1234 Fjordvik, Norway",
            at_house("Birchveien - North", "5b"),
        ),
        case(
            "Birchvn. - North 5B, 1234 Fjordvik, Norway",
            at_house("Birchveien - North", "5b"),
        ),
        case(
            "Pinevn. - North 5, 1234 Fjordvik, Norway",
            at_house("Pinevn. - North", "5"),
        ),
    ]
}

#[test]
fn a_whole_address_with_one_part_out_of_place_suggests_its_house() {
    let want = house(street("Granveien"), "5b");
    for typed in [
        "Norway, 1234 Fjordvik, Granveien 5B",
        "Granveien 5B 1234 Fjordvik Norway",
        // a four-digit postcode is read without its country
        "Granveien 5B, 1234 Fjordvik",
        // the town or the street a letter off
        "Granveien 5B, 1234 Fjordvk, Norway",
        "Grnaveien 5B, 1234 Fjordvik, Norway",
    ] {
        let found = index().suggest(typed, 5, None, None, false);
        let first = found.iter().find(|s| s.kind == "address");
        assert!(
            first.is_some_and(|s| metres((s.x, s.y), want) < 5.0),
            "{typed}: {:?}",
            found.iter().map(|s| &s.display_name).collect::<Vec<_>>()
        );
    }
    // a postal town a letter off
    let found = index().suggest(
        "Dispatchveien 51, 6780 Psotalvik, Norway",
        5,
        None,
        None,
        false,
    );
    assert_eq!(found.first().map(|s| s.kind), Some("address"));
    assert!(
        metres(
            (found[0].x, found[0].y),
            house(street("Dispatchveien"), "51")
        ) < 5.0
    );
    // a number the street lacks, another place's postcode or another town of the postcode, and
    // the house in a country not asked for
    for (typed, cc) in [
        ("Granveien 99, 1234 Fjordvik, Norway", None),
        ("Granveien 5B, 6780 Fjordvik, Norway", None),
        ("Granveien 5B, 1234 Birchlet, Norway", None),
        ("Granveien 5B, 1234 Fjordvik", Some("SE")),
    ] {
        assert!(
            index()
                .suggest(typed, 5, cc, None, false)
                .iter()
                .all(|s| s.kind != "address"),
            "{typed}"
        );
    }
}
