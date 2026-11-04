use super::*;
use crate::*;

pub const POINT_ONLY: &[&str] = &["kr-garam-saeachim"];
pub const AREAS: &[(&str, &str, bool, bool, f64, f64, f64)] = &[
    (
        "kr-garam-saeachim",
        "locality",
        true,
        true,
        140.35,
        35.,
        0.01,
    ),
    (
        "kr-garam-other-county",
        "county",
        true,
        true,
        140.3507,
        35.,
        0.00003,
    ),
];
pub const LICENSING: &[(&str, &str, f64)] = &[
    ("가람도 솔빛시 새아침리 821", "821", 140.3502),
    ("가람도 솔빛시 새아침리 산822-2", "산822-2", 140.3503),
    (
        "가람도 솔빛시 새아침리 99999-99999",
        "99999-99999",
        140.3504,
    ),
    ("가람도 솔빛시 해오름리 830", "830", 140.3505),
];

pub fn write_licensing(path: &Path) {
    let mut data = "lot_address\tlon\tlat\n".to_string();
    for &(address, _, lon) in LICENSING {
        data.push_str(&format!("{address}\t{lon}\t35\n"));
    }
    std::fs::write(path, data).unwrap();
}

pub async fn check_licensing_search(resolver: &Resolver) {
    for &(address, number, lon) in LICENSING {
        for query in [
            format!("{address}, South Korea"),
            format!("대한민국, {}", address.replacen("가람도", "경기", 1)),
        ] {
            let got = resolver.resolve(&query).await.unwrap();
            assert!(got.good_match, "{query}: {got:?}");
            let details = got.address_details.as_option().unwrap();
            assert_eq!(details.country_code, "kr");
            assert_eq!(details.house_number, number);
            let p = got.geo_coordinates.as_option().unwrap();
            assert!(metres((p.lon as f64, p.lat as f64), (lon, 35.)) < 5.);
        }
    }
}

pub const LISTINGS: &[(&str, f64, f64)] = &[
    ("솔빛시 새봄면 새로12길 812", 140.0002, 35.),
    ("새벽리 산8-2", 140.3001, 35.),
    ("새로12길 13-34", 140.0009, 35.),
    ("없는길 813", 140., 35.),
    ("없는리 815", 140.3, 35.),
    ("새벽리 814", 2.34, 48.86),
    ("새로12길 816 3층", 140., 35.),
    ("새로12길 817-", 140., 35.),
    ("나래도 솔빛시 새벽리 818", 140.3001, 35.),
    ("새아침리 산8-2", 140.3501, 35.),
];

#[test]
fn licensing_lots_are_additive_geographically_corroborated_and_not_reverse_houses() {
    use std::io::Write;
    use tantivy_fst::Streamer;
    let root = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("kr-licensing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let release = root.join("release");
    world::write_release(&release);
    world::write_poi_addresses(&release);
    let road = RecordBatch::try_from_iter([
        ("class", strings(["residential"])),
        ("subtype", strings(["road"])),
        ("names", names(&[("길벗리", &[])])),
        ("bbox", bbox(&[[140.3501, 140.3508, 35., 35.]])),
        (
            "geometry",
            binary(vec![line(&[(140.3501, 35.), (140.3508, 35.)])]),
        ),
    ])
    .unwrap();
    let mut writer = ArrowWriter::try_new(
        File::create(release.join("segment/licensing-road.parquet")).unwrap(),
        road.schema(),
        None,
    )
    .unwrap();
    writer.write(&road).unwrap();
    writer.close().unwrap();
    let input = File::open(release.join("address/part-0.parquet")).unwrap();
    let batch = ParquetRecordBatchReaderBuilder::try_new(input)
        .unwrap()
        .build()
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    let schema = batch.schema();
    let mut columns = batch.slice(0, 1).columns().to_vec();
    for (name, data) in [
        ("country", strings(["KR"])),
        ("street", strings(["새벽리"])),
        ("number", strings(["825"])),
        ("unit", strings([""])),
        ("postcode", strings(["12349"])),
        ("postal_city", strings(["새벽리"])),
        ("geometry", binary(vec![point(140.30005, 35.)])),
    ] {
        columns[schema.index_of(name).unwrap()] = data;
    }
    let row = RecordBatch::try_new(schema.clone(), columns).unwrap();
    let mut writer = ArrowWriter::try_new(
        File::create(release.join("address/licensing-primary.parquet")).unwrap(),
        schema,
        None,
    )
    .unwrap();
    writer.write(&row).unwrap();
    writer.close().unwrap();
    let source = root.join("lots.tsv");
    write_licensing(&source);
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&source)
        .unwrap();
    for (address, x, y) in [
        ("가람도 솔빛시 새아침리 산8-2", 140.3518, 35.),
        ("가람도 솔빛시 새벽리 825", 140.3015, 35.),
        ("가람도 솔빛시 새벽리 899", 140.3001, 35.),
        ("나래도 솔빛시 새아침리 823", 140.3502, 35.),
        ("가람도 없는시 새아침리 824", 140.3502, 35.),
        ("가람도 솔빛시 새아침리 826", 2.34, 48.86),
        ("가람도 솔빛시 새아침리 827", 142., 35.),
        ("가람도 솔빛시 없는리 828", 140.3502, 35.),
        ("가람도 솔빛시 길벗리 829", 140.3502, 35.),
        ("가람도 솔빛시 새아침리 831", 140.3507, 35.),
    ] {
        writeln!(file, "{address}\t{x}\t{y}").unwrap();
    }
    drop(file);
    let build = |name: &str, input: &str| {
        let out = root.join(name);
        let built = std::process::Command::new(env!("CARGO_BIN_EXE_build_index"))
            .arg(&release)
            .arg(&out)
            .arg("--korea-lots")
            .arg(input)
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        out
    };
    let base = build("base", "");
    let head = build("head", source.to_str().unwrap());
    let before = Index::open(&base, Arc::new(Parser::unloaded(0))).unwrap();
    let after = Index::open(&head, Arc::new(Parser::unloaded(0))).unwrap();
    for &(address, number, lon) in LICENSING {
        let q = format!("{address}, South Korea");
        assert!(
            !before
                .suggest(&q, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address")
        );
        let got = after.suggest(&q, 5, None, None, false);
        assert_eq!((got.len(), got[0].kind), (1, "address"), "{q}");
        assert!(metres((got[0].x, got[0].y), (lon, 35.)) < 0.2);
        assert!(after.suggest(&q, 5, Some("JP"), None, false).is_empty());
        let reverse = after.reverse_search(35., lon, 100., 20);
        assert!(
            reverse
                .iter()
                .all(|(_, v, _)| v["address"]["house_number"] != number)
        );
    }
    for q in [
        "가람도 솔빛시 새벽리 899",
        "가람도 솔빛시 새아침리 823",
        "가람도 없는시 새아침리 824",
        "가람도 솔빛시 새아침리 826",
        "가람도 솔빛시 새아침리 827",
        "가람도 솔빛시 없는리 828",
        "가람도 솔빛시 새아침리 822-2",
        "가람도 솔빛시 길벗리 829",
        "가람도 솔빛시 새아침리 831",
    ] {
        assert!(
            !after
                .suggest(q, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{q}"
        );
    }
    for q in [
        "가람도 솔빛시 새아침리 산8-2",
        "가람도 솔빛시 새벽리 825",
        "가람도 솔빛시 새봄면 새로12길 13-34",
        "가람도 솔빛시 새아침리",
    ] {
        let old = before.suggest(q, 5, None, None, false);
        let new = after.suggest(q, 5, None, None, false);
        assert_eq!(old[0].kind, new[0].kind);
        assert!(
            metres((old[0].x, old[0].y), (new[0].x, new[0].y)) < 0.1,
            "{q}"
        );
        assert_eq!(old[0].display_name, new[0].display_name, "{q}");
    }
    let shards: usize = std::fs::read_to_string(base.join("numbers.shards"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    for shard in 0..shards {
        for prefix in ["numbers", "numbers-far"] {
            let file = format!("{prefix}-{shard:02}.fst");
            let a = tantivy_fst::Map::from_bytes(std::fs::read(base.join(&file)).unwrap()).unwrap();
            let b = tantivy_fst::Map::from_bytes(std::fs::read(head.join(&file)).unwrap()).unwrap();
            let mut stream = a.stream();
            while let Some((key, value)) = stream.next() {
                assert_eq!(b.get(key), Some(value), "existing {file} house changed");
            }
        }
    }
    let postings = |dir: &Path, file: &str, width: usize| {
        use tantivy::schema::Value as _;
        let mut numbered = std::collections::HashSet::new();
        for shard in 0..shards {
            for prefix in ["numbers", "numbers-far"] {
                let map = tantivy_fst::Map::from_bytes(
                    std::fs::read(dir.join(format!("{prefix}-{shard:02}.fst"))).unwrap(),
                )
                .unwrap();
                let mut stream = map.stream();
                while let Some((key, _)) = stream.next() {
                    numbered.insert(u32::from_be_bytes(key[..4].try_into().unwrap()) as u64);
                }
            }
        }
        let (_, fields) = opengeocoding::index::schema::schema();
        let index = tantivy::Index::open_in_dir(dir).unwrap();
        let searcher = index.reader().unwrap().searcher();
        let ids: Vec<_> = searcher
            .segment_readers()
            .iter()
            .map(|r| r.fast_fields().u64("id").unwrap())
            .collect();
        let data = std::fs::read(dir.join(file)).unwrap();
        assert_eq!(data.len() % width, 0);
        // Only numbered street IDs stay fixed; segment-only IDs and document addresses can shift.
        let mut records: Vec<_> = data
            .chunks_exact(width)
            .map(|record| {
                let split = width - 4;
                let packed = u32::from_be_bytes(record[split..].try_into().unwrap());
                let (segment, doc) = opengeocoding::index::reverse::unpack_doc(packed);
                let sid = ids[segment as usize].first(doc).unwrap();
                let reader = &searcher.segment_readers()[segment as usize];
                let fast = reader.fast_fields();
                let d: tantivy::TantivyDocument = searcher
                    .doc(tantivy::DocAddress::new(segment, doc))
                    .unwrap();
                let strings = |field| {
                    d.get_all(field)
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                };
                let identity = format!(
                    "{:?}",
                    (
                        numbered.contains(&sid).then_some(sid),
                        fast.u64("place").unwrap().first(doc),
                        fast.u64("importance").unwrap().first(doc),
                        fast.f64("x").unwrap().first(doc).map(f64::to_bits),
                        fast.f64("y").unwrap().first(doc).map(f64::to_bits),
                        strings(fields.name),
                        strings(fields.pcode),
                        strings(fields.postal_town),
                    )
                );
                (record[..split].to_vec(), identity)
            })
            .collect();
        records.sort_unstable();
        records
    };
    for (file, width) in [("reverse.bin", 10), ("streets.bin", 12)] {
        assert_eq!(
            postings(&base, file, width),
            postings(&head, file, width),
            "{file}"
        );
    }
    let metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(head.join("korea-lots-source.json")).unwrap())
            .unwrap();
    assert_eq!(metadata["geographically_corroborated_rows"], 4);
    assert_eq!(metadata["mixed_streets_rejected"], 1);
    assert_eq!(metadata["mapped_roads_rejected"], 1);
    assert_eq!(metadata["administration_guard_rejected"], 1);
    let audit = std::fs::read_to_string(head.join("korea-lots-admitted.tsv")).unwrap();
    assert_eq!(audit.lines().count(), LICENSING.len() + 1);
    for &(address, _, lon) in LICENSING {
        assert!(
            audit
                .lines()
                .any(|line| line == format!("{address}\t{lon:.9}\t35.000000000"))
        );
    }
    drop((before, after));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn listed_native_addresses_are_complete_not_units_or_romanizations() {
    use opengeocoding::index::country::kr::listed_lot;
    for (q, street, number) in [
        ("가상면 새벽리 산 8-2", "새벽리", "산8-2"),
        ("새벽리 산8-2", "새벽리", "산8-2"),
        ("새봄동 19", "새봄동", "19"),
        ("새봄1가 19", "새봄1가", "19"),
        ("새벽리 99999-99999", "새벽리", "99999-99999"),
    ] {
        assert_eq!(
            listed_lot(q, |_| true),
            Some((street, number.into())),
            "{q}"
        );
    }
    let boundary = format!("{}면 새벽리 1", "가".repeat(249));
    assert_eq!(boundary.chars().count(), 256);
    assert!(listed_lot(&boundary, |_| true).is_some());
    assert!(listed_lot(&format!("가{boundary}"), |_| true).is_none());
    assert!(listed_lot("새벽리 산8-2", |_| false).is_none());
    assert!(listed_lot("나래도 새벽리 812", |name| name == "새벽리").is_none());
    for q in [
        "새벽리",
        "새벽리 1-",
        "새벽리 1-2-3",
        "새벽리 0",
        "새벽리 01",
        "새벽리 1-0",
        "새벽리 １２",
        "새벽리 1-123456",
        "새벽리 12,000",
        "새벽리 123456",
        "새벽리 12 3층",
        "새벽리 12 (빌딩)",
        "새로12길 산12",
        "새로12길 812",
        "1100로 12",
        "새벽리 산 산8-2",
        "12 새로12길",
        "Saero-gil 12",
        "South Korea, 새로12길 12",
        "없는건물 새로12길 12",
    ] {
        assert!(listed_lot(q, |_| true).is_none(), "{q}");
    }
}

pub fn check_listed_houses(poi: &Index) {
    for (tail, number, point) in [
        ("새벽리 산8-2", "산8-2", (140.3001, 35.)),
        ("새봄면 새로12길 13-34", "13-34", (140., 35.)),
    ] {
        let q = format!("가람도 솔빛시 {tail}, South Korea");
        let found = poi.suggest(&q, 5, None, None, false);
        assert_eq!(found.len(), 1, "{q}");
        assert_eq!(found[0].kind, "address");
        assert!(
            found[0].display_name.starts_with(&format!("{number}, ")),
            "{q}"
        );
        assert!(metres((found[0].x, found[0].y), point) < 5., "{q}");
        assert!(poi.suggest(&q, 5, Some("JP"), None, false).is_empty());
    }
    for tail in [
        "새로12길 812",
        "없는길 813",
        "없는리 815",
        "새벽리 814",
        "새로12길 816",
        "새로12길 817",
        "새벽리 818",
    ] {
        let q = format!("가람도 솔빛시 새봄면 {tail}, South Korea");
        assert!(
            !poi.suggest(&q, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{q}"
        );
    }
    let q = "가람도 솔빛시 새봄면 새로12길, South Korea";
    let before = index().suggest(q, 5, None, None, false);
    let after = poi.suggest(q, 5, None, None, false);
    assert_eq!(before[0].kind, "street");
    assert_eq!(after[0].kind, "street");
    assert!(metres((before[0].x, before[0].y), (after[0].x, after[0].y)) < 0.1);
    let lot = poi.suggest("가람도 솔빛시 새벽리, South Korea", 5, None, None, false);
    assert_eq!(lot[0].kind, "street");
    assert!(metres((lot[0].x, lot[0].y), (140.3001, 35.)) < 5.);
    let reversed = poi.reverse_search(35., 140.3001, 250., 20);
    assert!(
        reversed
            .iter()
            .all(|(_, v, _)| v["address"]["house_number"] != "산8-2")
    );
}

pub const PLACES: &[Place] = &[
    place!(
        "kr",
        "country",
        "",
        "KR",
        "",
        50_000_000,
        (141.0, 34.0),
        "",
        "대한민국",
        &[("en", "South Korea")]
    ),
    place!(
        "kr-garam",
        "region",
        "",
        "KR",
        "KR-41",
        1_000_000,
        (140.0, 35.0),
        "kr",
        "가람도",
        &[("en", "Garam"), ("ko", "경기도")]
    ),
    place!(
        "kr-narae",
        "region",
        "",
        "KR",
        "KR-43",
        1_000_000,
        (142.0, 35.0),
        "kr",
        "나래도",
        &[("en", "Narae"), ("ko", "충청북도")]
    ),
    place!(
        "kr-garam-solbit",
        "county",
        "",
        "KR",
        "KR-41",
        10_000,
        (140.0, 35.0),
        "kr-garam",
        "솔빛시",
        &[("en", "Solbit")]
    ),
    place!(
        "kr-narae-solbit",
        "county",
        "",
        "KR",
        "KR-43",
        10_000,
        (142.0, 35.0),
        "kr-narae",
        "솔빛시",
        &[("en", "Solbit")]
    ),
    place!(
        "kr-garam-saebom",
        "locality",
        "town",
        "KR",
        "KR-41",
        5_000,
        (140.0, 35.0),
        "kr-garam-solbit",
        "새봄면",
        &[("en", "Saebom")]
    ),
    place!(
        "kr-narae-saebom",
        "locality",
        "town",
        "KR",
        "KR-43",
        5_000,
        (142.0, 35.0),
        "kr-narae-solbit",
        "새봄면",
        &[("en", "Saebom")]
    ),
    place!(
        "kr-garam-other-county",
        "county",
        "",
        "KR",
        "KR-41",
        300,
        (140.3507, 35.0),
        "kr-garam",
        "다른시",
        &[("en", "Other County")]
    ),
    place!(
        "kr-garam-saeachim",
        "locality",
        "village",
        "KR",
        "KR-41",
        300,
        (140.35, 35.0),
        "kr-garam-solbit",
        "새아침리",
        &[
            ("en", "Saeachim"),
            ("ko-old", "해오름리"),
            ("ko-alt", "길벗리")
        ]
    ),
    place!(
        "kr-garam-saebyeok",
        "locality",
        "village",
        "KR",
        "KR-41",
        500,
        (140.3, 35.0),
        "kr-garam-solbit",
        "새벽리",
        &[("en", "Saebyeok")]
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "새로12길",
        postcode: "12345",
        city: "새봄면",
        cc: "KR",
        lon: 140.0,
        lat: 35.0,
        numbers: &["13-34", "290", "17"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "새로12길",
        postcode: "54321",
        city: "새봄면",
        cc: "KR",
        lon: 142.0,
        lat: 35.0,
        numbers: &["13-34", "999"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "새로12길",
        postcode: "12345",
        city: "새봄면",
        cc: "KR",
        lon: 140.15,
        lat: 35.0,
        numbers: &["17"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "새로12길20번길",
        postcode: "12345",
        city: "새봄면",
        cc: "KR",
        lon: 140.001,
        lat: 35.0,
        numbers: &["99"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "옛리",
        postcode: "12345",
        city: "새봄면",
        cc: "KR",
        lon: 140.002,
        lat: 35.0,
        numbers: &["45-6", "산45-6"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "들길",
        postcode: "12345",
        city: "새벽리",
        cc: "KR",
        lon: 140.3,
        lat: 35.0,
        numbers: &["7"],
    },
];

#[test]
fn country_first_preserves_scope_and_completion_boundaries() {
    for country in ["South Korea", "대한민국"] {
        for (province, lon) in [
            ("가람도", 140.),
            ("경기", 140.),
            ("나래도", 142.),
            ("충북", 142.),
        ] {
            let query = format!("{country}, {province} 솔빛시 새봄면 새로12길 13-34");
            let found = index().suggest(&query, 5, None, None, false);
            assert_eq!(found.len(), 1, "{query}");
            assert_eq!(found[0].kind, "address");
            assert!(found[0].display_name.starts_with("13-34, "));
            assert!(metres((found[0].x, found[0].y), (lon, 35.)) < 5., "{query}");
            assert!(
                index()
                    .suggest(&query, 5, Some("JP"), None, false)
                    .is_empty()
            );
        }
        for (tail, want) in [("새로1", 2), ("새로12길 13-", 1)] {
            let query = format!("{country}, 가람도 솔빛시 새봄면 {tail}");
            assert_eq!(
                index().suggest(&query, 5, None, None, false).len(),
                want,
                "{query}"
            );
            let closed = index().suggest(&format!("{query} "), 5, None, None, false);
            if tail == "새로1" {
                assert!(closed.is_empty(), "{query}");
            } else {
                assert_eq!(closed.len(), 1, "{query}");
                assert_eq!(closed[0].kind, "street", "{query}");
                assert!(closed[0].display_name.starts_with("새로12길, "), "{query}");
            }
        }
        let query = format!("{country}, 가람도 솔빛시 새봄면 들길 7");
        let found = index().suggest(&query, 5, None, None, false);
        assert_eq!(found.len(), 1, "{query}");
        assert!(metres((found[0].x, found[0].y), (140.3, 35.)) < 5.);
    }
    for query in [
        "Japan, 가람도 솔빛시 새봄면 새로12길 13-34",
        "South Korea, 가람도 없는시 새봄면 새로12길 13-34",
        "South Korea, 나래도 솔빛시 새봄면 들길 7",
        "South Korea, 가람도 솔빛시 없는면 들길 7",
        "South Korea, 가람도 솔빛시 새봄면 새로12길 999",
    ] {
        assert!(
            !index()
                .suggest(query, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
}

#[test]
fn short_provinces_keep_the_house_and_qualifiers() {
    for (province, number, lon) in [
        ("경기", "13-34", 140.0),
        ("경기", "290", 140.0001),
        ("충북", "13-34", 142.0),
    ] {
        let query = format!("{province} 솔빛시 새봄면 새로12길 {number}, South Korea");
        let found = index().suggest(&query, 5, Some("KR"), None, false);
        assert_eq!((found.len(), found[0].kind), (1, "address"), "{query}");
        assert!(
            found[0].display_name.starts_with(&format!("{number}, ")),
            "{query}"
        );
        assert!(
            metres((found[0].x, found[0].y), (lon, 35.0)) < 5.,
            "{query}"
        );
        assert!(
            index()
                .suggest(&query, 5, Some("JP"), None, false)
                .is_empty()
        );
    }
    for query in [
        "경기 솔빛시 새봄면 들길 7, South Korea",
        "경기 솔빛시 새봄면 새로12길 13-",
    ] {
        let found = index().suggest(query, 5, None, None, false);
        assert_eq!((found.len(), found[0].kind), (1, "address"), "{query}");
    }
    for query in [
        "경기 솔빛시 새봄면 새로12길 999, South Korea",
        "충북 솔빛시 새봄면 들길 7, South Korea",
        "경기 없는시 새봄면 새로12길 13-34, South Korea",
        "경기 솔빛시 없는면 들길 7, South Korea",
        "경기 솔빛시 새봄면 새로12길 13-34, Japan",
        "경기 솔빛시 새봄면 새로12길 13-, South Korea",
    ] {
        assert!(
            !index()
                .suggest(query, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
}

pub async fn check_short_provinces(resolver: &opengeocoding::resolver::Resolver) {
    for (province, number, lon, code) in [
        ("경기", "13-34", 140.0, "KR-41"),
        ("경기", "290", 140.0001, "KR-41"),
        ("충북", "13-34", 142.0, "KR-43"),
    ] {
        for query in [
            format!("{province} 솔빛시 새봄면 새로12길 {number}, South Korea"),
            format!("South Korea, {province} 솔빛시 새봄면 새로12길 {number}"),
            format!("대한민국, {province} 솔빛시 새봄면 새로12길 {number}"),
        ] {
            let found = resolver.resolve(&query).await.unwrap();
            let detail = found.address_details.as_option().unwrap();
            assert!(found.good_match, "{query}: {found:?}");
            assert_eq!(detail.house_number, number, "{query}");
            assert!(
                detail.region_codes.iter().any(|r| r.code == code),
                "{query}"
            );
            let point = found.geo_coordinates.as_option().unwrap();
            assert!(
                metres((point.lon as f64, point.lat as f64), (lon, 35.0)) < 5.,
                "{query}"
            );
        }
    }
    for query in [
        "Japan, 경기 솔빛시 새봄면 새로12길 13-34",
        "South Korea, 충북 솔빛시 새봄면 들길 7",
        "South Korea, 경기 없는시 새봄면 새로12길 13-34",
        "South Korea, 경기 솔빛시 새봄면 새로12길 999",
        "South Korea, 경기 솔빛시 새봄면 새로12길 17",
        "경기 솔빛시 새봄면 새로12길 999, South Korea",
        "충북 솔빛시 새봄면 들길 7, South Korea",
        "경기 없는시 새봄면 새로12길 13-34, South Korea",
        "경기 솔빛시 새봄면 새로12길 13-34, Japan",
        "경기 솔빛시 새봄면 새로12길 17, South Korea",
    ] {
        let found = resolver.resolve(query).await.unwrap();
        assert!(!found.good_match, "{query}: {found:?}");
        if query.starts_with("Japan,") {
            assert!(
                found.geo_coordinates.as_option().is_none(),
                "{query}: {found:?}"
            );
        }
        if query.starts_with("South Korea, 충북") {
            assert!(
                found
                    .address_details
                    .as_option()
                    .is_none_or(|d| d.region_codes.iter().all(|r| r.code != "KR-41")),
                "{query}: {found:?}"
            );
        }
    }
}

#[test]
fn korean_houses_keep_their_number_and_administrative_hierarchy() {
    for (province, number, lon, code) in [
        ("가람도", "13-34", 140.0, "KR-41"),
        ("가람도", "290", 140.0001, "KR-41"),
        ("나래도", "13-34", 142.0, "KR-43"),
    ] {
        let query = format!("{province} 솔빛시 새봄면 새로12길 {number}, South Korea");
        let found = index().suggest(&query, 5, Some("KR"), None, false);
        assert_eq!((found.len(), found[0].kind), (1, "address"), "{query}");
        let want = (lon, 35.0);
        assert!(metres((found[0].x, found[0].y), want) < 5., "{query}");
        assert!(
            index()
                .suggest(&query, 5, Some("JP"), None, false)
                .is_empty()
        );
        let reversed = index().reverse_search(want.1, want.0, 5., 1);
        assert_eq!(reversed[0].1["address"]["house_number"], number);
        assert_eq!(reversed[0].1["address"]["ISO3166-2-lvl4"], code);
        assert_eq!(
            index().reverse_region(want.1, want.0)["address"]["ISO3166-2-lvl4"],
            code
        );
    }
    for prefix in [
        "가람도 솔빛시 새봄면 새로1",
        "가람도 솔빛시 새봄면 새로12길 13",
        "가람도 솔빛시 새봄면 새로12길 13-",
    ] {
        let found = index().suggest(prefix, 1, None, None, false);
        assert_eq!(found.len(), 1, "{prefix}");
        assert!(
            metres((found[0].x, found[0].y), (140.0, 35.0)) < 200.,
            "{prefix}"
        );
        if prefix.ends_with(['3', '-']) {
            assert_eq!(found[0].kind, "address", "{prefix}");
        }
    }
    for (number, lon) in [
        ("45-6", 140.002),
        ("산45-6", 140.0021),
        ("산 45-6", 140.0021),
    ] {
        let query = format!("가람도 솔빛시 새봄면 옛리 {number}, 대한민국");
        let found = index().suggest(&query, 5, None, None, false);
        assert_eq!((found.len(), found[0].kind), (1, "address"), "{query}");
        assert!(metres((found[0].x, found[0].y), (lon, 35.)) < 5., "{query}");
    }
    // 새벽리 sits straight under its 시, as the data often puts a 리 outside its stated 면.
    for query in [
        "가람도 솔빛시 새봄면 들길 7, South Korea",
        "가람도 솔빛시 새봄면 들길 7",
    ] {
        let found = index().suggest(query, 5, None, None, false);
        assert_eq!((found.len(), found[0].kind), (1, "address"), "{query}");
        assert!(
            metres((found[0].x, found[0].y), (140.3, 35.)) < 5.,
            "{query}"
        );
    }
    for query in [
        "나래도 솔빛시 새봄면 들길 7, South Korea",
        "가람도 솔빛시 없는면 들길 7, South Korea",
        "가람도 솔빛시 새봄면 새로12길 999, South Korea",
        "가람도 솔빛시 새봄면 새로12길 99, South Korea",
        "가람도 솔빛시 새봄면 새로12길 13-34, Japan",
        "가람도 없는시 새봄면 새로12길 13-34, South Korea",
    ] {
        assert!(
            !index()
                .suggest(query, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
}
