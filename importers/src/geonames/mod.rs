use crate::client::OpenGeocodingApiClient;
use crate::config::Config;
use crate::data::{calculate_hash, CityDocument};
use crate::wof::region_detector::RegionDetector;
use csv::ReaderBuilder;
use itertools::Itertools;
use rayon::prelude::*;
use std::fs;

pub async fn extract_cities() {
    let config = Config::new();
    let table_name = "geonames_cities";
    let full_table_name = config.get_table_name(table_name.to_string());

    let mut client = OpenGeocodingApiClient::new().await.unwrap();
    println!("Creating table...");

    let query_result = client.run_query(format!("CREATE TABLE IF NOT EXISTS {}(city text, region text, lat float, long float, country_code string, population int)  rt_mem_limit = '1G'", config.get_table_name(table_name.to_string()))).await;
    match query_result {
        Ok(_) => {}
        Err(e) => {
            panic!("{}", e);
        }
    };

    // if config.manticore_is_cluster {
    //     let query_result = client
    //         .run_query(format!("ALTER CLUSTER {} ADD {}", cluster_name, table_name).as_str())
    //         .await;
    //     match query_result {
    //         Ok(_) => {}
    //         Err(e) => {
    //             println!("{}", e);
    //         }
    //     };
    // }

    println!("Done creating tables.");

    let region_detector = RegionDetector::new();

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

        if feature_class != "P" {
            return None;
        }

        let hash_base = format!("{}-{}", name, country_code);
        return Some(CityDocument {
            id: calculate_hash(&hash_base),
            city: name.to_string(),
            country_code: country_code.to_lowercase().to_string(),
            region: "".to_string(), // will be calculated later
            lat: latitude,
            long: longitude,
            population: population.parse().unwrap(),
        });
    });

    let page_size = 20000;

    for (index, chunk) in documents.chunks(page_size).into_iter().enumerate() {
        // let now = Instant::now();
        if index != 0 && index * page_size % 100000 == 0 {
            println!("Done with {} documents", index * page_size);
        }
        let mut documents = chunk.filter(|doc| doc.is_some()).peekable();
        if !documents.peek().is_some() {
            continue;
        }
        let documents: Vec<String> = documents
            .into_iter()
            .collect::<Vec<Option<CityDocument>>>()
            .par_iter()
            .map(|doc| {
                let doc = doc.as_ref().unwrap();
                let region = &region_detector
                    .detect(doc.country_code.clone(), doc.lat, doc.long)
                    .unwrap_or("".to_string());
                format!(
                    r"({},'{}','{}',{},{},'{}', {})",
                    doc.id,
                    clean_string(&doc.city),
                    clean_string(region),
                    doc.lat,
                    doc.long,
                    doc.country_code,
                    doc.population,
                )
            })
            .collect();
        // println!("Elapsed: {:.2?}", now.elapsed());
        // if now.elapsed().as_millis() > 1000 {
        //     let mut countries_result: HashMap<String, u32> = HashMap::new();
        //     for document in documents {
        //         let count = countries_result.get(&document);
        //         countries_result.insert(document, count.unwrap_or(&0) + 1);
        //     }
        //     println!("{:?}", countries_result);
        // }
        let query = format!(
            "REPLACE INTO {}(id,city,region,lat,long,country_code,population) VALUES {};",
            full_table_name,
            documents.join(", ")
        );

        let query_result = client.run_background_query(query).await;

        match query_result {
            Ok(_) => {}
            Err(e) => {
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
