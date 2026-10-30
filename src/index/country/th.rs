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

/// "171 ม.8", "11 Moo 1", "225 หมู่ที่ 2", "บ้านเลขที่32 หมู่8" (a house number with its village,
/// หมู่) and "267/108-109" (a range of units): OpenStreetMap keys these houses whole, as typed,
/// so the words are one number.
pub fn house_number(run: &str) -> bool {
    let n = run.strip_prefix("บ้านเลขที่").unwrap_or(run).trim_start();
    let len = n
        .find(|c: char| !(c.is_ascii_digit() || c == '/' || c == '-'))
        .unwrap_or(n.len());
    let (number, rest) = n.split_at(len);
    if !number.starts_with(|c: char| c.is_ascii_digit()) {
        return false;
    }
    if rest.is_empty() {
        return number
            .find('/')
            .is_some_and(|at| number[at..].contains('-'));
    }
    let rest = rest.trim_start().to_lowercase();
    let village = ["หมู่ที่", "หมู่", "ม.", "moo"]
        .iter()
        .find_map(|m| rest.strip_prefix(m))
        .map(str::trim_start);
    village.is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_village_and_the_range_as_the_number() {
        for run in [
            "171 ม.8",
            "11 Moo 1",
            "367 หมู่ 1",
            "225 หมู่ที่ 2",
            "159 หมู่8",
            "บ้านเลขที่32 หมู่8",
            "267/108-109",
        ] {
            assert!(super::house_number(run), "{run}");
        }
        for run in [
            "171",
            "267/108",
            "12-14",
            "หมู่ 1",
            "171 ม.",
            "6 367 หมู่ 1",
            "11 Moo",
        ] {
            assert!(!super::house_number(run), "{run}");
        }
    }

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
