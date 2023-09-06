use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Serialize, Deserialize)]
pub struct CityDocument {
    pub id: u64,
    pub city: String,
    pub region: String,
    pub country_code: String,
    // pub postcode: String,
    pub lat: f64,
    pub long: f64,
    pub population: u32,
}

#[derive(Serialize, Deserialize)]
pub struct AddressDocument {
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

pub fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}
