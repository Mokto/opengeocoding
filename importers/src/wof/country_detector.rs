use bzip2_rs::DecoderReader;
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
struct CountryData {
    name: String,
    geometry: Geometry,
    centroid: Point,
}

pub struct CountryDetector {
    countries: Vec<CountryData>,
}

struct DataFile {
    country: String,
    file_path: String,
}

impl CountryDetector {
    pub fn detect(&self, lat: f64, long: f64) -> Option<String> {
        let point = point! {x: long, y: lat};

        // sort regions by distance from point to centroid. Will probably find the matching region first.
        let mut regions = self.countries.clone();
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
        let regions_folder = "./data/whosonfirst-data-country-latest";

        let folder_exists: bool = Path::new(regions_folder).is_dir();

        if !folder_exists {
            let file = fs::File::open(std::path::Path::new(
                "data/whosonfirst-data-country-latest.tar.bz2",
            ))
            .unwrap();

            let reader = DecoderReader::new(file);

            println!("Unpacking file...");
            let mut archive = Archive::new(reader);
            archive.unpack(regions_folder).unwrap();
        }

        println!("Loading countries...");
        let now = Instant::now();

        let file =
            fs::File::open(regions_folder.to_owned() + "/meta/whosonfirst-data-country-latest.csv")
                .unwrap();
        let mut rdr = ReaderBuilder::new().from_reader(file);

        let mut records = vec![];

        for result in rdr.records() {
            let record = result.unwrap();
            let path = record.get(19).unwrap();
            let country_code = record.get(25).unwrap().to_lowercase();

            let path = regions_folder.to_owned() + "/data/" + path;
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
            .flatten()
            .collect::<Vec<_>>();

        let elapsed = now.elapsed();
        println!("Took {:.2?}\n", elapsed);

        Self {
            countries: vec_result,
        }
    }
}