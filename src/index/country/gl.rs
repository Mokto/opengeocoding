use crate::index::{Hit, Index, Suggestion, fold};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

pub const FILE: &str = "gl-buildings.json";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Building {
    pub locality: String,
    pub town: String,
    pub number: String,
    pub point: [f64; 2],
    pub place: u32,
}

pub(crate) fn load(path: &Path, places: usize) -> Result<BTreeMap<String, Building>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let rows: BTreeMap<String, Building> =
        serde_json::from_slice(&std::fs::read(path)?).context("Greenland building references")?;
    for (key, b) in &rows {
        ensure!(
            *key == format!("{}:{}", fold(&b.town), b.number.to_ascii_lowercase())
                && canonical(&b.number, false)
                && !fold(&b.town).is_empty()
                && !b.town.chars().any(|c| c.is_control() || c.is_ascii_digit())
                && b.locality.len() == 7
                && b.locality.bytes().all(|c| c.is_ascii_alphanumeric())
                && b.place < places as u32
                && (59. ..84.).contains(&b.point[0])
                && (-74. ..-10.).contains(&b.point[1]),
            "invalid Greenland building reference"
        );
    }
    Ok(rows)
}

fn canonical(number: &str, prefix: bool) -> bool {
    let Some(digits) = number
        .strip_prefix("B-")
        .or_else(|| number.strip_prefix("b-"))
    else {
        return false;
    };
    (prefix || !digits.is_empty())
        && digits.len() <= 5
        && !digits.starts_with('0')
        && digits.bytes().all(|b| b.is_ascii_digit())
}

fn query(index: &Index, text: &str, cc: Option<&str>, complete: bool) -> Option<(String, bool)> {
    if text.chars().count() > 200 || cc.is_some_and(|c| !c.eq_ignore_ascii_case("gl")) {
        return None;
    }
    let parts: Vec<_> = text.split(',').map(str::trim).collect();
    let pair = match parts.as_slice() {
        [a, b, country] if index.country(country).as_deref() == Some("gl") => [*a, *b],
        [a, b] if cc.is_some_and(|c| c.eq_ignore_ascii_case("gl")) => [*a, *b],
        _ => return None,
    };
    let number_at = pair
        .iter()
        .position(|p| p.starts_with("B-") || p.starts_with("b-"))?;
    let (number, town) = (pair[number_at], pair[1 - number_at]);
    let open = !complete
        && number_at + 1 == parts.len()
        && !text.ends_with(|c: char| c == ',' || c.is_whitespace());
    if !canonical(number, open)
        || town.is_empty()
        || town.chars().any(|c| c.is_ascii_digit() || c.is_control())
    {
        return None;
    }
    let town = format!("{}:", fold(town));
    if index
        .buildings
        .range(town.clone()..)
        .next()
        .is_none_or(|(key, _)| !key.starts_with(&town))
    {
        return None;
    }
    Some((format!("{town}{}", number.to_ascii_lowercase()), open))
}

pub(crate) fn address(index: &Index, text: &str) -> Option<Value> {
    let (key, _) = query(index, text, None, true)?;
    Some(index.buildings.get(&key).map_or(Value::Null, |b| {
        let mut value = index.render(&Hit::Place(b.place), true);
        value["lat"] = json!(b.point[0]);
        value["lon"] = json!(b.point[1]);
        value["boundingbox"] = Value::Null;
        value["display_name"] = json!(format!("{}, {}, Greenland", b.number, b.town));
        let details = value["address"].as_object_mut().unwrap();
        for field in ["road", "house_number", "town", "village", "hamlet"] {
            details.remove(field);
        }
        details.insert("house_name".into(), json!(b.number));
        details.insert("city".into(), json!(b.town));
        details.insert("country".into(), json!("Greenland"));
        details.insert("country_code".into(), json!("gl"));
        value
    }))
}

fn suggestion(index: &Index, key: &str, b: &Building) -> Suggestion {
    Suggestion {
        id: format!("b{key}"),
        retrieval_id: format!("{}/b{key}", index.suggestion_scope),
        display_name: format!("{}, {}, Greenland", b.number, b.town),
        unverified_suffix: String::new(),
        kind: "building",
        category: "",
        country_code: "gl".into(),
        x: b.point[1],
        y: b.point[0],
        bbox: None,
    }
}

pub(crate) fn suggest(
    index: &Index,
    text: &str,
    cc: Option<&str>,
    want: usize,
) -> Option<Vec<Suggestion>> {
    let (key, open) = query(index, text, cc, false)?;
    Some(
        index
            .buildings
            .range(key.clone()..)
            .take_while(|(k, _)| *k == &key || open && k.starts_with(&key))
            .take(want)
            .map(|(k, b)| suggestion(index, k, b))
            .collect(),
    )
}

pub(crate) fn retrieve(index: &Index, key: &str) -> Option<Suggestion> {
    index.buildings.get(key).map(|b| suggestion(index, key, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn building_numbers_do_not_become_plain_house_numbers() {
        for number in ["B-1", "b-99999"] {
            assert!(canonical(number, false));
        }
        for number in [
            "1", "B1", "B-0", "B-01", "B-123456", "B-1A", "B-1;B-2", "B-١", "B-",
        ] {
            assert!(!canonical(number, false), "{number}");
        }
        assert!(canonical("B-", true));
    }
}
