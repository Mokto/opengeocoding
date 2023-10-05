use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::error::Error;

use crate::{client::OpenGeocodingApiClient, data::helper::clean_string};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StreetPoint {
    pub lat: f64,
    pub long: f64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StreetDocument {
    pub id: String,
    pub street: String,
    pub points: Vec<StreetPoint>,
    pub country_code: Option<String>,
    pub region: String,
    pub city: String,
    pub lat: f64,
    pub long: f64,
}
