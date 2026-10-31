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

pub fn thailand(country: &str) -> bool {
    matches!(country, "thailand" | "ประเทศไทย" | "ไทย")
}

/// "132 หมู่ที่ 2 ชลประทาน": a house number with its village ([`house_number`]), then the road.
/// libpostal takes the village word for a town and the rest for a house.
pub fn village_head(head: &str) -> Option<(String, String)> {
    let words: Vec<&str> = head.split_whitespace().collect();
    (2..words.len().min(5)).rev().find_map(|n| {
        let number = words[..n].join(" ");
        house_number(&number).then(|| (number, words[n..].join(" ")))
    })
}

fn lane(word: &str) -> bool {
    word.starts_with("ซอย") || word.starts_with("ซ.")
}

/// "18/8 ซอยเกอเธ่ ซอยสาทร 1 ถนนสาทรใต้": a house on a lane (ซอย, here with two names, the second
/// numbered) off a road (ถนน). libpostal reads two houses and two roads, and the later of each
/// would stand: the house 1 of the road. The lane's number goes back to the lane, which is the
/// house's street under its first name.
pub fn lane_as_road(q: &str, parsed: &mut Vec<(String, String)>) {
    let words: Vec<&str> = q.split([',', ' ']).filter(|w| !w.is_empty()).collect();
    let numbers_lane = |n: &str| words.windows(2).any(|w| w[1] == n && lane(w[0]));
    let mut at = 1;
    while at < parsed.len() {
        if parsed[at].0 == "house_number"
            && parsed[at - 1].0 == "road"
            && parsed[at - 1].1.split(' ').next_back().is_some_and(lane)
            && parsed[..at - 1].iter().any(|(k, _)| k == "house_number")
            && numbers_lane(&parsed[at].1)
        {
            let (_, number) = parsed.remove(at);
            parsed[at - 1].1.push(' ');
            parsed[at - 1].1.push_str(&number);
        } else {
            at += 1;
        }
    }
    if parsed.iter().filter(|(k, _)| k == "road").count() < 2 {
        return;
    }
    let Some(at) = parsed.iter().position(|(k, v)| k == "road" && lane(v)) else {
        return;
    };
    let name = &parsed[at].1;
    let end = name
        .match_indices(' ')
        .find(|&(i, _)| lane(&name[i + 1..]))
        .map_or(name.len(), |(i, _)| i);
    parsed[at].1.truncate(end);
    let mut index = 0;
    parsed.retain(|(k, _)| {
        index += 1;
        k != "road" || index - 1 == at
    });
}

/// A Thai vowel or tone mark written above or below its consonant, which no word starts with.
fn combining(c: char) -> bool {
    matches!(c, '\u{e31}' | '\u{e34}'..='\u{e3a}' | '\u{e47}'..='\u{e4e}')
}

/// "47 ถนนมาลัยแมน, อำเภออู่ทอง, จังหวัดสุพรรณบุรี": libpostal can cut a Thai word before a vowel
/// mark (a road "ถนนมาลัยแมน อำเภออ" and a town "ู่ทอง"). The word cut short goes back to the field
/// after it, when the query has them joined.
pub fn rejoin_cut_words(q: &str, parsed: &mut Vec<(String, String)>) {
    let mut at = 1;
    while at < parsed.len() {
        let (before, after) = parsed.split_at_mut(at);
        let (prev, value) = (&mut before[at - 1].1, &mut after[0].1);
        if value.starts_with(combining) {
            let cut = prev.rfind(' ').map_or(0, |i| i + 1);
            let joined = format!("{}{value}", &prev[cut..]);
            if cut < prev.len() && q.to_lowercase().contains(&joined) {
                *value = joined;
                prev.truncate(cut.saturating_sub(1));
                if prev.is_empty() {
                    parsed.remove(at - 1);
                    continue;
                }
            }
        }
        at += 1;
    }
}

#[cfg(test)]
mod tests {
    fn pairs(p: &[(&str, &str)]) -> Vec<(String, String)> {
        p.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

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
    fn reads_the_village_number_before_the_road() {
        let read = |head| super::village_head(head);
        assert_eq!(
            read("132 หมู่ที่ 2 ชลประทาน"),
            Some(("132 หมู่ที่ 2".into(), "ชลประทาน".into()))
        );
        assert_eq!(
            read("159 หมู่8 ถนนสายเอเชีย"),
            Some(("159 หมู่8".into(), "ถนนสายเอเชีย".into()))
        );
        for head in ["121 หมู่ที่ 9", "62 ถนนนิคมบูรพา", "หมู่ที่ 2 132 ชลประทาน"]
        {
            assert_eq!(read(head), None, "{head}");
        }
    }

    #[test]
    fn reads_the_lane_as_the_road() {
        let mut parsed = pairs(&[
            ("house_number", "18/8"),
            ("road", "ซอยเกอเธ่ ซอยสาทร"),
            ("house_number", "1"),
            ("road", "ถนนสาทรใต้"),
            ("postcode", "10120"),
        ]);
        super::lane_as_road(
            "18/8 ซอยเกอเธ่ ซอยสาทร 1 ถนนสาทรใต้, 10120, Thailand",
            &mut parsed,
        );
        assert_eq!(
            parsed,
            pairs(&[
                ("house_number", "18/8"),
                ("road", "ซอยเกอเธ่"),
                ("postcode", "10120"),
            ])
        );
        let mut parsed = pairs(&[
            ("house_number", "18/8"),
            ("road", "ซอยสาทร"),
            ("house_number", "1"),
            ("road", "ถนนสาทรใต้"),
        ]);
        super::lane_as_road("18/8 ซอยสาทร 1 ถนนสาทรใต้, Thailand", &mut parsed);
        assert_eq!(
            parsed,
            pairs(&[("house_number", "18/8"), ("road", "ซอยสาทร 1")])
        );
        // a house on the lane, no other number
        let mut lone = pairs(&[("road", "ซอยสาทร"), ("house_number", "1")]);
        super::lane_as_road("ซอยสาทร 1, Thailand", &mut lone);
        assert_eq!(lone.len(), 2);
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

    #[test]
    fn rejoins_a_word_cut_before_its_vowel_mark() {
        let q = "47 ถนนมาลัยแมน, อำเภออู่ทอง, จังหวัดสุพรรณบุรี, 72160, Thailand";
        let mut parsed = pairs(&[
            ("house_number", "47"),
            ("road", "ถนนมาลัยแมน อำเภออ"),
            ("city", "\u{e39}\u{e48}ทอง"),
        ]);
        super::rejoin_cut_words(q, &mut parsed);
        assert_eq!(
            parsed,
            pairs(&[
                ("house_number", "47"),
                ("road", "ถนนมาลัยแมน"),
                ("city", "อำเภออู่ทอง"),
            ])
        );
        let mut alone = pairs(&[("road", "อ"), ("city", "\u{e39}\u{e48}ทอง")]);
        super::rejoin_cut_words("อู่ทอง, Thailand", &mut alone);
        assert_eq!(alone, pairs(&[("city", "อู่ทอง")]));
    }
}
