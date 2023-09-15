use std::{fs, path::Path};

use crate::{
    client::OpenGeocodingApiClient,
    data::{calculate_hash, insert_address_documents, AddressDocument},
    download::download_file,
    wof::zone_detector::ZoneDetector,
};
use csv::ReaderBuilder;
use flate2::read::GzDecoder;
use rayon::prelude::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};

pub async fn extract_file(
    opengeocoding_client: &mut OpenGeocodingApiClient,
    file_url: &str,
    region_detector: Option<&ZoneDetector>,
    full_table_name: &str,
) {
    let time = std::time::Instant::now();

    println!("Loading data for {}...", file_url);

    let path = std::path::Path::new(file_url);
    let existing_file = "data/".to_string() + path.file_name().unwrap().to_str().unwrap();
    if !Path::new(&existing_file).is_file() {
        println!("Downloading file...");
        download_file(file_url, &existing_file).await.unwrap();
        println!("Done. Reading it...");
    }

    let file = fs::File::open(&existing_file).unwrap();
    let file = GzDecoder::new(file);

    let mut rdr = ReaderBuilder::new().delimiter(b'\t').from_reader(file);

    let mut records = vec![];

    for result in rdr.records() {
        let record = result.unwrap();
        let postal_code = record.get(0).unwrap();
        let city = record.get(1).unwrap();
        let street = record.get(2).unwrap();
        let house_number = record.get(3).unwrap();
        let longitude = record.get(4).unwrap().parse::<f64>().unwrap();
        let latitude = record.get(5).unwrap().parse::<f64>().unwrap();
        let country_code = record.get(6).unwrap().to_lowercase();

        let hash_base = format!(
            "{}{}{}{}{}",
            postal_code, city, street, house_number, country_code
        );

        records.push(AddressDocument {
            postcode: postal_code.to_string(),
            city: city.to_string(),
            street: street.to_string(),
            number: house_number.to_string(),
            long: longitude,
            lat: latitude,
            country_code: Some(country_code.to_string()),
            region: "".to_string(),
            district: "".to_string(),
            unit: "".to_string(),
            id: calculate_hash(&hash_base),
        });
    }

    println!("Calculating regions for {} elements...", records.len());
    let documents = records
        .par_iter()
        .map(|record| {
            let region = region_detector
                .unwrap()
                .detect(
                    record.country_code.clone().unwrap().clone(),
                    record.lat,
                    record.long,
                )
                .unwrap_or("".to_string());

            AddressDocument {
                region: region,
                city: record.city.clone(),
                postcode: record.postcode.clone(),
                street: record.street.clone(),
                number: record.number.clone(),
                long: record.long,
                lat: record.lat,
                country_code: record.country_code.clone(),
                district: record.district.clone(),
                unit: record.unit.clone(),
                id: record.id.clone(),
            }
        })
        .collect::<Vec<_>>();

    insert_address_documents(
        opengeocoding_client,
        full_table_name.to_string(),
        documents,
        None,
    )
    .await
    .unwrap();

    fs::remove_file(&existing_file).unwrap();
    println!("Done in {:?}s", time.elapsed().as_secs());
}
