extern crate opengeocoding;

use crate::opengeocoding::openstreetmap::extract::extract_all;

#[tokio::main]
async fn main() {
    extract_all().await;
}
