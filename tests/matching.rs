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
    let aligned = comma_aligned(&head, " East Amherst", &tail, &whole).unwrap();
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
    assert!(comma_aligned(&head, "East Amherst", &tail, &agreeing).is_none());
    let mut with_country = whole.clone();
    with_country.push(("country".into(), "usa".into()));
    assert!(comma_aligned(&head, "East Amherst", &tail, &with_country).is_none());
    assert!(comma_aligned(&head, "Unit 4", &tail, &whole).is_none());
    let no_town: Vec<_> = whole.iter().filter(|(k, _)| k != "city").cloned().collect();
    assert!(comma_aligned(&head, "Bronx", &tail, &no_town).is_none());
    let mut split = whole.clone();
    split.push(("suburb".into(), "avondale".into()));
    assert!(comma_aligned(&head, "Avondale Goodyear", &tail, &split).is_none());
    let town_in_tail = pairs(&[("city", "ithaca"), ("state", "ny"), ("postcode", "14850")]);
    assert!(comma_aligned(&head, "East Amherst", &town_in_tail, &whole).is_none());
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
