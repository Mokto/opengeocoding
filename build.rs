use std::{collections::HashSet, path::PathBuf};

/// Hashes what decides an index's contents: the builder's code, the schema, analyzer and
/// reverse-lookup layout it shares with the service, and the locked versions of the crates that write or fold it. The chart pins
/// the result, so a change here is a visible chart bump that triggers a rebuild.
/// fetch.rs and store.rs are left out: they only move sources into the store, and an index is
/// built from sealed source directories, which are reused as they are.
fn builder_hash() -> String {
    let mut files: Vec<PathBuf> = std::fs::read_dir("src/bin/build_index")
        .expect("src/bin/build_index")
        .map(|e| e.expect("builder source").path())
        .filter(|p| !p.ends_with("fetch.rs") && !p.ends_with("store.rs"))
        .chain(
            [
                "src/index/schema.rs",
                "src/index/canon.rs",
                "src/index/reverse.rs",
            ]
            .map(PathBuf::from),
        )
        .collect();
    files.sort();
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    for f in &files {
        println!("cargo:rerun-if-changed={}", f.display());
        ctx.update(f.to_string_lossy().as_bytes());
        ctx.update(&std::fs::read(f).expect("builder source"));
    }
    println!("cargo:rerun-if-changed=src/bin/build_index");
    println!("cargo:rerun-if-changed=Cargo.lock");
    let lock = std::fs::read_to_string("Cargo.lock").expect("Cargo.lock");
    for package in lock.split("[[package]]") {
        let name = package.lines().find_map(|l| l.strip_prefix("name = "));
        if name.is_some_and(|n| n.starts_with("\"tantivy") || n == "\"deunicode\"") {
            let version = package.lines().find(|l| l.starts_with("version = "));
            ctx.update(format!("{name:?} {version:?}").as_bytes());
        }
    }
    ctx.finish()
        .as_ref()
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn main() {
    println!("cargo:rustc-env=BUILDER_HASH={}", builder_hash());
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
        .files(&["opengeocoding-v1.proto"])
        .include_file("_connectrpc.rs")
        .compile()
        .expect("generate Connect contract");
    println!("cargo:rerun-if-changed=proto");
    println!("cargo:rerun-if-changed=buf.yaml");
    println!("cargo:rerun-if-changed={source}");
    println!("cargo:rerun-if-env-changed=REFERENCE_DATA");
}
