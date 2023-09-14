use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use tokio::time::sleep;

use crate::client::OpenGeocodingApiClient;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StreetPoint {
    pub lat: f64,
    pub long: f64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StreetDocument {
    pub id: u64,
    pub street: String,
    pub points: Vec<StreetPoint>,
    pub country_code: Option<String>,
    pub region: String,
    pub city: String,
    pub lat: f64,
    pub long: f64,
}

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

#[derive(Serialize, Deserialize, Debug)]
pub struct AddressDocument {
    pub id: u64,
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

pub fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

pub async fn insert_address_documents(
    client: &mut OpenGeocodingApiClient,
    full_table_name: String,
    documents: Vec<AddressDocument>,
    country_code: Option<String>,
) -> Result<(), Box<dyn Error>> {
    let page_size = 10000;
    for (index, chunk) in documents.chunks(page_size).enumerate() {
        if index != 0 && index * page_size % 100000 == 0 {
            println!("Done with {} documents", index * page_size);
        }
        let values = chunk
            .par_iter()
            .map(|doc| {
                return format!(
                    r"({},'{}','{}','{}','{}','{}','{}','{}',{},{}, '{}')",
                    doc.id,
                    clean_string(&doc.street),
                    clean_string(&doc.number),
                    clean_string(&doc.unit),
                    clean_string(&doc.city),
                    clean_string(&doc.district),
                    clean_string(&doc.region),
                    clean_string(&doc.postcode),
                    doc.lat,
                    doc.long,
                    doc.country_code
                        .clone()
                        .unwrap_or(country_code.clone().unwrap_or("".to_string()))
                );
            })
            .collect::<Vec<String>>();
        let query = format!("REPLACE INTO {}(id,street,number,unit,city,district,region,postcode,lat,long,country_code) VALUES {};", full_table_name, values.join(","));

        let result = client.run_background_query(query.clone()).await;
        if result.is_err() {
            println!("Error: {:?}. Retrying...", result);
            sleep(Duration::from_millis(2000)).await;
            client.run_background_query(query).await?;
        }
    }

    Ok(())
}

pub async fn insert_street_documents(
    client: &mut OpenGeocodingApiClient,
    full_table_name: String,
    documents: Vec<StreetDocument>,
    country_code: Option<String>,
) -> Result<(), Box<dyn Error>> {
    let page_size = 10000;
    for (index, chunk) in documents.chunks(page_size).enumerate() {
        if index != 0 && index * page_size % 100000 == 0 {
            println!("Done with {} documents", index * page_size);
        }
        let values = chunk
            .par_iter()
            .map(|doc| {
                let points = doc
                    .points
                    .iter()
                    .map(|p| format!("{},{}", p.lat, p.long))
                    .collect::<Vec<String>>()
                    .join("],[");

                format!(
                    r"({},'{}','{}',{},{},'{}',{}, '{}')",
                    doc.id,
                    clean_string(&doc.street),
                    doc.country_code
                        .clone()
                        .unwrap_or(country_code.clone().unwrap_or("".to_string())),
                    doc.lat,
                    doc.long,
                    clean_string(&doc.region),
                    "'[[".to_string() + points.as_str() + "]]'",
                    clean_string(&doc.city),
                )
            })
            .collect::<Vec<String>>();
        let query = format!(
            "REPLACE INTO {}(id,street,country_code,lat,long,region,points,city) VALUES {};",
            full_table_name,
            values.join(",")
        );

        client.run_background_query(query).await?;
    }

    Ok(())
}

fn clean_string(s: &str) -> String {
    return s.replace(r"\", r"\\").replace("'", r"\'");
}
