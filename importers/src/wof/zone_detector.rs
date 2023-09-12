use bzip2_rs::DecoderReader;
use collecting_hashmap::CollectingHashMap;
use csv::ReaderBuilder;
use geo::{Centroid, Contains, VincentyDistance};
use geo_types::{point, Geometry, Point};
use geozero::geojson::GeoJson;
use geozero::ToGeo;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Instant;
use tar::Archive;

struct ZoneData {
    name: String,
    geometry: Geometry,
    centroid: Point,
}

pub struct ZoneDetector {
    regions: HashMap<String, ZoneData>,
    regions_names: CollectingHashMap<String, String>,
}

struct DataFile {
    country_code: String,
    zone: String,
    file_path: String,
}

impl ZoneDetector {
    pub fn detect(&self, country_code: String, lat: f64, long: f64) -> Option<String> {
        let country_code = country_code.to_lowercase();

        let regions_names = self.regions_names.get_all(&country_code);
        if regions_names.is_none() {
            return None;
        }

        let regions_names = regions_names.unwrap();

        let point = point! {x: long, y: lat};

        // sort regions by distance from point to centroid. Will probably find the matching region first.
        let mut regions_names = regions_names.to_vec();
        regions_names.sort_by_cached_key(|region_name| {
            let centroid = &self
                .regions
                .get((country_code.clone() + region_name.as_str()).as_str())
                .unwrap()
                .centroid;
            (centroid.vincenty_distance(&point)).unwrap_or(f64::MAX) as u32
        });

        for region_name in regions_names {
            if self
                .regions
                .get((country_code.clone() + region_name.as_str()).as_str())
                .unwrap()
                .geometry
                .contains(&point)
            {
                return Some(region_name);
            }
        }

        None
    }
    pub fn new() -> Self {
        let regions_folder = "./data/whosonfirst-data-region-latest";

        let folder_exists: bool = Path::new(regions_folder).is_dir();

        if !folder_exists {
            let file = fs::File::open(std::path::Path::new(
                "data/whosonfirst-data-region-latest.tar.bz2",
            ))
            .unwrap();

            let reader = DecoderReader::new(file);

            println!("Unpacking file...");
            let mut archive = Archive::new(reader);
            archive.unpack(regions_folder).unwrap();
        }

        println!("Loading regions...");
        let now = Instant::now();

        let file =
            fs::File::open(regions_folder.to_owned() + "/meta/whosonfirst-data-region-latest.csv")
                .unwrap();
        let mut rdr = ReaderBuilder::new().from_reader(file);

        let mut records = vec![];

        for result in rdr.records() {
            let record = result.unwrap();
            let name = record.get(17).unwrap();
            let path = record.get(19).unwrap();
            let country_code = record.get(25).unwrap().to_lowercase();

            let path = regions_folder.to_owned() + "/data/" + path;
            records.push(DataFile {
                country_code: country_code.clone(),
                zone: name.to_string(),
                file_path: path.clone(),
            });
        }
        println!("Found {} regions to load.", records.len());

        let regions_iter = records
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

        let hashmap_result = regions_iter
            .clone()
            .map(|region| (region.0.to_string() + region.1.name.as_str(), region.1))
            .collect::<Vec<_>>()
            .into_iter()
            .collect::<HashMap<_, _>>();

        let regions_names = regions_iter
            // .map(|d| => (d.0.clone(), d.1.name.clone()))
            .map(|region| (region.0.clone(), region.1.name.clone()))
            .collect::<Vec<_>>()
            .into_iter()
            .collect::<CollectingHashMap<_, _>>();

        let elapsed = now.elapsed();
        println!("Took {:.2?}\n", elapsed);

        Self {
            regions: hashmap_result,
            regions_names: regions_names,
        }
    }
}
