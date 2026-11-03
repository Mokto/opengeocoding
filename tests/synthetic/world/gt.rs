use super::*;
use crate::*;

/// Guatemala City's zones are written like postcodes ("Zona 13") and the data keys them so, under
/// the zone of that name. A house number's letter ("3A") is a postcode somewhere too.
pub const PLACES: &[Place] = &[
    place!(
        "gt",
        "country",
        "",
        "GT",
        "",
        17_000_000,
        (-90.25, 15.5),
        "",
        "Guatemala",
        &[("en", "Guatemala")]
    ),
    place!(
        "gt-zona-13",
        "locality",
        "",
        "GT",
        "",
        50_000,
        (-90.53, 14.59),
        "gt",
        "Zona 13",
        &[("en", "Zona 13")]
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Avenida Hincapié",
        postcode: "Zona 13",
        city: "Zona 13",
        cc: "GT",
        lon: -90.529,
        lat: 14.589,
        numbers: &["5"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Calzada Aguilar",
        postcode: "3A",
        city: "Zona 13",
        cc: "GT",
        lon: -90.531,
        lat: 14.591,
        numbers: &["4"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Calle Postal",
        postcode: "Zona 14",
        city: "Zona 13",
        cc: "GT",
        lon: -90.532,
        lat: 14.592,
        numbers: &["7"],
    },
];

#[test]
fn address_rewrites_end_without_reconsuming_numbers_or_postcodes() {
    for typed in [
        "Zona 9, Zona 13",
        "Calzada Aguilar 2, 3A",
        "Avenida Hincapié 5, Zona 13, Zona 14, Guatemala",
        "Avenida Hincapié 5, Zona 14, Zona 13, Guatemala",
        "Missing Road, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, Guatemala",
        "Missing Road 9, Zona 13, Zona 13, Zona 13, Zona 13, Zona 13, Zona 13, Zona 13, Zona 13, Zona 13, Zona 13",
        "Missing Road, 1, 2, 3, 4, 5, Zona 13, Zona 13, Zona 13, Zona 13, Zona 13",
    ] {
        let events = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        let captured = events.clone();
        let mut options = sentry::ClientOptions::new();
        options.transport = Some(std::sync::Arc::new(
            sentry::transports::DefaultTransportFactory,
        ));
        options.before_send = Some(std::sync::Arc::new(move |event| {
            captured.lock().unwrap().push(event.message);
            None
        }));
        let client = sentry::Client::from_config(("https://public@example.invalid/1", options));
        assert!(client.is_enabled());
        let hub = sentry::Hub::new(
            Some(std::sync::Arc::new(client)),
            std::sync::Arc::new(sentry::Scope::default()),
        );
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            sentry::Hub::run(hub.into(), || {
                let mut found = index().suggest(typed, 5, None, None, false);
                found.extend(index().suggest_houses(typed, None, 5));
                tx.send(found)
            })
        });
        let found = rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap_or_else(|_| panic!("{typed} did not end"));
        assert!(found.iter().all(|s| s.kind != "address"), "{typed}");
        assert!(events.lock().unwrap().is_empty(), "{typed}: {events:?}");
    }
    for query in [
        "Avenida Hincapié, 5, Zona 13",
        "Avenida Hincapié, 5, Zona 13, Guatemala",
        "Avenida Hincapié 5, Zona 13, Zona 13, Guatemala",
    ] {
        let found = index().suggest_houses(query, Some("gt"), 5);
        assert!(
            found.iter().any(|s| {
                s.kind == "address"
                    && metres((s.x, s.y), house(street("Avenida Hincapié"), "5")) < 5.
            }),
            "{query}"
        );
    }
}
