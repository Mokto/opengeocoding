use crate::client::{opengeocoding, OpenGeocodingApiClient};
use rayon::prelude::*;
use rayon::str::ParallelString;
use serde::{Deserialize, Serialize};
use std::io::prelude::*;
use std::{fs, vec};

#[derive(Serialize, Deserialize)]
pub struct GeoPointGeometry {
    pub r#type: String,
    pub coordinates: Vec<f64>,
}
#[derive(Serialize, Deserialize)]
pub struct GeoPointProperties {
    pub hash: String,
    pub street: Option<String>,
    pub number: Option<String>,
    pub unit: Option<String>,
    pub city: Option<String>,
    pub district: Option<String>,
    pub region: Option<String>,
    pub postcode: Option<String>,
}
#[derive(Serialize, Deserialize)]
pub struct GeoPoint {
    pub r#type: String,
    pub properties: GeoPointProperties,
    pub geometry: Option<GeoPointGeometry>,
}

pub async fn import_addresses() {
    let mut client = OpenGeocodingApiClient::new().await.unwrap();

    let fname = std::path::Path::new("data/collection-global.zip");
    let file = fs::File::open(fname).unwrap();

    let mut archive = zip::ZipArchive::new(file).unwrap();

    let start_from: Option<&str> = None;
    // let start_from = Some("ca/on/city_of_vaughan-addresses-city.geojson");

    let exclude_files: Vec<String> = vec![];

    let mut has_started = false;
    'outer: for i in (0..archive.len()).rev() {
        let mut file = archive.by_index(i).unwrap();
        let outpath = match file.enclosed_name() {
            Some(path) => path.to_owned(),
            None => continue,
        };

        if start_from.is_some() && !has_started {
            if outpath.to_str().unwrap() == start_from.unwrap() {
                has_started = true;
            } else {
                continue;
            }
        }

        let to_exclude_regexes = vec![
            "-parcels-county.geojson",
            "-parcels-city.geojson",
            "-parcels-state.geojson",
            "-parcels-town.geojson",
            "-parcels-country.geojson",
            "-parcels-province.geojson",
            "-buildings-county.geojson",
            "-buildings-city.geojson",
            "-buildings-state.geojson",
            "-buildings-town.geojson",
            "-buildings-territory.geojson",
            "-buildings-country.geojson",
            "-buildings-region.geojson",
        ];

        if outpath.file_name().is_some()
            && outpath.extension().unwrap().to_str().unwrap() == "geojson"
        {
            let file_name = outpath.to_str().unwrap().to_string();
            if exclude_files.contains(&file_name) {
                println!("Filename excluded: {}", outpath.display());
                continue;
            }

            for regex in to_exclude_regexes.iter() {
                if file_name.contains(regex) {
                    println!("Filename excluded ({}): {}", regex, outpath.display());
                    continue 'outer;
                }
            }

            println!("Filename: {} {}", outpath.display(), file.size());
            let mut contents = String::new();
            file.read_to_string(&mut contents).unwrap();

            let country_code = file_name.split("/").next().unwrap().to_string();

            run_file(contents, country_code, &mut client).await;
        };
    }
}

async fn run_file(content: String, country_code: String, client: &mut OpenGeocodingApiClient) {
    let locations = content
        .par_lines()
        .map(|line| {
            let p: GeoPoint = serde_json::from_str(line).unwrap();

            if p.geometry.is_none() {
                return None;
            }
            let geometry = p.geometry.unwrap();
            if geometry.r#type != "Point" {
                return None;
            }
            return Some(opengeocoding::Location {
                id: Some(p.properties.hash),
                city: Some(p.properties.city.unwrap_or("".to_string())),
                street: Some(p.properties.street.unwrap_or("".to_string())),
                country_code: Some(country_code.clone()),
                region: Some(p.properties.region.unwrap_or("".to_string())),
                district: Some(p.properties.district.unwrap_or("".to_string())),
                lat: geometry.coordinates[1] as f32,
                long: geometry.coordinates[0] as f32,
                population: None,
                number: Some(p.properties.number.unwrap_or("".to_string())),
                unit: Some(p.properties.unit.unwrap_or("".to_string())),
                postcode: Some(p.properties.postcode.unwrap_or("".to_string())),
                source: opengeocoding::Source::OpenAddresses.into(),
                full_street_address: None,
            });
        })
        .flatten()
        .collect::<Vec<_>>();

    let query_result = client.insert_locations(locations).await;

    match query_result {
        Ok(_) => {}
        Err(e) => {
            println!("Error: {}", e);
            panic!("Error running SQL");
        }
    };
}
