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
    pub id: u64,
    pub street: String,
    pub points: Vec<StreetPoint>,
    pub country_code: Option<String>,
    pub region: String,
    pub city: String,
    pub lat: f64,
    pub long: f64,
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
