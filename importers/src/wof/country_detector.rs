use csv::ReaderBuilder;
use geo::algorithm::closest_point::ClosestPoint;
use geo::algorithm::simplify::Simplify;
use geo::{Centroid, Closest, Contains, VincentyDistance};
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
    wof_country_mapping: HashMap<String, String>,
}

struct DataFile {
    country: String,
    file_path: String,
    centroid: Option<Point>,
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

        // we try to find what country contains the point first
        for country_code in &country_codes {
            if self
                .countries
                .get(country_code)
                .unwrap()
                .geometry
                .contains(&point)
            {
                return Some(
                    self.wof_country_mapping
                        .get(country_code)
                        .unwrap_or(&country_code)
                        .to_string(),
                );
            }
        }
        // if not found, we try to find if the closest country is less than a km away. we only check the first 25 countries as the computation is quite expensive.
        for country_code in country_codes.iter().take(25) {
            let country_data = self.countries.get(country_code).unwrap();
            let geometry_point = &country_data.geometry.closest_point(&point);
            match geometry_point {
                Closest::SinglePoint(p) => {
                    if (p.vincenty_distance(&point)).unwrap_or(f64::MAX) < 1000.0 {
                        // 1km
                        return Some(
                            self.wof_country_mapping
                                .get(country_code)
                                .unwrap_or(&country_code)
                                .to_string(),
                        );
                    }
                }
                Closest::Indeterminate => {}
                Closest::Intersection(_) => {}
            }
        }

        None
    }

    pub async fn new() -> Self {
        let countries_folder = "./data/whosonfirst-data-country-latest".to_string();
        let mut wof_country_mapping = HashMap::new();

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
            let longitude = record.get(15).unwrap().parse::<f64>().unwrap();
            let latitude = record.get(14).unwrap().parse::<f64>().unwrap();
            let wof_country_code = record.get(25).unwrap().to_lowercase();
            let country_code = record.get(12).unwrap().to_lowercase();
            let centroid = if longitude != 0.0 || latitude != 0.0 {
                Some(point! {x: longitude, y: latitude})
            } else {
                None
            };

            if wof_country_code != country_code {
                wof_country_mapping.insert(country_code.clone(), wof_country_code.clone());
            }

            let path = countries_folder.clone() + "/data/" + path;
            records.push(DataFile {
                country: country_code,
                file_path: path.clone(),
                centroid: centroid,
            });
        }
        println!("Found {} countries to load.", records.len());

        let vec_result = records
            .par_iter()
            .map(|record| {
                let file_data = fs::read_to_string(&record.file_path).unwrap();

                let geojson = GeoJson(file_data.as_str()).to_geo();
                if let Ok(Geometry::Point(_poly)) = geojson {
                    return None;
                }
                match geojson {
                    Ok(data) => {
                        if record.centroid.is_some() {
                            return Some(CountryData {
                                geometry: data,
                                name: record.country.clone(),
                                centroid: record.centroid.unwrap(),
                            });
                        }
                        return Some(CountryData {
                            geometry: data.clone(),
                            name: record.country.clone(),
                            centroid: data.centroid().unwrap(),
                        });
                    }
                    Err(_) => return None,
                }
            })
            .flatten()
            .collect::<Vec<_>>();

        let country_codes = vec_result
            .iter()
            .map(|country| country.name.clone())
            .collect::<Vec<_>>();

        let countries = vec_result
            .into_par_iter()
            .map(|c| (c.name.clone(), c))
            .collect::<HashMap<_, _>>();
        println!("Took {:.2?} to load countries.", now.elapsed());

        Self {
            countries: countries,
            country_codes: country_codes,
            wof_country_mapping: wof_country_mapping,
        }
    }
}
