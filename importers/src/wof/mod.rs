pub mod country_detector;
pub mod zone_detector;

pub fn detect_zones(
    lat: f64,
    long: f64,
    country_detector: Option<&country_detector::CountryDetector>,
    region_detector: Option<&zone_detector::ZoneDetector>,
    locality_detector: Option<&zone_detector::ZoneDetector>,
) -> (String, String, String) {
    let mut country_code = "".to_string();
    if country_detector.is_some() {
        country_code = country_detector
            .unwrap()
            .detect(lat, long)
            .unwrap_or("".to_string())
            .to_string()
    }
    let mut region = "".to_string();
    let mut locality = "".to_string();
    if country_code != "" {
        if region_detector.is_some() {
            region = region_detector
                .unwrap()
                .detect(country_code.to_string(), lat, long)
                .unwrap_or("".to_string());
        }
        if locality_detector.is_some() {
            locality = locality_detector
                .unwrap()
                .detect(country_code.to_string(), lat, long)
                .unwrap_or("".to_string());
        }
    }

    return (country_code, region, locality);
}
