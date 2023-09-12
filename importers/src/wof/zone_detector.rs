use collecting_hashmap::CollectingHashMap;
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

struct ZoneData {
    name: String,
    geometry: Geometry,
    centroid: Point,
}

pub struct ZoneDetector {
    zones: HashMap<String, ZoneData>,
    zones_names: CollectingHashMap<String, String>,
}

struct DataFile {
    country_code: String,
    zone: String,
    file_path: String,
}

impl ZoneDetector {
    pub fn detect(&self, country_code: String, lat: f64, long: f64) -> Option<String> {
        let country_code = country_code.to_lowercase();

        let zones_names = self.zones_names.get_all(&country_code);
        if zones_names.is_none() {
            return None;
        }

        let zones_names = zones_names.unwrap();

        let point = point! {x: long, y: lat};

        // sort zones by distance from point to centroid. Will probably find the matching region first.
        let mut zones_names = zones_names.to_vec();
        zones_names.sort_by_cached_key(|zone_name| {
            let centroid = &self
                .zones
                .get((country_code.clone() + zone_name.as_str()).as_str())
                .unwrap()
                .centroid;
            (centroid.vincenty_distance(&point)).unwrap_or(f64::MAX) as u32
        });

        for zone_name in zones_names {
            if self
                .zones
                .get((country_code.clone() + zone_name.as_str()).as_str())
                .unwrap()
                .geometry
                .contains(&point)
            {
                return Some(zone_name);
            }
        }

        None
    }
    pub async fn new(zone_type: &str) -> Self {
        let zones_folder = format!("./data/whosonfirst-data-{}-latest", zone_type);

        download_and_extract(
            format!(
                "https://data.geocode.earth/wof/dist/legacy/whosonfirst-data-{}-latest.tar.bz2",
                &zone_type
            )
            .as_str(),
            format!("data/whosonfirst-{}-latest.tar.bz2", zone_type).as_str(),
            &zones_folder,
        )
        .await
        .unwrap();

        println!("Loading zones...");
        let now = Instant::now();

        let file = fs::File::open(format!(
            "{}/meta/whosonfirst-data-{}-latest.csv",
            zones_folder, zone_type
        ))
        .unwrap();
        let mut rdr = ReaderBuilder::new().from_reader(file);

        let mut records = vec![];

        for result in rdr.records() {
            let record = result.unwrap();
            let name = record.get(17).unwrap();
            let path = record.get(19).unwrap();
            let country_code = record.get(25).unwrap().to_lowercase();

            let path = zones_folder.clone() + "/data/" + path;
            records.push(DataFile {
                country_code: country_code.clone(),
                zone: name.to_string(),
                file_path: path.clone(),
            });
        }
        println!("Found {} zones to load.", records.len());

        let zones_iter = records
            .par_iter()
            .map(|record| {
                let file_data = fs::read_to_string(&record.file_path).unwrap();

                let geojson = GeoJson(file_data.as_str());
                if let Ok(Geometry::Point(_poly)) = geojson.to_geo() {
                    return None;
                }
                match geojson.to_geo() {
                    Ok(data) => {
                        return Some((
                            record.country_code.clone(),
                            ZoneData {
                                geometry: data.clone(),
                                name: record.zone.clone(),
                                centroid: data.centroid().unwrap(),
                            },
                        ));
                    }
                    Err(_) => return None,
                }
            })
            .into_par_iter()
            .flatten();

        let hashmap_result = zones_iter
            .clone()
            .map(|region| (region.0.to_string() + region.1.name.as_str(), region.1))
            .collect::<Vec<_>>()
            .into_iter()
            .collect::<HashMap<_, _>>();

        let zones_names = zones_iter
            // .map(|d| => (d.0.clone(), d.1.name.clone()))
            .map(|region| (region.0.clone(), region.1.name.clone()))
            .collect::<Vec<_>>()
            .into_iter()
            .collect::<CollectingHashMap<_, _>>();

        let elapsed = now.elapsed();
        println!("Took {:.2?}\n", elapsed);

        Self {
            zones: hashmap_result,
            zones_names: zones_names,
        }
    }
}
