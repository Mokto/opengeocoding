use opengeocoding::{
    matching::*, proto::opengeocoding::*, regions::Regions, resolver::format_response,
};
use serde_json::json;

fn parsed(values: &[(&str, &str)]) -> ParsedAddress {
    values
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn candidate(country: &str, city: &str, state: &str, postcode: &str) -> FullAddress {
    FullAddress {
        address_details: AddressDetails {
            country_code: country.into(),
            city: city.into(),
            state: state.into(),
            postcode: postcode.into(),
            ..Default::default()
        }
        .into(),
        ..Default::default()
    }
}

#[test]
fn multilingual_normalization() {
    for (input, expected) in [
        ("München", "munchen"),
        ("São-Paulo", "sao paulo"),
        ("Straße", "strasse"),
        ("Łódź", "lodz"),
        ("København", "kobenhavn"),
        ("Αθήνα", "athena"),
        ("Москва", "moskva"),
        ("東京", "dong jing"),
        ("  Paris\u{1c}France ", "paris france"),
    ] {
        assert_eq!(normalize_place(input), expected, "{input}");
    }
}

#[test]
fn taiwanese_number_roles_are_not_road_typos() {
    let details = AddressDetails {
        country_code: "tw".into(),
        road: "明光路3段389巷6弄".into(),
        house_number: "11之2".into(),
        ..Default::default()
    };
    for (road, number, agrees) in [
        ("明光路三段３８９巷６弄", "１１之２號", true),
        ("明光路3段389巷6弄", "11之2", true),
        ("明光路四段389巷6弄", "11之2", false),
        ("明光路3段388巷6弄", "11之2", false),
        ("明光路3段389巷5弄", "11之2", false),
        ("明光路3段389巷6弄", "11", false),
        ("明光路3段389巷6弄", "2之11", false),
    ] {
        assert_eq!(
            road_agrees(
                &parsed(&[("road", road), ("house_number", number)]),
                Some(&details)
            ),
            agrees,
            "{road} {number}"
        );
    }
    let mut dropped = details;
    dropped.road = "明光路三段三百八十九巷六弄十一巷".into();
    assert!(!road_agrees(
        &parsed(&[
            ("road", "明光路三段三百八十九巷六弄十二巷"),
            ("house_number", "11之2"),
        ]),
        Some(&dropped)
    ));
}

#[test]
fn polish_ulica_is_the_same_road_with_or_without_it() {
    let details = AddressDetails {
        country_code: "pl".into(),
        road: "ulica Marszałkowska".into(),
        house_number: "140".into(),
        ..Default::default()
    };
    for (road, agrees) in [
        ("marszałkowska", true),
        ("ul. marszałkowska", true),
        ("ulica marszałkowska", true),
        ("puławska", false),
    ] {
        assert_eq!(
            road_agrees(
                &parsed(&[("road", road), ("house_number", "140")]),
                Some(&details)
            ),
            agrees,
            "{road}"
        );
    }
}

#[test]
fn public_country_names_and_regions() {
    for (name, code) in [
        ("France", "fr"),
        ("Deutschland", "de"),
        ("USA", "us"),
        ("日本", "jp"),
        ("Danmark", "dk"),
    ] {
        assert_eq!(
            expected_country(&parsed(&[("country", name)])),
            code,
            "{name}"
        );
    }
    assert_eq!(implied_country(&parsed(&[("state", "Texas")])), "us");
    assert_eq!(implied_country(&parsed(&[("state", "Ontario")])), "ca");
    assert_eq!(implied_country(&parsed(&[("state", "Georgia")])), "");
}

#[test]
fn a_single_postcode_part_repairs_an_invalid_parsed_postcode() {
    for (country, before, query, after) in [
        (
            "Hungary",
            "csanád",
            "Tűzoltó street 2., 6728, Szeged, Csongrád- Csanád, Hungary",
            "6728",
        ),
        ("Hungary", "csanád", "6728, 6729, Szeged, Hungary", "csanád"),
        (
            "Hungary",
            "csanád",
            "Tűzoltó street 6728, Szeged, Hungary",
            "csanád",
        ),
        ("Hungary", "6729", "6728, Szeged, Hungary", "6729"),
        ("Hungary", "", "6728, Szeged, Hungary", ""),
        ("", "csanád", "6728, Szeged", "csanád"),
        (
            "United Kingdom",
            "surrey",
            "Cheam Road, SM1 2AX, Sutton, Surrey, United Kingdom",
            "SM1 2AX",
        ),
        ("Canada", "M5J 2T3", "181, Toronto, Canada", "M5J 2T3"),
    ] {
        let mut address = parsed(&[("country", country), ("postcode", before)]);
        recover_postcode(query, &mut address);
        assert_eq!(part(&address, "postcode"), after, "{query}");
    }
    assert_eq!(postcode_shape("hu", "6728"), Some(true));
    assert_eq!(postcode_shape("hu", "csanád"), Some(false));
}

#[test]
fn postcode_precision() {
    for (expected, actual, compatible) in [
        ("E14 5HQ", "E14", true),
        ("E14 5HQ", "E14 9ZZ", true),
        ("E14 5HQ", "E15 9ZZ", false),
        ("08019", "08019-1234", true),
        ("08019", "08930", false),
        ("D02 AF30", "D02", true),
        ("D02 AF30", "D02 AB12", false),
        ("D02 AF30", "D04 AF30", false),
        ("", "08019", false),
        ("08019", "", false),
        ("1234 AB", "1234", true),
        ("K1A 0B1", "K1A", true),
    ] {
        assert_eq!(postcodes_compatible(expected, actual), compatible);
        assert_eq!(postcodes_compatible(actual, expected), compatible);
    }
}

#[test]
fn eircode_control_separators() {
    for separator in [' ', '\u{1c}', '\u{1d}', '\u{1e}', '\u{1f}'] {
        let code = format!("D02{separator}AF30");
        assert_eq!(eircode(&format!("Dublin {code} Ireland")), code);
    }
}

#[test]
fn stated_country_cannot_override_corroborated_region() {
    let address = parsed(&[("country", "France"), ("state", "Texas"), ("city", "Paris")]);
    let candidates = vec![
        candidate("fr", "Paris", "Île-de-France", ""),
        candidate("us", "Paris", "Texas", ""),
    ];
    assert_eq!(
        choose_candidate(&address, "fr", "", &candidates),
        Some((1, "city_region"))
    );
}

#[test]
fn country_conflict_without_region_agreement_is_rejected() {
    let address = parsed(&[("country", "France"), ("state", "Texas"), ("city", "Paris")]);
    assert_eq!(
        choose_candidate(&address, "fr", "", &[candidate("fr", "Paris", "", "")]),
        None
    );
}

#[test]
fn country_postcode_conflict_is_rejected() {
    let address = parsed(&[("country", "France"), ("city", "Paris")]);
    assert_eq!(
        choose_candidate(
            &address,
            "fr",
            "75001",
            &[candidate("us", "Paris", "Texas", "75460")]
        ),
        None
    );
}

#[test]
fn mistyped_postcode_is_overruled_by_house_road_and_town() {
    let address = parsed(&[
        ("house_number", "312"),
        ("road", "Lowell Street"),
        ("city", "Lexington"),
        ("postcode", "02423"),
    ]);
    let mut house = candidate("us", "Lexington", "Massachusetts", "02421");
    let details = house.address_details.get_or_insert_default();
    details.road = "Lowell Street".into();
    details.house_number = "312".into();
    assert_eq!(
        choose_candidate(&address, "", "02423", std::slice::from_ref(&house)).map(|(i, _)| i),
        Some(0)
    );
    // a candidate holding the postcode still comes first
    let other = candidate("us", "Lexington", "Massachusetts", "02423");
    assert_eq!(
        choose_candidate(&address, "", "02423", &[house.clone(), other]).map(|(i, _)| i),
        Some(1)
    );
    // another house number on the road is no match
    house.address_details.get_or_insert_default().house_number = "310".into();
    assert_eq!(choose_candidate(&address, "", "02423", &[house]), None);
}

fn pairs(values: &[(&str, &str)]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn field_of<'a>(parsed: &'a [(String, String)], key: &str) -> &'a str {
    parsed
        .iter()
        .find(|(k, _)| k == key)
        .map_or("", |(_, v)| v.as_str())
}

#[test]
fn commas_bound_the_town_in_street_town_state_postcode() {
    use opengeocoding::index::comma_aligned;
    let head = pairs(&[("house_number", "14"), ("road", "breezewood common")]);
    let tail = pairs(&[("state", "ny"), ("postcode", "14051")]);
    // libpostal moved "east" into the road
    let whole = pairs(&[
        ("house_number", "14"),
        ("road", "breezewood common east"),
        ("city", "amherst"),
        ("state", "ny"),
        ("postcode", "14051"),
    ]);
    let aligned = comma_aligned(&head, &[], " East Amherst", &tail, &whole).unwrap();
    assert_eq!(
        aligned,
        pairs(&[
            ("house_number", "14"),
            ("road", "breezewood common"),
            ("city", "east amherst"),
            ("state", "ny"),
            ("postcode", "14051"),
        ])
    );
    // already agreeing, a country stated, a middle that is no town, or outer parts that carry
    // more than the street and the region leave the parse alone
    let agreeing = pairs(&[("city", "east amherst")]);
    assert!(comma_aligned(&head, &[], "East Amherst", &tail, &agreeing).is_none());
    let mut with_country = whole.clone();
    with_country.push(("country".into(), "usa".into()));
    assert!(comma_aligned(&head, &[], "East Amherst", &tail, &with_country).is_none());
    assert!(comma_aligned(&head, &[], "Unit 4", &tail, &whole).is_none());
    let no_town: Vec<_> = whole.iter().filter(|(k, _)| k != "city").cloned().collect();
    assert!(comma_aligned(&head, &[], "Bronx", &tail, &no_town).is_none());
    // a road that swallowed the town ("4724 3rd Place Northwest, Washingotn, DC 20011")
    let swallowed = pairs(&[
        ("house_number", "14"),
        ("road", "breezewood common washingotn"),
        ("state", "ny"),
        ("postcode", "14051"),
    ]);
    let aligned = comma_aligned(&head, &[], "Washingotn", &tail, &swallowed).unwrap();
    assert_eq!(field_of(&aligned, "city"), "washingotn");
    assert_eq!(field_of(&aligned, "road"), "breezewood common");
    let mut split = whole.clone();
    split.push(("suburb".into(), "avondale".into()));
    assert!(comma_aligned(&head, &[], "Avondale Goodyear", &tail, &split).is_none());
    let separate_state = pairs(&[
        ("house_number", "14"),
        ("road", "breezewood common"),
        ("city", "east amherst"),
        ("state", "ny"),
        ("postcode", "14051-1234"),
    ]);
    assert!(
        comma_aligned(
            &head,
            &["East Amherst"],
            " NY",
            &pairs(&[("postcode", "14051-1234")]),
            &separate_state,
        )
        .is_none()
    );
    let town_in_tail = pairs(&[("city", "ithaca"), ("state", "ny"), ("postcode", "14850")]);
    assert!(comma_aligned(&head, &[], "East Amherst", &town_in_tail, &whole).is_none());
    // a county between street and town, read into the road
    let county = pairs(&[
        ("house_number", "14"),
        ("road", "breezewood common erie"),
        ("city", "east amherst"),
        ("state", "ny"),
        ("postcode", "14051"),
    ]);
    let aligned = comma_aligned(&head, &[" Erie"], " East Amherst", &tail, &county).unwrap();
    assert_eq!(field_of(&aligned, "road"), "breezewood common");
    assert_eq!(field_of(&aligned, "state_district"), "erie");
    assert!(comma_aligned(&head, &["12"], " East Amherst", &tail, &county).is_none());
    // the road read as a suburb ("2219 Pacific Coast Highway, Lomita")
    let suburb = pairs(&[
        ("house_number", "14"),
        ("suburb", "breezewood common"),
        ("city", "east amherst"),
        ("state", "ny"),
        ("postcode", "14051"),
    ]);
    let aligned = comma_aligned(&head, &[], "East Amherst", &tail, &suburb).unwrap();
    assert_eq!(field_of(&aligned, "road"), "breezewood common");
}

#[test]
fn a_street_type_is_not_the_town() {
    use opengeocoding::index::street_aligned;
    let head = pairs(&[("house_number", "701"), ("road", "greenwood avenue")]);
    let tail = pairs(&[("state", "oh"), ("postcode", "45229")]);
    let whole = pairs(&[
        ("house_number", "701"),
        ("road", "greenwood"),
        ("city", "avenue"),
        ("state", "oh"),
        ("postcode", "45229"),
    ]);
    let aligned = street_aligned(&head, "701 GREENWOOD Avenue", &tail, &whole).unwrap();
    assert_eq!(field_of(&aligned, "road"), "greenwood avenue");
    assert_eq!(field_of(&aligned, "city"), "");
    // a town in its own part is left alone
    let town = pairs(&[
        ("road", "main street"),
        ("city", "springfield"),
        ("state", "oh"),
    ]);
    assert!(street_aligned(&head, "701 Greenwood Avenue", &tail, &town).is_none());
    // the state read into the road ("1516 RUTH Avenue, OH")
    let state = pairs(&[("state", "oh")]);
    let swallowed = pairs(&[("house_number", "701"), ("road", "greenwood avenue oh")]);
    let aligned = street_aligned(&head, "701 Greenwood Avenue", &state, &swallowed).unwrap();
    assert_eq!(field_of(&aligned, "state"), "oh");
    assert_eq!(field_of(&aligned, "road"), "greenwood avenue");
    // "5005 Kutztown Rd. Temple, PA 19560": the town is inside the street part, and the
    // whole parse already split it out. Dropping it leaves the postcode, which France shares.
    let head = pairs(&[("house_number", "5005"), ("road", "kutztown rd. temple")]);
    let tail = pairs(&[("state", "pa"), ("postcode", "19560")]);
    let whole = pairs(&[
        ("house_number", "5005"),
        ("road", "kutztown rd."),
        ("city", "temple"),
        ("state", "pa"),
        ("postcode", "19560"),
    ]);
    assert!(street_aligned(&head, "5005 Kutztown Rd. Temple", &tail, &whole).is_none());
    // the street name taken for the town, and no road left ("185 Clairemont, TX 77351")
    let head = pairs(&[("house_number", "185"), ("road", "clairemont")]);
    let tail = pairs(&[("state", "tx"), ("postcode", "77351")]);
    let whole = pairs(&[
        ("house_number", "185"),
        ("city", "clairemont"),
        ("state", "tx"),
        ("postcode", "77351"),
    ]);
    let aligned = street_aligned(&head, "185 Clairemont", &tail, &whole).unwrap();
    assert_eq!(field_of(&aligned, "road"), "clairemont");
    // the street type ends the false town ("1603 West Shady Lane, IN 47201")
    let head = pairs(&[("house_number", "1603"), ("road", "west shady lane")]);
    let tail = pairs(&[("state", "in"), ("postcode", "47201")]);
    let whole = pairs(&[
        ("house_number", "1603"),
        ("road", "west"),
        ("city", "shady lane"),
        ("state", "in"),
        ("postcode", "47201"),
    ]);
    let aligned = street_aligned(&head, "1603 West Shady Lane", &tail, &whole).unwrap();
    assert_eq!(field_of(&aligned, "road"), "west shady lane");
}

#[test]
fn postcode_missing_is_eligible_for_backfill() {
    assert!(postcode_matches("75001", Some(&AddressDetails::default())));
    assert!(!postcode_agrees("75001", Some(&AddressDetails::default())));
}

#[test]
fn fallback_query_order_is_postcode_first() {
    let address = parsed(&[
        ("country", "France"),
        ("city", "Paris"),
        ("postcode", "75001"),
        ("road", "Rue A"),
    ]);
    assert_eq!(
        create_query(&address, &["postcode", "city", "country"]),
        "75001, Paris, France"
    );
    assert_eq!(
        without_country("Paris, France, 75001", &address),
        "Paris, 75001"
    );
    assert_eq!(without_country("Paris, 75001", &address), "");
}

#[test]
fn index_response_and_region_code_filtering() {
    let result = format_response(&json!({
        "display_name": "Paris, France", "lat": "48.8566", "lon": "2.3522",
        "address": { "city": "Paris", "country_code": "fr", "unknown_key": "ignored",
            "ISO3166-2-lvl4": "FR-IDF", "ISO3166-2-lvl12": "FR-IGNORED", "ISO3166-2-lvl6": "RS-KM" }
    }))
    .unwrap();
    assert_eq!(result.address_details.city, "Paris");
    assert_eq!(result.address_details.region_codes.len(), 1);
    assert_eq!(result.address_details.region_codes[0].code, "FR-IDF");
}

#[test]
fn regions_come_from_codes_and_list_in_a_stable_order() {
    let region = |country: &str, code: &str, level: u32, name: &str| {
        (
            format!("{}-{code}", country.to_uppercase()),
            Region {
                country_code: country.into(),
                country_name: country.to_uppercase(),
                admin_code: code.into(),
                admin_level: level,
                local_name: name.into(),
                en_name: name.into(),
                ..Default::default()
            },
        )
    };
    let code = |code: &str, admin_level: u32| ISO3166_2 {
        code: code.into(),
        admin_level,
        ..Default::default()
    };
    let rows = vec![
        region("dk", "84", 4, "Hovedstaden"),
        region("ie", "D", 4, "County Dublin"),
        region("dk", "82", 4, "Midtjylland"),
    ];
    let regions = Regions::from_rows(rows.clone());
    assert_eq!(regions.len(), 3);
    let found = regions.reformat(&[code("DK-84", 4), code("DK-XX", 5), code("dk-82", 3)]);
    // lowest admin level first; a code with no region is left out; case does not matter
    assert_eq!(
        found
            .iter()
            .map(|r| r.local_name.as_str())
            .collect::<Vec<_>>(),
        ["Midtjylland", "Hovedstaden"]
    );
    // the list does not depend on the order the index listed the regions in
    let reversed = Regions::from_rows(rows.into_iter().rev().collect());
    assert_eq!(regions.list(), reversed.list());
    assert_eq!(
        regions
            .list()
            .iter()
            .map(|r| (r.country_code.as_str(), r.admin_code.as_str()))
            .collect::<Vec<_>>(),
        [("dk", "82"), ("dk", "84"), ("ie", "D")]
    );
}

#[test]
fn place_names_agree_across_qualifiers_spelling_and_script() {
    for (expected, actual, agree) in [
        ("Valga", "Valga city", true),
        ("Jakarta", "Special Capital Region of Jakarta", true),
        ("Hyderabad", "Hyderabad City Taluka", true),
        ("Cambridge", "Town of Cambridge", true),
        ("Århus", "Aarhus", true),
        ("Hanoi", "Hà Nội", true),
        ("Baarle-Nassau", "Baarle Nassau", true),
        ("大阪", "大阪市", true),
        ("京都", "京都府", true),
        ("上海", "上海市", true),
        ("大阪", "大坂", false),
        ("上海", "Shanghai", false),
        ("Capital Region", "Region", false),
        ("Paris", "Parish", false),
        ("Kansas", "Arkansas", false),
    ] {
        assert_eq!(
            names_agree(expected, actual),
            agree,
            "{expected} vs {actual}"
        );
    }
}

#[test]
fn province_corroborates_region() {
    let address = parsed(&[("state", "Ontario"), ("city", "London")]);
    let details = AddressDetails {
        city: "London".into(),
        province: "Ontario".into(),
        country_code: "ca".into(),
        ..Default::default()
    };
    assert!(region_agrees(&address, Some(&details)));
}

#[test]
fn postcode_and_a_place_above_the_locality_corroborate_the_town() {
    let at = |state: &str, suburb: &str, postcode: &str| FullAddress {
        address_details: AddressDetails {
            country_code: "de".into(),
            city: "Mitte".into(),
            state: state.into(),
            suburb: suburb.into(),
            postcode: postcode.into(),
            ..Default::default()
        }
        .into(),
        ..Default::default()
    };
    let berlin = parsed(&[
        ("road", "unter den linden"),
        ("postcode", "10117"),
        ("city", "berlin"),
    ]);
    let house = at("Berlin", "", "10117");
    assert!(!city_agrees(&berlin, house.address_details.as_option()));
    assert!(postcode_place_agrees(
        &berlin,
        "10117",
        house.address_details.as_option()
    ));
    // the postcode must be the stated one: a same-named state alone is no evidence
    assert!(!postcode_place_agrees(
        &berlin,
        "10117",
        at("Berlin", "", "12043").address_details.as_option()
    ));
    assert!(!postcode_place_agrees(
        &berlin,
        "",
        house.address_details.as_option()
    ));
    assert!(!postcode_place_agrees(
        &berlin,
        "10117",
        at("Bayern", "", "10117").address_details.as_option()
    ));
    // a Danish postal district letter is not part of the town
    let copenhagen = parsed(&[("postcode", "1165"), ("city", "københavn k")]);
    let k = at("Region Hovedstaden", "København", "1165");
    assert!(postcode_place_agrees(
        &copenhagen,
        "1165",
        k.address_details.as_option()
    ));
    assert!(!postcode_place_agrees(
        &parsed(&[("city", "københavn k")]),
        "1165",
        at("", "Aarhus", "1165").address_details.as_option()
    ));
}

fn at(country: &str, road: &str, house: &str, city: &str, postcode: &str) -> FullAddress {
    FullAddress {
        address_details: AddressDetails {
            country_code: country.into(),
            road: road.into(),
            house_number: house.into(),
            city: city.into(),
            postcode: postcode.into(),
            ..Default::default()
        }
        .into(),
        ..Default::default()
    }
}

#[test]
fn place_names_agree_across_abbreviations() {
    for (expected, actual) in [
        ("Mt. Gretna", "Mount Gretna"),
        ("St. Louis", "Saint Louis"),
        ("Ft Wayne", "Fort Wayne"),
        ("Sankt Gallen", "St. Gallen"),
    ] {
        assert!(names_agree(expected, actual), "{expected} vs {actual}");
    }
    assert!(!names_agree("Mt. Gretna", "Mount Pleasant"));
}

#[test]
fn a_junk_or_merged_town_does_not_outweigh_house_road_and_postcode() {
    let street = at("us", "South 5th Street", "567", "West Dundee", "60118");
    let stated = |city: &str| {
        parsed(&[
            ("house_number", "567"),
            ("road", "s 5th street"),
            ("city", city),
            ("postcode", "60118"),
        ])
    };
    let details = street.address_details.as_option();
    assert!(!city_agrees(&stated("idx"), details));
    assert!(house_road_postcode(&stated("idx"), "60118", details));
    assert!(!house_road_postcode(&stated("idx"), "60119", details));
    assert!(!house_road_postcode(
        &parsed(&[("house_number", "568"), ("road", "s 5th street")]),
        "60118",
        details
    ));
    // a merged municipality's name holds its old village's: only beside the exact house and road
    let village = at("de", "Hegelstraße", "8", "Bad Soden", "");
    let merged = parsed(&[
        ("house_number", "8"),
        ("road", "hegelstraße"),
        ("city", "bad soden-salmünster"),
    ]);
    assert!(house_road_city(
        &merged,
        village.address_details.as_option()
    ));
    let other_house = parsed(&[
        ("house_number", "9"),
        ("road", "hegelstraße"),
        ("city", "bad soden-salmünster"),
    ]);
    assert!(!house_road_city(
        &other_house,
        village.address_details.as_option()
    ));
}

#[test]
fn a_town_without_postcodes_accepts_the_stated_one_but_not_a_contradicting_one() {
    let address = parsed(&[
        ("city", "mt. gretna"),
        ("state", "pa"),
        ("postcode", "17064"),
    ]);
    let held = |postcode: &str, state: &str, code: &str| FullAddress {
        address_details: AddressDetails {
            country_code: "us".into(),
            city: "Mount Gretna".into(),
            state: state.into(),
            postcode: postcode.into(),
            region_codes: vec![ISO3166_2 {
                admin_level: 4,
                code: code.into(),
                ..Default::default()
            }],
            ..Default::default()
        }
        .into(),
        ..Default::default()
    };
    let pa = held("", "Pennsylvania", "US-PA");
    assert_eq!(
        unpostcoded_candidate(&address, "us", "17064", std::slice::from_ref(&pa), |_| {
            false
        }),
        Some(0)
    );
    // a postcode the town holds is judged by choose_candidate, not this
    assert_eq!(
        unpostcoded_candidate(
            &address,
            "us",
            "17064",
            &[held("17042", "Pennsylvania", "US-PA")],
            |_| false
        ),
        None
    );
    // another state's town of that name
    assert_eq!(
        unpostcoded_candidate(
            &address,
            "us",
            "17064",
            &[held("", "Ohio", "US-OH")],
            |_| false
        ),
        None
    );
    // a US ZIP is no Canadian postcode: "Taunton, MA 02780, Canada" is not Taunton, Ontario
    let ontario = FullAddress {
        address_details: AddressDetails {
            country_code: "ca".into(),
            city: "Taunton".into(),
            ..Default::default()
        }
        .into(),
        ..Default::default()
    };
    let taunton = parsed(&[
        ("city", "taunton"),
        ("postcode", "02780"),
        ("country", "canada"),
    ]);
    assert_eq!(
        unpostcoded_candidate(&taunton, "ca", "02780", &[ontario], |_| false),
        None
    );
    // nor does a country whose postcodes are unknown to us vouch for one
    let congo = FullAddress {
        address_details: AddressDetails {
            country_code: "cd".into(),
            city: "Avenue".into(),
            ..Default::default()
        }
        .into(),
        ..Default::default()
    };
    let avenue = parsed(&[("city", "avenue"), ("postcode", "45229")]);
    assert_eq!(
        unpostcoded_candidate(&avenue, "", "45229", &[congo], |_| false),
        None
    );
    // the data knows the postcode, so the town it names there is another one
    assert_eq!(
        unpostcoded_candidate(&address, "us", "17064", &[pa], |cc| cc == "us"),
        None
    );
    assert!(postcode_fits("ca", "H9R 5Z5") && !postcode_fits("ca", "02780"));
    assert!(postcode_fits("xx", "anything"));
}

#[test]
fn a_country_written_as_the_last_part_is_found_when_the_parser_missed_it() {
    let missed = parsed(&[("house", "kingdom of bahrain")]);
    assert_eq!(
        country_in_tail(
            "7th Flr. Al Zamel Building, Manama. P.O. Box 54482, Kingdom of Bahrain",
            &missed
        )
        .as_deref(),
        Some("Kingdom of Bahrain")
    );
    // a state code, a state the parser labelled, a lone part and a numbered part are no country
    assert_eq!(
        country_in_tail("287 Traffic Road, VA 23924", &parsed(&[])),
        None
    );
    assert_eq!(country_in_tail("Springfield, VA", &parsed(&[])), None);
    assert_eq!(
        country_in_tail("Atlanta, Georgia", &parsed(&[("state", "georgia")])),
        None
    );
    assert_eq!(country_in_tail("Bahrain", &parsed(&[])), None);
}

#[test]
fn coverage_counts_the_query_words_a_candidate_explains() {
    let thuin = ["Thuin, Hainaut, Wallonia, Belgium", "Twin"];
    let (explained, total) = coverage(
        "Shop 35, Pietersburg Rd, Twin City Shopping Centre, Burgersfort, Lebowokgomo",
        thuin,
    );
    assert!(total >= 6 && explained * 2 < total, "{explained}/{total}");
    let burgersfort = ["Burgersfort, Sekhukhune District Municipality, Limpopo, South Africa"];
    let (explained, _) = coverage(
        "Shop 35, Pietersburg Rd, Twin City Shopping Centre, Burgersfort, Lebowokgomo",
        burgersfort,
    );
    assert_eq!(explained, 1);
    // a postcode written with a space is the candidate's unspaced one
    let toronto = ["M3J3G8, North York, Toronto, Ontario, Canada"];
    assert_eq!(coverage("Toronto, Ontario, M3J 3G8", toronto), (4, 4));
    // a repeated word counts once, and single letters and street types say nothing
    assert_eq!(coverage("Afrique, Afrique", ["Paris"]), (0, 1));
    assert_eq!(coverage("A, St", ["Paris"]), (0, 0));
    assert_eq!(coverage("State of Paris", ["Paris"]), (1, 1));
    assert_eq!(coverage("Missing Road", ["Paris, France"]), (0, 1));
    let (explained, total) = coverage("Jl. Imaginary Km 7.400", ["Paris, France"]);
    assert!(explained < total);
}

#[test]
fn postcodes_are_written_as_the_country_writes_them() {
    for (country, input, expected) in [
        ("nl", "1012LG", "1012 LG"),
        ("nl", "1012 lg", "1012 LG"),
        ("ca", "H9R5Z5", "H9R 5Z5"),
        ("ca", "v2y 3b3", "V2Y 3B3"),
        ("gb", "ha5 5ne", "HA5 5NE"),
        ("gb", "SW1A2AA", "SW1A 2AA"),
        ("gb", "EC4", "EC4"),
        ("gb", "rh14", "RH14"),
        ("gb", "N12 0Dr", "N12 0DR"),
        ("ie", "d02af30", "D02 AF30"),
        ("cz", "14000", "140 00"),
        ("pl", "00926", "00-926"),
        ("pt", "7040303", "7040-303"),
        ("br", "04815050", "04815-050"),
        ("us", "606011234", "60601-1234"),
        ("us", "60601", "60601"),
        // not ours to rewrite: another alphabet, a prefix, an unknown country, an odd shape
        ("lv", "LV-1050", "LV-1050"),
        ("de", "63628", "63628"),
        ("gb", "12345", "12345"),
        ("", "1012LG", "1012LG"),
    ] {
        assert_eq!(
            format_postcode(country, input),
            expected,
            "{country} {input}"
        );
    }
}

#[test]
fn a_territory_named_as_the_country_matches_its_sovereign() {
    let us = at("us", "", "", "San Juan", "");
    let pr = at("pr", "", "", "San Juan", "");
    assert!(country_matches("pr", us.address_details.as_option()));
    assert!(country_matches("pr", pr.address_details.as_option()));
    assert!(!country_matches("us", pr.address_details.as_option()));
    assert!(!country_matches(
        "gl",
        at("dk", "", "", "", "").address_details.as_option()
    ));
}

#[test]
fn a_po_box_is_told_from_a_misfiled_town() {
    for text in [
        "p.o. box 21",
        "PO Box 9103",
        "Postbus 12",
        "apartado 33",
        "BP 177",
        "box 4",
    ] {
        assert!(is_po_box(text), "{text}");
    }
    for text in ["", "cc baarle-nassau", "boxtel"] {
        assert!(!is_po_box(text), "{text}");
    }
}

#[test]
fn tidy_repairs_parser_slips_that_cost_the_whole_lookup() {
    let mut address = parsed(&[("postcode", "se-262 73"), ("city", "ängelholm")]);
    tidy(&mut address);
    assert_eq!(part(&address, "postcode"), "262 73");
    // Latvian postcodes are stored with their plate
    let mut address = parsed(&[("postcode", "lv-1002")]);
    tidy(&mut address);
    assert_eq!(part(&address, "postcode"), "lv-1002");
    let mut address = parsed(&[("city", "l-1855"), ("country", "luxemburg")]);
    tidy(&mut address);
    assert_eq!(part(&address, "postcode"), "1855");
    assert_eq!(part(&address, "city"), "");
    let mut address = parsed(&[("city", "st julians stj1000"), ("country", "malta")]);
    tidy(&mut address);
    assert_eq!(part(&address, "city"), "st julians");
    assert_eq!(part(&address, "postcode"), "stj1000");
    let mut address = parsed(&[("city", "г. воронеж"), ("road", "ул.")]);
    tidy(&mut address);
    assert_eq!(part(&address, "city"), "воронеж");
    assert_eq!(part(&address, "road"), "");
    // a street named like a type is a street
    let mut address = parsed(&[("road", "burg"), ("city", "bruges")]);
    tidy(&mut address);
    assert_eq!(part(&address, "road"), "burg");
}

#[test]
fn a_state_code_the_candidate_contradicts_is_a_conflict() {
    let coded = |country: &str, code: &str| FullAddress {
        address_details: AddressDetails {
            country_code: country.into(),
            region_codes: vec![ISO3166_2 {
                admin_level: 4,
                code: code.into(),
                ..Default::default()
            }],
            ..Default::default()
        }
        .into(),
        ..Default::default()
    };
    let address = parsed(&[("city", "dorchester"), ("state", "ma")]);
    let conflicts = |f: &FullAddress| state_code_conflict(&address, f.address_details.as_option());
    assert!(conflicts(&coded("gb", "GB-ENG")));
    assert!(conflicts(&coded("us", "US-WI")));
    assert!(!conflicts(&coded("us", "US-MA")));
    assert!(!conflicts(&candidate("gb", "Dorchester", "", "")));
    // a state written out is not a code
    let named = parsed(&[("state", "massachusetts")]);
    assert!(!state_code_conflict(
        &named,
        coded("us", "US-WI").address_details.as_option()
    ));
}

#[test]
fn a_country_ending_the_query_without_a_comma_gets_one() {
    for (query, expected) in [
        ("Dublin 2 Ireland", Some("Dublin 2, Ireland")),
        ("Paris France", Some("Paris, France")),
        ("12 Rue X, Paris France", Some("12 Rue X, Paris, France")),
        ("Santa Fe New Mexico", None),
        ("Dublin 2, Ireland", None),
        ("Dublin", None),
    ] {
        assert_eq!(comma_before_country(query).as_deref(), expected, "{query}");
    }
}

#[test]
fn a_stray_word_is_removed_from_the_query() {
    assert_eq!(
        without_word("Dr. Nikiski, AK 99611", "dr."),
        "Nikiski, AK 99611"
    );
}

#[test]
fn road_abbreviations_are_country_scoped_and_preserve_house_numbers() {
    for (cc, typed, found, agrees) in [
        ("fr", "R. du Bosquet", "Rue du Bosquet", true),
        ("ch", "Ulstr.", "Ulstrasse", true),
        ("de", "Ulstr.", "Ulstrasse", true),
        ("at", "Ulstr.", "Ulstrasse", true),
        ("gb", "Ulstr.", "Ulstrasse", false),
        ("ch", "Ulstr", "Ulstrasse", false),
        ("za", "Acacia Straat", "Acacia Street", true),
        ("za", "Acacia Street", "Acacia Straat", true),
        ("gb", "Acacia Straat", "Acacia Street", false),
        ("za", "Acacia Straat", "Other Street", false),
        ("ch", "Birkenstarsse", "Birkenstrasse", true),
        ("de", "Birkenstrase", "Birkenstrasse", true),
        ("at", "Birkensrasse", "Birkenstrasse", true),
        ("ch", "Birkensrase", "Birkenstrasse", false),
        ("no", "Granvn.", "Granveien", true),
        ("no", "Granvn.", "Granvegen", true),
        ("nl", "Willowln.", "Willowlaan", true),
        ("no", "Granvn. - North", "Granveien - North", true),
        ("no", "Granvn. - North", "Granvegen - North", true),
        ("no", "Dr. Granvn. North", "Dr. Granveien North", true),
        ("nl", "Willowln. - East", "Willowlaan - East", true),
        ("no", "Granvn. North", "Granveien South", false),
        ("no", "Solhavn North", "Solhaveien North", false),
        ("nl", "Koeln East", "Koelaan East", false),
        ("no", "Gran vn. North", "Granveien North", false),
        ("no", "Granvn. Askvn.", "Granveien Askveien", false),
        ("se", "Granvn. North", "Granveien North", false),
        ("no", "Granvn.", "Grandveien", false),
        ("no", "Solhavn", "Solhaveien", false),
        ("nl", "Koeln", "Koelaan", false),
        ("nl", "Willowln.", "Willowstraat", false),
        ("pt", "R. du Bosquet", "Rua du Bosquet", true),
        ("pt", "Rua dos Farias", "R FARIAS", true),
        ("pt", "Rua Tenente Valadim", "R TEN VALADIM", true),
        ("pt", "Bairro de Santa Bárbara", "BR S BARBARA", true),
        ("pt", "Caminho Figueirinhas", "CAM FIGUEIRINHAS", true),
        ("pt", "Rua dos Farias", "R FARIA", false),
        ("pt", "Rua da Cruz", "TV CRUZ", false),
        ("pt", "Rua Manuel da Fonseca", "R MANUEL FONSECA", false),
        ("es", "Rua dos Farias", "R FARIAS", false),
        ("pt", "R. du Bosquet", "Rue du Bosquet", false),
        ("gb", "R. du Bosquet", "Rue du Bosquet", false),
        ("se", "Granvn.", "Granveien", false),
        ("gb", "Willowln.", "Willowlaan", false),
    ] {
        let address = parsed(&[("house_number", "5b"), ("road", typed), ("country", cc)]);
        let mut details = AddressDetails {
            country_code: cc.into(),
            road: found.into(),
            house_number: "5b".into(),
            ..Default::default()
        };
        assert_eq!(
            road_agrees(&address, Some(&details)),
            agrees,
            "{cc}: {typed} -> {found}"
        );
        details.house_number = "5c".into();
        assert!(!road_agrees(&address, Some(&details)));
    }
    let address = parsed(&[
        ("house_number", "5b"),
        ("road", "Granvn."),
        ("country", "Sweden"),
    ]);
    let details = AddressDetails {
        country_code: "no".into(),
        road: "Granveien".into(),
        house_number: "5b".into(),
        ..Default::default()
    };
    assert!(!road_agrees(&address, Some(&details)));
}

#[test]
fn a_road_the_index_respelt_agrees() {
    let house = |road: &str| AddressDetails {
        road: road.into(),
        house_number: "3717".into(),
        ..Default::default()
    };
    let agrees = |typed: &str, found: &str| {
        road_agrees(
            &parsed(&[("house_number", "3717"), ("road", typed)]),
            Some(&house(found)),
        )
    };
    assert!(agrees("West 54th Street", "W 54th St"));
    assert!(agrees("West 54th Srteet", "West 54th Street"));
    assert!(agrees("Whte Dawn Lane", "White Dawn Lane"));
    assert!(agrees("Bowood Dr", "Boxwood Drive"));
    // two words off, another first letter, or another street type are other roads
    assert!(!agrees("Whte Dwan Lane", "White Dawn Lane"));
    assert!(!agrees("Bowood Dr", "Dowood Drive"));
    assert!(!agrees("Boxwood Dr", "Boxwood Court"));
}

#[test]
fn a_part_that_repeats_another_is_collapsed() {
    for (query, expected) in [
        // a country by its code beside its name
        (
            "San Francisco, US, United States",
            "San Francisco, United States",
        ),
        ("Delhi, IN, India", "Delhi, India"),
        ("Madrid, ES, Spain", "Madrid, Spain"),
        ("Taipei, Taipei 10462, TW, Taiwan", "Taipei 10462, Taiwan"),
        // the same text twice; the later copy goes, except a country that ends the address
        ("Kyiv, Kyiv, Ukraine", "Kyiv, Ukraine"),
        ("Chile, Chile", "Chile"),
        (
            "New York, United States, New York, United States",
            "New York, United States",
        ),
        (
            "ENCINO, California, United States, ENCINO, United States",
            "ENCINO, California, United States",
        ),
        ("Mexico, Mexico City, Mexico", "Mexico City, Mexico"),
        // a town beside the same town with its postcode
        ("Madrid, Madrid 28007, ES, Spain", "Madrid 28007, Spain"),
    ] {
        assert_eq!(
            without_redundant_parts(query).as_deref(),
            Some(expected),
            "{query}"
        );
    }
    // no part repeats: a state code that is not the country's, a street named as its town
    for query in [
        "Paris, Texas",
        "Atlanta, GA, United States",
        "Dover, DE 19901",
        "12 Main Street, Springfield, IL 62701",
        "Rue de Paris, Paris, France",
        "Berlin",
        // "México" is the state, "Mexico" the country: two names, not a code and its name
        "Calimaya, México, Mexico",
        // combining marks (Thai vowels) are part of a word, not a separator
        "Shizhou, Chencun, ซุ่นเตื่อ, ฝอซาน, กวางตุ้ง 528313, จีน",
        // the same name in another script is left to the parser
        "Варна, бул. Приморски полк 43, 9000 Varna, Bulgaria",
    ] {
        assert_eq!(without_redundant_parts(query), None, "{query}");
    }
}

#[test]
fn a_region_that_only_repeats_the_town_does_not_outweigh_the_stated_country() {
    // "Tokyo" is a city and a prefecture, "Delhi" a city and a union territory
    let found_in_japan = candidate("jp", "Tokyo", "Tokyo", "");
    let tokyo = parsed(&[
        ("city", "tokyo"),
        ("state", "tokyo"),
        ("country", "united states"),
    ]);
    assert_eq!(
        choose_candidate(&tokyo, "us", "", std::slice::from_ref(&found_in_japan)),
        None
    );
    let delhi = parsed(&[("city", "delhi"), ("state", "delhi"), ("country", "canada")]);
    let india = candidate("in", "Delhi", "Delhi", "");
    let ontario = candidate("ca", "Delhi", "Ontario", "");
    assert_eq!(
        choose_candidate(&delhi, "ca", "", std::slice::from_ref(&india)),
        None
    );
    assert_eq!(
        choose_candidate(&delhi, "ca", "", &[india, ontario]).map(|(i, _)| i),
        Some(1)
    );
    // a region stated beside the town still places it, and a lone region is the region
    let paris = parsed(&[("city", "paris"), ("state", "texas")]);
    let found = [
        candidate("fr", "Paris", "Île-de-France", ""),
        candidate("us", "Paris", "Texas", ""),
    ];
    assert_eq!(
        choose_candidate(&paris, "", "", &found).map(|(i, _)| i),
        Some(1)
    );
    let california = parsed(&[
        ("state", "california"),
        ("city", "california"),
        ("country", "united states"),
    ]);
    let state = candidate("us", "", "California", "");
    assert!(choose_candidate(&california, "us", "", &[state]).is_some());
}

#[test]
fn a_postcode_in_another_region_does_not_outweigh_the_stated_town() {
    let address = parsed(&[
        ("city", "multan"),
        ("state", "punjab"),
        ("country", "pakistan"),
    ]);
    let pick =
        |items: &[FullAddress]| choose_candidate(&address, "pk", "66000", items).map(|(i, _)| i);
    assert_eq!(
        pick(&[
            candidate("pk", "Kingri", "Sindh", "66000"),
            candidate("pk", "Multan", "Punjab", ""),
        ]),
        Some(1)
    );
    // the postcode still wins when it is the stated town, and when nothing in that region was found
    assert_eq!(
        pick(&[
            candidate("pk", "Kingri", "Sindh", "66000"),
            candidate("pk", "Multan", "Punjab", "66000"),
        ]),
        Some(1)
    );
    assert_eq!(
        pick(&[candidate("pk", "Kingri", "Sindh", "66000")]),
        Some(0)
    );
    // no region stated: the postcode is the evidence there is
    let unstated = parsed(&[("city", "multan"), ("country", "pakistan")]);
    assert_eq!(
        choose_candidate(
            &unstated,
            "pk",
            "66000",
            &[
                candidate("pk", "Kingri", "Sindh", "66000"),
                candidate("pk", "Multan", "Punjab", ""),
            ],
        )
        .map(|(i, _)| i),
        Some(0)
    );
}

#[test]
fn a_leading_region_code_is_tried_behind_the_place() {
    assert_eq!(
        place_before_region_code("NJ, WILLIAMSTOWN, United States").as_deref(),
        Some("WILLIAMSTOWN, NJ, United States")
    );
    assert_eq!(place_before_region_code("CA, Toronto, Canada"), None);
    assert_eq!(place_before_region_code("PA, 19560"), None);
    assert_eq!(
        place_before_region_code("Williamstown, NJ, United States"),
        None
    );
}

#[test]
fn canadian_unit_prefixes_need_an_unambiguous_canadian_country() {
    for (unit, number) in [
        ("100", "18107"),
        ("A", "12A"),
        ("BSMT", "51"),
        ("UPPR", "67"),
        ("APT", "707B"),
        ("1", "178 1/2"),
    ] {
        let query = format!("{unit}-{number} Maple Road, Delhi, ON N4B 1A1, Canada");
        let (building, civic) = canadian_building(&query).unwrap();
        assert_eq!(civic, number);
        assert_eq!(
            building,
            format!("{number} Maple Road, Delhi, ON N4B 1A1, Canada")
        );
        assert!(canadian_building(building).is_none());
    }
    for query in [
        "12 Maple Road, Delhi, ON N4B 1A1, Canada",
        "1-12 Maple Road, Delhi, ON N4B 1A1",
        "1-12 Maple Road, Delhi, ON N4B 1A1, CA",
        "1-12 Maple Road, Delhi, Canada, United States",
        "1-12 Maple Road, Delhi, Australia",
        "1-12 Maple Road, Delhi, Poland",
        "1-12 Maple Road, Delhi, New Zealand",
        "1-12 Maple Road, Delhi, Unknownland",
        "1-12, Canada",
        "1-12 , Canada",
        "1-0 Maple Road, Canada",
        "1-2-12 Maple Road, Canada",
        "12 Maple Road, 1-12, Canada",
    ] {
        assert_eq!(canadian_building(query), None, "{query}");
    }
    let too_long = format!("1-12 {}, Canada", "x".repeat(256));
    assert!(canadian_building(&too_long).is_none());
}

#[test]
fn glued_house_suffixes_and_canadian_street_directions_have_retry_forms() {
    assert_eq!(
        spaced_house_suffix("82A AV MERCIER, SALABERRY-DE-VALLEYFIELD, QC J6S 6L9, Canada"),
        Some((
            "82 A AV MERCIER, SALABERRY-DE-VALLEYFIELD, QC J6S 6L9, Canada".into(),
            "82A".into()
        ))
    );
    for query in ["82 AV MERCIER, Canada", "82AB Mercier, Canada", "Rue 82A"] {
        assert_eq!(spaced_house_suffix(query), None, "{query}");
    }
    for (query, undirected) in [
        (
            "31 THATCHER DR E, MOOSE JAW, SK S6J 1L8, Canada",
            "31 THATCHER DR, MOOSE JAW, SK S6J 1L8, Canada",
        ),
        (
            "162 North Klondike HWY, Whitehorse, YT Y1A 5W5, Canada",
            "162 Klondike HWY, Whitehorse, YT Y1A 5W5, Canada",
        ),
        (
            "870 6e Rue Ouest, Senneterre, QC J0Y 2M0",
            "870 6e Rue, Senneterre, QC J0Y 2M0",
        ),
    ] {
        let (got, number, _) = without_street_direction(query).unwrap();
        assert_eq!(got, undirected);
        assert_eq!(number, query.split(' ').next().unwrap());
    }
    assert_eq!(
        without_street_direction("31 THATCHER DR E, MOOSE JAW, SK S6J1L8")
            .unwrap()
            .2,
        "S6J1L8"
    );
    for query in [
        "31 THATCHER DR E, MOOSE JAW, Canada",
        "31 THATCHER DR, MOOSE JAW, SK S6J 1L8",
        "100 Rue E, Quebec, QC G1A 0A3",
        "Thatcher Dr E 31, Moose Jaw, SK S6J 1L8",
        "31 THATCHER DR E",
    ] {
        assert_eq!(without_street_direction(query), None, "{query}");
    }
}

#[test]
fn a_danish_floor_and_door_part_is_set_aside_without_touching_other_countries() {
    let unit = |q| without_danish_unit(q);
    // the published DAR forms: the unit is its own part, the building keeps the rest
    for (query, building) in [
        (
            "Mogensgade 9, st. th, 8500 Grenaa, Denmark",
            "Mogensgade 9, 8500 Grenaa, Denmark",
        ),
        (
            "Tjærbyvej 19, st. th, 8930 Randers NØ, Denmark",
            "Tjærbyvej 19, 8930 Randers NØ, Denmark",
        ),
        (
            "Vestergade 6, 1. tv, 9500 Hobro, Danmark",
            "Vestergade 6, 9500 Hobro, Danmark",
        ),
        (
            "Broparken 4, 2. 3, 9560 Hadsund, Denmark",
            "Broparken 4, 9560 Hadsund, Denmark",
        ),
        // a floor alone and a door alone, with Denmark stated
        (
            "Vendelbogade 64, st., 9800 Hjørring, Denmark",
            "Vendelbogade 64, 9800 Hjørring, Denmark",
        ),
        (
            "Rosenåen 7, 1., 2600 Glostrup, Denmark",
            "Rosenåen 7, 2600 Glostrup, Denmark",
        ),
        (
            "Øverødvej 90A, 1, Øverød, 2840 Holte, Denmark",
            "Øverødvej 90A, Øverød, 2840 Holte, Denmark",
        ),
        (
            "Skovlyporten 6, st. anex, Øverød, 2840 Holte, Denmark",
            "Skovlyporten 6, Øverød, 2840 Holte, Denmark",
        ),
        // a side door is Danish enough with no country stated
        (
            "Skovvejen 42, st. mf, 6000 Kolding",
            "Skovvejen 42, 6000 Kolding",
        ),
    ] {
        assert_eq!(unit(query).as_deref(), Some(building), "{query}");
    }
    for query in [
        "Mogensgade 9, 8500 Grenaa, Denmark",
        // another country keeps its own TH and NO codes
        "Mogensgade 9, st. th, Bangkok, Thailand",
        "9 Soi 3, st. th, Bangkok, TH",
        "Granveien 5, st. th, 1234 Fjordvik, Norway",
        // a door that is not th/tv/mf, a floor alone or a door alone needs Denmark stated
        "Rue 5, 2. 3, 75001 Paris",
        "Main Street 5, st. 1, Springfield",
        "Rue 5, 2., 75001 Paris",
        "Main Street 5, 12, Springfield",
        "Soi 5, th, Bangkok",
        "Soi 5, th, Bangkok, Thailand",
        // a postcode is no door
        "Mogensgade 9, 8500, Denmark",
        // the unit stands after a house, not first
        "st. th, 8500 Grenaa, Denmark",
        "Mogensgade, st. th, 8500 Grenaa, Denmark",
    ] {
        assert_eq!(unit(query), None, "{query}");
    }
}

#[test]
fn a_danish_supplementary_place_is_set_aside_only_between_house_and_postcode() {
    let place = |q| without_danish_place(q);
    assert_eq!(
        place("Pittevej 6, Aidt, 8881 Thorsø, Denmark"),
        Some(("Pittevej 6, 8881 Thorsø, Denmark".into(), "Aidt".into()))
    );
    assert_eq!(
        place("Helmklit 214, Nr Fjand, 6990 Ulfborg, Danmark"),
        Some((
            "Helmklit 214, 6990 Ulfborg, Danmark".into(),
            "Nr Fjand".into()
        ))
    );
    for query in [
        "Pittevej 6, 8881 Thorsø, Denmark",
        // Denmark must be stated, and no other country
        "Pittevej 6, Aidt, 8881 Thorsø",
        "Rue 6, Aidt, 8881 Thorsø, France",
        "Rue 6, Aidt, 8881 Thorsø, Denmark, France",
        // the place stands after a house and before a four-digit postcode and town
        "Pittevej, Aidt, 8881 Thorsø, Denmark",
        "Pittevej 6, Aidt, Thorsø 8881, Denmark",
        "Pittevej 6, Aidt 2, 8881 Thorsø, Denmark",
    ] {
        assert_eq!(place(query), None, "{query}");
    }
}

#[test]
fn an_australian_slashed_unit_is_set_aside_without_touching_other_countries() {
    for (query, building) in [
        (
            "71/206 PHILLIP STREET, MOUNT PLEASANT QLD 4740, Australia",
            "206 PHILLIP STREET, MOUNT PLEASANT QLD 4740, Australia",
        ),
        (
            "54/554-560 Mowbray Road West, Lane Cove North NSW 2066",
            "554-560 Mowbray Road West, Lane Cove North NSW 2066",
        ),
        (
            "1501/1D Greenbank St, Hurstville, NSW, Australia",
            "1D Greenbank St, Hurstville, NSW, Australia",
        ),
        (
            "Harbour View, G01/5 Smith St, Fitzroy VIC 3065",
            "Harbour View, 5 Smith St, Fitzroy VIC 3065",
        ),
    ] {
        assert_eq!(
            without_australian_unit(query).as_deref(),
            Some(building),
            "{query}"
        );
    }
    for query in [
        "206 Phillip Street, Mount Pleasant QLD 4740, Australia",
        // Czech descriptive/orientation and Polish building/flat numbers
        "Vinohradská 1511/230, 100 00 Praha, Czechia",
        "ul. Marszałkowska 10/16, 00-590 Warszawa, Poland",
        // no Australian state and postcode, nor Australia stated
        "12/3 Main Street, Springfield",
        // another country stated beside a state-like code
        "5/12 Smith St, Fitzroy VIC 3065, New Zealand",
        // the slash is not a unit before a street
        "1/2 Smith St",
    ] {
        assert_eq!(without_australian_unit(query), None, "{query}");
    }
}

#[test]
fn a_suburb_is_its_own_town_only_when_no_road_narrows_it() {
    // Blairgowrie is a suburb the index files under Johannesburg
    let suburb = AddressDetails {
        suburb: "Blairgowrie".into(),
        country_code: "za".into(),
        ..Default::default()
    };
    let address = parsed(&[("city", "blairgowrie")]);
    assert!(city_agrees(&address, Some(&suburb)));
    let on_a_road = AddressDetails {
        road: "Conrad Drive".into(),
        ..suburb
    };
    assert!(!city_agrees(&address, Some(&on_a_road)));
    // a fragment of a few letters names no suburb
    let lettered = AddressDetails {
        suburb: "C".into(),
        ..Default::default()
    };
    assert!(!city_agrees(&parsed(&[("city", "c")]), Some(&lettered)));
}

#[test]
fn a_neighbourhood_is_its_own_town_only_when_no_road_or_house_narrows_it() {
    let area = AddressDetails {
        neighbourhood: "Lower Orchard".into(),
        town: "Beacon".into(),
        ..Default::default()
    };
    let address = parsed(&[("city", "lower orchard")]);
    assert!(city_agrees(&address, Some(&area)));
    assert!(!city_agrees(&parsed(&[("city", "orchard")]), Some(&area)));
    for on_a_road in [
        AddressDetails {
            road: "High Street".into(),
            ..area.clone()
        },
        AddressDetails {
            house_number: "12".into(),
            ..area.clone()
        },
    ] {
        assert!(!city_agrees(&address, Some(&on_a_road)));
    }
}

#[test]
fn a_postal_district_after_the_town_is_the_town() {
    for (key, value) in [("city_district", "dublin 4"), ("postcode", "dublin 4")] {
        let mut address = parsed(&[
            ("road", "barrow street"),
            (key, value),
            ("country", "ireland"),
        ]);
        tidy(&mut address);
        assert_eq!(part(&address, "city"), "dublin", "{key}");
        assert_eq!(part(&address, key), "", "{key}");
    }
    // real postcodes and a town that has its own are left alone
    for address in [
        parsed(&[("postcode", "k1a 0b1")]),
        parsed(&[("postcode", "1234 ab")]),
        parsed(&[("city", "paris"), ("city_district", "paris 15")]),
    ] {
        let mut tidied = address.clone();
        tidy(&mut tidied);
        assert_eq!(tidied, address);
    }
}

#[test]
fn a_region_code_after_the_first_part_is_the_region_name() {
    let region = |code: &str| match code {
        "BCS" => Some("Baja California Sur".to_string()),
        "NSW" => Some("New South Wales".to_string()),
        _ => None,
    };
    for (query, expected) in [
        ("La Paz, B.C.S", "La Paz, Baja California Sur"),
        ("La Paz, BCS, Mexico", "La Paz, Baja California Sur, Mexico"),
        ("St Peters, NSW", "St Peters, New South Wales"),
    ] {
        assert_eq!(with_region_names(query, region).as_deref(), Some(expected));
    }
    // the first part is the place, a word is not a code, two letters are the parser's, and
    // a code the data does not know stays
    for query in [
        "BCS, Mexico",
        "La Paz, Bcs",
        "La Paz, BC",
        "La Paz, XYZ",
        "La Paz, Mexico",
    ] {
        assert_eq!(with_region_names(query, region), None, "{query}");
    }
}

#[test]
fn a_country_override_needs_more_than_a_street_or_a_digit() {
    // a postcode in the shape of the found country's and not of the stated one's
    assert!(postcode_shaped_for(
        "Molenweg 5, 5757 BD Liessel, Belgium",
        "be",
        "nl"
    ));
    assert!(!postcode_shaped_for(
        "Molenweg 5, 2000 Liessel, Belgium",
        "be",
        "nl"
    ));
    assert!(!postcode_shaped_for(
        "99 Conrad Drive, 2194, Blairgowrie, South Africa",
        "za",
        "au"
    ));
    // a road that is only the stated state's name with a street type is no answer to it
    let road = |road: &str| AddressDetails {
        road: road.into(),
        ..Default::default()
    };
    let state = parsed(&[("city", "la paz"), ("state", "baja california sur")]);
    assert!(road_named_for_place(
        &state,
        Some(&road("Calle Baja California Sur"))
    ));
    assert!(!road_named_for_place(&state, Some(&road("Calle Abasolo"))));
    assert!(!road_named_for_place(&state, None));
    // one country named more than once is the town of that name, or the country
    assert!(is_repeated_country("Luxembourg, Luxembourg"));
    assert!(is_repeated_country("Chile, Chile"));
    assert!(!is_repeated_country("Kyiv, Kyiv"));
    assert!(!is_repeated_country("Paris, France"));
}

#[test]
fn a_name_and_its_region_name_the_place_whichever_fields_the_parser_chose() {
    let place = |city: &str, state: &str| AddressDetails {
        village: city.into(),
        state: state.into(),
        country: "South Africa".into(),
        country_code: "za".into(),
        ..Default::default()
    };
    // libpostal reads "Kriel" as a suburb of a town "Mpumalanga"
    let kriel = place("Kriel", "Mpumalanga");
    assert!(named_in_region(
        "Kriel, Mpumalanga, South Africa",
        Some(&kriel)
    ));
    // the region without its administrative word
    let juan = place("Villa San Agustín", "San Juan Province");
    assert!(named_in_region(
        "Villa San Agustín, San Juan, Argentina",
        Some(&juan)
    ));
    // another region's namesake is not the place, nor is a different name in the right one
    assert!(!named_in_region(
        "Kriel, Gauteng, South Africa",
        Some(&kriel)
    ));
    assert!(!named_in_region(
        "Kriek, Mpumalanga, South Africa",
        Some(&kriel)
    ));
    // a lone country is evidence only when no region is stated
    assert!(named_in_region("Kriel, South Africa", Some(&kriel)));
    assert!(!named_in_region(
        "Kriel, Gauteng, South Africa",
        Some(&kriel)
    ));
    assert!(!named_in_region("Kriel", Some(&kriel)));
    assert!(!named_in_region("Kriel, Germany", Some(&kriel)));
}
