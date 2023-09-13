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
    id: String,
    name: String,
    geometry: Geometry,
    centroid: Point,
}

pub struct ZoneDetector {
    zones: HashMap<String, ZoneData>,
    zones_ids: CollectingHashMap<String, String>,
}

struct DataFile {
    country_code: String,
    name: String,
    file_path: String,
    id: String,
    centroid: Option<Point>,
}

impl ZoneDetector {
    pub fn detect(&self, country_code: String, lat: f64, long: f64) -> Option<String> {
        let country_code = country_code.to_lowercase();

        let zones_ids = self.zones_ids.get_all(&country_code);
        if zones_ids.is_none() {
            return None;
        }

        let zones_ids = zones_ids.unwrap();

        let point = point! {x: long, y: lat};

        // sort zones by distance from point to centroid. Will probably find the matching region first.
        let mut zones_ids = zones_ids.to_vec();
        zones_ids.sort_by_cached_key(|zone_id| {
            let centroid = &self.zones.get(zone_id).unwrap().centroid;
            (centroid.vincenty_distance(&point)).unwrap_or(f64::MAX) as u32
        });

        for zone_id in &zones_ids {
            let zone = self.zones.get(zone_id).unwrap();
            if zone.geometry.contains(&point) {
                return Some(zone.name.clone());
            }
        }

        None
    }

    pub async fn new_region_detector() -> Self {
        Self::new("region", 14).await
    }
    pub async fn new_locality_detector() -> Self {
        unimplemented!("Locality detector not implemented yet. It's supposed to work but apparently always returns None.");
        // Self::new("locality", 8).await
    }

    async fn new(zone_type: &str, lat_index: usize) -> Self {
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

        println!("Loading {}s...", zone_type);
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
            let id = record.get(9).unwrap();
            let name = record.get(17).unwrap();
            let path = record.get(19).unwrap();
            let country_code = record.get(25).unwrap().to_lowercase();
            let longitude = record.get(lat_index + 1).unwrap().parse::<f64>().unwrap();
            let latitude = record.get(lat_index).unwrap().parse::<f64>().unwrap();
            let centroid = if longitude != 0.0 || latitude != 0.0 {
                Some(point! {x: longitude, y: latitude})
            } else {
                None
            };
            // println!(
            //     "{}, {}, {}, {}, {}, {}",
            //     id, name, path, country_code, longitude, latitude,
            // );

            let path = zones_folder.clone() + "/data/" + path;
            records.push(DataFile {
                country_code: country_code.clone(),
                name: name.to_string(),
                id: id.to_string(),
                file_path: path.clone(),
                centroid: centroid,
            });
        }
        println!("Found {} zones to load.", records.len());

        let zones_iter = records
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
                            return Some((
                                record.country_code.clone(),
                                ZoneData {
                                    geometry: data,
                                    name: record.name.clone(),
                                    id: record.id.clone(),
                                    centroid: record.centroid.unwrap(),
                                },
                            ));
                        }
                        return Some((
                            record.country_code.clone(),
                            ZoneData {
                                geometry: data.clone(),
                                name: record.name.clone(),
                                id: record.id.clone(),
                                centroid: data.centroid().unwrap(),
                            },
                        ));
                    }
                    Err(_) => return None,
                }
            })
            .flatten()
            .collect::<Vec<_>>();

        let zones_ids = zones_iter
            .iter()
            .map(|(country, zone_data)| (country.clone(), zone_data.id.clone()))
            .collect::<Vec<_>>()
            .into_iter()
            .collect::<CollectingHashMap<_, _>>();

        let hashmap_result = zones_iter
            .into_par_iter()
            .map(|(_, zone_data)| (zone_data.id.clone(), zone_data))
            .collect::<Vec<_>>()
            .into_iter()
            .collect::<HashMap<_, _>>();

        let elapsed = now.elapsed();
        println!("Took {:.2?}", elapsed);

        Self {
            zones: hashmap_result,
            zones_ids: zones_ids,
        }
    }
}
