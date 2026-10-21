use crate::index::*;

/// Reads 一 to 九十九.
pub(crate) fn kanji_number(t: &str) -> Option<u32> {
    let digit = |c: char| {
        "〇一二三四五六七八九"
            .chars()
            .position(|d| d == c)
            .map(|d| d as u32)
    };
    let chars: Vec<char> = t.chars().collect();
    match chars.as_slice() {
        ['十'] => Some(10),
        [c] => digit(*c),
        ['十', u] => Some(10 + digit(*u)?),
        [t, '十'] => Some(digit(*t)? * 10),
        [t, '十', u] => Some(digit(*t)? * 10 + digit(*u)?),
        _ => None,
    }
}

/// Candidate (municipality, street, number) readings of a Japanese block address, most likely
/// first: "東京都港区芝公園4-2-8" is chōme 4 of 芝公園 in 港区, block 2, house 8, stored as street
/// "芝公園四丁目" and number "2-8"; towns without chōme store "block-house" under the town.
pub(crate) fn japanese(q: &str) -> Vec<(String, String, String)> {
    static POSTCODE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"〒?\d{3}-\d{4}|^日本国?|,?Japan$").unwrap());
    static CHOME: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new("([〇一二三四五六七八九十]+)丁目").unwrap());
    let t: String = q
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap(),
            '－' | '−' | '‐' | 'ー' | '―' | '‑' => '-',
            _ => c,
        })
        .collect();
    let t = POSTCODE.replace_all(&t, "");
    let t = CHOME.replace_all(&t, |c: &regex::Captures| {
        kanji_number(&c[1]).map_or(c[0].to_string(), |n| format!("{n}丁目"))
    });
    let Some(at) = t.find(|c: char| c.is_ascii_digit()) else {
        return vec![];
    };
    let (prefix, rest) = t.split_at(at);
    if prefix.is_empty() || !prefix.chars().all(|c| c as u32 >= 0x3040) {
        return vec![];
    }
    let rest = rest
        .replace("丁目", "-")
        .replace("番地", "-")
        .replace(['番', 'の'], "-")
        .replace('号', "");
    let numbers: Vec<u32> = rest
        .split(|c: char| !c.is_ascii_digit() && c != '-')
        .next()
        .unwrap_or("")
        .split('-')
        .filter_map(|n| n.parse().ok())
        .collect();
    if numbers.len() < 2 {
        return vec![];
    }
    let joined = |ns: &[u32]| ns.iter().map(u32::to_string).collect::<Vec<_>>().join("-");
    let mut out = vec![];
    for (i, c) in prefix.char_indices().rev() {
        let end = i + c.len_utf8();
        if !"都道府県市区郡町村".contains(c) || end == prefix.len() {
            continue;
        }
        let (city, town) = prefix.split_at(end);
        // the municipality, without its prefecture
        let city = city
            .rfind(['都', '道', '府', '県'])
            .map(|k| &city[k + 3..])
            .filter(|c| !c.is_empty())
            .unwrap_or(city);
        out.push((
            city.to_string(),
            format!("{town}{}丁目", numbers[0]),
            joined(&numbers[1..numbers.len().min(3)]),
        ));
        out.push((city.to_string(), town.to_string(), joined(&numbers[..2])));
    }
    out
}

/// "朝日町伊勢宮 472-2", "472-2 朝日町伊勢宮" or "Koyodai 258-2": a Japanese town, district or
/// road and its number, which libpostal cuts into unrelated fields when the name is unspaced kanji
/// or romaji.
pub(crate) fn japanese_head(head: &str) -> Option<Vec<(String, String)>> {
    const NUMBER: &str = "[0-9０-９]+(?:[-－‐−][0-9０-９]+)*";
    static HEAD: LazyLock<[regex::Regex; 2]> = LazyLock::new(|| {
        [
            format!(r"^\s*(?<name>\S.*?)\s+(?<number>{NUMBER})\s*$"),
            format!(r"^\s*(?<number>{NUMBER})\s+(?<name>\S.*?)\s*$"),
        ]
        .map(|re| regex::Regex::new(&re).unwrap())
    });
    let c = HEAD.iter().find_map(|re| re.captures(head))?;
    if !c["name"].chars().any(char::is_alphabetic) {
        return None;
    }
    Some(vec![
        ("road".into(), c["name"].to_lowercase()),
        ("house_number".into(), c["number"].to_lowercase()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn japanese_block_addresses() {
        let got = japanese("〒105-0011 東京都港区芝公園４丁目２番８号");
        assert_eq!(got[0], ("港区".into(), "芝公園4丁目".into(), "2-8".into()));
        assert_eq!(got[1], ("港区".into(), "芝公園".into(), "4-2".into()));
        assert_eq!(
            japanese("東京都千代田区大手町1-1-1")[0],
            ("千代田区".into(), "大手町1丁目".into(), "1-1".into())
        );
        assert_eq!(japanese("大阪府大阪市北区梅田三丁目1-3")[0].1, "梅田3丁目");
        assert_eq!(kanji_number("二十一"), Some(21));
        assert!(japanese("1600 Pennsylvania Avenue").is_empty());
        assert!(japanese("東京都").is_empty());
    }

    #[test]
    fn japanese_town_and_number_heads() {
        let house = |road: &str, number: &str| {
            Some(vec![
                ("road".to_string(), road.to_string()),
                ("house_number".to_string(), number.to_string()),
            ])
        };
        assert_eq!(
            japanese_head("朝日町伊勢宮 472-2"),
            house("朝日町伊勢宮", "472-2")
        );
        assert_eq!(japanese_head("828 今津町福岡"), house("今津町福岡", "828"));
        assert_eq!(japanese_head("Koyodai 258-2"), house("koyodai", "258-2"));
        assert_eq!(
            japanese_head("国道１５３号 3380-1"),
            house("国道１５３号", "3380-1")
        );
        assert_eq!(
            japanese_head("433-1 畦町433-1"),
            house("畦町433-1", "433-1")
        );
        // a number with letters or kanji in it is the parser's to read
        assert_eq!(japanese_head("中宮 オ9"), None);
        assert_eq!(japanese_head("浜北袋井線 山科字前田3399"), None);
        assert_eq!(japanese_head("7-1"), None);
        assert_eq!(japanese_head("12 34"), None);
    }
}
