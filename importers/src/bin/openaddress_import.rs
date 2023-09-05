extern crate opengeocoding;
use opengeocoding::openaddresses::import_addresses;

#[tokio::main]
async fn main() {
    import_addresses().await;
}
