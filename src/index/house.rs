use super::*;

/// A number with a letter addition is keyed as its source writes it: glued ("11d"), spaced as OSM
/// stores one letter ("11 d"), or after a slash as Italian registers write it ("11/a", "9/bis").
/// [`schema::number_key`] keeps those separators, so the other two spellings are further keys for
/// the same house, the spaced one first. A Cyrillic letter is then tried in its Latin form, as
/// Serbia's sources key the register's "10А" ("10a", "93v" for "93В").
pub(super) fn letter_forms(number: &str) -> Vec<String> {
    let digits = number.bytes().take_while(u8::is_ascii_digit).count();
    let (base, rest) = number.split_at(digits);
    let (sep, letters) = match rest.as_bytes().first() {
        Some(b' ' | b'/') => rest.split_at(1),
        _ => ("", rest),
    };
    let latin = cyrillic_letter(letters);
    if digits == 0
        || letters.is_empty()
        || !letters.bytes().all(|b| b.is_ascii_lowercase()) && latin.is_none()
    {
        return vec![];
    }
    // ponytail: only one letter is tried spaced, as OSM writes it; a stored "12 bis" is missed.
    let mut seps = vec![""];
    if letters.chars().count() == 1 {
        seps.insert(0, " ");
    }
    seps.push("/");
    let mut forms: Vec<String> = (seps.iter().filter(|s| **s != sep))
        .map(|s| format!("{base}{s}{letters}"))
        .collect();
    if let Some(l) = latin {
        forms.extend(seps.iter().map(|s| format!("{base}{s}{l}")));
    }
    forms
}

/// Whether written number key `n` names stored key `stored`: equal, or with its Cyrillic letter
/// in Latin, the form [`letter_forms`] finds it under ("10a" for a written "10а").
pub fn same_number(n: &str, stored: &str) -> bool {
    n == stored
        || n.starts_with(|c: char| c.is_ascii_digit())
            && n.char_indices().last().is_some_and(|(i, _)| {
                cyrillic_letter(&n[i..]).is_some_and(|l| stored.strip_suffix(l) == Some(&n[..i]))
            })
}

/// The Latin letter of one lowercase Cyrillic house letter, in Serbian romanization; letters
/// romanized with a diacritic or two letters (ж, ћ, љ) have none, as `letter_forms` keys only a-z.
fn cyrillic_letter(letters: &str) -> Option<char> {
    let mut chars = letters.chars();
    let c = chars.next().filter(|_| chars.next().is_none())?;
    let i = "абвгдезијклмнопрстуфхц".chars().position(|k| k == c)?;
    "abvgdezijklmnoprstufhc".chars().nth(i)
}

impl Index {
    /// The houses whose number is one of `numbers` (as keyed) on the streets named `road` that
    /// hold `postcode` or (`near`) lie within 5 km of a point, or with no road on the streets
    /// holding `postcode`; on streets `named` accepts, nearest the point first, at most 20.
    #[expect(clippy::too_many_arguments)]
    pub fn houses_numbered(
        &self,
        numbers: &[String],
        postcode: &str,
        road: &str,
        near: bool,
        cc: Option<&str>,
        (lat, lon): (f64, f64),
        named: impl Fn(&str) -> bool,
    ) -> Vec<Value> {
        let cos = lat.to_radians().cos().max(0.01);
        let meters = |x: f64, y: f64| ((x - lon) * cos).hypot(y - lat) * 111_195.0;
        let pc = pc_norm(postcode);
        let with_postcode = |mut cl: Vec<(Occur, Box<dyn TQuery>)>, n| {
            cl.push(must(term(self.f.pcode, &pc)));
            self.top_of(Kind::Street, cc, cl, n)
        };
        let mut seen = std::collections::HashSet::new();
        let (mut key, mut found) = (vec![], vec![]);
        let mut look = |docs: Vec<DocAddress>, found: &mut Vec<_>| {
            for a in docs {
                if !seen.insert(a) {
                    continue;
                }
                let sid = self.fast(a, "id") as u32;
                for n in numbers {
                    let forms = letter_forms(n);
                    for form in std::iter::once(n.as_str()).chain(forms.iter().map(String::as_str))
                    {
                        key.clear();
                        key.extend_from_slice(&sid.to_be_bytes());
                        key.extend_from_slice(form.as_bytes());
                        if let Some(v) = self.numbers[sid as usize % self.numbers.len()].get(&key) {
                            let (x, y, own) = schema::unpack(v);
                            found.push((meters(x, y), a, form.to_string(), x, y, own));
                            break;
                        }
                    }
                }
            }
        };
        if !pc.is_empty() {
            let n = if road.is_empty() { 1000 } else { 200 };
            look(with_postcode(self.musts(self.f.names, road), n), &mut found);
        }
        // the street's namesakes around the answer when its postcode holds none of the numbers
        if near && !road.is_empty() && found.is_empty() {
            let near = self
                .top_of(Kind::Street, cc, self.musts(self.f.names, road), 200)
                .into_iter()
                .filter(|&a| meters(self.fast_f64(a, "x"), self.fast_f64(a, "y")) < 5_000.0)
                .collect();
            look(near, &mut found);
        }
        found.sort_by(|p, q| p.0.total_cmp(&q.0));
        found
            .into_iter()
            .filter_map(|(_, a, n, x, y, own)| {
                let hit = self.house_hit(a, n.clone(), x, y, own)?;
                matches!(&hit, Hit::Street { name, .. } if name.split('\x1f').any(&named))
                    .then(|| self.render(&hit, true))
            })
            .take(20)
            .collect()
    }

    /// The stored number and its record, under another [`letter_forms`] spelling when the
    /// written one is absent ("11d" for OSM's "11 d", "35a" for the register's "35/a").
    pub(super) fn house_record(&self, sid: u32, n: &str) -> Option<(String, u64)> {
        let get = |form: &str| {
            let mut key = sid.to_be_bytes().to_vec();
            key.extend_from_slice(form.as_bytes());
            self.numbers[sid as usize % self.numbers.len()].get(&key)
        };
        if let Some(v) = get(n) {
            return Some((n.to_string(), v));
        }
        letter_forms(n)
            .into_iter()
            .find_map(|alt| get(&alt).map(|v| (alt, v)))
    }

    /// House `number` of street document `a`, at its position and with its postcode slot.
    pub(super) fn house_hit(
        &self,
        a: DocAddress,
        number: String,
        x: f64,
        y: f64,
        own: u8,
    ) -> Option<Hit> {
        use tantivy::schema::Value as _;
        let doc = self.document(a).ok()?;
        let postcode = (own > 0)
            .then(|| {
                doc.get_all(self.f.pcode)
                    .nth(own as usize - 1)?
                    .as_str()
                    .map(pc_norm)
            })
            .flatten();
        Some(Hit::Street {
            doc: a,
            sid: self.fast(a, "id") as u32,
            place: self.fast(a, "place") as u32,
            name: stored(&doc, self.f.name),
            x,
            y,
            postal_town: postal_town(&doc, self.f.postal_town, &number, postcode.as_deref()),
            number: Some(number),
            postcode,
            city: None,
            cc: String::new(),
        })
    }

    /// The house a house suggestion of this index names, rendered as a search answer.
    pub fn suggested_house(&self, s: &Suggestion) -> Option<Value> {
        let (_, key) = s.retrieval_id.split_once('/')?;
        let mut parts = key.strip_prefix('h')?.split(':');
        let (segment, doc) = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
        let number = parts.next()?;
        let a = DocAddress::new(segment, doc);
        let sid = self.fast(a, "id") as u32;
        let mut key = sid.to_be_bytes().to_vec();
        key.extend_from_slice(number.as_bytes());
        let value = self.numbers[sid as usize % self.numbers.len()].get(&key)?;
        let (x, y, own) = schema::unpack(value);
        Some(self.render(&self.house_hit(a, number.into(), x, y, own)?, true))
    }

    /// Calls `each` with every house of street `sid`: its number, position and postcode slot.
    pub(super) fn each_house(&self, sid: u32, mut each: impl FnMut(&[u8], f64, f64, u8)) {
        use tantivy_fst::{IntoStreamer, Streamer};
        let mut range = self.numbers[sid as usize % self.numbers.len()]
            .range()
            .ge(sid.to_be_bytes());
        if let Some(next) = sid.checked_add(1) {
            range = range.lt(next.to_be_bytes());
        }
        let mut houses = range.into_stream();
        while let Some((key, value)) = houses.next() {
            let (x, y, own) = schema::unpack(value);
            each(&key[4..], x, y, own);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_letter_addition_is_glued_spaced_or_after_a_slash() {
        assert_eq!(letter_forms("11d"), ["11 d", "11/d"]);
        assert_eq!(letter_forms("11 d"), ["11d", "11/d"]);
        assert_eq!(letter_forms("35/a"), ["35 a", "35a"]);
        assert_eq!(letter_forms("9/bis"), ["9bis"]);
        assert_eq!(letter_forms("12bis"), ["12/bis"]);
        assert_eq!(letter_forms("10а"), ["10 а", "10/а", "10 a", "10a", "10/a"]);
        assert_eq!(letter_forms("93 в"), ["93в", "93/в", "93 v", "93v", "93/v"]);
        for n in ["7", "1/19", "285/1", "a1", "12 b1", "12/", "5ж", "5аб"] {
            assert!(letter_forms(n).is_empty(), "{n}");
        }
        assert!(same_number("13в", "13v") && same_number("13 в", "13 v") && same_number("7", "7"));
        for (n, stored) in [("13в", "13b"), ("13v", "13в"), ("13ж", "13z"), ("в", "v")] {
            assert!(!same_number(n, stored), "{n} {stored}");
        }
    }
}
