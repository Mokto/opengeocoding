use crate::client::opengeocoding::{self, open_geocoding_client};
use crate::client::OpenGeocodingApiClient;
use crate::data::city::CityDocument;
use crate::wof::zone_detector::ZoneDetector;
use csv::ReaderBuilder;
use itertools::Itertools;
use rayon::prelude::*;
use std::fs;

pub async fn extract_cities() {
    let mut client = OpenGeocodingApiClient::new().await.unwrap();

    let region_detector = ZoneDetector::new_region_detector().await;

    let fname = std::path::Path::new("./data/allCountries.zip");
    let file = fs::File::open(fname).unwrap();

    let mut archive = zip::ZipArchive::new(file).unwrap();
    let file = archive.by_index(0).unwrap();

    let mut rdr = ReaderBuilder::new().delimiter(b'\t').from_reader(file);

    println!("Preparing documents...");

    let documents = rdr.records().map(|result| {
        let record = result.unwrap();
        let id = record.get(0).unwrap();
        let name = record.get(1).unwrap();
        let latitude: f64 = record.get(4).unwrap().parse().unwrap();
        let longitude: f64 = record.get(5).unwrap().parse().unwrap();
        let feature_class = record.get(6).unwrap();
        let country_code = record.get(8).unwrap().to_lowercase();
        let population: u32 = record.get(14).unwrap().parse().unwrap_or(0);

        if feature_class != "P" {
            return None;
        }

        return Some(CityDocument {
            id: id.to_string(),
            city: name.to_string(),
            country_code: country_code.to_lowercase().to_string(),
            region: "".to_string(), // will be calculated later
            lat: latitude,
            long: longitude,
            population: population,
        });
    });

    let page_size = 20000;

    for (index, chunk) in documents.chunks(page_size).into_iter().enumerate() {
        // let now = Instant::now();
        if index != 0 && index * page_size % 100000 == 0 {
            println!("Done with {} documents", index * page_size);
        }
        let mut documents = chunk.filter(|doc| doc.is_some()).peekable();
        if !documents.peek().is_some() {
            continue;
        }
        let locations: Vec<opengeocoding::Location> = documents
            .into_iter()
            .collect::<Vec<Option<CityDocument>>>()
            .par_iter()
            .map(|doc| {
                let doc = doc.as_ref().unwrap();
                let region = &region_detector
                    .detect(doc.country_code.clone(), doc.lat, doc.long)
                    .unwrap_or("".to_string());
                opengeocoding::Location {
                    id: Some(doc.id.clone()),
                    city: Some(doc.city.clone()),
                    street: None,
                    country_code: Some(doc.country_code.clone()),
                    region: Some(region.to_owned()),
                    district: None,
                    lat: doc.lat as f32,
                    long: doc.long as f32,
                    population: Some(doc.population),
                    number: None,
                    unit: None,
                    postcode: None,
                    source: opengeocoding::Source::Geonames.into(),
                    full_street_address: None,
                }
            })
            .collect();

        let query_result = client.insert_locations(locations).await;

        match query_result {
            Ok(_) => {}
            Err(e) => {
                println!("Error: {}", e);
                panic!("Error running SQL");
            }
        };
    }
    println!("Done.");
}
