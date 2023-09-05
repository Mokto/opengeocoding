extern crate opengeocoding;
use opengeocoding::geonames::extract_cities;

#[tokio::main]
async fn main() {
    extract_cities().await;
}
