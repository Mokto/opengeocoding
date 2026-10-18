//! A made-up world small enough to read, written as the Overture parquet `build_index` reads and
//! built by the real builder. Places, streets and houses are authored here, so what a test expects
//! of them is known without asking this service.
use arrow_array::{
    Array, ArrayRef, BooleanArray, Float64Array, Int32Array, RecordBatch, StringArray, StructArray,
    builder::{BinaryBuilder, ListBuilder, MapBuilder, StringBuilder, StructBuilder},
};
use arrow_schema::{DataType, Field, Fields};
use parquet::arrow::{ArrowWriter, arrow_reader::ParquetRecordBatchReaderBuilder};
use std::{fs::File, path::Path, sync::Arc};

pub struct Place {
    pub id: &'static str,
    pub subtype: &'static str,
    pub class: &'static str,
    pub cc: &'static str,
    pub region: &'static str,
    pub population: i32,
    pub lon: f64,
    pub lat: f64,
    pub parent: &'static str,
    pub primary: &'static str,
    /// (language, name)
    pub names: &'static [(&'static str, &'static str)],
}

macro_rules! place {
    ($id:expr, $subtype:expr, $class:expr, $cc:expr, $region:expr, $population:expr, ($lon:expr, $lat:expr), $parent:expr, $primary:expr, $names:expr $(,)?) => {
        Place {
            id: $id,
            subtype: $subtype,
            class: $class,
            cc: $cc,
            region: $region,
            population: $population,
            lon: $lon,
            lat: $lat,
            parent: $parent,
            primary: $primary,
            names: $names,
        }
    };
}

/// Paris in France and in Texas, Delhi in India and in Ontario, and Cologne under its German name.
/// Austria's made-up regions Alpha and Beta lie at the equator, where degrees of longitude and
/// latitude are as long: Grenzgasse is inside Suedhain's area but nearer Nordhain's point.
/// The Wikidata items of places that stand for an ISO 3166-2 subdivision, by place: the item of
/// FR-68 (Haut-Rhin) in `data/reference.json`.
pub const WIKIDATA: &[(&str, &str)] = &[("fr-haut-rhin", "Q12722")];

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
        "nl",
        "country",
        "",
        "NL",
        "",
        17_000_000,
        (5.0, 52.0),
        "",
        "Netherlands",
        &[("en", "Netherlands")]
    ),
    place!(
        "nl-polderdam",
        "locality",
        "town",
        "NL",
        "",
        20_000,
        (5.5, 52.3),
        "nl",
        "Polderdam",
        &[("en", "Polderdam")]
    ),
    place!(
        "fr",
        "country",
        "",
        "FR",
        "",
        68_000_000,
        (2.0, 46.0),
        "",
        "France",
        &[("en", "France")],
    ),
    place!(
        "fr-idf",
        "region",
        "",
        "FR",
        "FR-IDF",
        12_000_000,
        (2.5, 48.7),
        "fr",
        "Île-de-France",
        &[("en", "Ile-de-France")],
    ),
    place!(
        "fr-paris",
        "locality",
        "city",
        "FR",
        "FR-IDF",
        2_100_000,
        (2.3522, 48.8566),
        "fr-idf",
        "Paris",
        &[("en", "Paris"), ("fr", "Paris")],
    ),
    // villages that share a word with Paris's airport and a name with its museum, lighter
    // than a town
    place!(
        "fr-thingsmuseum",
        "locality",
        "village",
        "FR",
        "FR-IDF",
        0,
        (2.0, 47.0),
        "fr-idf",
        "Thingsmuseum",
        &[("en", "Thingsmuseum")],
    ),
    place!(
        "fr-nordville",
        "locality",
        "village",
        "FR",
        "FR-IDF",
        0,
        (2.0, 47.5),
        "fr-idf",
        "Nordville",
        &[("en", "Nordville")],
    ),
    place!(
        "us",
        "country",
        "",
        "US",
        "",
        330_000_000,
        (-100.0, 40.0),
        "",
        "United States",
        &[("en", "United States")],
    ),
    place!(
        "us-tx",
        "region",
        "",
        "US",
        "US-TX",
        29_000_000,
        (-97.0, 31.5),
        "us",
        "Texas",
        &[("en", "Texas")],
    ),
    place!(
        "us-tx-paris",
        "locality",
        "city",
        "US",
        "US-TX",
        25_000,
        (-95.5555, 33.6609),
        "us-tx",
        "Paris",
        &[("en", "Paris")],
    ),
    place!(
        "il",
        "country",
        "",
        "IL",
        "",
        1_000_000,
        (35.2, 31.8),
        "",
        "Israel",
        &[("en", "Israel")]
    ),
    place!(
        "ge",
        "country",
        "",
        "GE",
        "",
        1_000_000,
        (44.8, 41.7),
        "",
        "Georgia",
        &[("en", "Georgia")]
    ),
    place!(
        "us-ga",
        "region",
        "",
        "US",
        "US-GA",
        1_000_000,
        (-83.0, 32.0),
        "us",
        "Georgia",
        &[("en", "Georgia")]
    ),
    place!(
        "us-ga-arbor",
        "locality",
        "town",
        "US",
        "US-GA",
        10_000,
        (-83.0, 32.0),
        "us-ga",
        "Arbor",
        &[("en", "Arbor")]
    ),
    place!(
        "us-il",
        "region",
        "",
        "US",
        "US-IL",
        1_000_000,
        (-89.6, 40.3),
        "us",
        "Illinois",
        &[("en", "Illinois")]
    ),
    place!(
        "us-il-bayport",
        "locality",
        "town",
        "US",
        "US-IN",
        20_000,
        (-89.6, 40.3),
        "us-il",
        "Bayport",
        &[("en", "Bayport")]
    ),
    // a city beside Paris, Texas, at the east end of Long Road, also a point only
    place!(
        "us-tx-reno",
        "locality",
        "city",
        "US",
        "US-TX",
        30_000,
        (-95.575, 33.703),
        "us-tx",
        "Reno",
        &[("en", "Reno")],
    ),
    place!(
        "ca",
        "country",
        "",
        "CA",
        "",
        38_000_000,
        (-106.0, 56.0),
        "",
        "Canada",
        &[("en", "Canada")],
    ),
    place!(
        "ca-on",
        "region",
        "",
        "CA",
        "CA-ON",
        14_000_000,
        (-85.0, 50.0),
        "ca",
        "Ontario",
        &[("en", "Ontario")],
    ),
    place!(
        "ca-on-delhi",
        "locality",
        "town",
        "CA",
        "CA-ON",
        4_000,
        (-80.4953, 42.8532),
        "ca-on",
        "Delhi",
        &[("en", "Delhi")],
    ),
    place!(
        "in",
        "country",
        "",
        "IN",
        "",
        1_400_000_000,
        (78.0, 22.0),
        "",
        "India",
        &[("en", "India")],
    ),
    place!(
        "in-dl",
        "region",
        "",
        "IN",
        "IN-DL",
        17_000_000,
        (77.1, 28.65),
        "in",
        "Delhi",
        &[("en", "Delhi")],
    ),
    place!(
        "in-dl-delhi",
        "locality",
        "city",
        "IN",
        "IN-DL",
        11_000_000,
        (77.209, 28.6139),
        "in-dl",
        "Delhi",
        &[("en", "Delhi")],
    ),
    place!(
        "de",
        "country",
        "",
        "DE",
        "",
        83_000_000,
        (10.5, 51.0),
        "",
        "Germany",
        &[("en", "Germany")],
    ),
    place!(
        "de-nw",
        "region",
        "",
        "DE",
        "DE-NW",
        18_000_000,
        (7.5, 51.5),
        "de",
        "Nordrhein-Westfalen",
        &[("en", "North Rhine-Westphalia")],
    ),
    place!(
        "de-koeln",
        "locality",
        "city",
        "DE",
        "DE-NW",
        1_000_000,
        (6.9603, 50.9375),
        "de-nw",
        "Köln",
        &[("en", "Cologne"), ("de", "Köln"), ("es", "Colonia")],
    ),
    // a village Overture has as a point inside Cologne's polygon, with none of its own
    place!(
        "de-koeln-feldheim",
        "locality",
        "village",
        "DE",
        "DE-NW",
        800,
        (6.86, 50.903),
        "de-koeln",
        "Feldheim",
        &[("en", "Feldheim")],
    ),
    place!(
        "at-overlapping-county",
        "county",
        "",
        "AT",
        "AT-4",
        1_000,
        (60.55, 0.25),
        "at-d",
        "Overlapping County",
        &[("en", "Overlapping County")]
    ),
    place!(
        "fr-haut-rhin",
        "county",
        "",
        "FR",
        "FR-IDF",
        600000,
        (7.3, 48.0),
        "fr-idf",
        "Haut-Rhin",
        &[("en", "Haut-Rhin")],
    ),
    place!(
        "fr-colmar",
        "locality",
        "city",
        "FR",
        "FR-IDF",
        70000,
        (7.35, 48.08),
        "fr-haut-rhin",
        "Colmar",
        &[("en", "Colmar")],
    ),
    place!(
        "at",
        "country",
        "",
        "AT",
        "",
        8000000,
        (13.0, 47.5),
        "",
        "Austria",
        &[("en", "Austria")],
    ),
    place!(
        "at-a",
        "region",
        "",
        "AT",
        "AT-1",
        1000000,
        (12.0, 47.0),
        "at",
        "Alpha",
        &[("en", "Alpha")],
    ),
    place!(
        "at-b",
        "region",
        "",
        "AT",
        "AT-2",
        1000000,
        (14.0, 47.0),
        "at",
        "Beta",
        &[("en", "Beta")],
    ),
    place!(
        "at-nordhain",
        "locality",
        "",
        "AT",
        "AT-1",
        1000,
        (30.0, 0.21),
        "at-a",
        "Nordhain",
        &[("en", "Nordhain")],
    ),
    place!(
        "at-suedhain",
        "locality",
        "",
        "AT",
        "AT-2",
        1000,
        (30.19, 0.19),
        "at-b",
        "Suedhain",
        &[("en", "Suedhain")],
    ),
    place!(
        "at-ohnecode",
        "locality",
        "",
        "AT",
        "",
        1000,
        (40.0, 0.0),
        "at",
        "Ohnecode",
        &[("en", "Ohnecode")],
    ),
    place!(
        "at-mitcode",
        "locality",
        "",
        "AT",
        "AT-1",
        1000,
        (40.05, 0.0),
        "at-a",
        "Mitcode",
        &[("en", "Mitcode")],
    ),
    place!(
        "at-fern-b",
        "locality",
        "",
        "AT",
        "AT-2",
        1000,
        (100.0, -20.0),
        "at-b",
        "Fernort Bee",
        &[("en", "Fernort Bee")],
    ),
    place!(
        "at-fern-a",
        "locality",
        "",
        "AT",
        "AT-1",
        1000,
        (100.0, -18.0),
        "at-a",
        "Fernort Ay",
        &[("en", "Fernort Ay")],
    ),
    // Gamma and Delta share a border at 60.5° east; Grenzdorf, in Delta, is the nearest town to
    // Gamma's eastern edge. Liechtenstein, east of Delta, has no regions.
    place!(
        "at-g",
        "region",
        "",
        "AT",
        "AT-3",
        1000000,
        (60.0, 0.0),
        "at",
        "Gamma",
        &[("en", "Gamma")],
    ),
    place!(
        "at-d",
        "region",
        "",
        "AT",
        "AT-4",
        1000000,
        (61.0, 0.0),
        "at",
        "Delta",
        &[("en", "Delta")],
    ),
    place!(
        "at-gammadorf",
        "locality",
        "",
        "AT",
        "AT-3",
        1000,
        (60.1, 0.0),
        "at-g",
        "Gammadorf",
        &[("en", "Gammadorf")],
    ),
    place!(
        "at-grenzdorf",
        "locality",
        "",
        "AT",
        "AT-4",
        1000,
        (60.52, 0.0),
        "at-d",
        "Grenzdorf",
        &[("en", "Grenzdorf")],
    ),
    place!(
        "at-randdorf",
        "locality",
        "",
        "AT",
        "AT-4",
        1000,
        (61.45, 0.0),
        "at-d",
        "Randdorf",
        &[("en", "Randdorf")],
    ),
    place!(
        "li",
        "country",
        "",
        "LI",
        "",
        40000,
        (61.6, 0.0),
        "",
        "Liechtenstein",
        &[("en", "Liechtenstein")],
    ),
    place!(
        "li-vaduz",
        "locality",
        "",
        "LI",
        "",
        5000,
        (61.65, 0.0),
        "li",
        "Vaduz",
        &[("en", "Vaduz")],
    ),
    // ISO 3166-2:IE puts County Dublin in the province of Leinster, IE-L.
    place!(
        "ie",
        "country",
        "",
        "IE",
        "",
        5_000_000,
        (-8.0, 53.0),
        "",
        "Ireland",
        &[("en", "Ireland")],
    ),
    place!(
        "ie-d",
        "region",
        "",
        "IE",
        "IE-D",
        1_400_000,
        (-6.3, 53.4),
        "ie",
        "County Dublin",
        &[("en", "County Dublin")],
    ),
    place!(
        "ie-dublin",
        "locality",
        "city",
        "IE",
        "IE-D",
        590_000,
        (-6.2603, 53.3498),
        "ie-d",
        "Dublin",
        &[("en", "Dublin"), ("ga", "Baile Átha Cliath")],
    ),
    place!(
        "gb",
        "country",
        "",
        "GB",
        "",
        68_000_000,
        (30.0, 10.0),
        "",
        "United Kingdom",
        &[("en", "United Kingdom")]
    ),
    place!(
        "gb-beacon",
        "locality",
        "town",
        "GB",
        "",
        20_000,
        (30.0, 10.0),
        "gb",
        "Beacon",
        &[
            ("en", "Beacon"),
            ("ja", "桜森市"),
            ("zh", "松林市"),
            ("ko", "솔빛도시"),
            ("tr", "İncikent"),
        ]
    ),
    place!(
        "gb-bramble",
        "locality",
        "town",
        "GB",
        "",
        20_000,
        (31.0, 11.0),
        "gb",
        "6th of Bramble",
        &[("en", "Sixth of Bramble"), ("fr", "Bramble6")]
    ),
    place!(
        "gb-orchard",
        "microhood",
        "",
        "GB",
        "",
        500,
        (30.02, 10.02),
        "gb-beacon",
        "Orchard",
        &[("en", "Orchard")],
    ),
];

/// Administrative polygons besides the towns' (a square of half-side 0.2° each): (division,
/// subtype, is_land, is_territorial, lon, lat, half-side). Gamma's land stops 0.1° short of its
/// territorial sea on the west.
const AREAS: &[(&str, &str, bool, bool, f64, f64, f64)] = &[
    (
        "at-overlapping-county",
        "county",
        true,
        true,
        60.45,
        0.0,
        0.03,
    ),
    ("at-g", "region", true, false, 60.05, 0.0, 0.45),
    ("at-g", "region", false, true, 60.0, 0.0, 0.5),
    ("at-d", "region", true, true, 61.0, 0.0, 0.5),
    ("li", "country", true, true, 61.6, 0.0, 0.1),
];

/// A street and its houses: (street, postcode, city, cc, lon, lat of house 1, numbers); the houses
/// stand 11 m apart, going east. Its segment runs east from house 1 along the latitude, to the
/// last house or `length` degrees, whichever is longer.
pub struct Street {
    pub class: &'static str,
    pub length: f64,
    pub name: &'static str,
    pub postcode: &'static str,
    pub city: &'static str,
    pub cc: &'static str,
    pub lon: f64,
    pub lat: f64,
    pub numbers: &'static [&'static str],
}

pub const STREETS: &[Street] = &[
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
    Street {
        class: "residential",
        length: 0.0,
        name: "Willowlaan",
        postcode: "1234 AB",
        city: "Polderdam",
        cc: "NL",
        lon: 5.5,
        lat: 52.3,
        numbers: &["4a"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Township Lane",
        postcode: "60001",
        city: "Postalton",
        cc: "US",
        lon: -89.6,
        lat: 40.3004,
        numbers: &["15"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Cedar Road",
        postcode: "31000",
        city: "Arbor",
        cc: "US",
        lon: -83.0,
        lat: 32.0,
        numbers: &["16"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Meadow Road",
        postcode: "60001",
        city: "Bayport",
        cc: "US",
        lon: -89.6,
        lat: 40.3,
        numbers: &["14"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "High Street",
        postcode: "AA1 1AA",
        city: "Beacon",
        cc: "GB",
        lon: 30.01,
        lat: 10.0,
        numbers: &["1", "2", "3"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Rue de Rivoli",
        postcode: "75001",
        city: "Paris",
        cc: "FR",
        lon: 2.34,
        lat: 48.86,
        numbers: &["1", "2", "3", "4", "5", "6", "7bis", "8a"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Main Street",
        postcode: "75460",
        city: "Paris",
        cc: "US",
        lon: -95.556,
        lat: 33.66,
        numbers: &["100", "102", "104"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Canal Street",
        postcode: "75460",
        city: "Paris",
        cc: "US",
        lon: -95.55,
        lat: 33.65,
        numbers: &["1"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Canal Street",
        postcode: "75461",
        city: "Paris",
        cc: "US",
        lon: -95.58,
        lat: 33.65,
        numbers: &["2"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Hohe Straße",
        postcode: "50667",
        city: "Köln",
        cc: "DE",
        lon: 6.95,
        lat: 50.94,
        numbers: &["1", "2", "3"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Grenzgasse",
        postcode: "9999",
        city: "Suedhain",
        cc: "AT",
        lon: 30.0,
        lat: 0.0,
        numbers: &["1", "2"],
    },
    // a street with no houses
    Street {
        class: "residential",
        length: 0.02,
        name: "Feldweg",
        postcode: "",
        city: "Köln",
        cc: "DE",
        lon: 6.85,
        lat: 50.9,
        numbers: &[],
    },
    // a path, which is no street to answer a point with
    Street {
        class: "footway",
        length: 0.01,
        name: "Uferpfad",
        postcode: "",
        city: "Köln",
        cc: "DE",
        lon: 6.9,
        lat: 50.97,
        numbers: &[],
    },
    // houses at the west end of a 2.8 km street
    Street {
        class: "residential",
        length: 0.03,
        name: "Long Road",
        postcode: "75462",
        city: "Paris",
        cc: "US",
        lon: -95.6,
        lat: 33.7,
        numbers: &["1", "3"],
    },
    Street {
        class: "residential",
        length: 0.01,
        name: "Delta Road",
        postcode: "",
        city: "Grenzdorf",
        cc: "AT",
        lon: 60.5005,
        lat: 0.0,
        numbers: &[],
    },
    Street {
        class: "residential",
        length: 0.001,
        name: "Randweg",
        postcode: "",
        city: "Randdorf",
        cc: "AT",
        lon: 61.497,
        lat: 0.0,
        numbers: &[],
    },
];

/// Where a street's segment ends, east of house 1.
pub fn road_end(street: &Street) -> f64 {
    let houses = 0.00015 * street.numbers.len().saturating_sub(1) as f64;
    street.lon + houses.max(street.length)
}

/// Where house `number` of the street stands.
pub fn house(street: &Street, number: &str) -> (f64, f64) {
    let at = street.numbers.iter().position(|n| *n == number).unwrap();
    (street.lon + 0.00015 * at as f64, street.lat)
}

fn line(points: &[(f64, f64)]) -> Vec<u8> {
    let mut b = vec![1, 2, 0, 0, 0];
    b.extend((points.len() as u32).to_le_bytes());
    for (x, y) in points {
        b.extend(x.to_le_bytes());
        b.extend(y.to_le_bytes());
    }
    b
}

fn point(lon: f64, lat: f64) -> Vec<u8> {
    let mut b = vec![1, 1, 0, 0, 0];
    b.extend(lon.to_le_bytes());
    b.extend(lat.to_le_bytes());
    b
}

/// A square of half-side `r` degrees around a point.
fn square(lon: f64, lat: f64, r: f64) -> Vec<u8> {
    let mut b = vec![1, 3, 0, 0, 0];
    b.extend(1u32.to_le_bytes());
    b.extend(5u32.to_le_bytes());
    for (x, y) in [(-r, -r), (r, -r), (r, r), (-r, r), (-r, -r)] {
        b.extend((lon + x).to_le_bytes());
        b.extend((lat + y).to_le_bytes());
    }
    b
}

fn strings<'a>(it: impl IntoIterator<Item = &'a str>) -> ArrayRef {
    Arc::new(StringArray::from_iter_values(it))
}

fn optional<'a>(it: impl IntoIterator<Item = &'a str>) -> ArrayRef {
    Arc::new(StringArray::from_iter(
        it.into_iter().map(|s| (!s.is_empty()).then_some(s)),
    ))
}

fn binary(rows: Vec<Vec<u8>>) -> ArrayRef {
    let mut b = BinaryBuilder::new();
    rows.iter().for_each(|r| b.append_value(r));
    Arc::new(b.finish()) as ArrayRef
}

/// names.primary and names.common (language -> name).
fn names(rows: &[(&str, &[(&str, &str)])]) -> ArrayRef {
    let mut primary = StringBuilder::new();
    let mut common = MapBuilder::new(None, StringBuilder::new(), StringBuilder::new());
    for (p, c) in rows {
        primary.append_value(p);
        for (k, v) in *c {
            common.keys().append_value(k);
            common.values().append_value(v);
        }
        common.append(true).unwrap();
    }
    let (primary, common) = (primary.finish(), common.finish());
    Arc::new(StructArray::from(vec![
        (
            Arc::new(Field::new("primary", DataType::Utf8, true)),
            Arc::new(primary) as ArrayRef,
        ),
        (
            Arc::new(Field::new("common", common.data_type().clone(), true)),
            Arc::new(common) as ArrayRef,
        ),
    ]))
}

fn bbox(rows: &[[f64; 4]]) -> ArrayRef {
    let col =
        |i: usize| Arc::new(Float64Array::from_iter_values(rows.iter().map(|r| r[i]))) as ArrayRef;
    Arc::new(StructArray::from(
        ["xmin", "xmax", "ymin", "ymax"]
            .into_iter()
            .enumerate()
            .map(|(i, n)| (Arc::new(Field::new(n, DataType::Float64, true)), col(i)))
            .collect::<Vec<_>>(),
    ))
}

/// hierarchies: one chain of division ids per row, from the country down to the place itself.
fn hierarchies(rows: &[Vec<&str>]) -> ArrayRef {
    let ids = Fields::from(vec![Field::new("division_id", DataType::Utf8, true)]);
    let mut b = ListBuilder::new(ListBuilder::new(StructBuilder::new(
        ids.clone(),
        vec![Box::new(StringBuilder::new()) as _],
    )));
    for chain in rows {
        for id in chain {
            let inner = b.values();
            let s = inner.values();
            s.field_builder::<StringBuilder>(0)
                .unwrap()
                .append_value(id);
            s.append(true);
        }
        b.values().append(true);
        b.append(true);
    }
    Arc::new(b.finish()) as ArrayRef
}

fn write(dir: &Path, dataset: &str, columns: Vec<(&str, ArrayRef)>) {
    let dir = dir.join(dataset);
    std::fs::create_dir_all(&dir).unwrap();
    let batch = RecordBatch::try_from_iter(columns).unwrap();
    let mut w = ArrowWriter::try_new(
        File::create(dir.join("part-0.parquet")).unwrap(),
        batch.schema(),
        None,
    )
    .unwrap();
    w.write(&batch).unwrap();
    w.close().unwrap();
}

fn chain(p: &Place) -> Vec<&'static str> {
    let mut up = vec![p.id];
    let mut at = p.parent;
    while let Some(q) = PLACES.iter().find(|q| q.id == at) {
        up.insert(0, q.id);
        at = q.parent;
    }
    up
}

/// Localities without a polygon of their own.
const POINT_ONLY: [&str; 2] = ["de-koeln-feldheim", "us-tx-reno"];

pub fn cover_address_countries(dir: &Path) {
    let mut reader = ParquetRecordBatchReaderBuilder::try_new(
        File::open(dir.join("address/part-0.parquet")).unwrap(),
    )
    .unwrap()
    .build()
    .unwrap();
    let batch = reader.next().unwrap().unwrap();
    let mut writer = ArrowWriter::try_new(
        File::create(dir.join("address/registry.parquet")).unwrap(),
        batch.schema(),
        None,
    )
    .unwrap();
    for _ in 0..3500 {
        writer.write(&batch).unwrap();
    }
    writer.close().unwrap();
}

pub fn write_poi_addresses(dir: &Path) {
    let rows = [
        ("999 High Street", 30.011, 10.0),
        ("888 Market Street", 30.013, 10.0),
        ("1 High Street", 30.012, 10.0),
        ("998 Missing Road", 30.011, 10.0),
        ("997 High Street", 2.34, 48.86),
        ("12, Shop, High Street", 30.011, 10.0),
        ("996 High Street", f64::NAN, 10.0),
        ("995 High Street", -195.555, 10.0),
        ("994 Main Street", -95.555, 33.66),
    ];
    let mut addresses = ListBuilder::new(StructBuilder::new(
        Fields::from(vec![
            Field::new("freeform", DataType::Utf8, true),
            Field::new("country", DataType::Utf8, true),
        ]),
        vec![
            Box::new(StringBuilder::new()) as _,
            Box::new(StringBuilder::new()) as _,
        ],
    ));
    for (address, _, _) in rows {
        for (i, value) in [address, "CN"].iter().enumerate() {
            addresses
                .values()
                .field_builder::<StringBuilder>(i)
                .unwrap()
                .append_value(value);
        }
        addresses.values().append(true);
        addresses.append(true);
    }
    write(
        dir,
        "place",
        vec![
            ("addresses", Arc::new(addresses.finish())),
            (
                "geometry",
                binary(rows.iter().map(|(_, x, y)| point(*x, *y)).collect()),
            ),
        ],
    );
    let original = dir.join("segment/part-0.parquet");
    let saved = dir.join("segment/original.parquet");
    std::fs::rename(&original, &saved).unwrap();
    write(
        dir,
        "segment",
        vec![
            ("subtype", strings(["road"])),
            ("class", strings(["residential"])),
            ("names", names(&[("Market Street", &[])])),
            ("bbox", bbox(&[[30.011, 30.013, 10.0, 10.0]])),
            (
                "geometry",
                binary(vec![line(&[(30.011, 10.0), (30.013, 10.0)])]),
            ),
        ],
    );
}

/// Writes the division, division_area, segment and address datasets under `dir`.
pub fn write_release(dir: &Path) {
    let level = |p: &Place| match p.subtype {
        "region" => Some(4),
        _ => None,
    };
    write(
        dir,
        "division",
        vec![
            ("id", strings(PLACES.iter().map(|p| p.id))),
            ("subtype", strings(PLACES.iter().map(|p| p.subtype))),
            ("country", strings(PLACES.iter().map(|p| p.cc))),
            ("class", optional(PLACES.iter().map(|p| p.class))),
            ("region", optional(PLACES.iter().map(|p| p.region))),
            (
                "admin_level",
                Arc::new(Int32Array::from_iter(PLACES.iter().map(level))),
            ),
            (
                "parent_division_id",
                optional(PLACES.iter().map(|p| p.parent)),
            ),
            (
                "wikidata",
                optional(PLACES.iter().map(|p| {
                    WIKIDATA
                        .iter()
                        .find(|(id, _)| *id == p.id)
                        .map_or("", |(_, item)| *item)
                })),
            ),
            (
                "population",
                Arc::new(Int32Array::from_iter_values(
                    PLACES.iter().map(|p| p.population),
                )),
            ),
            (
                "geometry",
                binary(PLACES.iter().map(|p| point(p.lon, p.lat)).collect()),
            ),
            (
                "names",
                names(
                    &PLACES
                        .iter()
                        .map(|p| (p.primary, p.names))
                        .collect::<Vec<_>>(),
                ),
            ),
            (
                "hierarchies",
                hierarchies(&PLACES.iter().map(chain).collect::<Vec<_>>()),
            ),
            (
                "bbox",
                bbox(
                    &PLACES
                        .iter()
                        .map(|p| [p.lon - 0.2, p.lon + 0.2, p.lat - 0.2, p.lat + 0.2])
                        .collect::<Vec<_>>(),
                ),
            ),
        ],
    );

    let areas: Vec<_> = PLACES
        .iter()
        .filter(|p| p.subtype == "locality" && !POINT_ONLY.contains(&p.id))
        .map(|p| {
            let r = if p.id == "li-vaduz" { 0.05 } else { 0.2 };
            (p.id, p.subtype, true, true, p.lon, p.lat, r)
        })
        .chain(AREAS.iter().copied())
        .collect();
    write(
        dir,
        "division_area",
        vec![
            ("division_id", strings(areas.iter().map(|a| a.0))),
            ("subtype", strings(areas.iter().map(|a| a.1))),
            (
                "is_land",
                Arc::new(BooleanArray::from_iter(areas.iter().map(|a| Some(a.2)))),
            ),
            (
                "is_territorial",
                Arc::new(BooleanArray::from_iter(areas.iter().map(|a| Some(a.3)))),
            ),
            (
                "geometry",
                binary(areas.iter().map(|a| square(a.4, a.5, a.6)).collect()),
            ),
        ],
    );

    let road = |s: &Street| [s.lon, road_end(s), s.lat, s.lat];
    write(
        dir,
        "segment",
        vec![
            ("subtype", strings(STREETS.iter().map(|_| "road"))),
            ("class", strings(STREETS.iter().map(|s| s.class))),
            (
                "names",
                names(
                    &STREETS
                        .iter()
                        .map(|s| (s.name, &[][..]))
                        .collect::<Vec<_>>(),
                ),
            ),
            ("bbox", bbox(&STREETS.iter().map(road).collect::<Vec<_>>())),
            (
                "geometry",
                binary(
                    STREETS
                        .iter()
                        .map(|s| {
                            let end = road_end(s);
                            line(&[(s.lon, s.lat), ((s.lon + end) / 2.0, s.lat), (end, s.lat)])
                        })
                        .collect(),
                ),
            ),
        ],
    );

    let rows: Vec<(&Street, &str)> = STREETS
        .iter()
        .flat_map(|s| s.numbers.iter().map(move |n| (s, *n)))
        .collect();
    let levels = {
        let mut b = ListBuilder::new(StructBuilder::new(
            Fields::from(vec![Field::new("value", DataType::Utf8, true)]),
            vec![Box::new(StringBuilder::new()) as _],
        ));
        for (s, _) in &rows {
            b.values()
                .field_builder::<StringBuilder>(0)
                .unwrap()
                .append_value(s.city);
            b.values().append(true);
            b.append(true);
        }
        Arc::new(b.finish()) as ArrayRef
    };
    write(
        dir,
        "address",
        vec![
            ("country", strings(rows.iter().map(|(s, _)| s.cc))),
            ("street", strings(rows.iter().map(|(s, _)| s.name))),
            ("number", strings(rows.iter().map(|(_, n)| *n))),
            ("postcode", strings(rows.iter().map(|(s, _)| s.postcode))),
            ("postal_city", strings(rows.iter().map(|(s, _)| s.city))),
            ("address_levels", levels),
            (
                "geometry",
                binary(
                    rows.iter()
                        .map(|(s, n)| {
                            let (x, y) = house(s, n);
                            point(x, y)
                        })
                        .collect(),
                ),
            ),
        ],
    );
}

/// An OSM feature `build_index` may read as a landmark: a node at its point, or a closed square
/// way of half-side `r` degrees around it, or a multipolygon relation of such a way.
pub struct Feature {
    pub shape: Shape,
    pub lon: f64,
    pub lat: f64,
    pub tags: &'static [(&'static str, &'static str)],
}

#[derive(PartialEq)]
pub enum Shape {
    Node,
    Way(f64),
    Relation(f64),
}

/// Paris's airport, station and museum, which share the town's name, Nordville's station, and two
/// features that are not landmarks: a museum without a Wikidata item and a shop with one.
pub const FEATURES: &[Feature] = &[
    Feature {
        shape: Shape::Way(0.02),
        lon: 2.45,
        lat: 48.95,
        tags: &[
            ("aeroway", "aerodrome"),
            ("iata", "PNV"),
            ("wikidata", "Q900001"),
            ("name", "Aéroport de Paris-Nordville"),
            ("name:en", "Paris Nordville Airport"),
        ],
    },
    Feature {
        shape: Shape::Node,
        lon: 2.30,
        lat: 48.87,
        tags: &[
            ("railway", "station"),
            ("wikidata", "Q900002"),
            ("name", "Paris Gare de l'Ouest"),
        ],
    },
    Feature {
        shape: Shape::Relation(0.004),
        lon: 2.37,
        lat: 48.84,
        tags: &[
            ("type", "multipolygon"),
            ("tourism", "museum"),
            ("wikidata", "Q900003"),
            ("name", "Paris Museum of Things"),
            ("alt_name", "Thingsmuseum"),
        ],
    },
    Feature {
        shape: Shape::Node,
        lon: 2.01,
        lat: 47.5,
        tags: &[
            ("railway", "station"),
            ("wikidata", "Q900005"),
            ("name", "Nordville"),
        ],
    },
    Feature {
        shape: Shape::Node,
        lon: 2.36,
        lat: 48.86,
        tags: &[
            ("tourism", "museum"),
            ("wikidata", "Q900006"),
            ("name", "Twin Hall"),
        ],
    },
    Feature {
        shape: Shape::Node,
        lon: 2.36,
        lat: 48.86,
        tags: &[
            ("railway", "station"),
            ("wikidata", "Q900007"),
            ("name", "Twin Hall"),
        ],
    },
    Feature {
        shape: Shape::Node,
        lon: 2.36,
        lat: 48.86,
        tags: &[("tourism", "museum"), ("name", "Paris Unlisted Museum")],
    },
    Feature {
        shape: Shape::Node,
        lon: 2.33,
        lat: 48.86,
        tags: &[
            ("shop", "bakery"),
            ("wikidata", "Q900004"),
            ("name", "Paris Bakery"),
        ],
    },
];

fn varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

/// A length-delimited protobuf field.
fn field(out: &mut Vec<u8>, number: u64, bytes: &[u8]) {
    varint(out, number << 3 | 2);
    varint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

fn number(out: &mut Vec<u8>, number: u64, v: u64) {
    varint(out, number << 3);
    varint(out, v);
}

fn packed(out: &mut Vec<u8>, n: u64, values: impl IntoIterator<Item = u64>) {
    let mut b = vec![];
    values.into_iter().for_each(|v| varint(&mut b, v));
    field(out, n, &b);
}

fn blob(out: &mut Vec<u8>, kind: &str, data: &[u8]) {
    let mut blob = vec![];
    field(&mut blob, 1, data);
    number(&mut blob, 2, data.len() as u64);
    let mut header = vec![];
    field(&mut header, 1, kind.as_bytes());
    number(&mut header, 3, blob.len() as u64);
    out.extend((header.len() as u32).to_be_bytes());
    out.extend(header);
    out.extend(blob);
}

/// `FEATURES` as an OSM PBF file sorted by type then id, one block per type, as Geofabrik's are.
pub fn write_osm(dir: &Path) {
    let mut table: Vec<&str> = vec!["", "outer"];
    let mut sid = |s: &'static str| match table.iter().position(|t| *t == s) {
        Some(i) => i as u64,
        None => {
            table.push(s);
            table.len() as u64 - 1
        }
    };
    let mut tags = |f: &Feature| -> (Vec<u64>, Vec<u64>) {
        f.tags.iter().map(|(k, v)| (sid(k), sid(v))).unzip()
    };
    let mut relations: Vec<Vec<u8>> = vec![];
    let mut node_messages: Vec<Vec<u8>> = vec![];
    let mut way_messages: Vec<Vec<u8>> = vec![];
    let mut add_node = |lon: f64, lat: f64, t: (Vec<u64>, Vec<u64>)| {
        let id = node_messages.len() as i64 + 1;
        let mut n = vec![];
        number(&mut n, 1, zigzag(id));
        packed(&mut n, 2, t.0);
        packed(&mut n, 3, t.1);
        number(&mut n, 8, zigzag((lat * 1e7).round() as i64));
        number(&mut n, 9, zigzag((lon * 1e7).round() as i64));
        node_messages.push(n);
        id
    };
    for f in FEATURES {
        let t = tags(f);
        let r = match f.shape {
            Shape::Node => {
                add_node(f.lon, f.lat, t);
                continue;
            }
            Shape::Way(r) | Shape::Relation(r) => r,
        };
        let corners: Vec<i64> = [(-r, -r), (r, -r), (r, r), (-r, r)]
            .iter()
            .map(|(x, y)| add_node(f.lon + x, f.lat + y, (vec![], vec![])))
            .collect();
        let way_id = way_messages.len() as i64 + 1;
        let refs = corners.iter().chain(&corners[..1]);
        let mut last = 0;
        let deltas: Vec<u64> = refs
            .map(|&id| {
                let d = id - last;
                last = id;
                zigzag(d)
            })
            .collect();
        let (keys, vals) = if f.shape == Shape::Relation(r) {
            (vec![], vec![])
        } else {
            t.clone()
        };
        let mut w = vec![];
        number(&mut w, 1, way_id as u64);
        packed(&mut w, 2, keys);
        packed(&mut w, 3, vals);
        packed(&mut w, 8, deltas);
        way_messages.push(w);
        if f.shape == Shape::Relation(r) {
            let mut rel = vec![];
            number(&mut rel, 1, relations.len() as u64 + 1);
            packed(&mut rel, 2, t.0);
            packed(&mut rel, 3, t.1);
            packed(&mut rel, 8, [1]);
            packed(&mut rel, 9, [zigzag(way_id)]);
            packed(&mut rel, 10, [1]);
            relations.push(rel);
        }
    }
    let block = |group_field: u64, messages: &[Vec<u8>], table: &[&str]| {
        let mut strings = vec![];
        table
            .iter()
            .for_each(|s| field(&mut strings, 1, s.as_bytes()));
        let mut group = vec![];
        messages
            .iter()
            .for_each(|m| field(&mut group, group_field, m));
        let mut b = vec![];
        field(&mut b, 1, &strings);
        field(&mut b, 2, &group);
        b
    };
    let mut header = vec![];
    field(&mut header, 4, b"OsmSchema-V0.6");
    field(&mut header, 5, b"Sort.Type_then_ID");
    let mut file = vec![];
    blob(&mut file, "OSMHeader", &header);
    blob(&mut file, "OSMData", &block(1, &node_messages, &table));
    blob(&mut file, "OSMData", &block(3, &way_messages, &table));
    blob(&mut file, "OSMData", &block(4, &relations, &table));
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("landmarks.osm.pbf"), file).unwrap();
}
