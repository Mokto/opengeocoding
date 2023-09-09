use std::env;

pub struct Config {
    pub manticore_is_cluster: bool,
    manticore_cluster_name: String,
}

impl Config {
    pub fn new() -> Config {
        let cluster_name = env::var("MANTICORESEARCH_CLUSTER_NAME").unwrap_or("".to_string());
        let is_cluster = cluster_name == "";

        let table_name = "geonames_cities";
        let mut full_table_name = format!("{}:{}", cluster_name, table_name);
        if is_cluster {
            full_table_name = table_name.to_string();
        }

        return Config {
            manticore_is_cluster: is_cluster,
            manticore_cluster_name: cluster_name,
        };
    }

    pub fn get_table_name(&self, table_name: String) -> String {
        if self.manticore_is_cluster {
            return table_name.to_string();
        } else {
            return format!("{}:{}", self.manticore_cluster_name, table_name);
        }
    }
}

pub fn get_config() -> Config {
    let cluster_name = env::var("MANTICORESEARCH_CLUSTER_NAME").unwrap_or("".to_string());
    let is_cluster = cluster_name == "";

    let table_name = "geonames_cities";
    let mut full_table_name = format!("{}:{}", cluster_name, table_name);
    if is_cluster {
        full_table_name = table_name.to_string();
    }

    return Config {
        manticore_is_cluster: is_cluster,
        manticore_cluster_name: cluster_name,
    };
}
