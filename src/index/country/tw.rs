/// `query` without a part that is Taiwan Province (台灣省), a province in name only: no place of
/// the index has it, and the parser can take it for the town. None when no part is.
pub fn without_taiwan_province(query: &str) -> Option<String> {
    let parts: Vec<&str> = query.split(',').map(str::trim).collect();
    let province = |p: &&str| {
        matches!(
            p.to_lowercase().as_str(),
            "台灣省" | "臺灣省" | "台湾省" | "taiwan province"
        )
    };
    let kept: Vec<&str> = parts.iter().filter(|p| !province(p)).copied().collect();
    (kept.len() < parts.len() && !kept.is_empty()).then(|| kept.join(", "))
}
