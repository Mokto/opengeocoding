use crate::{
    client::OpenGeocodingApiClient,
    config::Config,
    openstreetmap::{extract_file, file_list},
};

pub async fn extract_all() {
    let config = Config::new();
    let table_name_addresses = "openstreetmap_addresses";
    let full_table_name_addresses = config.get_table_name(table_name_addresses.to_string());
    let table_name_streets = "openstreetmap_streets";
    let full_table_name_streets = config.get_table_name(table_name_streets.to_string());

    // let country_detector = CountryDetector::new().await;
    // let region_detector = ZoneDetector::new_region_detector().await;
    // let locality_detector = ZoneDetector::new_locality_detector().await;

    let mut client = OpenGeocodingApiClient::new().await.unwrap();

    println!("Creating tables...");
    // let query_result = client.run_query(format!("CREATE TABLE IF NOT EXISTS {}(street text, number text, unit text, city text, district text, region text, postcode text, lat float, long float, country_code string)  rt_mem_limit = '1G'", table_name_addresses)).await;
    // match query_result {
    //     Ok(_) => {}
    //     Err(e) => {
    //         panic!("{}", e);
    //     }
    // };
    // if config.manticore_is_cluster {
    //     let query_result = client
    //         .run_query(format!(
    //             "ALTER CLUSTER {} ADD {}",
    //             config.manticore_cluster_name, table_name_addresses
    //         ))
    //         .await;
    //     match query_result {
    //         Ok(_) => {}
    //         Err(e) => {
    //             println!("{}", e);
    //         }
    //     };
    // }

    // let query_result = client.run_query(format!("CREATE TABLE IF NOT EXISTS {}(street text, city text, region text, postcode text, lat float, long float, country_code string, points json)  rt_mem_limit = '1G'", table_name_streets)).await;
    // match query_result {
    //     Ok(_) => {}
    //     Err(e) => {
    //         panic!("{}", e);
    //     }
    // };
    // if config.manticore_is_cluster {
    //     let query_result = client
    //         .run_query(format!(
    //             "ALTER CLUSTER {} ADD {}",
    //             config.manticore_cluster_name, table_name_streets
    //         ))
    //         .await;
    //     match query_result {
    //         Ok(_) => {}
    //         Err(e) => {
    //             println!("{}", e);
    //         }
    //     };
    // }

    // let country_files = file_list::get_osm_country_files();

    // for country_file in country_files.into_iter() {
    //     extract_file::extract_file(
    //         &mut client,
    //         country_file,
    //         None,
    //         None,
    //         None,
    //         &full_table_name_addresses,
    //         &full_table_name_streets,
    //     )
    //     .await;
    // }
}
