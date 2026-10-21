use crate::index::*;
use regex::Regex;
use std::sync::LazyLock;

/// ISO 3166-2:IE's four provinces, above the counties that are Overture's Irish regions: code,
/// English and Irish name, and counties.
pub(crate) const IRISH_PROVINCES: [(&str, &str, &str, &[&str]); 4] = [
    (
        "IE-C",
        "Connacht",
        "Connachta",
        &["G", "LM", "MO", "RN", "SO"],
    ),
    (
        "IE-L",
        "Leinster",
        "Laighin",
        &[
            "CW", "D", "KE", "KK", "LS", "LD", "LH", "MH", "OY", "WH", "WX", "WW",
        ],
    ),
    (
        "IE-M",
        "Munster",
        "An Mhumhain",
        &["CE", "CO", "KY", "LK", "TA", "WD"],
    ),
    ("IE-U", "Ulster", "Ulaidh", &["CN", "DL", "MN"]),
];

pub(crate) fn irish_province(county: &str) -> Option<&'static str> {
    let county = county.strip_prefix("IE-")?;
    let province = IRISH_PROVINCES.iter().find(|p| p.3.contains(&county))?;
    Some(province.0)
}

/// A place for each Irish province whose counties the index has, by code: Overture has no
/// provinces. It sits at the mean of its counties' points and is in no other place's chain, so
/// it changes no search; it gives the province a region row and answers `ie, <code>`.
pub(in crate::index) fn irish_provinces(
    places: &[Place],
    regions: &HashMap<String, u32>,
) -> Vec<(String, Place)> {
    IRISH_PROVINCES
        .iter()
        .filter_map(|&(code, en, ga, counties)| {
            let members: Vec<&Place> = counties
                .iter()
                .filter_map(|c| regions.get(&format!("IE-{c}")))
                .map(|&p| &places[p as usize])
                .collect();
            let first = members.first()?;
            let n = members.len() as f32;
            let bbox = members
                .iter()
                .fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, p| {
                    [
                        b[0].min(p.bbox[0]),
                        b[1].min(p.bbox[1]),
                        b[2].max(p.bbox[2]),
                        b[3].max(p.bbox[3]),
                    ]
                });
            let names: Box<str> = format!("en={en}{SEP}ga={ga}").into();
            let mut keys = vec![name_key(en), name_key(ga)];
            keys.sort_unstable();
            Some((
                code.to_string(),
                Place {
                    subtype: "macroregion".into(),
                    class: "".into(),
                    cc: first.cc.clone(),
                    region: "".into(),
                    population: members.iter().map(|p| p.population).sum(),
                    x: members.iter().map(|p| p.x).sum::<f32>() / n,
                    y: members.iter().map(|p| p.y).sum::<f32>() / n,
                    bbox,
                    parent: first.parent,
                    primary: en.into(),
                    en: Place::english(&names),
                    names,
                    keys: keys.into(),
                    city_like: false,
                    settlement_twin: false,
                },
            ))
        })
        .collect()
}

pub fn eircode(query: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)[ac-fhknprtv-y]\d{2}[\s\x1c-\x1f]?[0-9ac-fhknprtv-y]{4}").unwrap()
    });
    let alphanum = |ch: char| ch.is_ascii_alphanumeric() || "İıſK".contains(ch);
    RE.find_iter(query)
        .find(|hit| {
            !query[..hit.start()]
                .chars()
                .next_back()
                .is_some_and(alphanum)
                && !query[hit.end()..].chars().next().is_some_and(alphanum)
        })
        .map(|hit| hit.as_str().to_owned())
        .unwrap_or_default()
}
