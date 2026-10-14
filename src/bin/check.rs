use anyhow::{Context, Result};
use opengeocoding::{
    matching::*,
    proto::opengeocoding::*,
    resolver::{Resolver, format_response},
};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

#[tokio::main]
async fn main() -> Result<()> {
    let mut resolver = None;
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line?)?;
        let string = |key: &str| input[key].as_str().unwrap_or("");
        let address: ParsedAddress =
            serde_json::from_value(input.get("address").cloned().unwrap_or_else(|| json!({})))?;
        let details: Option<AddressDetails> = input
            .get("details")
            .filter(|v| !v.is_null())
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?;
        let output = match string("op") {
            "normalize" => json!(normalize_place(string("value"))),
            "postcode" => json!(postcodes_compatible(string("expected"), string("actual"))),
            "country" => json!(expected_country(&address)),
            "implied" => json!(implied_country(&address)),
            "city" => json!(city_agrees(&address, details.as_ref())),
            "region" => json!(region_agrees(&address, details.as_ref())),
            "road" => json!(road_agrees(&address, details.as_ref())),
            "eircode" => json!(eircode(string("value"))),
            "without_country" => json!(without_country(string("value"), &address)),
            "format" => serde_json::to_value(format_response(&input["value"])?)?,
            "benchmark_choose" => {
                let candidates: Vec<FullAddress> =
                    serde_json::from_value(input["candidates"].clone())?;
                let iterations = input["iterations"].as_u64().context("iterations")?;
                let start = std::time::Instant::now();
                for _ in 0..iterations {
                    std::hint::black_box(choose_candidate(
                        &address,
                        string("country"),
                        string("postcode"),
                        &candidates,
                    ));
                }
                json!(start.elapsed().as_secs_f64())
            }
            "override" => serde_json::to_value(search_override(string("value")))?,
            "query" => {
                let order: Vec<String> = serde_json::from_value(input["order"].clone())?;
                json!(create_query(
                    &address,
                    &order.iter().map(String::as_str).collect::<Vec<_>>()
                ))
            }
            "choose" => {
                let candidates =
                    serde_json::from_value::<Vec<FullAddress>>(input["candidates"].clone())?;
                match choose_candidate(&address, string("country"), string("postcode"), &candidates)
                {
                    Some((index, tier)) => json!([index, tier]),
                    None => json!([null, null]),
                }
            }
            "parse" | "resolve" => {
                if resolver.is_none() {
                    resolver = Some(Resolver::open(
                        std::env::var_os("INDEX_DIR")
                            .context("INDEX_DIR is required")?
                            .as_ref(),
                    )?);
                }
                let resolver = resolver.as_ref().context("parser initialized")?;
                if string("op") == "parse" {
                    json!(resolver.parse(string("value")).await?)
                } else {
                    serde_json::to_value(resolver.resolve(string("value")).await?)?
                }
            }
            _ => anyhow::bail!("unknown parity operation"),
        };
        println!("{}", serde_json::to_string(&output)?);
        io::stdout().flush()?;
    }
    Ok(())
}
