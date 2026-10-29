use super::*;
use country::{br, mx};

/// A house's own postcode against the stated one. A street typed wrongly and a postcode the
/// house contradicts are two doubts at once, so the respelt street answers only without the second.
pub(super) fn postcode_not_contradicted(own: Option<&str>, stated: Option<&str>) -> bool {
    match (own.map(pc_norm), stated.map(pc_norm)) {
        (Some(o), Some(s)) if !o.is_empty() && !s.is_empty() => o == s,
        _ => true,
    }
}

impl Index {
    pub(super) fn postcodes(&self, pc: &str, cc: Option<&str>) -> Vec<Hit> {
        let pc = pc_norm(pc);
        if pc.is_empty() {
            return vec![];
        }
        self.top_of(Kind::Postcode, cc, vec![must(term(self.f.pcode, &pc))], 5)
            .into_iter()
            .map(|a| {
                let doc: tantivy::TantivyDocument = self.searcher.doc(a).unwrap();
                let get = |f: Field| stored(&doc, f);
                Hit::Postcode {
                    display: br::undotted_cep(get(self.f.name)),
                    cc: String::new(),
                    place: self.fast(a, "place") as u32,
                    x: self.fast_f64(a, "x"),
                    y: self.fast_f64(a, "y"),
                }
            })
            .collect()
    }

    /// A normalized postcode as the data writes it ("SW1A2AA" -> "SW1A 2AA").
    pub(super) fn postcode_display(&self, norm: &str, cc: Option<&str>) -> String {
        let cl = vec![must(term(self.f.pcode, norm))];
        match self.top_of(Kind::Postcode, cc, cl, 1).first() {
            Some(&a) => br::undotted_cep(stored(&self.searcher.doc(a).unwrap(), self.f.name)),
            None => norm.to_string(),
        }
    }

    /// The postcode's own places, as hits to render.
    pub fn postcode_hits(&self, pc: &str, cc: Option<&str>) -> Vec<Hit> {
        self.postcodes(pc, cc)
    }

    /// Whether the data holds this postcode (in country `cc`, if given).
    pub fn knows_postcode(&self, pc: &str, cc: Option<&str>) -> bool {
        self.postcode_exists(pc, cc)
    }

    /// Whether `text`, or its last words, names a place in the data (in country `cc`, if given).
    pub fn names_place(&self, text: &str, cc: Option<&str>) -> bool {
        self.resolve_place(text, cc).is_some()
    }

    pub(super) fn postcode_exists(&self, pc: &str, cc: Option<&str>) -> bool {
        let cl = vec![must(term(self.f.pcode, &pc_norm(pc)))];
        self.any_of(Kind::Postcode, cc, cl)
    }

    /// A postcode the data doesn't hold (in country `cc`, if given) written with a town in one
    /// comma-separated part after the first, which libpostal misreads with the town ("Limerick
    /// V94 N12Y" as a road), and `q` without it. One word of four to ten characters with a digit,
    /// or two of at most four (each with a digit or of at most three letters), so a town ("Oslo
    /// 0150") stays.
    pub fn unknown_postcode(&self, q: &str, cc: Option<&str>) -> Option<(String, String)> {
        let parts: Vec<&str> = q.split(',').map(str::trim).collect();
        let digit = |w: &str| w.chars().any(|c| c.is_ascii_digit());
        for len in [2, 1] {
            for (pi, part) in parts.iter().enumerate().skip(1) {
                let words: Vec<&str> = part.split_whitespace().collect();
                for start in 0..(words.len() + 1).saturating_sub(len) {
                    let run = &words[start..start + len];
                    let cand = run.join(" ");
                    let norm = pc_norm(&cand);
                    let shaped = digit(&norm)
                        && cand
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || " -".contains(c))
                        && match len {
                            1 => (4..=10).contains(&norm.len()),
                            _ => run
                                .iter()
                                .all(|w| w.len() <= 4 && (digit(w) || w.len() <= 3)),
                        };
                    let town = [&words[..start], &words[start + len..]].concat();
                    if !shaped || town.iter().all(|w| digit(w)) || self.postcode_exists(&cand, cc) {
                        continue;
                    }
                    let mut rest: Vec<String> = parts.iter().map(|p| p.to_string()).collect();
                    rest[pi] = town.join(" ");
                    return Some((cand, rest.join(", ")));
                }
            }
        }
        None
    }

    /// A one- or two-word postcode within a comma-separated part of `q`, and `q` without it.
    /// Without a known country a single word must mix letters and digits or have five or more
    /// characters, so house numbers aren't read as some country's postcode.
    pub(super) fn find_postcode(&self, q: &str, cc: Option<&str>) -> Option<(String, String)> {
        let parts: Vec<&str> = q.split(',').collect();
        for len in [2, 1] {
            for (pi, part) in parts.iter().enumerate() {
                let words: Vec<&str> = part.split_whitespace().collect();
                for start in 0..(words.len() + 1).saturating_sub(len) {
                    let cand = words[start..start + len].join(" ");
                    let norm = pc_norm(&cand);
                    let digits = norm.chars().any(|c| c.is_ascii_digit());
                    let letters = norm.chars().any(|c| c.is_alphabetic());
                    if !digits
                        || (len == 2 && mx::whole_postcode(cc))
                        || norm.len() < 3
                        || (len == 1 && cc.is_none() && !letters && norm.len() < 5)
                    {
                        continue;
                    }
                    if self.postcode_exists(&cand, cc) {
                        let mut rest_words = words.clone();
                        rest_words.drain(start..start + len);
                        let mut rest: Vec<String> =
                            parts.iter().map(|p| p.trim().to_string()).collect();
                        rest[pi] = rest_words.join(" ");
                        let rest = rest
                            .into_iter()
                            .filter(|p| !p.is_empty())
                            .collect::<Vec<_>>()
                            .join(", ");
                        return Some((cand, rest));
                    }
                }
            }
        }
        None
    }
}
