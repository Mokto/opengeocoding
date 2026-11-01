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
];

/// Each rewrite of these leads back to the text typed (the postcode's own place is the part it was
/// read from; the number's letter, joined to the road, splits off again as a postcode), which
/// recursed until the stack overflowed.
#[test]
fn an_address_whose_rewrites_lead_back_to_it_ends() {
    for typed in ["Zona 9, Zona 13", "Calzada Aguilar 2, 3A"] {
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
                tx.send(index().suggest(typed, 5, None, None, false))
            })
        });
        let found = rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap_or_else(|_| panic!("{typed} did not end"));
        assert!(found.iter().all(|s| s.kind != "address"), "{typed}");
        assert!(events.lock().unwrap().is_empty(), "{typed}: {events:?}");
    }
}
