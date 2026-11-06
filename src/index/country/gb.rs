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

/// "25 Aldis Road, Acle, Norwich, United Kingdom": Royal Mail writes a village or district
/// before its post town, which can be a town 20 km away, and the parser reads both as one town
/// that names nothing. Each British place (`gb_place`) among the parts after the first, last
/// first, as (the query without it, the part). The first stays: dropping the village would let
/// a namesake street in the post town answer. A county can end the query ("Norfolk" is also a
/// country).
pub fn without_post_town(query: &str, gb_place: impl Fn(&str) -> bool) -> Vec<(String, &str)> {
    let parts: Vec<&str> = query.split(',').map(str::trim).collect();
    let last = parts[parts.len() - 1];
    let country = crate::matching::country_named(last);
    if query.chars().count() > 256
        || !parts[0].contains(|c: char| c.is_ascii_digit())
        || !matches!(country.as_str(), "gb" | "") && !gb_place(last)
    {
        return vec![];
    }
    let end = parts.len() - usize::from(country == "gb");
    let towns: Vec<usize> = (1..end).filter(|&i| place_like(parts[i])).collect();
    towns
        .iter()
        .skip(1)
        .rev()
        .take(2)
        .filter(|&&at| gb_place(parts[at]))
        .map(|&at| {
            let kept: Vec<&str> = (parts.iter().enumerate())
                .filter(|&(i, _)| i != at)
                .map(|(_, p)| *p)
                .collect();
            (kept.join(", "), parts[at])
        })
        .collect()
}

fn place_like(part: &str) -> bool {
    !part.is_empty() && !part.contains(|c: char| c.is_ascii_digit())
}

/// The village of "25 Aldis Road, Acle, Norwich": the first part after the street that has no
/// number and names no country.
pub fn village(query: &str) -> &str {
    query
        .split(',')
        .skip(1)
        .map(str::trim)
        .find(|p| place_like(p) && crate::matching::country_named(p).is_empty())
        .unwrap_or("")
}

impl Index {
    /// "3 Lee Terrace, Shotton Colliery": many British villages are only points in the data,
    /// inside a parish of another name that their streets are filed under (Sleights in
    /// Eskdaleside cum Ugglebarnby), so a street lookup scoped to the village finds nothing. The
    /// one house of that number on a street of that name within 2 km of a British place named
    /// `town`; none when two such houses are.
    pub fn gb_house_by_village(&self, number: &str, road: &str, town: &str) -> Option<Value> {
        const NEAR_KM: f64 = 2.;
        let key = name_key(town);
        let local = |p: &Place| {
            p.named(key)
                && !matches!(
                    &*p.subtype,
                    "country" | "dependency" | "region" | "macroregion" | "county" | "macrocounty"
                )
        };
        let numbers = [schema::number_key(number)];
        let mut found: Vec<Value> = vec![];
        for hit in self
            .places(town, Some("gb"), None, &local)
            .into_iter()
            .take(5)
        {
            let Hit::Place(id) = hit else { continue };
            let p = &self.places[id as usize];
            let (lat, lon) = (p.y as f64, p.x as f64);
            let houses =
                self.houses_numbered(&numbers, "", road, true, Some("gb"), (lat, lon), |n| {
                    crate::matching::roads_agree(road, n)
                });
            for house in houses {
                let at = |k: &str| house[k].as_str().and_then(|v| v.parse::<f64>().ok());
                let (Some(y), Some(x)) = (at("lat"), at("lon")) else {
                    continue;
                };
                if ((x - lon) * lat.to_radians().cos()).hypot(y - lat) * 111.2 < NEAR_KM
                    && !found
                        .iter()
                        .any(|f| f["lat"] == house["lat"] && f["lon"] == house["lon"])
                {
                    found.push(house);
                }
            }
        }
        match <[Value; 1]>::try_from(found) {
            Ok([house]) => Some(house),
            Err(_) => None,
        }
    }

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
    use super::{postcode_and_area, village, without_post_town};

    #[test]
    fn post_town_dropped() {
        let gb = |p: &str| !matches!(p, "Paris" | "Belgium");
        assert_eq!(
            without_post_town("25 Aldis Road, Acle, Norwich, United Kingdom", gb),
            vec![("25 Aldis Road, Acle, United Kingdom".into(), "Norwich")]
        );
        assert_eq!(
            without_post_town("2 The Green, Hethersett, Norwich, Norfolk", gb),
            vec![
                ("2 The Green, Hethersett, Norwich".into(), "Norfolk"),
                ("2 The Green, Hethersett, Norfolk".into(), "Norwich"),
            ]
        );
        assert_eq!(
            without_post_town("2 The Green, Hethersett, Norwich, NR9 3AA", gb),
            vec![("2 The Green, Hethersett, NR9 3AA".into(), "Norwich")]
        );
        assert!(without_post_town("1 Rue X, Marais, Paris", gb).is_empty());
        assert!(without_post_town("25 Aldis Road, Acle, United Kingdom", gb).is_empty());
        assert!(without_post_town("Aldis Road, Acle, Norwich", gb).is_empty());
        assert!(without_post_town("25 Rue Haute, Uccle, Bruxelles, Belgium", gb).is_empty());
    }

    #[test]
    fn village_is_the_first_place_part() {
        assert_eq!(village("25 Aldis Road, Acle, Norwich"), "Acle");
        assert_eq!(village("Flat 2, 10 High Street, Acle, Norwich"), "Acle");
        assert_eq!(village("25 Aldis Road, NR13 3RJ, United Kingdom"), "");
    }

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
