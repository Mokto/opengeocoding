use crate::{
    client::OpenGeocodingApiClient, config::Config,
    openstreetdata::extract_streets_country::extract_file, wof::zone_detector::ZoneDetector,
};

use super::files_list;

pub async fn extract_streets() {
    let streets_files = files_list::get_osd_country_files();
    let config = Config::new();
    let table_name = "openstreetdata_streets";
    let full_table_name = config.get_table_name(table_name.to_string());

    let region_detector = ZoneDetector::new_region_detector().await;

    let mut client = OpenGeocodingApiClient::new().await.unwrap();

    println!("Creating tables...");
    let query_result = client.run_query(format!("CREATE TABLE IF NOT EXISTS {}(street text, city text, district text, region text, postcode text, lat_min float, long_min float, lat_max float, long_max float, house_min string, house_max string, house_odd bool, house_even bool, country_code string)  rt_mem_limit = '1G'", table_name)).await;
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

    for country_file in streets_files {
        match country_file.addresses {
            Some(house_file) => {
                extract_file(
                    &mut client,
                    &house_file,
                    Some(&region_detector),
                    &full_table_name,
                    &country_file.country_code,
                )
                .await;
            }
            None => {
                continue;
            }
        }
    }
}
