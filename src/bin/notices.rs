//! Regenerates THIRD_PARTY_NOTICES.txt from the locked Cargo dependency graph:
//! `cargo run --bin notices`. Run after dependency changes; never edit the output by hand.

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Line boundaries as Unicode defines them; licence texts contain form feeds and similar separators.
fn line_break(ch: char) -> bool {
    matches!(
        ch,
        '\n' | '\r' | '\u{0b}' | '\u{0c}' | '\u{1c}'
            ..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

fn blank(ch: char) -> bool {
    ch.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&ch)
}

/// Splits on every line boundary (treating \r\n as one) and trims trailing whitespace per line.
fn normalize(text: &str) -> String {
    let mut lines = vec![];
    let mut rest = text;
    while !rest.is_empty() {
        let end = rest.find(line_break).unwrap_or(rest.len());
        lines.push(rest[..end].trim_end_matches(blank));
        rest = &rest[end..];
        if let Some(stripped) = rest.strip_prefix("\r\n") {
            rest = stripped;
        } else if let Some(ch) = rest.chars().next() {
            rest = &rest[ch.len_utf8()..];
        }
    }
    lines.join("\n").trim_matches(blank).to_string() + "\n"
}

fn notice_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<_> = std::fs::read_dir(directory)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.is_file()
                && path.file_name().is_some_and(|name| {
                    let name = name.to_string_lossy().to_uppercase();
                    ["LICENSE", "LICENCE", "COPYING", "NOTICE"]
                        .iter()
                        .any(|p| name.starts_with(p))
                })
        })
        .collect();
    files.sort();
    Ok(files)
}

fn field(package: &Value, key: &str) -> String {
    package[key].as_str().map_or("None".into(), String::from)
}

fn main() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output =
        std::process::Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .args(["metadata", "--locked", "--format-version", "1"])
            .current_dir(root)
            .output()
            .context("run cargo metadata")?;
    anyhow::ensure!(
        output.status.success(),
        "cargo metadata: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout)?;
    let mut packages: Vec<_> = metadata["packages"]
        .as_array()
        .context("packages")?
        .iter()
        .collect();
    packages.sort_by_key(|p| (field(p, "name"), field(p, "version")));
    let (mut sections, mut missing, mut licenses) =
        (vec![], vec![], std::collections::BTreeSet::new());
    for package in packages {
        licenses.insert(
            package["license"]
                .as_str()
                .unwrap_or("UNSPECIFIED")
                .to_string(),
        );
        let name = field(package, "name");
        if name == env!("CARGO_PKG_NAME") {
            continue;
        }
        let directory = Path::new(package["manifest_path"].as_str().context("manifest_path")?)
            .parent()
            .context("package directory")?;
        let files = notice_files(directory)?;
        let texts: Vec<_> = files
            .iter()
            .map(|path| {
                std::fs::read(path).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            })
            .collect::<Result<_, _>>()?;
        sections.push(format!(
            "{name} {}\nLicense: {}\nRepository: {}\n{}",
            field(package, "version"),
            field(package, "license"),
            field(package, "repository"),
            texts.join("\n")
        ));
        if files.is_empty() {
            missing.push(name);
        }
    }
    let separator = format!("\n\n{}\n\n", "=".repeat(72));
    std::fs::write(
        root.join("THIRD_PARTY_NOTICES.txt"),
        normalize(&sections.join(&separator)),
    )?;
    println!("Wrote notices for {} Cargo packages", sections.len());
    println!(
        "Packages without top-level notice files: {}",
        if missing.is_empty() {
            "none".into()
        } else {
            missing.join(", ")
        }
    );
    println!("License expressions: {licenses:?}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn licence_text_normalization() {
        assert_eq!(normalize("a  \r\nb\u{0c}c \n\n"), "a\nb\nc\n");
        assert_eq!(normalize("  x\u{2028}y"), "x\ny\n");
    }
}
