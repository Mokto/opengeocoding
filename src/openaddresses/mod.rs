use flate2::read::GzDecoder;
use std::io::prelude::*;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;

#[derive(Serialize, Deserialize, Debug)]
struct GeoPointGeometry {
    r#type: String,
    coordinates: Vec<f64>,
}
#[derive(Serialize, Deserialize, Debug)]
struct GeoPointProperties {
    street: String,
    number: String,
    unit: String,
    city: String,
    district: String,
    region: String,
    postcode: String,
}
#[derive(Serialize, Deserialize, Debug)]
struct GeoPoint {
    r#type: String,
    properties: GeoPointProperties,
    geometry: GeoPointGeometry,
}

#[derive(Serialize, Deserialize, Debug)]
struct Document {
    // index: String,
    id: u64,
    street: String,
    number: String,
    unit: String,
    city: String,
    district: String,
    region: String,
    postcode: String,
    lat: f64,
    long: f64,
}

pub async fn extract_openaddresses() {
    println!("Opening file...");
    let file = File::open("data/denmark.geojson.gz").unwrap();
    let reader = BufReader::new(file);
    let mut d = GzDecoder::new(reader);
    let mut s = String::new();
    println!("Decompressing GZ file...");
    d.read_to_string(&mut s).unwrap();
    println!("Done. Reading lines...");

    let documents = s.lines().enumerate().map(|(index, line)| {
        let p: GeoPoint = serde_json::from_str(line).unwrap();
        return Document {
            id: index as u64,
            street: p.properties.street,
            number: p.properties.number,
            unit: p.properties.unit,
            city: p.properties.city,
            district: p.properties.district,
            region: p.properties.region,
            postcode: p.properties.postcode,
            lat: p.geometry.coordinates[1],
            long: p.geometry.coordinates[0],
        };
    });

    let page_size = 5000;

    let client = reqwest::Client::new();
    for (index, chunk) in documents.chunks(page_size).into_iter().enumerate() {
        println!("Done with {} documents", index * page_size);
        let query = format!("REPLACE INTO manticore_cluster:openaddresses(id,street,number,unit,city,district,region,postcode,lat,long) VALUES {};", chunk.map(|doc|
            format!("({},'{}','{}','{}','{}','{}','{}','{}',{},{})", doc.id, doc.street.replace("'", "''"), doc.number.replace("'", "''"), doc.unit.replace("'", "''"), doc.city.replace("'", "''"), doc.district.replace("'", "''"), doc.region.replace("'", "''"), doc.postcode.replace("'", "''"), doc.lat, doc.long)).join(", "));
        // println!("{}", query);
        let resp = client
            .post("http://localhost:9308/cli")
            .body(query)
            .send()
            .await
            .unwrap();

        if resp.status().as_u16() != 200 {
            println!("{:#?}", resp.text().await.unwrap());
        }
    }
}
