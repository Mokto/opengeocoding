extern crate opengeocoding;

use axum::{routing::get, Router};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    // https://www.fpcomplete.com/bslog/axum-hyper-tonic-tower-part4/
    let app = Router::new().route("/", get(root));

    println!("Listening on 8090");
    let addr = SocketAddr::from(([127, 0, 0, 1], 8090));

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn root() -> String {
    let client = reqwest::Client::new();
    let query = r#"SELECT * FROM openaddresses WHERE MATCH('"Geislersgade 1, 3mf Copenhagen"/0.62') limit 5"#;
    let resp = client
        .post("http://localhost:9308/sql")
        .query(&[("query", query)])
        .body(query)
        .send()
        .await
        .unwrap();

    // if resp.status().as_u16() != 200 {
    //     println!("{:#?}", resp.text().await.unwrap());
    // }

    return resp.text().await.unwrap();
}
