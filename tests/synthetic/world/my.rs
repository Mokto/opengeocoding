use super::*;

pub const PLACES: &[Place] = &[
    place!(
        "my",
        "country",
        "",
        "MY",
        "",
        34_000_000,
        (102.0, 4.0),
        "",
        "Malaysia",
        &[("en", "Malaysia")],
    ),
    place!(
        "my-03",
        "region",
        "",
        "MY",
        "MY-03",
        1_900_000,
        (102.0, 5.3),
        "my",
        "Kelantan",
        &[("en", "Kelantan")],
    ),
    place!(
        "my-03-kota-bharu",
        "locality",
        "city",
        "MY",
        "MY-03",
        500_000,
        (102.24, 6.12),
        "my-03",
        "Kota Bharu",
        &[("en", "Kota Bharu")],
    ),
];

/// Houses OpenStreetMap keys with their label ("PT 421", "Block 3 Lot 21"); house "21" is
/// another house than "Lot 21".
pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Jalan Sultan Ibrahim",
        postcode: "15100",
        city: "Kota Bharu",
        cc: "MY",
        lon: 102.238,
        lat: 6.123,
        numbers: &["PT 421", "21"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Jalan Hamzah",
        postcode: "15050",
        city: "Kota Bharu",
        cc: "MY",
        lon: 102.245,
        lat: 6.118,
        numbers: &["Block 3 Lot 21", "Plot 5574"],
    },
];
