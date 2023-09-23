extern crate opengeocoding;
use opengeocoding::openstreetdata::{extract_houses, extract_streets};

#[tokio::main]
async fn main() {
    extract_houses::extract_houses().await;
    // extract_streets::extract_streets().await;
}
