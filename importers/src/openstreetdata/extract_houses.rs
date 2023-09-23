use crate::{
    client::OpenGeocodingApiClient, config::Config,
    openstreetdata::extract_houses_country::extract_file, wof::zone_detector::ZoneDetector,
};

use super::files_list;

pub async fn extract_houses() {
    let countries_files = files_list::get_osd_country_files();

    let region_detector = ZoneDetector::new_region_detector().await;

    let mut client = OpenGeocodingApiClient::new().await.unwrap();

    for country_file in countries_files {
        match country_file.houses {
            Some(house_file) => {
                extract_file(&mut client, &house_file, Some(&region_detector)).await;
            }
            None => {
                continue;
            }
        }
    }
}
