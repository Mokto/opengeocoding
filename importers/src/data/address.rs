use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::time::Duration;
use tokio::time::sleep;

use crate::client::OpenGeocodingApiClient;

use super::helper::clean_string;

#[derive(Serialize, Deserialize, Debug)]
pub struct AddressDocument {
    pub id: String,
    pub street: String,
    pub number: String,
    pub unit: String,
    pub city: String,
    pub district: String,
    pub region: String,
    pub postcode: String,
    pub country_code: Option<String>,
    pub lat: f64,
    pub long: f64,
}
