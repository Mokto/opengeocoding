use crate::index::schema::fold;

/// Words that `addr:city` puts before an Egyptian town to name its administrative unit, folded:
/// markaz (district), qism and hayy (ward), first, second and third (qism), city and village.
const UNIT_WORDS: &[&str] = &[
    "مركز",
    "قسم",
    "حي",
    "اول",
    "ثان",
    "ثاني",
    "ثالث",
    "مدينه",
    "قريه",
];

/// The town without the unit words in front ("مركز منفلوط" is Manfalut, "حى ثان الاسماعيليه"
/// Ismailia), when a word remains.
pub(crate) fn bare_town(part: &str) -> Option<String> {
    let words: Vec<&str> = part.split_whitespace().collect();
    let units = words
        .iter()
        .take_while(|w| UNIT_WORDS.contains(&fold(w).as_str()))
        .count();
    (units > 0 && units < words.len()).then(|| words[units..].join(" "))
}

#[cfg(test)]
mod tests {
    #[test]
    fn bare_town_drops_unit_words_only() {
        let bare = super::bare_town;
        assert_eq!(bare("مركز منفلوط").as_deref(), Some("منفلوط"));
        assert_eq!(bare("حى ثان الاسماعيليه").as_deref(), Some("الاسماعيليه"));
        assert_eq!(bare("أول طنطا").as_deref(), Some("طنطا"));
        assert_eq!(bare("مدينة بدر").as_deref(), Some("بدر"));
        assert_eq!(bare("مركز"), None);
        assert_eq!(bare("منفلوط"), None);
        assert_eq!(bare("Cairo"), None);
    }
}
