use std::{collections::HashSet, path::PathBuf};

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let source = std::env::var("REFERENCE_DATA").unwrap_or_else(|_| "data/reference.json".into());
    let mut reference: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&source).expect("reference data"))
            .expect("reference JSON");
    let mut regions = serde_json::Map::new();
    let mut ambiguous = HashSet::new();
    for (name, country) in reference["region_countries"]
        .as_object()
        .expect("region names")
    {
        let key = deunicode::deunicode(name)
            .to_lowercase()
            .chars()
            .filter(|ch| ch.is_ascii_alphabetic() || "-/ ".contains(*ch))
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if key.is_empty() {
            continue;
        }
        if let Some(previous) = regions.insert(key.clone(), country.clone())
            && previous != *country
        {
            ambiguous.insert(key);
        }
    }
    regions.retain(|name, country| {
        !ambiguous.contains(name)
            && reference["countries"]
                .get(name)
                .is_none_or(|code| code == country)
    });
    reference["region_countries"] = serde_json::Value::Object(regions);
    reference
        .as_object_mut()
        .expect("reference object")
        .remove("transliteration");
    std::fs::write(
        out.join("reference.json"),
        serde_json::to_vec(&reference).expect("reference JSON"),
    )
    .expect("write reference");
    let descriptor = out.join("reflection.bin");
    let status = std::process::Command::new("buf")
        .args(["build", "--as-file-descriptor-set", "-o"])
        .arg(&descriptor)
        .status()
        .expect("buf must be installed");
    assert!(status.success(), "compile protobuf descriptors");
    connectrpc_build::Config::new()
        .descriptor_set(&descriptor)
        .files(&["nominatim_geolocation-v1.proto", "common-v1.proto"])
        .include_file("_connectrpc.rs")
        .compile()
        .expect("generate Connect contract");
    println!("cargo:rerun-if-changed=proto");
    println!("cargo:rerun-if-changed=buf.yaml");
    println!("cargo:rerun-if-changed={source}");
    println!("cargo:rerun-if-env-changed=REFERENCE_DATA");
}
