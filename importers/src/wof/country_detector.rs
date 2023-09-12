use csv::ReaderBuilder;
use geo::{Centroid, Contains, VincentyDistance};
use geo_types::{point, Geometry, Point};
use geozero::geojson::GeoJson;
use geozero::ToGeo;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::time::Instant;

use crate::extractor::tar_bz2::download_and_extract;

#[derive(Debug)]
struct CountryData {
    name: String,
    geometry: Geometry,
    centroid: Point,
}

pub struct CountryDetector {
    country_codes: Vec<String>,
    countries: HashMap<String, CountryData>,
}

struct DataFile {
    country: String,
    file_path: String,
}

impl CountryDetector {
    pub fn detect(&self, lat: f64, long: f64) -> Option<String> {
        let point = point! {x: long, y: lat};

        // sort countries by distance from point to centroid. Will probably find the matching region first.
        let mut country_codes = self.country_codes.clone();
        country_codes.sort_by_cached_key(|country_code| {
            let centroid = self.countries.get(country_code).unwrap().centroid;
            (centroid.vincenty_distance(&point)).unwrap_or(f64::MAX) as u32
        });

        for country_code in country_codes {
            if self
                .countries
                .get(&country_code)
                .unwrap()
                .geometry
                .contains(&point)
            {
                return Some(country_code);
            }
        }

        None
    }

    pub async fn new() -> Self {
        let countries_folder = "./data/whosonfirst-data-country-latest".to_string();

        download_and_extract(
            "https://data.geocode.earth/wof/dist/legacy/whosonfirst-data-country-latest.tar.bz2",
            "data/whosonfirst-country-latest.tar.bz2",
            &countries_folder,
        )
        .await
        .unwrap();

        println!("Loading countries...");
        let now = Instant::now();

        let file =
            fs::File::open(countries_folder.clone() + "/meta/whosonfirst-data-country-latest.csv")
                .unwrap();
        let mut rdr = ReaderBuilder::new().from_reader(file);

        let mut records = vec![];

        for result in rdr.records() {
            let record = result.unwrap();
            let path = record.get(19).unwrap();
            let country_code = record.get(25).unwrap().to_lowercase();

            let path = countries_folder.clone() + "/data/" + path;
            records.push(DataFile {
                country: country_code,
                file_path: path.clone(),
            });
        }
        println!("Found {} countries to load.", records.len());

        let vec_result = records
            .par_iter()
            .map(|record| {
                let file_data = fs::read_to_string(&record.file_path).unwrap();

                let geojson = GeoJson(file_data.as_str());
                if let Ok(Geometry::Point(_poly)) = geojson.to_geo() {
                    return None;
                }
                match geojson.to_geo() {
                    Ok(data) => {
                        return Some(CountryData {
                            geometry: data.clone(),
                            name: record.country.clone(),
                            centroid: data.centroid().unwrap(),
                        });
                    }
                    Err(_) => return None,
                }
            })
            .into_par_iter()
            .flatten();

        let country_codes = vec_result
            .clone()
            .map(|country| country.name.clone())
            .collect::<Vec<_>>();

        let elapsed = now.elapsed();
        println!("Took {:.2?}\n", elapsed);

        Self {
            countries: vec_result
                .map(|c| (c.name.clone(), c))
                .collect::<HashMap<_, _>>(),
            country_codes: country_codes,
        }
    }
}
