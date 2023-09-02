use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Serialize, Deserialize)]
pub struct Document {
    pub id: u64,
    pub street: String,
    pub number: String,
    pub unit: String,
    pub city: String,
    pub district: String,
    pub region: String,
    pub postcode: String,
    pub lat: f64,
    pub long: f64,
}

#[derive(Serialize, Deserialize)]
pub struct GeoPointGeometry {
    pub r#type: String,
    pub coordinates: Vec<f64>,
}
#[derive(Serialize, Deserialize, Hash)]
pub struct GeoPointProperties {
    pub street: Option<String>,
    pub number: Option<String>,
    pub unit: Option<String>,
    pub city: Option<String>,
    pub district: Option<String>,
    pub region: Option<String>,
    pub postcode: Option<String>,
}
#[derive(Serialize, Deserialize)]
pub struct GeoPoint {
    pub r#type: String,
    pub properties: GeoPointProperties,
    pub geometry: Option<GeoPointGeometry>,
}

pub fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}
