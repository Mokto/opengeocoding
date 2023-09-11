use crate::{
    client::OpenGeocodingApiClient,
    config::Config,
    data::{calculate_hash, insert_address_documents, AddressDocument},
    download::download_file,
    openstreetmap::file_list,
    wof::{country_detector::CountryDetector, region_detector::RegionDetector},
};
use osmpbfreader::OsmPbfReader;
use rayon::prelude::*;
use std::{
    fs::{self},
    path::Path,
};

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
    let table_name = "openstreetmap_addresses";
    let full_table_name = config.get_table_name(table_name.to_string());

    let region_detector = RegionDetector::new();
    let country_detector = CountryDetector::new();
    let mut client = OpenGeocodingApiClient::new().await.unwrap();

    println!("Creating table...");
    let query_result = client.run_query(format!("CREATE TABLE IF NOT EXISTS {}(street text, number text, unit text, city text, district text, region text, postcode text, lat float, long float, country_code string)  rt_mem_limit = '1G'", full_table_name)).await;
    match query_result {
        Ok(_) => {}
        Err(e) => {
            panic!("{}", e);
        }
    };

    let paths = file_list::get_osm_country_files();

    for p in paths {
        let time = std::time::Instant::now();

        println!("Loading data for {}...", p);

        let path = std::path::Path::new(p);
        let existing_file = "data/".to_string() + path.file_name().unwrap().to_str().unwrap();
        if !Path::new(&existing_file).is_file() {
            println!("Downloading file...");
            download_file(&p.to_string(), &existing_file).await.unwrap();
            println!("Done. Reading it...");
        }

        let file = fs::File::open(&existing_file).unwrap();
        let mut pbf = OsmPbfReader::new(file);

        let mut objects = vec![];

        for obj in pbf.par_iter() {
            let obj: osmpbfreader::OsmObj = obj.unwrap();
            if obj.is_node() {
                let tags = obj.tags();

                if tags.contains_key("addr:street")
                    && tags.contains_key("addr:city")
                    && tags.contains_key("addr:housenumber")
                {
                    let house = tags.get("addr:housenumber").unwrap();
                    let street = tags.get("addr:street").unwrap();
                    let city = tags.get("addr:city").unwrap();
                    let postcode = tags.get("addr:postcode");

                    let node = obj.node().unwrap();

                    objects.push(OsmData {
                        lat: node.lat(),
                        long: node.lon(),
                        properties: OsmProperties {
                            street: street.to_string(),
                            number: house.to_string(),
                            city: city.to_string(),
                            postcode: match postcode {
                                Some(p) => p.to_string(),
                                None => "".to_string(),
                            },
                        },
                    });
                }
            }

            // if obj.is_way() {
            //     let tags = obj.tags();

            //     if tags.contains_key("addr:interpolation") {
            //         println!("{:?}", tags.get("addr:interpolation").unwrap());
            //     }
            // }
        }

        println!(
            "Done loading data for {}. Found {} elements to compute...",
            p,
            objects.len()
        );
        let documents = objects
            .par_iter()
            .map(|obj| {
                let country_code = country_detector
                    .detect(obj.lat, obj.long)
                    .unwrap_or("".to_string())
                    .to_string();
                let mut region: String = "".to_string();
                if country_code != "" {
                    region = region_detector
                        .detect(country_code.to_string(), obj.lat, obj.long)
                        .unwrap_or("".to_string());
                }

                return AddressDocument {
                    id: calculate_hash(&obj.properties),
                    street: obj.properties.street.to_string(),
                    number: obj.properties.number.to_string(),
                    unit: "".to_string(),
                    city: obj.properties.city.to_string(),
                    district: "".to_string(),
                    region: region,
                    postcode: obj.properties.postcode.to_string(),
                    country_code: Some(country_code),
                    lat: obj.lat,
                    long: obj.long,
                };
            })
            .collect::<Vec<_>>();

        insert_address_documents(&mut client, full_table_name.clone(), documents, None)
            .await
            .unwrap();
        fs::remove_file(&existing_file).unwrap();
        println!("Done in {:?}s", time.elapsed().as_secs());
    }
}
