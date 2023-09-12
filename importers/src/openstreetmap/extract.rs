use crate::{
    client::OpenGeocodingApiClient,
    config::Config,
    data::{
        calculate_hash, insert_address_documents, insert_street_documents, AddressDocument,
        StreetDocument, StreetPoint,
    },
    download::download_file,
    openstreetmap::file_list,
    wof::{country_detector::CountryDetector, zone_detector::ZoneDetector},
};
use geo::Centroid;
use geo_types::MultiPoint;
use osmpbfreader::OsmPbfReader;
use rayon::prelude::*;
use std::{
    collections::HashMap,
    fs::{self},
    hash,
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

struct Node {
    lat: f64,
    long: f64,
}

struct Way {
    name: String,
    node_ids: Vec<i64>,
}

pub async fn extract_all() {
    let config = Config::new();
    let table_name_addresses = "openstreetmap_addresses";
    let full_table_name_addresses = config.get_table_name(table_name_addresses.to_string());
    let table_name_streets = "openstreetmap_streets";
    let full_table_name_streets = config.get_table_name(table_name_streets.to_string());
    println!("{}", full_table_name_streets);

    let region_detector = ZoneDetector::new();
    let country_detector = CountryDetector::new();
    let mut client = OpenGeocodingApiClient::new().await.unwrap();

    println!("Creating tables...");
    let query_result = client.run_query(format!("CREATE TABLE IF NOT EXISTS {}(street text, number text, unit text, city text, district text, region text, postcode text, lat float, long float, country_code string)  rt_mem_limit = '1G'", table_name_addresses)).await;
    match query_result {
        Ok(_) => {}
        Err(e) => {
            panic!("{}", e);
        }
    };
    if config.manticore_is_cluster {
        let query_result = client
            .run_query(format!(
                "ALTER CLUSTER {} ADD {}",
                config.manticore_cluster_name, table_name_addresses
            ))
            .await;
        match query_result {
            Ok(_) => {}
            Err(e) => {
                println!("{}", e);
            }
        };
    }

    let query_result = client.run_query(format!("CREATE TABLE IF NOT EXISTS {}(street text, city text, region text, postcode text, lat float, long float, country_code string, points json)  rt_mem_limit = '1G'", table_name_streets)).await;
    match query_result {
        Ok(_) => {}
        Err(e) => {
            panic!("{}", e);
        }
    };
    if config.manticore_is_cluster {
        let query_result = client
            .run_query(format!(
                "ALTER CLUSTER {} ADD {}",
                config.manticore_cluster_name, table_name_streets
            ))
            .await;
        match query_result {
            Ok(_) => {}
            Err(e) => {
                println!("{}", e);
            }
        };
    }

    let country_files = file_list::get_osm_country_files();

    for (index, country_file) in country_files.iter().enumerate() {
        // if index == 0 {
        //     continue;
        // }
        // if index > 1 {
        //     break;
        // }
        let time = std::time::Instant::now();

        println!("Loading data for {}...", country_file.url);

        let path = std::path::Path::new(&country_file.url);
        let existing_file = "data/".to_string() + path.file_name().unwrap().to_str().unwrap();
        if !Path::new(&existing_file).is_file() {
            println!("Downloading file...");
            download_file(&country_file.url, &existing_file)
                .await
                .unwrap();
            println!("Done. Reading it...");
        }

        let file = fs::File::open(&existing_file).unwrap();
        let mut pbf = OsmPbfReader::new(file);

        let mut objects = vec![];
        let mut all_nodes = HashMap::new();
        let mut ways = vec![];

        for obj in pbf.par_iter() {
            let obj: osmpbfreader::OsmObj = obj.unwrap();
            if obj.is_node() {
                let node = obj.node().unwrap();
                let tags = obj.tags();

                all_nodes.insert(
                    node.id.0,
                    Node {
                        lat: node.lat(),
                        long: node.lon(),
                    },
                );

                if tags.contains_key("addr:street")
                    && tags.contains_key("addr:city")
                    && tags.contains_key("addr:housenumber")
                {
                    let house = tags.get("addr:housenumber").unwrap();
                    let street = tags.get("addr:street").unwrap();
                    let city = tags.get("addr:city").unwrap();
                    let postcode = tags.get("addr:postcode");

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

            if obj.is_way() {
                let tags = obj.tags();
                let way = obj.way().unwrap();

                if tags.contains_key("name") && tags.contains("highway", "residential") {
                    let name = tags.get("name").unwrap();

                    ways.push(Way {
                        name: name.to_string(),
                        node_ids: way.nodes.iter().map(|node| node.0).collect(),
                    });
                }
            }
        }

        let street_points = ways
            .par_iter()
            .map(|way| StreetDocument {
                id: 0,
                street: way.name.to_string(),
                country_code: None,
                city: "".to_string(),
                region: "".to_string(),
                lat: 0.0,
                long: 0.0,
                points: way
                    .node_ids
                    .iter()
                    .map(|node_id| {
                        let node = all_nodes.get(node_id).unwrap();
                        StreetPoint {
                            lat: node.lat,
                            long: node.long,
                        }
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();

        println!(
            "Done loading data for {}. Found {} addresses & {} streets to compute...",
            &country_file.url,
            objects.len(),
            street_points.len(),
        );
        println!("Computing street points...");
        let street_points = street_points
            .par_iter()
            .map(|street| {
                let points: MultiPoint<_> = street
                    .points
                    .iter()
                    .map(|point| (point.long, point.lat))
                    .collect::<Vec<_>>()
                    .into();
                let centroid = points.centroid().unwrap();
                let country_code = country_detector
                    .detect(centroid.y(), centroid.x())
                    .unwrap_or("".to_string())
                    .to_string();
                let mut region = "".to_string();
                if country_code != "" {
                    region = region_detector
                        .detect(country_code.to_string(), centroid.y(), centroid.x())
                        .unwrap_or("".to_string());
                }

                let hash_base = format!("{}-{}-{}", street.street, country_code, region);

                StreetDocument {
                    id: calculate_hash(&hash_base),
                    street: street.street.to_string(),
                    country_code: Some(country_code),
                    city: "".to_string(),
                    region: region,
                    lat: centroid.y(),
                    long: centroid.x(),
                    points: street.points.to_vec(),
                }
            })
            .collect::<Vec<_>>();
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

        insert_address_documents(
            &mut client,
            full_table_name_addresses.clone(),
            documents,
            None,
        )
        .await
        .unwrap();

        insert_street_documents(
            &mut client,
            full_table_name_streets.clone(),
            street_points,
            None,
        )
        .await
        .unwrap();
        // fs::remove_file(&existing_file).unwrap();
        println!("Done in {:?}s", time.elapsed().as_secs());
        // return;
    }
}
