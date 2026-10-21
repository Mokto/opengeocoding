use crate::index::{place::Place, schema::fold};

/// Within this of a town's point, a county is one of its wards: "ثان المنصوره" (Mansoura's
/// second qism) holds the east of the city, whose point lies in the first.
const WARD_KM: f64 = 5.0;

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

/// Egypt's counties: its markaz and qism districts.
pub(in crate::index) fn counties(places: &[Place]) -> Vec<u32> {
    (0..places.len() as u32)
        .filter(|&i| {
            let p = &places[i as usize];
            p.cc.eq_ignore_ascii_case("eg") && &*p.subtype == "county"
        })
        .collect()
}

/// The counties the houses of the town that starts `chain` may be filed under: the markaz or
/// qism it lies in and the wards around it. Overture puts no town above Egypt's houses, only
/// these counties, which it names in English alone ("Adfu" is the markaz of إدفو).
pub(in crate::index) fn town_counties(
    places: &[Place],
    counties: &[u32],
    chain: &[u32],
) -> Vec<u32> {
    let town = &places[chain[0] as usize];
    let cos = (town.y as f64).to_radians().cos();
    let near =
        |p: &Place| ((p.x - town.x) as f64 * cos).hypot((p.y - town.y) as f64) * 111.195 <= WARD_KM;
    chain
        .iter()
        .copied()
        .filter(|&c| &*places[c as usize].subtype == "county")
        .chain(
            counties
                .iter()
                .copied()
                .filter(|&c| near(&places[c as usize])),
        )
        .collect()
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
