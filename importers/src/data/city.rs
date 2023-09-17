use serde::{Deserialize, Serialize};

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
