//! A made-up world small enough to read, written as the Overture parquet `build_index` reads and
//! built by the real builder. Places, streets and houses are authored here, so what a test expects
//! of them is known without asking this service.
use arrow_array::{
    Array, ArrayRef, BooleanArray, Float64Array, Int32Array, RecordBatch, StringArray, StructArray,
    builder::{BinaryBuilder, ListBuilder, MapBuilder, StringBuilder, StructBuilder},
};
use arrow_schema::{DataType, Field, Fields};
use parquet::arrow::{ArrowWriter, arrow_reader::ParquetRecordBatchReaderBuilder};
use std::{
    fs::File,
    path::Path,
    sync::{Arc, LazyLock},
};

#[derive(Clone)]
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
/// Source identities: Haut-Rhin's subdivision item, authored city/region entity pairs and
/// Wallis and Futuna's item, lent to a made-up territory.
pub const WIKIDATA: &[(&str, &str)] = &[
    ("fr-haut-rhin", "Q12722"),
    ("gb-harbor-region", "Q999900001"),
    ("gb-harbor-city", "Q999900001"),
    ("co-dc", "Q999900002"),
    ("co-bogota", "Q999900002"),
    ("wf", "Q35555"),
    ("fr-mq", "Q17054"),
    ("pr", "Q1183"),
    ("it-milano", "Q18288155"),
];

mod ad;
mod al;
mod at;
mod au;
mod be;
mod br;
mod ca;
mod cl;
mod co;
mod cw;
mod cz;
mod de;
mod dk;
mod ee;
mod eg;
mod es;
mod fr;
mod ga;
mod gb;
mod ge;
mod gg;
mod hk;
mod ie;
mod il;
mod r#in;
mod it;
mod jp;
mod kr;
mod li;
mod lt;
mod mk;
mod mx;
mod my;
mod nl;
mod no;
mod nz;
mod pk;
mod pl;
mod pr;
mod ps;
mod pt;
mod rs;
mod si;
mod tw;
mod us;
mod wf;
mod xk;
mod za;

pub static PLACES: LazyLock<Vec<Place>> = LazyLock::new(|| {
    [
        ad::PLACES,
        al::PLACES,
        at::PLACES,
        au::PLACES,
        be::PLACES,
        br::PLACES,
        ca::PLACES,
        cl::PLACES,
        co::PLACES,
        cz::PLACES,
        de::PLACES,
        dk::PLACES,
        ee::PLACES,
        eg::PLACES,
        es::PLACES,
        fr::PLACES,
        cw::PLACES,
        ga::PLACES,
        gb::PLACES,
        hk::PLACES,
        ge::PLACES,
        gg::PLACES,
        ie::PLACES,
        il::PLACES,
        it::PLACES,
        r#in::PLACES,
        jp::PLACES,
        kr::PLACES,
        li::PLACES,
        lt::PLACES,
        mk::PLACES,
        mx::PLACES,
        my::PLACES,
        nl::PLACES,
        no::PLACES,
        nz::PLACES,
        pk::PLACES,
        pl::PLACES,
        pr::PLACES,
        ps::PLACES,
        pt::PLACES,
        rs::PLACES,
        si::PLACES,
        tw::PLACES,
        us::PLACES,
        wf::PLACES,
        xk::PLACES,
        za::PLACES,
    ]
    .concat()
});

/// Administrative polygons besides the towns' (a square of half-side 0.2° each): (division,
/// subtype, is_land, is_territorial, lon, lat, half-side). Gamma's land stops 0.1° short of its
/// territorial sea on the west.
const AREAS: &[(&str, &str, bool, bool, f64, f64, f64)] = &[
    ("jp-aoba", "region", true, true, 145.0, 40.0, 0.4),
    ("kr-garam", "region", true, true, 140.0, 35.0, 0.4),
    ("kr-narae", "region", true, true, 142.0, 35.0, 0.4),
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
    ("li", "country", true, true, 79.5, 0.0, 0.5),
    ("at-e", "region", true, true, 80.5, 0.0, 0.5),
    ("co-bogota-dc", "county", true, true, -60.0, 30.3, 0.6),
    ("in-kl-thrissur", "county", true, true, 76.21, 10.52, 0.3),
    ("cl-ap", "region", true, true, -69.7, -18.5, 0.5),
    ("cl-ta", "region", true, true, -69.7, -19.8, 0.5),
    ("cl-ma", "region", true, true, -71.0, -53.0, 0.5),
    ("wf-reefward", "region", true, true, -150.0, -20.0, 0.4),
    ("ad-08", "region", true, true, 100.0, 70.0, 0.6),
    ("si-tolmin", "region", true, true, 120.0, -45.0, 0.5),
    ("si-kanal", "region", true, true, 121.5, -45.0, 0.5),
    ("si-koper", "region", true, true, 123.0, -45.0, 0.5),
    ("lt-ta", "region", true, true, 140.5, -50.0, 1.0),
    ("lt-pagegiai", "county", true, true, 140.0, -49.8, 0.45),
    ("lt-jurbarkas", "county", true, true, 141.0, -50.0, 0.45),
    ("ga-1-libreville-dept", "county", true, true, 9.65, 0.4, 0.4),
    (
        "eg-ast-manfalut-markaz",
        "county",
        true,
        true,
        31.1,
        27.31,
        0.45,
    ),
];

/// A street and its houses: (street, postcode, city, cc, lon, lat of house 1, numbers); the houses
/// stand 11 m apart, going east. Its segment runs east from house 1 along the latitude, to the
/// last house or `length` degrees, whichever is longer.
#[derive(Clone)]
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

const POSTAL_STREET: Street = Street {
    class: "residential",
    length: 0.0,
    name: "Courier Road",
    postcode: "ZZ1 1AA",
    city: "Posthaven",
    cc: "GB",
    lon: 32.001,
    lat: 12.0,
    numbers: &["31"],
};

const RECOVERY_STREET: Street = Street {
    class: "residential",
    length: 0.0,
    name: "Dispatchveien",
    postcode: "6780",
    city: "Postalvik",
    cc: "NO",
    lon: 18.001,
    lat: 62.0,
    numbers: &["51"],
};

const SPLIT_UNIT_ROAD: &str = "Courtyard Lane";

/// A road whose addresses name, like New Zealand's register, a town (town_city) before their
/// suburb, and the town is not a place the houses are in.
pub const TOWN_CITY_ROAD: (&str, &str) = ("Linden Rise", "Eastport");

pub fn cases() -> Vec<crate::Case> {
    [
        ad::cases(),
        al::cases(),
        at::cases(),
        au::cases(),
        ca::cases(),
        cl::cases(),
        co::cases(),
        cz::cases(),
        de::cases(),
        dk::cases(),
        ee::cases(),
        es::cases(),
        fr::cases(),
        ga::cases(),
        gb::cases(),
        gg::cases(),
        hk::cases(),
        ie::cases(),
        r#in::cases(),
        it::cases(),
        lt::cases(),
        mk::cases(),
        mx::cases(),
        nl::cases(),
        no::cases(),
        pl::cases(),
        pr::cases(),
        ps::cases(),
        pt::cases(),
        rs::cases(),
        si::cases(),
        us::cases(),
        wf::cases(),
        xk::cases(),
        za::cases(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

pub static STREETS: LazyLock<Vec<Street>> = LazyLock::new(|| {
    [
        ad::STREETS,
        al::STREETS,
        at::STREETS,
        au::STREETS,
        be::STREETS,
        br::STREETS,
        ca::STREETS,
        co::STREETS,
        cz::STREETS,
        de::STREETS,
        dk::STREETS,
        ee::STREETS,
        eg::STREETS,
        es::STREETS,
        fr::STREETS,
        cw::STREETS,
        ga::STREETS,
        gb::STREETS,
        hk::STREETS,
        ie::STREETS,
        r#in::STREETS,
        it::STREETS,
        jp::STREETS,
        kr::STREETS,
        lt::STREETS,
        mk::STREETS,
        mx::STREETS,
        my::STREETS,
        nl::STREETS,
        no::STREETS,
        nz::STREETS,
        pk::STREETS,
        pl::STREETS,
        ps::STREETS,
        pt::STREETS,
        rs::STREETS,
        si::STREETS,
        tw::STREETS,
        us::STREETS,
        xk::STREETS,
        za::STREETS,
    ]
    .concat()
});

/// Where a street's segment ends, east of house 1.
pub fn road_end(street: &Street) -> f64 {
    let houses = 0.00015 * street.numbers.len().saturating_sub(1) as f64;
    street.lon + houses.max(street.length)
}

/// Where house `number` of the street stands.
pub fn house(street: &Street, number: &str) -> (f64, f64) {
    let at = street.numbers.iter().position(|n| *n == number).unwrap();
    let spacing = if street.name == SPLIT_UNIT_ROAD {
        0.01
    } else {
        0.00015
    };
    (street.lon + spacing * at as f64, street.lat)
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
const POINT_ONLY: [&str; 10] = [
    "gb-harbor-city",
    "de-koeln-feldheim",
    "es-villaseca-pinedo",
    "nl-polderdam-centrum",
    "us-tx-reno",
    "at-eastufer",
    "li-westufer",
    "in-tn-chennai",
    "in-tn-chengalpattu",
    "in-tn-tiruvallur",
];

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
    let units: Vec<_> = rows
        .iter()
        .map(|(s, n)| match s.cc {
            // CartoCiudad's extension: house 12 B is number 12, unit B
            "ES" => n
                .split_once(' ')
                .map(|(number, unit)| (unit.to_string(), number)),
            // Taiwan's register: house 11之2 is number 11, unit 之2號
            "TW" => n
                .split_once('之')
                .map(|(number, sub)| (format!("之{sub}號"), number)),
            _ => n
                .split_once('/')
                .filter(|_| s.name == SPLIT_UNIT_ROAD)
                .map(|(unit, number)| (unit.to_string(), number)),
        })
        .collect();
    let levels = {
        let mut b = ListBuilder::new(StructBuilder::new(
            Fields::from(vec![Field::new("value", DataType::Utf8, true)]),
            vec![Box::new(StringBuilder::new()) as _],
        ));
        for (s, _) in &rows {
            let town = (s.name == TOWN_CITY_ROAD.0).then_some(TOWN_CITY_ROAD.1);
            for level in town.into_iter().chain([s.city]) {
                b.values()
                    .field_builder::<StringBuilder>(0)
                    .unwrap()
                    .append_value(level);
                b.values().append(true);
            }
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
            (
                "number",
                strings(
                    rows.iter()
                        .zip(&units)
                        .map(|((_, n), unit)| unit.as_ref().map_or(*n, |(_, number)| number)),
                ),
            ),
            (
                "unit",
                optional(
                    units
                        .iter()
                        .map(|unit| unit.as_ref().map_or("", |(u, _)| u.as_str())),
                ),
            ),
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
    Feature {
        shape: Shape::Node,
        lon: 140.04,
        lat: 35.02,
        tags: &[("addr:street", "배움길"), ("addr:housenumber", "704")],
    },
    Feature {
        shape: Shape::Way(0.0001),
        lon: 140.02,
        lat: 35.02,
        tags: &[("addr:street", "배움길"), ("addr:housenumber", "702")],
    },
    Feature {
        shape: Shape::Relation(0.0001),
        lon: 140.03,
        lat: 35.02,
        tags: &[("addr:street", "배움길"), ("addr:housenumber", "703")],
    },
    Feature {
        shape: Shape::Way(0.01),
        lon: 140.04,
        lat: 35.02,
        tags: &[("addr:street", "배움길"), ("addr:housenumber", "704")],
    },
    Feature {
        shape: Shape::Way(0.01),
        lon: 140.05,
        lat: 35.02,
        tags: &[
            ("addr:street", "배움길"),
            ("addr:housenumber", "705"),
            ("addr:postcode", "54399"),
            ("building", "no"),
        ],
    },
    Feature {
        shape: Shape::Relation(0.01),
        lon: 140.06,
        lat: 35.02,
        tags: &[
            ("addr:street", "배움길"),
            ("addr:housenumber", "706"),
            ("addr:postcode", "54399"),
        ],
    },
    Feature {
        shape: Shape::Way(0.01),
        lon: 140.07,
        lat: 35.02,
        tags: &[
            ("addr:street", "배움길"),
            ("addr:housenumber", "707"),
            ("building", "retail"),
        ],
    },
    Feature {
        shape: Shape::Relation(0.01),
        lon: 140.08,
        lat: 35.02,
        tags: &[
            ("addr:street", "배움길"),
            ("addr:housenumber", "708"),
            ("building", "yes"),
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

pub fn write_geonames(path: &Path) {
    std::fs::write(
        path,
        "1\tСеверная Роща\tSevernaya Roshcha\tNordhain,Untrusted Alias\t0.21\t30.0\tP\tPPL\tAT\t\t01\t\t\t\t1000\n\
         2\tThrissur\tThrissur\tTrichur\t10.527\t76.214\tP\tPPLA2\tIN\t\t13\t\t\t\t315957\n",
    )
    .unwrap();
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
