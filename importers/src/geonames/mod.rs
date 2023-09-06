use crate::data::{calculate_hash, CityDocument};
use crate::wof::RegionDetector;
use csv::ReaderBuilder;
use itertools::Itertools;
use mysql::prelude::*;
use mysql::*;
use std::{env, fs};

pub async fn extract_cities() {
    let table_name = "geonames_cities";
    let cluster_name = "opengeocoding_cluster";
    let url = format!(
        "mysql://root:password@{}:9306/default",
        env::var("MANTICORESEARCH_ENDPOINT").unwrap_or("localhost".to_string())
    );
    println!("Creating table...");
    let pool = Pool::new(Opts::from_url(url.as_str()).unwrap()).unwrap();
    let mut conn = pool.get_conn().unwrap();

    let query_result = conn.query_drop(format!("CREATE TABLE IF NOT EXISTS {}(city text, region text, lat float, long float, country_code string, population int)  rt_mem_limit = '1G'", table_name));
    match query_result {
        Ok(_) => {}
        Err(e) => {
            panic!("{}", e);
        }
    };
    let query_result =
        conn.query_drop(format!("ALTER CLUSTER {} ADD {}", cluster_name, table_name));
    match query_result {
        Ok(_) => {}
        Err(e) => {
            println!("{}", e);
        }
    };

    println!("Done creating tables.");

    let region_detector = RegionDetector::new();
    region_detector.debug();

    let fname = std::path::Path::new("./data/allCountries.zip");
    let file = fs::File::open(fname).unwrap();

    let mut archive = zip::ZipArchive::new(file).unwrap();
    let file = archive.by_index(0).unwrap();

    let mut rdr = ReaderBuilder::new().delimiter(b'\t').from_reader(file);

    println!("Preparing documents...");

    let documents = rdr.records().map(|result| {
        let record = result.unwrap();
        let name = record.get(1).unwrap();
        let latitude: f64 = record.get(4).unwrap().parse().unwrap();
        let longitude: f64 = record.get(5).unwrap().parse().unwrap();
        let feature_class = record.get(6).unwrap();
        let country_code = record.get(8).unwrap().to_lowercase();
        let population = record.get(14).unwrap();
        let region = region_detector.detect(country_code.clone(), latitude, longitude);

        if feature_class != "P" {
            return None;
        }

        let hash_base = format!("{}-{}", name, country_code);
        return Some(CityDocument {
            id: calculate_hash(&hash_base),
            city: name.to_string(),
            country_code: country_code.to_lowercase().to_string(),
            region: region.unwrap_or("".to_string()),
            lat: latitude,
            long: longitude,
            population: population.parse().unwrap(),
        });
    });

    let page_size = 2000;

    for (index, chunk) in documents.chunks(page_size).into_iter().enumerate() {
        println!("Batch {}.", index);
        if index != 0 && index * page_size % 100000 == 0 {
            println!("Done with {} documents", index * page_size);
        }
        let mut documents = chunk.filter(|doc| doc.is_some()).peekable();
        if !documents.peek().is_some() {
            continue;
        }
        let query = format!(
            "REPLACE INTO {}:{}(id,city,region,lat,long,country_code,population) VALUES {};",
            cluster_name,
            table_name,
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
