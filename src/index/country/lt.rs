/// A municipality as addresses write it ("Alytaus r. sav.", "Kauno m. sav.", "Birštono sav.",
/// "Zarasų rajono savivaldybė"), named as the index's regions are: "alytaus rajono savivaldybė".
fn municipality(part: &str) -> Option<String> {
    let p = part.to_lowercase();
    let stem = p
        .strip_suffix(" sav.")
        .or_else(|| p.strip_suffix(" savivaldybė"))?;
    let stem = match stem.rsplit_once(' ') {
        Some((name, "r.")) => format!("{name} rajono"),
        Some((name, "m.")) => format!("{name} miesto"),
        _ => stem.to_string(),
    };
    Some(format!("{stem} savivaldybė"))
}

/// "LT-62268 Alytus" or "62268 Alytus": the postcode as the index writes it, and the rest.
fn postcode(part: &str) -> Option<(String, &str)> {
    let (code, rest) = part.split_once(' ')?;
    let digits = code
        .strip_prefix("LT-")
        .or_else(|| code.strip_prefix("lt-"))
        .unwrap_or(code);
    (digits.len() == 5 && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| (format!("lt-{digits}"), rest.trim()))
}

/// A settlement: a name with its type ("Kaniūkų k.", "Dusetų m.", "Rusnės mstl.", "Kubilių
/// vs.") or a town in the nominative ("Dusetos").
fn settlement(part: &str) -> bool {
    !part.is_empty() && !part.contains(|c: char| c.is_ascii_digit()) && municipality(part).is_none()
}

/// "Taikos g. 4", "Vaškių k. 12A": the street (or a village that numbers its houses itself)
/// and a number without leading zero, with at most one letter.
fn house(part: &str) -> Option<(String, String)> {
    let (road, number) = part.rsplit_once(' ')?;
    let digits = number.trim_end_matches(|c: char| c.is_alphabetic());
    (digits.starts_with(|c: char| ('1'..='9').contains(&c))
        && digits.bytes().all(|b| b.is_ascii_digit())
        && number.chars().count() - digits.len() <= 1
        && road.contains(char::is_alphabetic))
    .then(|| (road.to_lowercase(), number.to_lowercase()))
}

/// A Lithuanian address written part by part, in Lietuvos paštas's order ("Agrastų g. 12,
/// Kaniūkų k., LT-64104 Alytaus r. sav., Lithuania") or the register's, municipality first
/// ("Alytaus r. sav., Kaniūkų k., Agrastų g. 12, Lithuania"). libpostal joins the parts into
/// one road ("alytaus r. sav. kaniūkų k. agrastų g.") or reads a postcode with its municipality
/// as a road ("lt-67312 lazdijų r.", city "sav."). The street and number, the settlement as the
/// town, the postcode, and the municipality as the state district. `None` unless every part has
/// its place.
pub(crate) fn aligned(q: &str) -> Option<Vec<(String, String)>> {
    let mut parts: Vec<&str> = q.split(',').map(str::trim).collect();
    let country = parts.pop()?.to_lowercase();
    if !matches!(country.as_str(), "lithuania" | "lietuva") || parts.len() < 2 {
        return None;
    }
    let (road, number, places) = match (house(parts[0]), house(parts[parts.len() - 1])) {
        (Some((road, number)), _) => (road, number, &parts[1..]),
        (None, Some((road, number))) => (road, number, &parts[..parts.len() - 1]),
        _ => return None,
    };
    let mut parsed = vec![
        ("road".to_string(), road),
        ("house_number".to_string(), number),
    ];
    let (mut town, mut district) = (None, None);
    for place in places {
        let (code, name) = match postcode(place) {
            Some((code, name)) => (Some(code), name),
            None => (None, *place),
        };
        parsed.extend(code.map(|c| ("postcode".to_string(), c)));
        if let Some(m) = municipality(name) {
            if district.replace(m).is_some() {
                return None;
            }
        } else if settlement(name) && town.is_none() {
            town = Some(name.to_lowercase());
        } else {
            return None;
        }
    }
    parsed.extend(town.map(|t| ("city".to_string(), t)));
    parsed.extend(district.map(|d| ("state".to_string(), d)));
    parsed.push(("country".to_string(), country));
    Some(parsed)
}

/// Lithuanian street types as addresses abbreviate them.
const STREET_TYPES: &[&str] = &[
    "g.", "pr.", "al.", "a.", "skg.", "aklg.", "pl.", "kel.", "skv.", "tak.", "krant.", "bul.",
    "aplinkl.", "vieškl.",
];

/// "Jono Basanavičiaus g." is "J. Basanavičiaus g.": the register spells out a street's given
/// names, which geoportal.lt's database (and so the index) abbreviates to initials, sometimes
/// after a title ("Kun. A. Dzekano g."). Word by word, each initial on one side begins the other
/// side's word, and every other word is the same.
pub(crate) fn initials_agree(typed: &str, found: &str) -> bool {
    let (typed, found) = (typed.to_lowercase(), found.to_lowercase());
    let (a, b): (Vec<&str>, Vec<&str>) = (
        typed.split_whitespace().collect(),
        found.split_whitespace().collect(),
    );
    let initial = |w: &str| {
        let mut c = w.chars();
        matches!((c.next(), c.next(), c.next()), (Some(i), Some('.'), None) if i.is_alphabetic())
    };
    let stands_for = |i: &str, w: &str| {
        initial(i) && !initial(w) && w.chars().count() > 2 && w.starts_with(&i[..i.len() - 1])
    };
    a.len() == b.len()
        && a.len() >= 3
        && a.last() == b.last()
        && STREET_TYPES.contains(a.last().unwrap())
        && a.iter().zip(&b).any(|(x, y)| x != y)
        && a.iter()
            .zip(&b)
            .all(|(x, y)| x == y || stands_for(x, y) || stands_for(y, x))
}

/// [`initials_agree`]'s street without its given names, to look the initialled one up by: the
/// surname and type ("basanavičiaus g."). `None` for a street of two words.
pub(crate) fn surname_road(road: &str) -> Option<String> {
    let words: Vec<&str> = road.split_whitespace().collect();
    (words.len() >= 3 && STREET_TYPES.contains(&words[words.len() - 1].to_lowercase().as_str()))
        .then(|| words[words.len() - 2..].join(" "))
}

/// SuggestAddress's reading of a Lithuanian address: the municipality written out as the index
/// names it ("Alytaus r. sav." as "Alytaus rajono savivaldybė", which the completion knows as a
/// region, unlike the abbreviation), and the register's order turned house first. `None` when
/// nothing changes.
pub(crate) fn suggest_text(text: &str) -> Option<String> {
    let full = |p: &str| match p.strip_suffix(" sav.") {
        Some(_) => municipality(p),
        None => None,
    };
    let mut changed = false;
    let mut parts: Vec<String> = text
        .split(',')
        .map(|p| {
            let p = p.trim();
            let written = match postcode(p) {
                Some((_, name)) => {
                    full(name).map(|m| format!("{} {m}", p[..p.len() - name.len()].trim_end()))
                }
                None => full(p),
            };
            changed |= written.is_some();
            written.unwrap_or_else(|| p.to_string())
        })
        .collect();
    let country = parts
        .last()
        .is_some_and(|p| matches!(p.to_lowercase().as_str(), "lithuania" | "lietuva"));
    let end = parts.len() - usize::from(country);
    if end >= 2 && municipality(&parts[0]).is_some() && house(&parts[end - 1]).is_some() {
        parts[..end].reverse();
        changed = true;
    }
    // a settlement written with its type ("Kaniūkų k.") is the houses' postal town, which the
    // completion finds alone but not beside a municipality
    let typed =
        |p: &String| settlement(p) && p.rsplit(' ').next().is_some_and(|w| w.ends_with('.'));
    if end >= 2 && parts[1..end].iter().any(typed) {
        let mut i = 0;
        parts.retain(|p| {
            i += 1;
            let kept = i == 1 || i > end || postcode(p).is_some() || municipality(p).is_none();
            changed |= !kept;
            kept
        });
    }
    changed.then(|| parts.join(", "))
}

/// The house part of `text` with its street's given names as initials ("Vlado Rozmano g. 2B,
/// …" as "V. Rozmano g. 2B, …"), for SuggestAddress to try where the index has only the
/// initialled street ([`initials_agree`]). `None` without a given name to shorten.
pub(crate) fn initialled(text: &str) -> Option<String> {
    let (head, rest) = text.split_once(',').unwrap_or((text, ""));
    let (road, number) = head.trim().rsplit_once(' ')?;
    house(head.trim())?;
    let words: Vec<&str> = road.split_whitespace().collect();
    let n = words.len();
    let named = n >= 3
        && STREET_TYPES.contains(&words[n - 1].to_lowercase().as_str())
        && words[n - 2].chars().all(char::is_alphabetic);
    let given = |w: &str| {
        w.chars().count() >= 3
            && w.chars().all(char::is_alphabetic)
            && w.starts_with(char::is_uppercase)
    };
    if !named || !words[..n - 2].iter().any(|w| given(w)) {
        return None;
    }
    let short: Vec<String> = words
        .iter()
        .enumerate()
        .map(|(i, w)| match w.chars().next() {
            Some(c) if i < n - 2 && given(w) => format!("{c}."),
            _ => w.to_string(),
        })
        .collect();
    let comma = if rest.is_empty() { "" } else { "," };
    Some(format!("{} {number}{comma}{rest}", short.join(" ")))
}

/// A settlement as the addresses write it, with its type abbreviation in lower case: "Kaniūkų
/// k.", "Rusnės mstl.", not title case's "Kaniūkų K.".
pub(crate) fn settlement_title(name: &str) -> String {
    let typed = |w: &str| matches!(w, "k." | "m." | "mstl." | "vs." | "km." | "glž." | "st.");
    name.split(' ')
        .map(|w| match w {
            _ if typed(w) => w.to_string(),
            _ => crate::index::title(w),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_postal_and_register_orders() {
        let read = |q: &str| {
            aligned(q).map(|p| {
                p.iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        };
        assert_eq!(
            read("Agrastų g. 12, Kaniūkų k., LT-64104 Alytaus r. sav., Lithuania").as_deref(),
            Some(
                "road=agrastų g. house_number=12 postcode=lt-64104 city=kaniūkų k. \
                 state=alytaus rajono savivaldybė country=lithuania"
            )
        );
        assert_eq!(
            read("Alytaus m. sav., Alytaus m., Punios g. 6, Lithuania").as_deref(),
            Some(
                "road=punios g. house_number=6 city=alytaus m. \
                 state=alytaus miesto savivaldybė country=lithuania"
            )
        );
        assert_eq!(
            read("Mikališkės k. 8, LT-67312 Lazdijų r. sav., Lithuania").as_deref(),
            Some(
                "road=mikališkės k. house_number=8 postcode=lt-67312 \
                 state=lazdijų rajono savivaldybė country=lithuania"
            )
        );
        assert_eq!(
            read("Punios g. 6, LT-62268 Alytus, Lithuania").as_deref(),
            Some("road=punios g. house_number=6 postcode=lt-62268 city=alytus country=lithuania")
        );
        assert_eq!(
            read("Pagėgių sav., Užbalių k. 1, Lithuania").as_deref(),
            Some(
                "road=užbalių k. house_number=1 state=pagėgių savivaldybė \
                 country=lithuania"
            )
        );
        for kept in [
            "Vilnius, Lithuania",
            "Gedimino pr. 9, Vilnius",
            "Gedimino pr. 9, Vilnius, Latvia",
            "Gedimino pr. 09, Vilnius, Lithuania",
            "Kauno m. sav., Vilniaus m. sav., Laisvės al. 1, Lithuania",
            "Gedimino pr. 9, Vilnius, Kaunas, Lithuania",
        ] {
            assert_eq!(read(kept), None, "{kept}");
        }
    }

    #[test]
    fn spelt_out_given_names_are_the_initialled_street() {
        assert!(initials_agree("vlado rozmano g.", "V. Rozmano g."));
        assert!(initials_agree(
            "Kun. Antonijaus Dzekano g.",
            "Kun. A. Dzekano g."
        ));
        assert!(initials_agree(
            "J. Basanavičiaus g.",
            "Jono Basanavičiaus g."
        ));
        assert!(initials_agree(
            "Jono Karolio Chodkevičiaus g.",
            "J. K. Chodkevičiaus g."
        ));
        for (typed, found) in [
            ("vlado rozmano g.", "A. Rozmano g."),
            ("vlado rozmano g.", "V. Rozmano al."),
            ("vlado rozmano g.", "Rozmano g."),
            ("vlado rozmano g.", "vlado rozmano g."),
            ("V. Rozmano g.", "W. Rozmano g."),
            ("rozmano g.", "rozmano g."),
            ("R. du Bosquet", "Rue du Bosquet"),
        ] {
            assert!(!initials_agree(typed, found), "{typed} {found}");
        }
        assert_eq!(
            surname_road("Jono Basanavičiaus g.").as_deref(),
            Some("Basanavičiaus g.")
        );
        assert_eq!(surname_road("Taikos g."), None);
        assert_eq!(settlement_title("kaniūkų k."), "Kaniūkų k.");
        assert_eq!(settlement_title("paežerio i k."), "Paežerio I k.");
        assert_eq!(
            initialled("Vlado Rozmano g. 2B, Paežerio I k., Lithuania").as_deref(),
            Some("V. Rozmano g. 2B, Paežerio I k., Lithuania")
        );
        assert_eq!(
            initialled("Kun. Antonijaus Dzekano g. 30").as_deref(),
            Some("Kun. A. Dzekano g. 30")
        );
        for kept in [
            "V. Rozmano g. 2B",
            "Taikos g. 4, Dusetos",
            "Žemaitukų 3-ioji al. 24",
            "Vlado Rozmano g.",
            "Gedimino pr. 9",
        ] {
            assert_eq!(initialled(kept), None, "{kept}");
        }
        assert_eq!(surname_road("Rue du Bosquet"), None);
    }

    #[test]
    fn suggests_with_the_municipality_written_out() {
        assert_eq!(
            suggest_text("Juočiškių k. 7, LT-62190 Alytaus r. sav., Lithuania").as_deref(),
            Some("Juočiškių k. 7, LT-62190 alytaus rajono savivaldybė, Lithuania")
        );
        assert_eq!(
            suggest_text("Alytaus r. sav., Kaniūkų k., Agrastų g. 12, Lithuania").as_deref(),
            Some("Agrastų g. 12, Kaniūkų k., Lithuania")
        );
        assert_eq!(
            suggest_text("Alytaus r. sav., Daugai, Maironio g. 25, Lithuania").as_deref(),
            Some("Maironio g. 25, Daugai, alytaus rajono savivaldybė, Lithuania")
        );
        assert_eq!(
            suggest_text("Pagėgių sav., Užbalių k. 1").as_deref(),
            Some("Užbalių k. 1, pagėgių savivaldybė")
        );
        assert_eq!(
            suggest_text("Agrastų g. 12, Kaniūkų k., LT-64104 Alytaus r. sav., Lithuania")
                .as_deref(),
            Some("Agrastų g. 12, Kaniūkų k., LT-64104 alytaus rajono savivaldybė, Lithuania")
        );
        for kept in [
            "londo ",
            "Punios g. 6, LT-62268 Alytus, Lithuania",
            "Užbalių k. 1, Pagėgių savivaldybė, Lithuania",
            "Agrastų g. 12, Kaniūkų k., Alytaus r. s",
        ] {
            assert_eq!(suggest_text(kept), None, "{kept}");
        }
    }
}
