use super::*;
use crate::{Case, Want, at_house, case, town};

pub const PLACES: &[Place] = &[
    place!(
        "vn",
        "country",
        "",
        "VN",
        "",
        100_000_000,
        (108.0, 15.0),
        "",
        "Việt Nam",
        &[("en", "Vietnam")]
    ),
    place!(
        "vn-lamhai",
        "region",
        "",
        "VN",
        "VN-99",
        200_000,
        (110.0, 12.0),
        "vn",
        "Lâm Hải",
        &[("vi", "Lâm Hải")]
    ),
    place!(
        "vn-minhgiang",
        "locality",
        "city",
        "VN",
        "VN-99",
        80_000,
        (110.0, 12.0),
        "vn-lamhai",
        "Minh Giang",
        &[("vi", "Minh Giang")]
    ),
    place!(
        "vn-binhtruc",
        "county",
        "",
        "VN",
        "VN-99",
        20_000,
        (110.01, 12.01),
        "vn-minhgiang",
        "Bình Trúc",
        &[("vi", "Bình Trúc")]
    ),
    place!(
        "vn-ansen",
        "neighborhood",
        "",
        "VN",
        "VN-99",
        2_000,
        (110.01, 12.01),
        "vn-binhtruc",
        "An Sen",
        &[("vi", "An Sen")]
    ),
];

pub const STREETS: &[Street] = &[Street {
    class: "residential",
    length: 0.0,
    name: "Đường Sao",
    postcode: "",
    city: "Minh Giang",
    cc: "VN",
    lon: 110.0,
    lat: 12.0,
    numbers: &["17"],
}];

pub fn cases() -> Vec<Case> {
    [
        "17 Đường Sao, TP. Minh Giang, Việt Nam",
        "17 Đường Sao, Thành phố Minh Giang, Vietnam",
        "17 Đường Sao, TP. Minh Giang, Tỉnh Lâm Hải",
        "17 Đường Sao, P. An Sen, TP. Minh Giang",
        "17 Đường Sao, Phường An Sen, TP. Minh Giang",
        "17 Đường Sao, Q. Bình Trúc, TP. Minh Giang",
        "17 Đường Sao, Quận Bình Trúc, TP. Minh Giang",
        "17 Đường Sao, Huyện Bình Trúc, TP. Minh Giang",
        "17 Đường Sao, Xã An Sen, TP. Minh Giang",
    ]
    .into_iter()
    .map(|q| case(q, at_house("Đường Sao", "17")))
    .chain([
        case("TP. Minh Giang, Tỉnh Lâm Hải", town("vn-minhgiang", true)),
        case("TP. Minh Giang, Lâm Hải, 47", town("vn-minhgiang", false)),
        case("17 Đường Sao, TP. Minh Giang, France", Want::Abstain),
        case("P. Paris, France", town("fr-paris", true)),
        case(
            "88 Unknown Road, Phường An Sen, Minh Giang, Vietnam",
            town("vn-minhgiang", false),
        ),
        case("Phường Không Tồn Tại", Want::Abstain),
    ])
    .collect()
}
