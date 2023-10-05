use crate::{
    client::OpenGeocodingApiClient,
    data::{
        address::AddressDocument,
        street::{StreetDocument, StreetPoint},
    },
    download::download_file,
    wof::{country_detector::CountryDetector, detect_zones, zone_detector::ZoneDetector},
};

use geo::Centroid;
use geo_types::MultiPoint;
use osmpbfreader::OsmPbfReader;
use rayon::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::{
    collections::HashMap,
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

struct Node {
    lat: f64,
    long: f64,
}

struct Way {
    name: String,
    node_ids: Vec<i64>,
}

use super::file_list::OsmCountryFiles;

pub async fn extract_file(
    opengeocoding_client: &mut OpenGeocodingApiClient,
    country_file: OsmCountryFiles,
    country_detector: Option<&CountryDetector>,
    region_detector: Option<&ZoneDetector>,
    locality_detector: Option<&ZoneDetector>,
    full_table_name_addresses: &str,
    full_table_name_streets: &str,
) {
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
        // if obj.is_relation() {
        //     let relation = obj.relation().unwrap();
        //     relation.
        // }
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
            id: "".to_string(),
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
            let (country_code, region, locality) = detect_zones(
                centroid.y(),
                centroid.x(),
                country_detector,
                region_detector,
                locality_detector,
            );

            let hash_base = format!("{}-{}-{}-{}", street.street, country_code, region, locality);

            StreetDocument {
                id: calculate_hash(&hash_base),
                street: street.street.to_string(),
                country_code: Some(country_code),
                city: locality,
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
            let (country_code, region, _) =
                detect_zones(obj.lat, obj.long, country_detector, region_detector, None);

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

    return;

    // insert_address_documents(
    //     opengeocoding_client,
    //     full_table_name_addresses.to_string(),
    //     documents,
    //     None,
    // )
    // .await
    // .unwrap();

    // insert_street_documents(
    //     opengeocoding_client,
    //     full_table_name_streets.to_string(),
    //     street_points,
    //     None,
    // )
    // .await
    // .unwrap();
    // fs::remove_file(&existing_file).unwrap();
    println!("Done in {:?}s", time.elapsed().as_secs());
    // return;
}

fn calculate_hash<T: Hash>(t: &T) -> String {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    let val = s.finish();
    val.to_string()
}
