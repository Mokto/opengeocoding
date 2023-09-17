use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::error::Error;

use crate::{
    client::OpenGeocodingApiClient,
    data::helper::{bool_to_sql, clean_string},
};

#[derive(Serialize, Deserialize, Debug)]
pub struct StreetDocumentV2 {
    pub id: u64,
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

pub async fn insert_street_documents_v2(
    client: &mut OpenGeocodingApiClient,
    full_table_name: String,
    documents: Vec<StreetDocumentV2>,
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
                format!(
                    r"({},'{}','{}','{}','{}',{},{},{},{}, '{}', '{}', {}, {})",
                    doc.id,
                    clean_string(&doc.street),
                    doc.country_code
                        .clone()
                        .unwrap_or(country_code.clone().unwrap_or("".to_string())),
                    clean_string(&doc.region),
                    clean_string(&doc.city),
                    doc.lat_min,
                    doc.lat_max,
                    doc.long_min,
                    doc.long_max,
                    clean_string(&doc.house_min),
                    clean_string(&doc.house_max),
                    bool_to_sql(doc.house_odd),
                    bool_to_sql(doc.house_even),
                )
            })
            .collect::<Vec<String>>();
        let query = format!(
            "REPLACE INTO {}(id,street,country_code,region,city,lat_min,lat_max,long_min,long_max,house_min,house_max,house_odd,house_even) VALUES {};",
            full_table_name,
            values.join(",")
        );

        client.run_background_query(query).await?;
    }

    Ok(())
}
