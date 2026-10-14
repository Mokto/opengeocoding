use crate::{
    proto::opengeocoding::{ISO3166_2, Region},
    resolver::Resolver,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

/// The administrative regions of the index, by ISO 3166-2 code, and the map of them
/// `GetRegionMap` serves. Both are derived from the index when the service starts.
pub struct Regions {
    by_code: HashMap<String, Region>,
    list: Vec<Region>,
    map: String,
}

impl Regions {
    pub fn from_resolver(resolver: &Resolver) -> Result<Self> {
        Self::from_rows(resolver.regions()?)
    }

    /// `rows` are each ISO 3166-2 code with its region.
    pub fn from_rows(mut rows: Vec<(String, Region)>) -> Result<Self> {
        rows.sort_by(|(_, a), (_, b)| {
            (&a.country_code, &a.admin_code).cmp(&(&b.country_code, &b.admin_code))
        });
        let list: Vec<Region> = rows.iter().map(|(_, region)| region.clone()).collect();
        Ok(Self {
            map: python_repr(&format_regions(&list)?)?,
            by_code: rows.into_iter().collect(),
            list,
        })
    }

    pub fn len(&self) -> usize {
        self.by_code.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_code.is_empty()
    }

    /// The regions of an address's ISO 3166-2 codes, lowest level first. A code the index has no
    /// region for is reported and left out.
    pub fn reformat(&self, codes: &[ISO3166_2]) -> Vec<Region> {
        let mut sorted: Vec<_> = codes.iter().collect();
        sorted.sort_by_key(|c| c.admin_level);
        let mut result = vec![];
        for code in sorted {
            if let Some(region) = self.by_code.get(&code.code.to_uppercase()) {
                result.push(region.clone());
            } else {
                tracing::warn!(
                    code = code.code,
                    admin_level = code.admin_level,
                    "No region found in the index"
                );
                sentry::capture_message("No region found in the index", sentry::Level::Warning);
            }
        }
        result
    }

    /// Every region, ordered by country then admin code.
    pub fn list(&self) -> &[Region] {
        &self.list
    }

    pub fn map(&self) -> &str {
        &self.map
    }
}

pub fn format_regions(regions: &[Region]) -> Result<Value> {
    let mut children: HashMap<String, Vec<&Region>> = HashMap::new();
    for region in regions
        .iter()
        .filter(|region| !region.parent_admin_code.is_empty())
    {
        children
            .entry(region.parent_admin_code.clone())
            .or_default()
            .push(region);
    }
    fn tree(
        region: &Region,
        children: &HashMap<String, Vec<&Region>>,
        visited: &mut HashSet<String>,
    ) -> Result<Value> {
        let key = format!(
            "{}-{}",
            region.country_code.to_uppercase(),
            region.admin_code
        );
        ensure!(
            visited.insert(key.clone()),
            "cycle in administrative regions"
        );
        let mut value =
            json!({"region_local_name": region.local_name, "region_english_name": region.en_name});
        if let Some(nodes) = children.get(&key) {
            for child in nodes {
                let child_tree = tree(child, children, visited)?;
                let sub = value
                    .as_object_mut()
                    .context("region object")?
                    .entry("sub_regions")
                    .or_insert_with(|| json!({}));
                let level = sub
                    .as_object_mut()
                    .context("subregion object")?
                    .entry(format!("lvl-{}", child.admin_level))
                    .or_insert_with(|| json!({}));
                level
                    .as_object_mut()
                    .context("level object")?
                    .insert(child.admin_code.clone(), child_tree);
            }
        }
        visited.remove(&key);
        Ok(value)
    }
    let mut result = json!({});
    for root in regions.iter().filter(|r| r.parent_admin_code.is_empty()) {
        let country = result
            .as_object_mut()
            .context("country map")?
            .entry(root.country_code.clone())
            .or_insert_with(|| json!({"country_name": root.country_name}));
        country.as_object_mut().context("country object")?.insert(
            root.admin_code.clone(),
            tree(root, &children, &mut HashSet::new())?,
        );
    }
    Ok(result)
}

pub fn python_repr(value: &Value) -> Result<String> {
    match value {
        Value::String(value) => {
            let quote = if value.contains('\'') && !value.contains('"') {
                '"'
            } else {
                '\''
            };
            let mut result = quote.to_string();
            for ch in value.chars() {
                match ch {
                    '\\' => result.push_str("\\\\"),
                    '\n' => result.push_str("\\n"),
                    '\r' => result.push_str("\\r"),
                    '\t' => result.push_str("\\t"),
                    ch if ch == quote => {
                        result.push('\\');
                        result.push(ch);
                    }
                    ch if !crate::matching::printable(ch) => {
                        let code = ch as u32;
                        if code < 256 {
                            result.push_str(&format!("\\x{code:02x}"));
                        } else if code <= 0xffff {
                            result.push_str(&format!("\\u{code:04x}"));
                        } else {
                            result.push_str(&format!("\\U{code:08x}"));
                        }
                    }
                    ch => result.push(ch),
                }
            }
            result.push(quote);
            Ok(result)
        }
        Value::Object(map) => Ok(format!(
            "{{{}}}",
            map.iter()
                .map(|(key, value)| {
                    Ok(format!(
                        "{}: {}",
                        python_repr(&Value::String(key.clone()))?,
                        python_repr(value)?
                    ))
                })
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        )),
        Value::Array(values) => Ok(format!(
            "[{}]",
            values
                .iter()
                .map(python_repr)
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        )),
        Value::Bool(value) => Ok(if *value { "True" } else { "False" }.into()),
        Value::Null => Ok("None".into()),
        Value::Number(value) => Ok(value.to_string()),
    }
}
