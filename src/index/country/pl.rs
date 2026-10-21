/// A Polish road as the index may name it: without "ul." (ulica), which most sources leave
/// out, and without its given names' initials ("ul. M. Kopernika" is Mikołaja Kopernika's
/// street). Without a stated country, only a road written with "ul." is taken to be Polish.
pub(crate) fn bare_road(road: &str, cc: Option<&str>) -> Option<String> {
    let initial = |w: &str| {
        w.strip_suffix('.')
            .is_some_and(|l| l.chars().count() == 1 && l.chars().all(char::is_alphabetic))
    };
    let mut words: Vec<&str> = road.split_whitespace().collect();
    let ulica = words
        .first()
        .is_some_and(|w| matches!(w.to_lowercase().as_str(), "ul." | "ul" | "ulica"));
    if !cc.map_or(ulica, |c| c.eq_ignore_ascii_case("pl")) {
        return None;
    }
    let before = words.len();
    if ulica {
        words.remove(0);
    }
    words.retain(|w| !initial(w));
    (words.len() < before
        && words
            .iter()
            .any(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 3))
    .then(|| words.join(" "))
}
