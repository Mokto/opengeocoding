use bzip2_rs::DecoderReader;
use csv::ReaderBuilder;
use geo::Contains;
use geo_types::{Geometry, MultiPolygon, Point, Polygon};
use geozero::geojson::{GeoJson, GeoJsonReader};
use geozero::ToGeo;
use rayon::prelude::*;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::io::{stdout, Write};
use std::path::Path;
use std::time::Instant;
use tar::Archive;

pub struct RegionData {
    pub name: String,
    pub geometry: geo::Geometry,
}

pub enum GeoLibGeometry {
    Polygon(Polygon),
    MultiPolygon(MultiPolygon),
    Point,
}
pub struct RegionDetector {
    regions: HashMap<String, Vec<RegionData>>,
}

impl RegionDetector {
    pub fn debug(&self) {
        for (country_code, regions) in &self.regions {
            let now = Instant::now();
            {
                (0..100).into_par_iter().for_each(|x| {
                    self.detect(country_code.to_string(), 0.0, 0.0);
                });
                // for _ in 0..50 {
                //     self.detect(country_code.to_string(), 0.0, 0.0);
                // }
            }
            let elapsed = now.elapsed();
            if now.elapsed() > std::time::Duration::from_secs(1) {
                println!("Country code: {}, regions: {}", country_code, regions.len());
                println!("Elapsed: {:.2?}", elapsed);
            }
        }
    }
    pub fn detect(&self, country_code: String, lat: f64, long: f64) -> Option<String> {
        let country_code = country_code.to_lowercase();

        let regions = self.regions.get(&country_code);

        if regions.is_none() {
            return None;
        }

        let regions = regions.unwrap();
        let point = Point::new(long, lat);

        for region in regions {
            if region.geometry.contains(&point) {
                return Some(region.name.clone());
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

        let mut hashmap_result = HashMap::new();

        let file =
            fs::File::open(regions_folder.to_owned() + "/meta/whosonfirst-data-region-latest.csv")
                .unwrap();
        let mut rdr = ReaderBuilder::new().from_reader(file);
        let estimated_total_count = 5064;
        let mut stdout: std::io::Stdout = stdout();
        for (index, result) in rdr.records().enumerate() {
            let progress = index as f64 / estimated_total_count as f64 * 100.0;
            stdout
                .write(format!("\rProcessing {}%...", progress.trunc()).as_bytes())
                .unwrap();
            let record = result.unwrap();
            let name = record.get(17).unwrap();
            let path = record.get(19).unwrap();
            let country_code = record.get(25).unwrap().to_lowercase();

            // optimize that code later
            let country_code_temp = country_code.clone();

            let path = regions_folder.to_owned() + "/data/" + path;

            let file_data = fs::read_to_string(path).unwrap();
            let geojson = GeoJson(file_data.as_str());
            if let Ok(Geometry::Point(_poly)) = geojson.to_geo() {
                continue;
            }
            match geojson.to_geo() {
                Ok(data) => {
                    if hashmap_result.get(&country_code).is_none() {
                        hashmap_result.insert(country_code, Vec::new());
                    }

                    hashmap_result
                        .get_mut(country_code_temp.as_str())
                        .unwrap()
                        .push(RegionData {
                            geometry: data,
                            name: name.to_string(),
                        });
                }
                Err(_) => {
                    continue;
                }
            }
        }

        let elapsed = now.elapsed();
        stdout
            .write(format!("\nDone.\nTook {:.2?}\n", elapsed).as_bytes())
            .unwrap();

        Self {
            regions: hashmap_result,
        }
    }
}
