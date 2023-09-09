use bzip2_rs::DecoderReader;
use collecting_hashmap::CollectingHashMap;
use csv::ReaderBuilder;
use geo::{Centroid, Contains, VincentyDistance};
use geo_types::{point, Geometry, Point};
use geozero::geojson::GeoJson;
use geozero::ToGeo;
use rayon::prelude::*;
use std::fs;
use std::path::Path;
use std::time::Instant;
use tar::Archive;

#[derive(Clone)]
struct RegionData {
    name: String,
    geometry: Geometry,
    centroid: Point,
}

pub struct RegionDetector {
    regions: CollectingHashMap<String, RegionData>,
}

struct DataFile {
    country_code: String,
    region: String,
    file_path: String,
}

impl RegionDetector {
    pub fn detect(&self, country_code: String, lat: f64, long: f64) -> Option<String> {
        let country_code = country_code.to_lowercase();

        let regions = self.regions.get_all(&country_code);

        if regions.is_none() {
            return None;
        }

        let point = point! {x: long, y: lat};

        // sort regions by distance from point to centroid. Will probably find the matching region first.
        let mut regions = regions.unwrap().to_vec();
        regions.sort_by_cached_key(|region| {
            let centroid = region.centroid;
            (centroid.vincenty_distance(&point)).unwrap_or(f64::MAX) as u32
        });

        for region in regions {
            if region.geometry.contains(&point) {
                return Some(region.name);
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
                region: name.to_string(),
                file_path: path.clone(),
            });
        }
        println!("Found {} regions to load.", records.len());

        let hashmap_result = records
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
                            RegionData {
                                geometry: data.clone(),
                                name: record.region.clone(),
                                centroid: data.centroid().unwrap(),
                            },
                        ));
                    }
                    Err(_) => return None,
                }
            })
            .into_par_iter()
            .flatten()
            .collect::<Vec<_>>()
            .into_iter()
            .collect::<CollectingHashMap<_, _>>();

        let elapsed = now.elapsed();
        println!("Took {:.2?}\n", elapsed);

        Self {
            regions: hashmap_result,
        }
    }
}
