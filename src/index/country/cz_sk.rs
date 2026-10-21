use crate::index::*;

/// Czech and Slovak number marks as the data keys them: a descriptive number ("č.p. 1213") bare,
/// an evidential one with its mark ("č.ev. 169"), which libpostal leaves on the road
/// ("topolová č.ev.", "169").
pub(crate) fn czech_number_marks(parsed: &mut Vec<(String, String)>) {
    const MARKS: [(&str, &str); 6] = [
        ("č.p.", ""),
        ("č. p.", ""),
        ("čp.", ""),
        ("č.ev.", "č.ev. "),
        ("č. ev.", "č.ev. "),
        ("čev.", "č.ev. "),
    ];
    let numeric = |n: &str| n.starts_with(|c: char| c.is_ascii_digit());
    let mut on_road = None;
    for (_, v) in parsed.iter_mut().filter(|(k, _)| k == "road") {
        if let Some((road, written)) = MARKS.iter().find_map(|(mark, written)| {
            let road = v.strip_suffix(mark)?;
            (road.is_empty() || road.ends_with(' '))
                .then(|| (road.trim_end().to_string(), *written))
        }) {
            *v = road;
            on_road = Some(written);
        }
    }
    // "Velký Luh č.ev. 52, 35134 Velký Luh": a village's number, the mark alone read as the road
    parsed.retain(|(k, v)| k != "road" || !v.is_empty());
    for (_, n) in parsed.iter_mut().filter(|(k, _)| k == "house_number") {
        let marked = MARKS.iter().find_map(|(mark, written)| {
            let rest = n.strip_prefix(mark)?.trim_start();
            numeric(rest).then(|| format!("{written}{rest}"))
        });
        if let Some(marked) = marked {
            *n = marked;
        } else if let Some(written) = on_road.filter(|_| numeric(n)) {
            *n = format!("{written}{n}");
        }
    }
}

/// Bytes of a house number but a Czech evidential mark's: "č.ev. 1057" is as short as "1057".
pub(crate) fn number_len(n: &str) -> usize {
    n.strip_prefix("č.ev. ").unwrap_or(n).len()
}

impl Index {
    /// The one "descriptive/orientation" house ("1213/5") of `streets` whose orientation number
    /// is `n`: Czech and Slovak data key a house by both, and a street's orientation number alone
    /// names it.
    pub(crate) fn combined_number(&self, streets: &[DocAddress], n: &str) -> Option<String> {
        if !n.starts_with(|c: char| c.is_ascii_digit()) || n.contains(['/', ' ']) {
            return None;
        }
        let suffix = format!("/{n}");
        let mut found = std::collections::BTreeSet::new();
        for &a in streets {
            let place = self.fast(a, "place") as u32;
            if self
                .places
                .get(place as usize)
                .is_some_and(|p| matches!(&*p.cc, "cz" | "sk"))
            {
                self.each_house(self.fast(a, "id") as u32, |key, _, _, _| {
                    if let Ok(key) = std::str::from_utf8(key)
                        && key.ends_with(&suffix)
                    {
                        found.insert(key.to_string());
                    }
                });
            }
        }
        // ponytail: two streets of the name each numbering an entrance `n` stay a street answer
        (found.len() == 1).then(|| found.pop_first()).flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn czech_number_marks_become_the_datas_keys() {
        let marked = |parts: &[(&str, &str)]| {
            let mut parsed: Vec<(String, String)> = parts
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            czech_number_marks(&mut parsed);
            parsed
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            marked(&[("road", "roštýnská"), ("house_number", "č.p. 1213")]),
            ["road=roštýnská", "house_number=1213"]
        );
        assert_eq!(
            marked(&[("road", "topolová č.ev."), ("house_number", "169")]),
            ["road=topolová", "house_number=č.ev. 169"]
        );
        assert_eq!(
            marked(&[
                ("road", "č. ev."),
                ("house_number", "52"),
                ("city", "velký luh")
            ]),
            ["house_number=č.ev. 52", "city=velký luh"]
        );
        assert_eq!(
            marked(&[("road", "náměstí čp."), ("house_number", "4")]),
            ["road=náměstí", "house_number=4"]
        );
        assert_eq!(
            marked(&[("road", "main street"), ("house_number", "12")]),
            ["road=main street", "house_number=12"]
        );
    }
}
