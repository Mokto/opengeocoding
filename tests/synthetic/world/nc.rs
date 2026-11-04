use super::*;
use crate::{Case, Resolver, at_house, case};

pub const PLACES: &[Place] = &[
    place!(
        "nc",
        "dependency",
        "",
        "NC",
        "",
        200_000,
        (160.0, -30.03),
        "fr",
        "New Caledonia",
        &[("en", "New Caledonia")]
    ),
    place!(
        "nc-rocail",
        "locality",
        "village",
        "NC",
        "",
        2_000,
        (160.02, -30.03),
        "nc",
        "Rocail",
        &[]
    ),
];

const ROUTE: Street = Street {
    class: "residential",
    length: 0.0,
    name: "Route Territoriale 7",
    postcode: "98880",
    city: "Rocail",
    cc: "NC",
    lon: 160.01,
    lat: -30.03,
    numbers: &["271", "813245", "61bis"],
};

pub const STREETS: &[Street] = &[
    ROUTE,
    Street {
        name: "Route Territoriale 8",
        lon: 160.03,
        numbers: &["999"],
        ..ROUTE
    },
    Street {
        name: "Rue du Corail",
        postcode: "98881",
        lon: 160.04,
        numbers: &["83"],
        ..ROUTE
    },
    Street {
        name: "Chemin du Sable",
        postcode: "30500",
        lon: 160.05,
        numbers: &["427"],
        ..ROUTE
    },
    Street {
        name: "Voie des Embruns",
        postcode: "30500",
        cc: "FR",
        lon: 160.06,
        numbers: &["503"],
        ..ROUTE
    },
    Street {
        name: "Rue Astrale",
        postcode: "",
        lon: 160.07,
        numbers: &["592"],
        ..ROUTE
    },
    Street {
        name: "Rue Franche",
        postcode: "30500",
        city: "Paris",
        cc: "FR",
        lon: 2.32,
        lat: 48.86,
        numbers: &["427"],
        ..ROUTE
    },
];

pub const FEATURES: &[Feature] = &[
    Feature {
        shape: Shape::Node,
        lon: 160.08,
        lat: -30.03,
        tags: &[
            ("addr:street", "Rue des Brisants"),
            ("addr:housenumber", "735421"),
            ("addr:postcode", "98880"),
            ("addr:city", "Rocail"),
        ],
    },
    Feature {
        shape: Shape::Node,
        lon: 160.09,
        lat: -30.03,
        tags: &[
            ("addr:street", "Rue du Lagon"),
            ("addr:housenumber", "8491"),
            ("addr:postcode", "75001"),
            ("addr:city", "Paris"),
        ],
    },
    Feature {
        shape: Shape::Node,
        lon: 160.011,
        lat: -30.03,
        tags: &[
            ("addr:street", "Route Territoriale 7"),
            ("addr:housenumber", "271"),
            ("addr:postcode", "98880"),
            ("addr:city", "Rocail"),
        ],
    },
];

pub fn cases() -> Vec<Case> {
    vec![
        case(
            "271 Route Territoriale 7, Rocail, 98880, New Caledonia",
            at_house(ROUTE.name, "271"),
        ),
        case(
            "Route Territoriale 7 271, Rocail, 98880, New Caledonia",
            at_house(ROUTE.name, "271"),
        ),
        case(
            "813245 Route Territoriale 7, Rocail, 98880, New Caledonia",
            at_house(ROUTE.name, "813245"),
        ),
        case(
            "61 bis Route Territoriale 7, Rocail, 98880, New Caledonia",
            at_house(ROUTE.name, "61bis"),
        ),
    ]
}

#[test]
fn nc_house_postcodes_keep_the_territory_namespace() {
    use opengeocoding::index::country::nc::permits_house_postcode;

    for postcode in [
        None,
        Some(""),
        Some("98859"),
        Some("98860"),
        Some(" 98880 "),
    ] {
        assert!(
            permits_house_postcode(*b"NC", None, postcode),
            "{postcode:?}"
        );
    }
    for postcode in ["30500", "75001", "9888O", "988800", "98880;75001"] {
        assert!(
            !permits_house_postcode(*b"NC", None, Some(postcode)),
            "{postcode}"
        );
    }
    assert!(permits_house_postcode(*b"FR", Some(*b"FR"), Some("30500")));
    assert!(!permits_house_postcode(*b"FR", Some(*b"NC"), Some("30500")));
    assert!(!permits_house_postcode(*b"NC", Some(*b"FR"), Some("30500")));
}

#[test]
fn nc_foreign_postcodes_keep_metadata_but_do_not_supply_houses() {
    let index = crate::index();
    for (query, country) in [
        ("427 Chemin du Sable", "NC"),
        ("503 Voie des Embruns", "FR"),
        ("8491 Rue du Lagon", "NC"),
    ] {
        let found = index.suggest(query, 5, Some(country), None, false);
        assert!(found.iter().all(|s| s.kind != "address"), "{query}");
    }
    use tantivy::schema::Value as _;
    let raw = tantivy::Index::open_in_dir(crate::dir()).unwrap();
    let searcher = raw.reader().unwrap().searcher();
    let (_, fields) = opengeocoding::index::schema::schema();
    let docs = searcher
        .search(
            &tantivy::query::AllQuery,
            &tantivy::collector::DocSetCollector,
        )
        .unwrap();
    assert!(docs.into_iter().any(|address| {
        let doc = searcher.doc::<tantivy::TantivyDocument>(address).unwrap();
        doc.get_first(fields.name).and_then(|v| v.as_str()) == Some("Chemin du Sable")
            && doc
                .get_all(fields.pcode)
                .any(|v| v.as_str() == Some("30500"))
    }));
    let postcodes = index.postcode_hits("30500", Some("fr"));
    assert_eq!(postcodes.len(), 1);
    let postcode = index.render(&postcodes[0], true);
    let point = (
        postcode["lon"].as_str().unwrap().parse().unwrap(),
        postcode["lat"].as_str().unwrap().parse().unwrap(),
    );
    assert!(crate::metres(point, (2.32, 48.86)) < 5.);
    let street = index.suggest("Chemin du Sable", 5, Some("NC"), None, false);
    assert!(street.iter().any(|s| s.kind == "street"));
    for (query, point) in [
        ("271 Route Territoriale 7", house(&ROUTE, "271")),
        ("813245 Route Territoriale 7", house(&ROUTE, "813245")),
        ("61bis Route Territoriale 7", house(&ROUTE, "61bis")),
        (
            "592 Rue Astrale",
            house(crate::street("Rue Astrale"), "592"),
        ),
        ("735421 Rue des Brisants", (160.08, -30.03)),
        ("427 Rue Franche", (2.32, 48.86)),
    ] {
        let found = index.suggest(query, 5, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{query}");
        assert!(
            crate::metres((found[0].x, found[0].y), point) < 5.,
            "{query}"
        );
    }
}

pub async fn check_search(resolver: &Resolver) {
    for query in [
        "999 Route Territoriale 7, Rocail, 98880, New Caledonia",
        "271 Route Territoriale 9, Rocail, 98880, New Caledonia",
        "813 Route Territoriale 7, Rocail, 98880, New Caledonia",
        "427 Chemin du Sable, Rocail, 30500, New Caledonia",
        "8491 Rue du Lagon, Paris, 75001, New Caledonia",
        "271 Route Territoriale 7, Paris, 75001, France",
    ] {
        let found = resolver.resolve(query).await.unwrap();
        assert!(!found.good_match, "{query}: {found:?}");
        assert!(
            found
                .address_details
                .as_option()
                .is_none_or(|d| d.house_number.is_empty()),
            "{query}: {found:?}"
        );
    }
    let found = resolver
        .resolve("271 Route Territoriale 7, Rocail, 98881, New Caledonia")
        .await
        .unwrap();
    assert!(!found.good_match, "{found:?}");
    if let Some(d) = found
        .address_details
        .as_option()
        .filter(|d| !d.house_number.is_empty())
    {
        assert_eq!(d.postcode, "98880", "{found:?}");
    }
    let found = resolver
        .resolve("271 Route Territoriale 7, Rocail, 98880, France")
        .await
        .unwrap();
    assert!(found.good_match, "{found:?}");
    let d = found.address_details.as_option().unwrap();
    assert_eq!(d.country_code, "nc", "{found:?}");
    assert_eq!(d.house_number, "271", "{found:?}");
    assert_eq!(d.postcode, "98880", "{found:?}");
}
