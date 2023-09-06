use bzip2_rs::DecoderReader;
use csv::ReaderBuilder;
use geo::algorithm::contains::Contains;
use geo_types::geometry::Point;
use geo_types::{Geometry, LineString, MultiPolygon, Polygon};
use geozero::geojson::{GeoJson, GeoJsonReader};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{stdout, Write};
use std::path::Path;
use tar::Archive;

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum GeoPointGeometry {
    MultiPolygon {
        coordinates: Vec<Vec<Vec<Vec<f64>>>>,
    },
    Polygon {
        coordinates: Vec<Vec<Vec<f64>>>,
    },
    Point {
        coordinates: Vec<f64>,
    },
}

#[derive(Serialize, Deserialize)]
pub struct GeoPoint {
    pub r#type: String,
    pub geometry: GeoPointGeometry,
}

pub struct RegionData {
    pub name: String,
    pub geometry: GeoLibGeometry,
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
            use std::time::Instant;
            let now = Instant::now();
            {
                for _ in 0..50 {
                    self.detect(country_code.to_string(), 0.0, 0.0);
                }
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
            match &region.geometry {
                GeoLibGeometry::MultiPolygon(polygon) => {
                    if Contains::contains(polygon, &point) {
                        return Some(region.name.clone());
                    }
                }
                GeoLibGeometry::Polygon(polygon) => {
                    if Contains::contains(polygon, &point) {
                        return Some(region.name.clone());
                    }
                }
                GeoLibGeometry::Point => {
                    return None;
                }
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

            let file = fs::File::open(regions_folder.to_owned() + "/data/" + path);
            // let geojson = GeoJsonReader(file.unwrap());
            // let geojson = geojson.into().unwrap();
            // if let Ok(Geometry::Polygon(poly)) = geojson.to_geo() {
            //     assert_eq!(poly.centroid().unwrap(), Point::new(5.0, 3.0));
            // }
            // geojson
            let geopoint: GeoPoint = serde_json::from_reader(file.unwrap()).unwrap();

            if hashmap_result.get(&country_code).is_none() {
                hashmap_result.insert(country_code, Vec::new());
            }

            hashmap_result
                .get_mut(country_code_temp.as_str())
                .unwrap()
                .push(RegionData {
                    geometry: RegionDetector::geometry_to_geolib_geometry(geopoint.geometry),
                    name: name.to_string(),
                });
        }
        stdout.write(format!("\nDone.\n\n").as_bytes()).unwrap();

        // no, ie, tz~, us, za, pl, gb, it, ru, ca
        Self {
            regions: hashmap_result,
        }
    }

    fn geometry_to_geolib_geometry(geometry: GeoPointGeometry) -> GeoLibGeometry {
        match &geometry {
            GeoPointGeometry::MultiPolygon { coordinates } => {
                let polygons = coordinates.into_iter().map(|polygon| {
                    return RegionDetector::get_polygon(polygon);
                });
                return GeoLibGeometry::MultiPolygon(MultiPolygon::new(polygons.collect()));
            }
            GeoPointGeometry::Polygon { coordinates } => {
                let polygon: Polygon = RegionDetector::get_polygon(coordinates);
                return GeoLibGeometry::Polygon(polygon);
            }
            GeoPointGeometry::Point { coordinates: _ } => {
                return GeoLibGeometry::Point;
            }
        }
    }

    fn get_line_string(line: &Vec<Vec<f64>>) -> LineString<f64> {
        let line_string: Vec<(f64, f64)> = line
            .par_iter()
            .map(|point| {
                return (point[0], point[1]);
            })
            .collect();
        return LineString::from(line_string);
    }
    fn get_polygon(polygon_data: &Vec<Vec<Vec<f64>>>) -> Polygon {
        let mut interiors = vec![];
        for n in 1..(polygon_data.len()) {
            interiors.push(RegionDetector::get_line_string(
                polygon_data.get(n).unwrap(),
            ))
        }
        return Polygon::new(
            RegionDetector::get_line_string(polygon_data.get(0).unwrap()),
            interiors,
        );
    }
}
