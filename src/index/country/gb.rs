use crate::index::*;
use regex::Regex;
use std::sync::LazyLock;

static POSTCODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^[a-z]{1,2}[0-9][a-z0-9]?(?: ?[0-9][a-z]{2})?$").unwrap());

/// "SW8, Lambeth", "CB10 2TR, Uttlesford, United Kingdom": a postcode, whole or its outward
/// code, and the area written after it ("Newry, Mourne and Down" is one).
fn postcode_and_area(q: &str) -> Option<(&str, &str)> {
    let (pc, rest) = q.split_once(',')?;
    let rest = rest.trim();
    let area = match rest.rsplit_once(',') {
        Some((area, country)) if crate::matching::country_named(country) == "gb" => area,
        _ => rest,
    };
    let (pc, area) = (pc.trim(), area.trim());
    (POSTCODE.is_match(pc) && !area.is_empty() && !area.contains(|c: char| c.is_ascii_digit()))
        .then_some((pc, area))
}

impl Index {
    /// A British postcode written with the area it is in: one its own places name, or a place
    /// of that name near it. Local authorities like Chichester or Uttlesford are only points in
    /// the data, not areas a postcode is filed under, so "near" is within about the radius of
    /// a large district. A full postcode the data lacks answers with its sector, which is not a
    /// match (false).
    pub fn gb_postcode_in_area(&self, q: &str) -> Option<(Hit, bool)> {
        const NEAR_KM: f64 = 30.;
        let (pc, area) = postcode_and_area(q)?;
        let (hit, exact) = match self.postcodes(pc, Some("gb")).into_iter().next() {
            Some(hit) => (hit, true),
            None => (self.gb_sector(pc)?, false),
        };
        let &Hit::Postcode { x, y, .. } = &hit else {
            return None;
        };
        let agrees = crate::matching::agrees_with(area);
        // the official names of some unitary authorities: "Herefordshire, County of"
        let named = |name: &str| {
            agrees(name)
                || name
                    .split_once(", ")
                    .is_some_and(|(n, form)| matches!(form, "County of" | "City of") && agrees(n))
        };
        let own = self.render(&hit, true)["address"]
            .as_object()
            .is_some_and(|a| a.values().filter_map(|v| v.as_str()).any(named));
        let near = || {
            self.places(area, Some("gb"), None, &|p| p.all_names().any(named))
                .iter()
                .any(|h| match h {
                    Hit::Place(p) => {
                        let p = &self.places[*p as usize];
                        ((p.x as f64 - x) * y.to_radians().cos()).hypot(p.y as f64 - y) * 111.2
                            < NEAR_KM
                    }
                    _ => false,
                })
        };
        (own || near()).then_some((hit, exact))
    }

    /// Full postcode `pc` at the mean of the known postcodes of its sector ("PO21 9" of
    /// "PO21 9PR"), filed under the first one's place.
    fn gb_sector(&self, pc: &str) -> Option<Hit> {
        let norm = pc_norm(pc);
        let inward = norm.as_bytes().get(norm.len().checked_sub(3)?..)?;
        if norm.len() < 5
            || !inward[0].is_ascii_digit()
            || !inward[1..].iter().all(u8::is_ascii_alphabetic)
        {
            return None;
        }
        let sector = &norm[..norm.len() - 2];
        let mut after = sector.as_bytes().to_vec();
        *after.last_mut()? += 1;
        let mut terms = std::collections::BTreeSet::new();
        for segment in self.searcher.segment_readers() {
            let inverted = segment.inverted_index(self.f.pcode).ok()?;
            let mut stream = inverted
                .terms()
                .range()
                .ge(sector.as_bytes())
                .lt(&after)
                .into_stream()
                .ok()?;
            while stream.advance() {
                if stream.key().len() == norm.len() {
                    terms.insert(String::from_utf8_lossy(stream.key()).into_owned());
                }
            }
        }
        let any = terms.iter().map(|t| (Occur::Should, term(self.f.pcode, t)));
        let cl = vec![must(Box::new(BooleanQuery::new(any.collect())))];
        let docs = self.top_of(Kind::Postcode, Some("gb"), cl, terms.len());
        let first = *docs.first()?;
        let n = docs.len() as f64;
        Some(Hit::Postcode {
            display: format!("{} {}", &norm[..norm.len() - 3], &norm[norm.len() - 3..]),
            cc: String::new(),
            place: self.fast(first, "place") as u32,
            x: docs.iter().map(|&a| self.fast_f64(a, "x")).sum::<f64>() / n,
            y: docs.iter().map(|&a| self.fast_f64(a, "y")).sum::<f64>() / n,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::postcode_and_area;

    #[test]
    fn postcode_then_area() {
        assert_eq!(postcode_and_area("SW8, Lambeth"), Some(("SW8", "Lambeth")));
        assert_eq!(
            postcode_and_area("CB10 2TR, Uttlesford, United Kingdom"),
            Some(("CB10 2TR", "Uttlesford"))
        );
        assert_eq!(
            postcode_and_area("ec1a1bb , London"),
            Some(("ec1a1bb", "London"))
        );
        assert_eq!(
            postcode_and_area("BT34, Newry, Mourne and Down"),
            Some(("BT34", "Newry, Mourne and Down"))
        );
        assert_eq!(
            postcode_and_area("SW8, Lambeth, France"),
            Some(("SW8", "Lambeth, France"))
        );
        assert_eq!(postcode_and_area("SW8 1JQ"), None);
        assert_eq!(postcode_and_area("10115, Berlin"), None);
        assert_eq!(postcode_and_area("M5V 3L9, Toronto"), None);
        assert_eq!(postcode_and_area("SW8, 12 Lambeth Road"), None);
    }
}
