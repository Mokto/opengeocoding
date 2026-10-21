use regex::Regex;
use std::sync::LazyLock;

/// "Avenida Calle 43 S" as Bogotá's register writes it, "AC 43 S", and back: an avenue
/// that is also a numbered calle or carrera (AK).
pub(crate) fn bogota_road(road: &str) -> Option<String> {
    static LONG: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*(?:avenida|av\.?)\s+(calle|cl|carrera|kr|cra)\s+(.+)$").unwrap()
    });
    static SHORT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\s*(ac|ak)\s+(.+)$").unwrap());
    if let Some(c) = LONG.captures(road) {
        let kind = match c[1].to_lowercase().as_str() {
            "calle" | "cl" => "AC",
            _ => "AK",
        };
        return Some(format!("{kind} {}", &c[2]));
    }
    SHORT.captures(road).map(|c| {
        let kind = if c[1].eq_ignore_ascii_case("ac") {
            "Calle"
        } else {
            "Carrera"
        };
        format!("Avenida {kind} {}", &c[2])
    })
}

/// A Colombian plate ("# 86B-18", "54D-35 S": the house 18 m along from the crossing road 86B)
/// as Bogotá's register keys it, "86B 18" and "54D 35 S". None when it is not one.
pub(crate) fn colombian_plate(n: &str) -> Option<String> {
    static PLATE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*(?:#|no\.?)?\s*(\d+\s?[a-z]?(?:\s?bis)?)\s*-\s*(\d+)(\s+[a-z]+)?\s*$")
            .unwrap()
    });
    let c = PLATE.captures(n)?;
    Some(format!(
        "{} {}{}",
        c[1].replace(' ', ""),
        &c[2],
        c.get(3).map_or("", |m| m.as_str())
    ))
}
