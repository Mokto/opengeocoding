use crate::{
    proto::geolocation::{ISO3166_2, Region},
    resolver::Resolver,
};
use anyhow::{Context, Result, ensure};
use reqwest::Client;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::RwLock,
    time::Duration,
};
use tokio::{io::AsyncWriteExt, sync::Mutex};

pub struct Regions {
    client: Client,
    url: String,
    user: String,
    password: String,
    table: String,
    database: String,
    single_node: bool,
    cache: RwLock<HashMap<(String, String), Region>>,
    map_path: PathBuf,
    refresh: Mutex<()>,
}

fn identifier(value: &str) -> Result<&str> {
    ensure!(
        !value.is_empty()
            && value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_'),
        "invalid ClickHouse identifier"
    );
    Ok(value)
}

impl Regions {
    pub fn from_env() -> Result<Self> {
        let database =
            std::env::var("CLICKHOUSE_DATABASE").unwrap_or_else(|_| "geolocation".into());
        identifier(&database)?;
        let prefix = std::env::var("CLICKHOUSE_PREFIX").unwrap_or_default();
        let table = if prefix.is_empty() {
            "administrative_levels".into()
        } else {
            format!("{}_administrative_levels", identifier(&prefix)?)
        };
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(120))
                .connect_timeout(Duration::from_secs(60))
                .read_timeout(Duration::from_secs(60))
                .build()?,
            url: std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://127.0.0.1:8123".into()),
            user: std::env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".into()),
            password: std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_default(),
            table: format!("{database}.{table}"),
            database,
            single_node: std::env::var("CLICKHOUSE_SINGLE_NODE").map_or(true, |s| !s.is_empty()),
            cache: RwLock::new(HashMap::new()),
            map_path: std::env::var_os("REGION_MAP_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|| "regions.json".into()),
            refresh: Mutex::new(()),
        })
    }

    async fn execute(&self, query: &str, params: &[(&str, String)]) -> Result<String> {
        for attempt in 0..=4 {
            let result = async {
                let response = self
                    .client
                    .post(&self.url)
                    .basic_auth(&self.user, Some(&self.password))
                    .query(&[("query", query)])
                    .query(params)
                    .header(reqwest::header::CONTENT_LENGTH, "0")
                    .send()
                    .await?;
                let status = response.status();
                let text = response.text().await?;
                ensure!(
                    status.is_success(),
                    "ClickHouse returned {status}: {}",
                    text.chars().take(500).collect::<String>()
                );
                Ok(text)
            }
            .await;
            match result {
                Ok(text) => return Ok(text),
                Err(error) if attempt < 4 => {
                    tracing::warn!(attempt = attempt + 1, %error, "Retrying ClickHouse");
                    tokio::time::sleep(Duration::from_millis((attempt + 1) * 500)).await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!()
    }

    pub async fn initialize(&self) -> Result<()> {
        let cluster = if self.single_node {
            ""
        } else {
            " ON CLUSTER '{cluster}'"
        };
        self.execute(
            &format!("CREATE DATABASE IF NOT EXISTS {}{cluster}", self.database),
            &[],
        )
        .await?;
        let engine = if self.single_node {
            "ReplacingMergeTree"
        } else {
            "ReplicatedReplacingMergeTree('/clickhouse/tables/{shard}/{database}/{table}', '{replica}')"
        };
        self.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {}{cluster} (
            country_code String, country_name String, level UInt8, local_name String,
            en_name String, admin_code String, updated_at Nullable(DateTime('UTC')),
            parent_admin_code String
        ) ENGINE = {engine} ORDER BY (country_code, admin_code)",
                self.table
            ),
            &[],
        )
        .await?;
        let rows = self.all().await?;
        let warmed = rows
            .into_iter()
            .filter(|row| row.admin_level <= 10)
            .map(|row| ((row.country_code.clone(), row.admin_code.clone()), row))
            .collect();
        *self
            .cache
            .write()
            .map_err(|_| anyhow::anyhow!("region cache lock poisoned"))? = warmed;
        tracing::info!(
            regions = self
                .cache
                .read()
                .map_err(|_| anyhow::anyhow!("region cache lock poisoned"))?
                .len(),
            "Region cache warmed"
        );
        Ok(())
    }

    fn select(&self) -> String {
        format!(
            "SELECT country_code, country_name, level AS admin_level, local_name, en_name, admin_code, parent_admin_code FROM {}",
            self.table
        )
    }

    async fn rows(&self, query: &str, params: &[(&str, String)]) -> Result<Vec<Region>> {
        let text = self.execute(query, params).await?;
        text.lines()
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_str(line).context("ClickHouse region row"))
            .collect()
    }

    pub async fn all(&self) -> Result<Vec<Region>> {
        self.rows(&format!("{} FORMAT JSONEachRow", self.select()), &[])
            .await
    }

    pub async fn get(&self, country: &str, code: &str) -> Result<Option<Region>> {
        let key = (country.to_owned(), code.to_owned());
        if let Some(row) = self
            .cache
            .read()
            .map_err(|_| anyhow::anyhow!("region cache lock poisoned"))?
            .get(&key)
            .cloned()
        {
            return Ok(Some(row));
        }
        let rows = self.rows(&format!("{} WHERE country_code = {{country:String}} AND admin_code = {{code:String}} LIMIT 1 FORMAT JSONEachRow", self.select()),
            &[("param_country", country.into()), ("param_code", code.into())]).await?;
        let Some(row) = rows.into_iter().next().filter(|row| row.admin_level <= 10) else {
            return Ok(None);
        };
        self.cache
            .write()
            .map_err(|_| anyhow::anyhow!("region cache lock poisoned"))?
            .insert(key, row.clone());
        Ok(Some(row))
    }

    pub async fn insert(&self, row: &Region) -> Result<()> {
        if row.admin_level > 10 {
            return Ok(());
        }
        self.execute(&format!("INSERT INTO {} (country_code, country_name, level, local_name, en_name, admin_code, parent_admin_code, updated_at)
            SELECT {{country:String}}, {{name:String}}, {{level:UInt8}}, {{local:String}}, {{en:String}}, {{code:String}}, {{parent:String}}, now('Europe/Copenhagen')", self.table),
            &[("param_country", row.country_code.clone()), ("param_name", row.country_name.clone()),
              ("param_level", row.admin_level.to_string()), ("param_local", row.local_name.clone()),
              ("param_en", row.en_name.clone()), ("param_code", row.admin_code.clone()), ("param_parent", row.parent_admin_code.clone())]).await?;
        self.cache
            .write()
            .map_err(|_| anyhow::anyhow!("region cache lock poisoned"))?
            .insert(
                (row.country_code.clone(), row.admin_code.clone()),
                row.clone(),
            );
        Ok(())
    }

    pub async fn reformat(&self, resolver: &Resolver, codes: &[ISO3166_2]) -> Result<Vec<Region>> {
        let mut sorted: Vec<_> = codes.iter().collect();
        sorted.sort_by_key(|c| c.admin_level);
        let mut previous = "";
        let mut result = vec![];
        for code in sorted {
            let mut parts = code.code.split('-');
            let country = parts.next().unwrap_or("").to_lowercase();
            let admin = parts.collect::<Vec<_>>().join("_");
            if let Some(region) = self.get(&country, &admin).await? {
                result.push(region);
            } else if let Some(mut region) =
                resolver.state(&format!("{country}, {}", code.code)).await?
            {
                region.parent_admin_code = previous.into();
                self.insert(&region).await?;
                result.push(region);
            } else {
                tracing::warn!(
                    code = code.code,
                    admin_level = code.admin_level,
                    "No region found in Nominatim"
                );
                sentry::capture_message("No region found in Nominatim", sentry::Level::Warning);
            }
            previous = &code.code;
        }
        Ok(result)
    }

    pub async fn refresh_map(&self) -> Result<()> {
        let _guard = self.refresh.lock().await;
        let value = format_regions(&self.all().await?)?;
        let bytes = serde_json::to_vec_pretty(&value)?;
        let temporary = self
            .map_path
            .with_extension(format!("tmp.{}", std::process::id()));
        let mut file = tokio::fs::File::create(&temporary).await?;
        file.write_all(&bytes).await?;
        file.sync_all().await?;
        drop(file);
        tokio::fs::rename(&temporary, &self.map_path).await?;
        Ok(())
    }

    pub async fn read_map(&self) -> Result<String> {
        let bytes = tokio::fs::read(&self.map_path).await?;
        python_repr(&serde_json::from_slice(&bytes)?)
    }
}

pub fn format_regions(regions: &[Region]) -> Result<Value> {
    let mut children: HashMap<String, Vec<&Region>> = HashMap::new();
    for region in regions
        .iter()
        .filter(|region| !region.parent_admin_code.is_empty())
    {
        children
            .entry(region.parent_admin_code.clone())
            .or_default()
            .push(region);
    }
    fn tree(
        region: &Region,
        children: &HashMap<String, Vec<&Region>>,
        visited: &mut HashSet<String>,
    ) -> Result<Value> {
        let key = format!(
            "{}-{}",
            region.country_code.to_uppercase(),
            region.admin_code
        );
        ensure!(
            visited.insert(key.clone()),
            "cycle in administrative regions"
        );
        let mut value =
            json!({"region_local_name": region.local_name, "region_english_name": region.en_name});
        if let Some(nodes) = children.get(&key) {
            for child in nodes {
                let child_tree = tree(child, children, visited)?;
                let sub = value
                    .as_object_mut()
                    .context("region object")?
                    .entry("sub_regions")
                    .or_insert_with(|| json!({}));
                let level = sub
                    .as_object_mut()
                    .context("subregion object")?
                    .entry(format!("lvl-{}", child.admin_level))
                    .or_insert_with(|| json!({}));
                level
                    .as_object_mut()
                    .context("level object")?
                    .insert(child.admin_code.clone(), child_tree);
            }
        }
        visited.remove(&key);
        Ok(value)
    }
    let mut result = json!({});
    for root in regions.iter().filter(|r| r.parent_admin_code.is_empty()) {
        let country = result
            .as_object_mut()
            .context("country map")?
            .entry(root.country_code.clone())
            .or_insert_with(|| json!({"country_name": root.country_name}));
        country.as_object_mut().context("country object")?.insert(
            root.admin_code.clone(),
            tree(root, &children, &mut HashSet::new())?,
        );
    }
    Ok(result)
}

pub fn python_repr(value: &Value) -> Result<String> {
    match value {
        Value::String(value) => {
            let quote = if value.contains('\'') && !value.contains('"') {
                '"'
            } else {
                '\''
            };
            let mut result = quote.to_string();
            for ch in value.chars() {
                match ch {
                    '\\' => result.push_str("\\\\"),
                    '\n' => result.push_str("\\n"),
                    '\r' => result.push_str("\\r"),
                    '\t' => result.push_str("\\t"),
                    ch if ch == quote => {
                        result.push('\\');
                        result.push(ch);
                    }
                    ch if !crate::matching::printable(ch) => {
                        let code = ch as u32;
                        if code < 256 {
                            result.push_str(&format!("\\x{code:02x}"));
                        } else if code <= 0xffff {
                            result.push_str(&format!("\\u{code:04x}"));
                        } else {
                            result.push_str(&format!("\\U{code:08x}"));
                        }
                    }
                    ch => result.push(ch),
                }
            }
            result.push(quote);
            Ok(result)
        }
        Value::Object(map) => Ok(format!(
            "{{{}}}",
            map.iter()
                .map(|(key, value)| {
                    Ok(format!(
                        "{}: {}",
                        python_repr(&Value::String(key.clone()))?,
                        python_repr(value)?
                    ))
                })
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        )),
        Value::Array(values) => Ok(format!(
            "[{}]",
            values
                .iter()
                .map(python_repr)
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        )),
        Value::Bool(value) => Ok(if *value { "True" } else { "False" }.into()),
        Value::Null => Ok("None".into()),
        Value::Number(value) => Ok(value.to_string()),
    }
}
