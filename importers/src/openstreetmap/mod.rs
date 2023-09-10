use std::fs;

use crate::{
    client::OpenGeocodingApiClient,
    config::Config,
    data::{calculate_hash, insert_address_documents, AddressDocument},
    wof::RegionDetector,
};
use osmpbfreader::OsmPbfReader;
use rayon::prelude::*;

#[derive(Hash)]
pub struct OsmProperties {
    pub street: String,
    pub number: String,
    pub city: String,
    pub postcode: String,
}

pub struct OsmData {
    pub lat: f64,
    pub long: f64,
    pub properties: OsmProperties,
}

pub async fn extract_all() {
    let config = Config::new();
    let table_name = "osm_addresses";
    let full_table_name = config.get_table_name(table_name.to_string());

    let region_detector = RegionDetector::new();
    let mut client = OpenGeocodingApiClient::new().await.unwrap();

    println!("Creating table...");
    let query_result = client.run_query(format!("CREATE TABLE IF NOT EXISTS {}(street text, number text, unit text, city text, district text, region text, postcode text, lat float, long float, country_code string)  rt_mem_limit = '1G'", full_table_name)).await;
    match query_result {
        Ok(_) => {}
        Err(e) => {
            panic!("{}", e);
        }
    };

    let time = std::time::Instant::now();

    let country = "denmark";
    let country_code = "dk";

    println!("Loading data for {}...", country);

    let path = format!("data/{}-latest.osm.pbf", country);
    let file = fs::File::open(std::path::Path::new(&path)).unwrap();
    let mut pbf = OsmPbfReader::new(file);

    let mut objects = vec![];

    for obj in pbf.par_iter() {
        let obj: osmpbfreader::OsmObj = obj.unwrap();
        if obj.is_node() {
            let tags = obj.tags();

            if tags.contains_key("addr:street")
                && tags.contains_key("addr:housenumber")
                && tags.contains_key("addr:postcode")
                && tags.contains_key("addr:city")
            {
                let house = tags.get("addr:housenumber").unwrap();
                let street = tags.get("addr:street").unwrap();
                let postcode = tags.get("addr:postcode").unwrap();
                let city = tags.get("addr:city").unwrap();

                let node = obj.node().unwrap();

                objects.push(OsmData {
                    lat: node.lat(),
                    long: node.lon(),
                    properties: OsmProperties {
                        street: street.to_string(),
                        number: house.to_string(),
                        city: city.to_string(),
                        postcode: postcode.to_string(),
                    },
                });
            }
        }
    }

    println!(
        "Done loading data for {}. Found {} elements to compute...",
        country,
        objects.len()
    );
    let documents = objects
        .par_iter()
        .map(|obj| AddressDocument {
            id: calculate_hash(&obj.properties),
            street: obj.properties.street.to_string(),
            number: obj.properties.number.to_string(),
            unit: "".to_string(),
            city: obj.properties.city.to_string(),
            district: "".to_string(),
            region: region_detector
                .detect(country_code.to_string(), obj.lat, obj.long)
                .unwrap_or("".to_string()),
            postcode: obj.properties.postcode.to_string(),
            lat: obj.lat,
            long: obj.long,
        })
        .collect::<Vec<_>>();

    insert_address_documents(
        &mut client,
        full_table_name,
        documents,
        country_code.to_string(),
    )
    .await
    .unwrap();

    println!("Done in {:?}s", time.elapsed().as_secs());
}
