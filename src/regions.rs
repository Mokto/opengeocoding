use crate::{
    proto::opengeocoding::{ISO3166_2, Region},
    resolver::Resolver,
};
use anyhow::Result;
use std::collections::HashMap;

/// The administrative regions of the index, by ISO 3166-2 code, derived when the service starts.
pub struct Regions {
    by_code: HashMap<String, Region>,
    list: Vec<Region>,
}

impl Regions {
    pub fn from_resolver(resolver: &Resolver) -> Result<Self> {
        Ok(Self::from_rows(resolver.regions()?))
    }

    /// `rows` are each ISO 3166-2 code with its region.
    pub fn from_rows(mut rows: Vec<(String, Region)>) -> Self {
        rows.sort_by(|(_, a), (_, b)| {
            (&a.country_code, &a.admin_code).cmp(&(&b.country_code, &b.admin_code))
        });
        let list: Vec<Region> = rows.iter().map(|(_, region)| region.clone()).collect();
        Self {
            by_code: rows.into_iter().collect(),
            list,
        }
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
}
