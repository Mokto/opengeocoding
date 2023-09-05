use crate::data::{calculate_hash, CityDocument};
use csv::ReaderBuilder;
use itertools::Itertools;
use mysql::prelude::*;
use mysql::*;
use std::fs;

pub async fn extract_cities() {
    let fname = std::path::Path::new("./data/allCountries.zip");
    let file = fs::File::open(fname).unwrap();

    let mut archive = zip::ZipArchive::new(file).unwrap();
    let file = archive.by_index(0).unwrap();

    let mut rdr = ReaderBuilder::new().delimiter(b'\t').from_reader(file);

    let documents = rdr.records().map(|result| {
        let record = result.unwrap();
        let name = record.get(1).unwrap();
        let latitude = record.get(4).unwrap();
        let longitude = record.get(5).unwrap();
        let feature_class = record.get(6).unwrap();
        let country_code = record.get(8).unwrap().to_lowercase();
        let population = record.get(14).unwrap();

        if feature_class != "P" {
            return None;
        }

        let hash_base = format!("{}-{}", name, country_code);
        return Some(CityDocument {
            id: calculate_hash(&hash_base),
            city: name.to_string(),
            country_code: country_code.to_lowercase().to_string(),
            region: "".to_string(),
            lat: latitude.parse().unwrap(),
            long: longitude.parse().unwrap(),
            population: population.parse().unwrap(),
        });
    });

    let page_size = 20000;

    let url = "mysql://root:password@localhost:9306/default";

    let pool = Pool::new(url).unwrap();
    let mut conn = pool.get_conn().unwrap();

    for (index, chunk) in documents.chunks(page_size).into_iter().enumerate() {
        if index != 0 && index * page_size % 100000 == 0 {
            println!("Done with {} documents", index * page_size);
        }
        let mut documents = chunk.filter(|doc| doc.is_some()).peekable();
        if !documents.peek().is_some() {
            continue;
        }
        let query = format!(
            "REPLACE INTO geonames_cities(id,city,region,lat,long,country_code,population) VALUES {};",
            documents
                .map(|doc| {
                    let doc = doc.as_ref().unwrap();
                    return format!(
                        r"({},'{}','{}',{},{},'{}', {})",
                        doc.id,
                        clean_string(&doc.city),
                        clean_string(&doc.region),
                        doc.lat,
                        doc.long,
                        doc.country_code,
                        doc.population,
                    );
                })
                .join(", ")
        );

        let query_result = conn.query_drop(&query);

        match query_result {
            Ok(_) => {}
            Err(e) => {
                println!("Query: {}", query);
                println!("Error: {}", e);
                panic!("Error running SQL");
            }
        };
    }
    println!("Done.");
}

fn clean_string(s: &str) -> String {
    return s.replace(r"\", r"\\").replace("'", r"\'");
}
