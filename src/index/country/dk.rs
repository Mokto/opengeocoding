use crate::index::suggest::*;
use crate::index::*;
use crate::matching::*;
use regex::Regex;
use std::sync::LazyLock;

/// A Danish unit part: a floor with or without a door ("st.", "1. th", "st. 11", "1. c"), or a
/// side door alone.
pub(crate) fn danish_floor(text: &str) -> bool {
    let folded = text.to_ascii_lowercase().replace('.', " ");
    let words: Vec<_> = folded.split_whitespace().collect();
    let Some(floor) = words.first() else {
        return false;
    };
    let side = |w: &str| matches!(w, "t" | "th" | "tv" | "m" | "mf");
    // "st. john" is a saint, not a door
    let door = |w: &str| {
        side(w)
            || (w.len() <= 4
                && w.bytes().all(|b| b.is_ascii_alphanumeric())
                && (w.len() == 1 || w.bytes().any(|b| b.is_ascii_digit())))
    };
    let floored = matches!(*floor, "st" | "kl")
        || (floor.len() <= 2 && floor.bytes().all(|b| b.is_ascii_digit()));
    match words.len() {
        1 => floored || matches!(*floor, "th" | "tv" | "mf"),
        2 => floored && door(words[1]),
        _ => false,
    }
}

/// `query` without a Danish unit written as a comma part of its own: floor and door ("Mogensgade
/// 9, st. th, 8500 Grenaa, Denmark"), a floor alone ("1.") or a door alone ("1", "3b"). It names no
/// place, yet libpostal swallows the house and postcode into the road and reads "th" as Thailand.
/// The building's house is the answer; the unit is not modelled. Only floor and door with a
/// th/tv/mf door stand without Denmark stated, and a part naming another country keeps the
/// query as written. None without such a part after a house.
pub fn without_danish_unit(query: &str) -> Option<String> {
    static UNIT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?:(?:st|kl|\d{1,2})\.(?:\s*([a-zæøå]{1,5}|\d{1,4}[a-z]?))?|\d{1,3}[a-z]?|th|tv|mf)$",
        )
        .unwrap()
    });
    let parts: Vec<&str> = query
        .split(',')
        .map(|p| p.trim_matches(whitespace))
        .collect();
    let at = (1..parts.len())
        .find(|&i| parts[i - 1].chars().any(|c| c.is_ascii_digit()) && UNIT.is_match(parts[i]))?;
    // the unit itself is left out: "1. tv" reads as Tuvalu and "st. 1" as São Tomé
    let countries: Vec<String> = parts
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != at)
        .map(|(_, p)| expected_country(&parsed_country(p)))
        .filter(|c| !c.is_empty())
        .collect();
    let side = UNIT
        .captures(parts[at])?
        .get(1)
        .is_some_and(|door| matches!(door.as_str().to_lowercase().as_str(), "th" | "tv" | "mf"));
    let danish = countries.iter().any(|c| c == "dk");
    (countries.iter().all(|c| c == "dk") && (danish || side)).then(|| {
        let mut kept = parts;
        kept.remove(at);
        kept.join(", ")
    })
}

/// A Danish address's supplementary place ("Pittevej 6, Aidt, 8881 Thorsø, Denmark") and the query
/// without it: between the house and a four-digit postcode part, with Denmark stated. libpostal
/// then reads the postcode as the house and the road as the town. None without such a part.
pub fn without_danish_place(query: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = query
        .split(',')
        .map(|p| p.trim_matches(whitespace))
        .collect();
    let postcoded = |p: &str| {
        let (code, town) = p.split_once(' ').unwrap_or((p, ""));
        code.len() == 4 && code.bytes().all(|b| b.is_ascii_digit()) && !town.is_empty()
    };
    let at = (1..parts.len().saturating_sub(1)).find(|&i| {
        parts[i - 1].chars().any(|c| c.is_ascii_digit())
            && postcoded(parts[i + 1])
            && !parts[i].chars().any(|c| c.is_ascii_digit())
            && (1..=4).contains(&parts[i].split_whitespace().count())
    })?;
    let countries: Vec<String> = parts
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != at)
        .map(|(_, p)| expected_country(&parsed_country(p)))
        .filter(|c| !c.is_empty())
        .collect();
    (!countries.is_empty() && countries.iter().all(|c| c == "dk")).then(|| {
        let mut kept = parts.clone();
        kept.remove(at);
        (kept.join(", "), parts[at].to_owned())
    })
}

impl Index {
    pub(in crate::index) fn suggest_floor(
        &self,
        typed: &Typed,
        cc: Option<&str>,
        focus: Option<&Focus>,
        want: usize,
    ) -> Vec<Suggestion> {
        if cc.is_some_and(|c| !c.eq_ignore_ascii_case("dk"))
            || typed.parts.len() < 2
            || !danish_floor(&typed.parts[1])
        {
            return vec![];
        }
        let mut building = Typed {
            parts: typed.parts.clone(),
            open: typed.open && typed.parts.len() > 2,
        };
        let suffix = building.parts.remove(1);
        self.suggest_streets(&building, Some("dk"), focus, want)
            .into_iter()
            .filter(|s| s.kind == "address" && s.country_code == "dk")
            .map(|mut s| {
                if let Some((at, _)) = s.display_name.match_indices(", ").nth(1) {
                    s.display_name.insert_str(at, &format!(", {suffix}"));
                }
                s.unverified_suffix = suffix.clone();
                s
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn danish_unit_parts_are_floors_doors_and_nothing_else() {
        for unit in [
            "st.", "kl.", "1.", "st. th", "1. tv", "st. 11", "2. 3b", "1. c", "th",
        ] {
            assert!(danish_floor(unit), "{unit}");
        }
        for other in [
            "st. john", "st john", "12 main", "1. 12345", "th mf", "", "Grenaa", "8500",
        ] {
            assert!(!danish_floor(other), "{other}");
        }
    }
}
