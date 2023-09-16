use crate::{
    client::OpenGeocodingApiClient, config::Config,
    openstreetdata::extract_houses_country::extract_file, wof::zone_detector::ZoneDetector,
};

use super::files_list;

pub async fn extract_houses() {
    let countries_files = files_list::get_osd_country_files();
    let config = Config::new();
    let table_name = "openstreetdata_houses";
    let full_table_name = config.get_table_name(table_name.to_string());

    let region_detector = ZoneDetector::new_region_detector().await;

    let mut client = OpenGeocodingApiClient::new().await.unwrap();

    println!("Creating tables...");
    let query_result = client.run_query(format!("CREATE TABLE IF NOT EXISTS {}(street text, number text, unit text, city text, district text, region text, postcode text, lat float, long float, country_code string)  rt_mem_limit = '1G'", table_name)).await;
    match query_result {
        Ok(_) => {}
        Err(e) => {
            panic!("{}", e);
        }
    };
    if config.manticore_is_cluster {
        let query_result = client
            .run_query(format!(
                "ALTER CLUSTER {} ADD {}",
                config.manticore_cluster_name, table_name
            ))
            .await;
        match query_result {
            Ok(_) => {}
            Err(e) => {
                println!("{}", e);
            }
        };
    }

    let start_from: Option<&str> = None;
    // let start_from = Some("de");
    let mut has_started = false;
    for country_file in countries_files {
        if start_from.is_some() && !has_started {
            if country_file.country_code == start_from.unwrap() {
                has_started = true;
            } else {
                continue;
            }
        }

        match country_file.houses {
            Some(house_file) => {
                extract_file(
                    &mut client,
                    &house_file,
                    Some(&region_detector),
                    &full_table_name,
                )
                .await;
            }
            None => {
                continue;
            }
        }
    }
}
