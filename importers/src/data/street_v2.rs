use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::error::Error;

use crate::{
    client::OpenGeocodingApiClient,
    data::helper::{bool_to_sql, clean_string},
};

#[derive(Serialize, Deserialize, Debug)]
pub struct StreetDocumentV2 {
    pub id: String,
    pub street: String,
    pub country_code: Option<String>,
    pub region: String,
    pub postal_code: String,
    pub city: String,
    pub lat_min: f64,
    pub long_min: f64,
    pub lat_max: f64,
    pub long_max: f64,
    pub house_min: String,
    pub house_max: String,
    pub house_odd: bool,
    pub house_even: bool,
}
