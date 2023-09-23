use std::{fs, path::Path};

use crate::{
    client::OpenGeocodingApiClient,
    data::street_v2::{insert_street_documents_v2, StreetDocumentV2},
    download::download_file,
    wof::zone_detector::ZoneDetector,
};
use csv::ReaderBuilder;
use flate2::read::GzDecoder;
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};

pub async fn extract_file(
    opengeocoding_client: &mut OpenGeocodingApiClient,
    file_url: &str,
    region_detector: Option<&ZoneDetector>,
    country_code: &str,
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
        let long_min = record.get(3).unwrap().parse::<f64>().unwrap();
        let long_max = record.get(4).unwrap().parse::<f64>().unwrap();
        let lat_min = record.get(5).unwrap().parse::<f64>().unwrap();
        let lat_max = record.get(6).unwrap().parse::<f64>().unwrap();
        let house_min = record.get(7).unwrap();
        let house_max = record.get(8).unwrap();
        let house_odd = record.get(9).unwrap() == "True";
        let house_even = record.get(10).unwrap() == "True";

        let hash_base = format!(
            "{}{}{}{}{}{}",
            postal_code, city, street, house_min, house_max, country_code
        );
        records.push(StreetDocumentV2 {
            postal_code: postal_code.to_string(),
            city: city.to_string(),
            street: street.to_string(),
            country_code: Some(country_code.to_string()),
            region: "".to_string(),
            id: hash_base,
            lat_min,
            long_min,
            lat_max,
            long_max,
            house_min: house_min.to_string(),
            house_max: house_max.to_string(),
            house_odd,
            house_even,
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
                    record.lat_min,
                    record.long_max,
                )
                .unwrap_or("".to_string());

            StreetDocumentV2 {
                region: region,
                city: record.city.clone(),
                street: record.street.clone(),
                country_code: record.country_code.clone(),
                id: record.id.clone(),
                postal_code: record.postal_code.clone(),
                lat_min: record.lat_min,
                long_min: record.long_min,
                lat_max: record.lat_max,
                long_max: record.long_max,
                house_min: record.house_min.clone(),
                house_max: record.house_max.clone(),
                house_odd: record.house_odd,
                house_even: record.house_even,
            }
        })
        .collect::<Vec<_>>();

    // insert_street_documents_v2(
    //     opengeocoding_client,
    //     full_table_name.to_string(),
    //     documents,
    //     None,
    // )
    // .await
    // .unwrap();

    fs::remove_file(&existing_file).unwrap();
    println!("Done in {:?}s", time.elapsed().as_secs());
}
