use itertools::Itertools;
use mysql::prelude::*;
use mysql::*;
use serde::{Deserialize, Serialize};
use std::io::prelude::*;
use std::{env, fs, vec};

use crate::data::{calculate_hash, AddressDocument};

#[derive(Serialize, Deserialize)]
pub struct GeoPointGeometry {
    pub r#type: String,
    pub coordinates: Vec<f64>,
}
#[derive(Serialize, Deserialize, Hash)]
pub struct GeoPointProperties {
    pub street: Option<String>,
    pub number: Option<String>,
    pub unit: Option<String>,
    pub city: Option<String>,
    pub district: Option<String>,
    pub region: Option<String>,
    pub postcode: Option<String>,
}
#[derive(Serialize, Deserialize)]
pub struct GeoPoint {
    pub r#type: String,
    pub properties: GeoPointProperties,
    pub geometry: Option<GeoPointGeometry>,
}

pub async fn import_addresses() {
    let table_name = "openaddresses";
    let cluster_name = "opengeocoding_cluster";
    let url = format!(
        "mysql://root:password@{}:9306/default",
        env::var("MANTICORESEARCH_ENDPOINT").unwrap_or("localhost".to_string())
    );
    println!("Creating table...");
    let pool = Pool::new(Opts::from_url(url.as_str()).unwrap()).unwrap();
    let mut conn: PooledConn = pool.get_conn().unwrap();

    let query_result = conn.query_drop(format!("CREATE TABLE IF NOT EXISTS {}(street text, number text, unit text, city text, district text, region text, postcode text, lat float, long float, country_code string)  rt_mem_limit = '1G'", table_name));
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

    let fname = std::path::Path::new("data/collection-global.zip");
    let file = fs::File::open(fname).unwrap();

    let mut archive = zip::ZipArchive::new(file).unwrap();

    // let start_from: Option<&str> = None;
    let start_from = Some("za/countrywide-addresses-country.geojson");

    let exclude_files: Vec<String> = vec![];

    let mut has_started = false;
    'outer: for i in (0..archive.len()).rev() {
        let mut file = archive.by_index(i).unwrap();
        let outpath = match file.enclosed_name() {
            Some(path) => path.to_owned(),
            None => continue,
        };

        if start_from.is_some() && !has_started {
            if outpath.to_str().unwrap() == start_from.unwrap() {
                has_started = true;
            } else {
                continue;
            }
        }

        let to_exclude_regexes = vec![
            "-parcels-county.geojson",
            "-parcels-city.geojson",
            "-parcels-state.geojson",
            "-parcels-town.geojson",
            "-parcels-country.geojson",
            "-parcels-province.geojson",
            "-buildings-county.geojson",
            "-buildings-city.geojson",
            "-buildings-state.geojson",
            "-buildings-town.geojson",
            "-buildings-territory.geojson",
            "-buildings-country.geojson",
            "-buildings-region.geojson",
        ];

        if outpath.file_name().is_some()
            && outpath.extension().unwrap().to_str().unwrap() == "geojson"
        {
            let file_name = outpath.to_str().unwrap().to_string();
            if exclude_files.contains(&file_name) {
                println!("Filename excluded: {}", outpath.display());
                continue;
            }

            for regex in to_exclude_regexes.iter() {
                if file_name.contains(regex) {
                    println!("Filename excluded ({}): {}", regex, outpath.display());
                    continue 'outer;
                }
            }

            println!("Filename: {} {}", outpath.display(), file.size());
            let mut contents = String::new();
            file.read_to_string(&mut contents).unwrap();

            let country_code = file_name.split("/").next().unwrap().to_string();

            string_to_db(contents, country_code, file_name, &mut conn).await;
        };
    }
}

async fn string_to_db(
    content: String,
    country_code: String,
    file_name: String,
    conn: &mut PooledConn,
) {
    let table_name = "openaddresses";
    let cluster_name = "opengeocoding_cluster";
    let documents = content.lines().map(|line| {
        let p: GeoPoint = serde_json::from_str(line).unwrap();

        if p.geometry.is_none() {
            return None;
        }
        let geometry = p.geometry.unwrap();
        if geometry.r#type != "Point" {
            return None;
        }
        return Some(AddressDocument {
            id: calculate_hash(&p.properties),
            street: p.properties.street.unwrap_or("".to_string()),
            number: p.properties.number.unwrap_or("".to_string()),
            unit: p.properties.unit.unwrap_or("".to_string()),
            city: p.properties.city.unwrap_or("".to_string()),
            district: p.properties.district.unwrap_or("".to_string()),
            region: p.properties.region.unwrap_or("".to_string()),
            postcode: p.properties.postcode.unwrap_or("".to_string()),
            lat: geometry.coordinates[1],
            long: geometry.coordinates[0],
        });
    });

    let page_size = 20000;

    for (index, chunk) in documents.chunks(page_size).into_iter().enumerate() {
        if index != 0 && index * page_size % 100000 == 0 {
            println!("Done with {} documents", index * page_size);
        }
        let mut documents = chunk.filter(|doc| doc.is_some()).peekable();
        if !documents.peek().is_some() {
            continue;
        }
        let query = format!("REPLACE INTO {}:{}(id,street,number,unit,city,district,region,postcode,lat,long,country_code) VALUES {};", cluster_name, table_name, documents.map(|doc|
            {
                let doc = doc.as_ref().unwrap();
                return format!(r"({},'{}','{}','{}','{}','{}','{}','{}',{},{}, '{}')", doc.id, clean_string(&doc.street), clean_string(&doc.number), clean_string(&doc.unit), clean_string(&doc.city), clean_string(&doc.district), clean_string(&doc.region), clean_string(&doc.postcode), doc.lat, doc.long, country_code)
            }
        ).join(", "));

        let query_result = conn.query_drop(&query);

        match query_result {
            Ok(_) => {}
            Err(e) => {
                let query_result = conn.query_drop(&query);

                match query_result {
                    Ok(_) => {}
                    Err(e) => {
                        println!("Query: {}", query);
                        println!("Error: {}", e);
                        println!("File name: {}", file_name);
                        panic!("Error running SQL");
                    }
                };
            }
        };
    }
    println!("Done with batch");
}

fn clean_string(s: &str) -> String {
    return s.replace(r"\", r"\\").replace("'", r"\'");
}
