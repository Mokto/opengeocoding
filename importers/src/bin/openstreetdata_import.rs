extern crate opengeocoding;
use opengeocoding::openstreetdata::extract_houses;

#[tokio::main]
async fn main() {
    extract_houses::extract_houses().await;
}
