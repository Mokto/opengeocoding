mod act;
mod anncsu;
mod bag;
mod ban;
mod berlin;
mod best;
mod bogota;
mod cartociudad;
mod cnefe;
mod dar;
mod gnaf;
mod kartverket;
mod korea;
mod linz;
mod nad;
mod nar;
mod onemap;
mod prg;
mod rgz;
mod ruian;
mod swisstopo;
mod taipei;

use anyhow::Result;
use indexmap::IndexMap;
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub const ALL: &[Registry] = &[
    nad::REGISTRY,
    nar::REGISTRY,
    prg::REGISTRY,
    ban::REGISTRY,
    cartociudad::REGISTRY,
    dar::REGISTRY,
    gnaf::REGISTRY,
    kartverket::REGISTRY,
    korea::REGISTRY,
    ruian::REGISTRY,
    bag::REGISTRY,
    swisstopo::REGISTRY,
    best::REGISTRY,
    linz::REGISTRY,
    taipei::REGISTRY,
    bogota::REGISTRY,
    berlin::REGISTRY,
    act::REGISTRY,
    anncsu::REGISTRY,
    cnefe::REGISTRY,
    onemap::REGISTRY,
    rgz::REGISTRY,
];

type Prepare = fn(&Path, &BTreeSet<String>) -> Result<BTreeMap<String, Entry>>;

pub struct Registry {
    pub source: &'static str,
    pub command: &'static str,
    pub country: &'static str,
    pub input: &'static str,
    pub license: &'static str,
    pub location: &'static str,
    pub url: fn(&str) -> String,
    pub sample: fn(&Path, &str) -> Result<Vec<Value>>,
    pub prepare: Prepare,
}

pub enum Point {
    Wgs84([f64; 2]),
    Lv95([f64; 2]),
}

pub struct Entry {
    pub point: Point,
    pub components: IndexMap<&'static str, String>,
    pub radius_floor_km: f64,
    pub provenance: Map<String, Value>,
    pub number_aliases: Option<BTreeSet<String>>,
    /// The source's names for the town or municipality the address is in.
    pub places: Vec<String>,
}

/// Distinct non-empty names, in order, for `Entry::places`.
pub fn places<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for name in names.into_iter().map(str::trim).filter(|n| !n.is_empty()) {
        if !out.iter().any(|o| o == name) {
            out.push(name.into());
        }
    }
    out
}

pub fn get(source: &str) -> Option<&'static Registry> {
    ALL.iter().find(|r| r.source == source)
}

pub fn provenance(path: &Path) -> Result<Map<String, Value>> {
    Ok(Map::from_iter([
        (
            "release".into(),
            json!(path.file_name().and_then(|s| s.to_str())),
        ),
        ("sha256".into(), json!(crate::run::sha256_file(path)?)),
    ]))
}

pub fn with_number_aliases(expect: Value, numbers: &Option<BTreeSet<String>>) -> Value {
    match numbers {
        None => expect,
        Some(numbers) => json!({"any_of": numbers.iter().map(|number| {
            let mut alternative = expect.clone();
            alternative["components"]["house_number"] = json!(number);
            alternative
        }).collect::<Vec<_>>()}),
    }
}

pub use swisstopo::{REFRAME, reframed_point};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_is_unique_and_every_registry_has_docs_and_notice() {
        let mut sources = BTreeSet::new();
        let mut commands = BTreeSet::new();
        let mut inputs = BTreeSet::new();
        for registry in ALL {
            assert!(sources.insert(registry.source));
            assert!(commands.insert(registry.command));
            assert!(inputs.insert(registry.input));
            assert!(!registry.license.is_empty());
            assert!((registry.url)("123").starts_with("https://"));
            assert!(
                crate::model::root()
                    .join(format!("docs/evaluation/registry/{}.md", registry.country))
                    .is_file()
            );
            assert!(
                crate::model::root()
                    .join(format!("tests/registry/{}.NOTICE", registry.country))
                    .is_file()
            );
        }
        for path in crate::model::registry_corpora() {
            let mut cases = crate::model::load_corpus(&path).unwrap();
            for case in &cases {
                let source = case.source.as_ref().unwrap();
                let registry = get(source["name"].as_str().unwrap()).unwrap();
                assert_eq!(source["license"], registry.license);
            }
            cases[0].source.as_mut().unwrap()["license"] = json!("OGL-UK-3.0");
            assert!(crate::model::validate_gold(&cases).is_err());
        }
    }
}
