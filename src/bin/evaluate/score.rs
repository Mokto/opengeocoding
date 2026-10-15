use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub const LEVELS: [&str; 7] = [
    "none", "country", "region", "locality", "postcode", "street", "house",
];
/// Ordered from best to worst; a case's class is the least severe over its documented alternatives.
pub const CLASSES: [&str; 12] = [
    "pass",
    "confidence_mismatch",
    "too_coarse",
    "abstain",
    "too_fine",
    "wrong_component",
    "wrong_region",
    "near_miss",
    "unexpected_result",
    "far_miss",
    "wrong_country",
    "error",
];
pub const CRITICAL: [&str; 3] = ["wrong_country", "far_miss", "unexpected_result"];
const LOCALITY_KEYS: [&str; 15] = [
    "city",
    "town",
    "village",
    "municipality",
    "hamlet",
    "suburb",
    "cityDistrict",
    "district",
    "borough",
    "neighbourhood",
    "quarter",
    "croft",
    "isolatedDwelling",
    "subdivision",
    "cityBlock",
];
const REGION_KEYS: [&str; 5] = ["state", "stateDistrict", "county", "province", "region"];

pub fn rank(level: &str) -> usize {
    LEVELS.iter().position(|l| *l == level).unwrap_or(0)
}

pub fn severity(class: &str) -> usize {
    CLASSES
        .iter()
        .position(|c| *c == class)
        .unwrap_or(CLASSES.len())
}

pub fn critical(class: &str) -> bool {
    CRITICAL.contains(&class)
}

/// A coarser answer is placed by its own centroid, so judge it against that feature's typical size.
fn coarse_km(level: &str) -> Option<f64> {
    match level {
        "street" => Some(2.),
        "postcode" => Some(5.),
        "locality" => Some(25.),
        _ => None,
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Expect {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub any_of: Option<Vec<Expect>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub level: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    #[serde(default)]
    pub country: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius_km: Option<f64>,
    #[serde(default)]
    pub region: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin_level: Option<u64>,
    #[serde(default)]
    pub components: IndexMap<String, String>,
    #[serde(default)]
    pub good_match: Option<bool>,
}

impl Expect {
    pub fn alternatives(&self) -> &[Expect] {
        self.any_of.as_deref().unwrap_or(std::slice::from_ref(self))
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Summary {
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub point: Option<[f64; 2]>,
    #[serde(default)]
    pub good_match: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    #[serde(default)]
    pub region_codes: Vec<String>,
    // Optional so records from runs that predate these fields are not reported as drift.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regions: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub house_number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub road: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postcode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locality: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Score {
    pub class: &'static str,
    pub level: &'static str,
    pub flags: Vec<String>,
    pub distance_km: Option<f64>,
}

pub fn haversine(a: [f64; 2], b: [f64; 2]) -> f64 {
    let [lat1, lon1, lat2, lon2] = [a[0], a[1], b[0], b[1]].map(f64::to_radians);
    let h = ((lat2 - lat1) / 2.).sin().powi(2)
        + lat1.cos() * lat2.cos() * ((lon2 - lon1) / 2.).sin().powi(2);
    2. * 6371.0088 * h.sqrt().asin()
}

/// Rounds the exact binary value through decimal formatting; `(x * 10^n).round()` can
/// round the other way after the inexact multiplication (51.1563185 -> 51.156319).
pub fn round(value: f64, digits: usize) -> f64 {
    format!("{value:.digits$}").parse().unwrap_or(value)
}

fn compact(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !(ch.is_whitespace() || *ch == '-' || ('\u{1c}'..='\u{1f}').contains(ch)))
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}

pub fn details(response: &Value) -> &Value {
    &response["addressDetails"]
}

/// proto3 JSON omits zero floats; a present message with neither field is still a point at 0,0.
pub fn coordinates(response: &Value) -> Option<[f64; 2]> {
    let geo = response.get("geoCoordinates")?;
    Some([
        geo["lat"].as_f64().unwrap_or(0.),
        geo["lon"].as_f64().unwrap_or(0.),
    ])
}

pub fn region_codes(response: &Value) -> BTreeSet<String> {
    details(response)["regionCodes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|code| text(code, "code").to_uppercase())
        .collect()
}

pub fn level_of(response: &Value) -> &'static str {
    let d = details(response);
    if text(d, "countryCode").is_empty() && coordinates(response).is_none() {
        return "none";
    }
    if !text(d, "houseNumber").is_empty() {
        return "house";
    }
    if !text(d, "road").is_empty() {
        return "street";
    }
    let postcode = text(d, "postcode");
    if !postcode.is_empty()
        && compact(text(response, "displayName")).starts_with(&compact(postcode))
    {
        return "postcode";
    }
    if LOCALITY_KEYS.iter().any(|key| !text(d, key).is_empty()) {
        return "locality";
    }
    if REGION_KEYS.iter().any(|key| !text(d, key).is_empty()) {
        return "region";
    }
    "country"
}

/// Shapes a raw index JSON hit like a FullAddress so both are scored identically.
pub fn from_index(hit: &Value) -> Value {
    let mut address = serde_json::Map::new();
    let mut codes = vec![];
    for (key, value) in hit["address"].as_object().into_iter().flatten() {
        if key.contains("ISO3166-2") {
            codes.push(json!({ "code": value }));
        } else {
            let mut camel = String::new();
            let mut upper = false;
            for ch in key.chars() {
                if ch == '_' {
                    upper = true;
                } else if upper {
                    camel.extend(ch.to_uppercase());
                    upper = false;
                } else {
                    camel.push(ch);
                }
            }
            address.insert(camel, value.clone());
        }
    }
    address.insert("regionCodes".into(), Value::Array(codes));
    let mut response = json!({
        "displayName": text(hit, "display_name"),
        "addressDetails": address,
    });
    let number = |key: &str| text(hit, key).parse::<f64>().ok();
    if let (Some(lat), Some(lon)) = (number("lat"), number("lon")) {
        response["geoCoordinates"] = json!({ "lat": lat, "lon": lon });
    }
    response
}

fn score_one(expect: &Expect, response: &Value, confidence: bool) -> Score {
    let level = level_of(response);
    let mut result = Score {
        level,
        ..Default::default()
    };
    let done = |mut result: Score, class| {
        result.class = class;
        result
    };
    if expect.level == "none" {
        return done(
            result,
            if level == "none" {
                "pass"
            } else {
                "unexpected_result"
            },
        );
    }
    if level == "none" {
        return done(result, "abstain");
    }
    let d = details(response);
    let point = coordinates(response);
    if let (Some(expected), Some(point)) = (expect.point, point) {
        result.distance_km = Some(round(haversine(expected, point), 3));
    }
    if !expect
        .country
        .iter()
        .any(|c| *c == text(d, "countryCode").to_lowercase())
    {
        return done(result, "wrong_country");
    }
    let codes = region_codes(response);
    let expected: BTreeSet<_> = expect.region.iter().cloned().collect();
    if expect.area.as_deref() == Some("region") && !codes.is_empty() && !expected.is_empty() {
        if codes.is_disjoint(&expected) {
            return done(result, "wrong_region");
        }
    } else if let Some(mut radius) = expect.radius_km.filter(|_| expect.level != "country") {
        if rank(level) < rank(&expect.level) {
            radius = radius.max(coarse_km(level).unwrap_or(radius));
        }
        let Some(distance) = result.distance_km else {
            return done(result, "abstain");
        };
        if distance > radius {
            let class = if distance > 100f64.max(2. * radius) {
                "far_miss"
            } else {
                "near_miss"
            };
            return done(result, class);
        }
    }
    if rank(level) < rank(&expect.level) {
        return done(result, "too_coarse");
    }
    if rank(level) > rank(expect.max_level.as_deref().unwrap_or("house")) {
        return done(result, "too_fine");
    }
    for (key, value) in &expect.components {
        let field = match key.as_str() {
            "house_number" => "houseNumber",
            "postcode" => "postcode",
            _ => continue,
        };
        let actual = text(d, field);
        // a ZIP+4 is its ZIP, more precisely
        let plus4 = key == "postcode"
            && actual.split_once('-').is_some_and(|(zip, four)| {
                compact(zip) == compact(value)
                    && four.len() == 4
                    && four.bytes().all(|b| b.is_ascii_digit())
            });
        if !actual.is_empty() && compact(actual) != compact(value) && !plus4 {
            result.flags.push(format!("{key}={actual:?}"));
            return done(result, "wrong_component");
        }
    }
    if !expected.is_empty() && rank(level) >= rank("region") {
        if codes.is_empty() {
            result.flags.push("region_codes_missing".into());
        } else if codes.is_disjoint(&expected) {
            return done(result, "wrong_region");
        }
    }
    if let Some(good) = expect.good_match.filter(|_| confidence)
        && response["goodMatch"].as_bool().unwrap_or(false) != good
    {
        return done(result, "confidence_mismatch");
    }
    done(result, "pass")
}

/// `confidence = false` ignores goodMatch, for raw backend hits that have none.
pub fn score(expect: Option<&Expect>, response: &Value, confidence: bool) -> Score {
    let Some(expect) = expect else {
        return Score {
            class: "unscored",
            level: level_of(response),
            ..Default::default()
        };
    };
    let mut best = expect
        .alternatives()
        .iter()
        .map(|alt| score_one(alt, response, confidence))
        .min_by_key(|score| severity(score.class))
        .unwrap_or_default();
    if critical(best.class) && response["goodMatch"].as_bool().unwrap_or(false) {
        best.flags.push("confident".into());
    }
    best
}

pub fn region_flags(response: &Value) -> Vec<String> {
    let d = details(response);
    let by_code: IndexMap<_, _> = d["regionCodes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| (text(c, "code").to_uppercase(), c["adminLevel"].clone()))
        .collect();
    let regions = d["regions"].as_array().cloned().unwrap_or_default();
    let mut flags = vec![];
    if !by_code.is_empty() && regions.is_empty() {
        flags.push("enrichment_missing".into());
    }
    let code_of = |r: &Value| {
        format!(
            "{}-{}",
            text(r, "countryCode").to_uppercase(),
            text(r, "adminCode")
        )
    };
    let present: BTreeSet<_> = regions.iter().map(code_of).collect();
    for region in &regions {
        let code = code_of(region);
        if let Some(level) = by_code.get(&code)
            && *level != region["adminLevel"]
        {
            flags.push(format!(
                "region_level_mismatch:{code}:{level}!={}",
                region["adminLevel"]
            ));
        }
        let parent = text(region, "parentAdminCode").to_uppercase();
        if !parent.is_empty() && !present.contains(&parent) {
            flags.push(format!("parent_not_returned:{code}->{parent}"));
        }
    }
    flags
}

pub fn summary_of(response: &Value) -> Summary {
    let d = details(response);
    let field = |key| Some(text(d, key).to_owned());
    let mut regions: Vec<_> = d["regions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            format!(
                "{}-{}:{}",
                text(r, "countryCode").to_uppercase(),
                text(r, "adminCode"),
                r["adminLevel"].as_u64().unwrap_or(0)
            )
        })
        .collect();
    regions.sort();
    Summary {
        country: text(d, "countryCode").into(),
        level: level_of(response).into(),
        point: coordinates(response).map(|[lat, lon]| [round(lat, 5), round(lon, 5)]),
        good_match: response["goodMatch"].as_bool().unwrap_or(false),
        display_name: text(response, "displayName").into(),
        region_codes: region_codes(response).into_iter().collect(),
        regions: Some(regions),
        house_number: field("houseNumber"),
        road: field("road"),
        postcode: field("postcode"),
        locality: Some(
            LOCALITY_KEYS
                .iter()
                .map(|key| text(d, key))
                .find(|v| !v.is_empty())
                .unwrap_or("")
                .into(),
        ),
    }
}

pub const DRIFT_KINDS: [&str; 11] = [
    "gained",
    "lost",
    "country",
    "finer",
    "coarser",
    "moved",
    "nudged",
    "good_match",
    "components",
    "regions",
    "error",
];

/// Snapshots store coordinates at 3 decimals (about 100 m).
pub fn snapshot_point(point: [f64; 2]) -> [String; 2] {
    point.map(|v| format!("{v:.3}"))
}

/// Kinds of change between two records; display names alone are ignored.
pub fn drift(
    (a_error, a): (Option<&str>, Option<&Summary>),
    (b_error, b): (Option<&str>, Option<&Summary>),
) -> Vec<&'static str> {
    if a_error != b_error {
        return vec!["error"];
    }
    let empty = Summary::default();
    let (a, b) = (a.unwrap_or(&empty), b.unwrap_or(&empty));
    match (a.point, b.point) {
        (None, Some(_)) => return vec!["gained"],
        (Some(_), None) => return vec!["lost"],
        (None, None) => return vec![],
        _ => {}
    }
    let (pa, pb) = (a.point.unwrap_or_default(), b.point.unwrap_or_default());
    let mut kinds = vec![];
    if a.country != b.country {
        kinds.push("country");
    }
    if rank(&a.level) != rank(&b.level) {
        kinds.push(if rank(&b.level) > rank(&a.level) {
            "finer"
        } else {
            "coarser"
        });
    }
    if haversine(pa, pb) > 1. {
        kinds.push("moved");
    } else if snapshot_point(pa) != snapshot_point(pb) {
        kinds.push("nudged");
    }
    if a.good_match != b.good_match {
        kinds.push("good_match");
    }
    let differs = |x: &Option<String>, y: &Option<String>| x.is_some() && y.is_some() && x != y;
    if differs(&a.house_number, &b.house_number)
        || differs(&a.road, &b.road)
        || differs(&a.postcode, &b.postcode)
        || differs(&a.locality, &b.locality)
    {
        kinds.push("components");
    }
    if a.region_codes != b.region_codes
        || (a.regions.is_some() && b.regions.is_some() && a.regions != b.regions)
    {
        kinds.push("regions");
    }
    kinds
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full(cc: &str, lat: f64, lon: f64, good: bool, fields: Value) -> Value {
        let mut d = json!({ "countryCode": cc });
        d.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let name = d
            .as_object_mut()
            .unwrap()
            .remove("displayName")
            .unwrap_or(json!("x"));
        json!({ "displayName": name, "geoCoordinates": { "lat": lat, "lon": lon },
                "addressDetails": d, "goodMatch": good })
    }

    fn expect(value: Value) -> Expect {
        serde_json::from_value(value).unwrap()
    }

    fn class(e: &Expect, r: &Value) -> &'static str {
        score(Some(e), r, true).class
    }

    #[test]
    fn scoring_classes() {
        let kyoto = expect(
            json!({"country": ["jp"], "level": "locality", "max_level": "house",
            "point": [35.0116, 135.7681], "radius_km": 15, "region": ["JP-26"], "good_match": true}),
        );
        let tokyo = full(
            "jp",
            35.677,
            139.764,
            true,
            json!({"city": "Tokyo", "regionCodes": [{"code": "JP-13"}]}),
        );
        let scored = score(Some(&kyoto), &tokyo, true);
        assert_eq!(scored.class, "far_miss");
        assert!(scored.flags.contains(&"confident".into()));
        let kyoto_ok = json!({"city": "Kyoto", "regionCodes": [{"code": "JP-26"}]});
        assert_eq!(
            class(&kyoto, &full("jp", 35.0, 135.76, true, kyoto_ok)),
            "pass"
        );
        assert_eq!(
            class(
                &kyoto,
                &full("jp", 35.0, 135.76, false, json!({"city": "Kyoto"}))
            ),
            "confidence_mismatch"
        );
        assert_eq!(
            score(
                Some(&kyoto),
                &full("jp", 35.0, 135.76, false, json!({"city": "Kyoto"})),
                false
            )
            .class,
            "pass"
        );
        let wrong_prefecture = json!({"city": "Kyoto", "regionCodes": [{"code": "JP-27"}]});
        assert_eq!(
            class(&kyoto, &full("jp", 35.0, 135.76, true, wrong_prefecture)),
            "wrong_region"
        );
        assert!(
            score(
                Some(&kyoto),
                &full("jp", 35.0, 135.76, true, json!({"city": "Kyoto"})),
                true
            )
            .flags
            .contains(&"region_codes_missing".into())
        );
        assert_eq!(
            class(
                &kyoto,
                &full("jp", 35.2, 135.8, true, json!({"city": "Kyoto"}))
            ),
            "near_miss"
        );
        assert_eq!(class(&kyoto, &json!({})), "abstain");
        let mut brussels = kyoto.clone();
        brussels.country = vec!["be".into()];
        assert_eq!(
            class(
                &brussels,
                &full("ca", 49.487, -98.919, true, json!({"city": "Bruxelles"}))
            ),
            "wrong_country"
        );

        let house = expect(
            json!({"country": ["us"], "level": "house", "max_level": "house",
            "point": [40.7479, -73.9851], "radius_km": 0.3,
            "components": {"house_number": "350", "postcode": "10118"}, "good_match": true}),
        );
        let fallback = full(
            "us",
            40.748,
            -73.986,
            false,
            json!({"postcode": "10118", "displayName": "10118, Manhattan", "suburb": "Manhattan"}),
        );
        assert_eq!(level_of(&fallback), "postcode");
        assert_eq!(class(&house, &fallback), "too_coarse");
        assert_eq!(
            class(
                &house,
                &full("us", 40.75, -73.99, false, json!({"road": "5th Avenue"}))
            ),
            "too_coarse"
        );
        assert_eq!(
            class(
                &house,
                &full("us", 40.80, -73.99, false, json!({"road": "5th Avenue"}))
            ),
            "near_miss"
        );
        let wrong_house = json!({"houseNumber": "352", "road": "5th Avenue", "postcode": "10118"});
        assert_eq!(
            class(&house, &full("us", 40.7479, -73.9851, true, wrong_house)),
            "wrong_component"
        );
        let right_house = json!({"houseNumber": "350", "road": "5th Avenue", "postcode": "10118"});
        assert_eq!(
            class(&house, &full("us", 40.7479, -73.9851, true, right_house)),
            "pass"
        );
        let zip4 = json!({"houseNumber": "350", "road": "5th Avenue", "postcode": "10118-0110"});
        assert_eq!(
            class(&house, &full("us", 40.7479, -73.9851, true, zip4)),
            "pass"
        );
        let other = json!({"houseNumber": "350", "road": "5th Avenue", "postcode": "10119-0110"});
        assert_eq!(
            class(&house, &full("us", 40.7479, -73.9851, true, other)),
            "wrong_component"
        );

        let mut capped = kyoto.clone();
        capped.max_level = Some("postcode".into());
        let house_in_kyoto = json!({"houseNumber": "1", "road": "x", "city": "Kyoto"});
        assert_eq!(
            class(&capped, &full("jp", 35.0, 135.76, true, house_in_kyoto)),
            "too_fine"
        );
        let none = expect(json!({"level": "none"}));
        assert_eq!(class(&none, &json!({})), "pass");
        assert_eq!(class(&none, &tokyo), "unexpected_result");
        let mut alternative = kyoto.clone();
        alternative.point = Some([35.677, 139.764]);
        alternative.region = vec!["JP-13".into()];
        let either = Expect {
            any_of: Some(vec![kyoto.clone(), alternative]),
            ..Default::default()
        };
        assert_eq!(class(&either, &tokyo), "pass");

        let region = expect(
            json!({"country": ["de"], "level": "region", "max_level": "region", "area": "region",
            "point": [49.0, 11.4], "radius_km": 400, "region": ["DE-BY"], "good_match": true}),
        );
        let bavaria = json!({"state": "Bavaria", "regionCodes": [{"code": "DE-BY"}]});
        assert_eq!(
            class(&region, &full("de", 48.9, 11.4, true, bavaria)),
            "pass"
        );
        let hesse = json!({"state": "Hesse", "regionCodes": [{"code": "DE-HE"}]});
        assert_eq!(
            class(&region, &full("de", 48.9, 11.4, true, hesse)),
            "wrong_region"
        );
        let city_state = expect(
            json!({"country": ["hu"], "level": "region", "max_level": "house",
            "point": [47.4979, 19.0402], "radius_km": 15, "region": ["HU-BU"], "good_match": true}),
        );
        for budapest in [
            json!({"state": "Budapest", "regionCodes": [{"code": "HU-BU"}]}),
            json!({"region": "Budapest", "regionCodes": [{"code": "HU-BU"}]}),
        ] {
            assert_eq!(
                class(&city_state, &full("hu", 47.4979, 19.0402, true, budapest)),
                "pass"
            );
        }
        let pest = json!({"state": "Pest", "regionCodes": [{"code": "HU-PE"}]});
        assert_eq!(
            class(&city_state, &full("hu", 47.2, 19.4, true, pest)),
            "near_miss"
        );
        assert_eq!(score(None, &tokyo, true).class, "unscored");
    }

    #[test]
    fn raw_hits_and_region_flags() {
        let raw = from_index(
            &json!({"display_name": "Tokyo", "lat": "35.6", "lon": "139.7",
            "address": {"city": "Tokyo", "country_code": "jp", "house_number": "1", "ISO3166-2-lvl4": "JP-13"}}),
        );
        assert_eq!(text(details(&raw), "houseNumber"), "1");
        assert_eq!(region_codes(&raw), BTreeSet::from(["JP-13".to_string()]));
        let regions = json!({"addressDetails": {"regionCodes": [{"code": "DK-84", "adminLevel": 4}],
            "regions": [{"countryCode": "dk", "adminCode": "84", "adminLevel": 6, "parentAdminCode": "DK-1"}]}});
        assert_eq!(
            region_flags(&regions),
            [
                "region_level_mismatch:DK-84:4!=6",
                "parent_not_returned:DK-84->DK-1"
            ]
        );
    }

    #[test]
    fn drift_kinds() {
        let at = |lat| Summary {
            country: "fr".into(),
            level: "locality".into(),
            point: Some([1., lat]),
            ..Default::default()
        };
        assert_eq!(
            drift((None, Some(&at(1.))), (None, Some(&at(1.02)))),
            ["moved"]
        );
        assert_eq!(
            drift((None, Some(&at(1.))), (None, Some(&at(1.002)))),
            ["nudged"]
        );
        assert_eq!(
            drift((None, Some(&at(1.))), (None, Some(&at(1.0001)))),
            Vec::<&str>::new()
        );
        assert_eq!(drift((None, None), (None, Some(&at(1.)))), ["gained"]);
        assert_eq!(round(51.156_318_5, 6), 51.156_318);
        assert_eq!(
            drift((None, Some(&at(1.))), (Some("timeout"), None)),
            ["error"]
        );
        let mut finer = at(1.);
        finer.level = "house".into();
        finer.locality = Some("Paris".into());
        assert_eq!(
            drift((None, Some(&at(1.))), (None, Some(&finer))),
            ["finer"]
        );
    }
}
