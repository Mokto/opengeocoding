/// "62 ถนนนิคมบูรพา, สตึก, บุรีรัมย์, 31150, Thailand" as "62 ถนนนิคมบูรพา, 31150, Thailand": the
/// road, then the postcode and what follows it, without the places between. Thai addresses name
/// their district and province in forms the index files under other names (a province with or
/// without จังหวัด, a district in English), while a Thai postcode covers about one district.
/// None without a 5-digit postcode after at least one place.
pub fn road_and_postcode(text: &str) -> Option<String> {
    let parts: Vec<&str> = text.split(',').map(str::trim).collect();
    let at = parts
        .iter()
        .position(|p| p.len() == 5 && p.bytes().all(|b| b.is_ascii_digit()))?;
    (at >= 2).then(|| [&parts[..1], &parts[at..]].concat().join(", "))
}

#[cfg(test)]
mod tests {
    #[test]
    fn keeps_the_road_and_the_postcode() {
        for (text, want) in [
            (
                "62 ถนนนิคมบูรพา, สตึก, บุรีรัมย์, 31150, Thailand",
                Some("62 ถนนนิคมบูรพา, 31150, Thailand"),
            ),
            (
                "206/4 ลาดยาว, Lat Yao District, จังหวัดนครสวรรค์, 60150, Thailand",
                Some("206/4 ลาดยาว, 60150, Thailand"),
            ),
            ("62 ถนนนิคมบูรพา, 31150, Thailand", None),
            ("62 ถนนนิคมบูรพา, สตึก, Thailand", None),
            ("62 ถนนนิคมบูรพา, สตึก, 3115, Thailand", None),
        ] {
            assert_eq!(super::road_and_postcode(text).as_deref(), want, "{text}");
        }
    }
}
