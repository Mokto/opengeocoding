use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};

use crate::client::OpenGeocodingApiClient;

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
    country_code: String,
) -> Result<(), Box<dyn Error>> {
    let page_size = 20000;
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
                    country_code
                );
            })
            .collect::<Vec<String>>();
        let query = format!("REPLACE INTO {}(id,street,number,unit,city,district,region,postcode,lat,long,country_code) VALUES {};", full_table_name, values.join(","));

        client.run_background_query(query).await?;
    }

    Ok(())
}

fn clean_string(s: &str) -> String {
    return s.replace(r"\", r"\\").replace("'", r"\'");
}
