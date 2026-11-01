use super::*;
use crate::{index, metres};
use opengeocoding::resolver::Resolver;

pub const PLACES: &[Place] = &[
    place!(
        "gl",
        "dependency",
        "",
        "GL",
        "",
        10_000,
        (-45.0, 63.0),
        "dk",
        "Greenland",
        &[("da", "Grønland")]
    ),
    place!(
        "gl-fjordhaven",
        "locality",
        "town",
        "GL",
        "",
        5_000,
        (-45.1, 63.1),
        "gl",
        "Fjordhaven",
        &[]
    ),
    place!(
        "gl-icehaven",
        "locality",
        "town",
        "GL",
        "",
        5_000,
        (-45.3, 63.3),
        "gl",
        "Icehaven",
        &[]
    ),
];

pub const STREETS: &[Street] = &[Street {
    name: "Seal Road",
    class: "residential",
    length: 0.,
    postcode: "3900",
    city: "Fjordhaven",
    cc: "GL",
    lon: -45.101,
    lat: 63.101,
    numbers: &["345"],
}];

pub fn write(path: &Path) {
    std::fs::write(
        path,
        concat!(
            "9900FJO\tFjordhaven\tB-345\t63.103\t-45.103\n",
            "9900FJO\tFjordhaven\tB-3456\t63.104\t-45.104\n",
            "9900ICE\tIcehaven\tB-345\t63.303\t-45.303\n",
        ),
    )
    .unwrap();
}

#[test]
fn references_are_scoped_not_house_numbers_or_landmarks() {
    for text in [
        "B-345, Fjordhaven, Greenland",
        "Fjordhaven, B-345, Grønland",
    ] {
        for landmarks in [false, true] {
            let found = index().suggest(text, 5, None, None, landmarks);
            assert_eq!(found.len(), 1, "{text}");
            assert_eq!(found[0].kind, "building");
            assert_eq!(found[0].category, "");
            assert_eq!(found[0].country_code, "gl");
            assert!(metres((found[0].x, found[0].y), (-45.103, 63.103)) < 1.);
            let retrieved = index().retrieve_suggestion(&found[0].retrieval_id).unwrap();
            assert_eq!(retrieved.display_name, found[0].display_name);
            assert_eq!((retrieved.x, retrieved.y), (found[0].x, found[0].y));
        }
        assert!(
            index()
                .suggest(text, 5, Some("dk"), None, false)
                .iter()
                .all(|s| s.kind != "building")
        );
    }
    let other = index().suggest("B-345, Icehaven, Greenland", 5, None, None, false);
    assert_eq!(other.len(), 1);
    assert!(metres((other[0].x, other[0].y), (-45.303, 63.303)) < 1.);
    assert_eq!(
        index()
            .suggest("Fjordhaven, B-345", 5, Some("GL"), None, false)
            .len(),
        2
    );
    assert_eq!(
        index()
            .suggest("Fjordhaven, B-345 ", 5, Some("gl"), None, false)
            .len(),
        1
    );
    for text in ["B-999, Fjordhaven, Greenland", "B-345, Unknown, Greenland"] {
        assert!(index().suggest(text, 5, None, None, false).is_empty());
    }
    for text in [
        "B-345, Fjordhaven, Germany",
        "Fjordhaven, B-345",
        "345, Fjordhaven, Greenland",
    ] {
        assert!(
            index()
                .suggest(text, 5, None, None, false)
                .iter()
                .all(|s| s.kind != "building")
        );
    }
    assert!(
        index()
            .suggest("345 Seal Road, Fjordhaven, Greenland", 5, None, None, false)
            .iter()
            .any(|s| s.kind == "address")
    );
}

pub async fn check_search(resolver: &Resolver) {
    for text in [
        "B-345, Fjordhaven, Greenland",
        "Fjordhaven, b-345, Greenland",
    ] {
        let got = resolver.resolve(text).await.unwrap();
        let at = got.geo_coordinates.as_option().unwrap();
        let d = got.address_details.as_option().unwrap();
        assert!(got.good_match, "{text}: {got:?}");
        assert_eq!(d.house_name, "B-345");
        assert_eq!(d.house_number, "");
        assert_eq!(d.road, "");
        assert_eq!(d.city, "Fjordhaven");
        assert!(metres((at.lon as f64, at.lat as f64), (-45.103, 63.103)) < 1.);
    }
    let missing = resolver
        .resolve("B-999, Fjordhaven, Greenland")
        .await
        .unwrap();
    assert!(!missing.good_match && missing.geo_coordinates.as_option().is_none());
    let house = resolver
        .resolve("345 Seal Road, Fjordhaven, Greenland")
        .await
        .unwrap();
    assert!(house.good_match);
    assert_eq!(
        house.address_details.as_option().unwrap().house_number,
        "345"
    );
    assert_eq!(house.address_details.as_option().unwrap().house_name, "");
}
