pub mod openaddresses;

use openaddresses::extract_openaddresses;
// use storage::run_clickhouse;

#[tokio::main]
async fn main() {
    // storage::run_clickhouse().await.unwrap();
    extract_openaddresses().await;
}
