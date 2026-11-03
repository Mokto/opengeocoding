use crate::index::suggest::*;
use crate::index::*;

pub(super) struct Address<'a> {
    pub areas: Vec<&'a str>,
    pub road: &'a str,
    pub number: String,
    pub country: Option<&'a str>,
    open: bool,
    country_first: bool,
}

impl Address<'_> {
    fn house_matches(
        &self,
        road: &str,
        number: &str,
        good_match: bool,
        unique: impl FnOnce() -> bool,
    ) -> bool {
        (self.country_first && !good_match)
            || ((road.is_empty() || fold(road) == fold(self.road))
                && (number.is_empty() || unique()))
    }
}

/// The address without the 읍, 면 or 동 after its 시, 군 or 구 (or Sejong), where its road name is
/// unique. The data often puts a 리 straight under its 군, outside the stated 읍 or 면, so a house
/// that is not under the stated one is looked for in the district.
fn district<'a>(address: &Address<'a>) -> Option<Address<'a>> {
    let areas = &address.areas;
    let keep = areas
        .iter()
        .rposition(|w| w.ends_with(['시', '군', '구']))
        .filter(|&i| i > 0 || areas[0].ends_with("특별자치시"))?;
    (keep + 1 < areas.len()).then(|| Address {
        areas: areas[..=keep].to_vec(),
        road: address.road,
        number: address.number.clone(),
        country: address.country,
        open: address.open,
        country_first: address.country_first,
    })
}

pub(super) fn parse(q: &str, partial: bool) -> Option<Address<'_>> {
    let open = !q.ends_with(|c: char| c.is_whitespace() || c == ',');
    let q = q.trim();
    if q.chars().take(257).count() > 256 {
        return None;
    }
    let mut parts = q.split(',').map(str::trim);
    let first = parts.next()?;
    let second = parts.next();
    if first.is_empty() || parts.next().is_some() {
        return None;
    }
    let (native, country, native_last) = if first.starts_with(|c| ('가'..='힣').contains(&c))
        && first.split_whitespace().nth(1).is_some()
    {
        (first, second.filter(|p| !p.is_empty()), second.is_none())
    } else {
        (second?, Some(first), true)
    };
    if !native.starts_with(|c| ('가'..='힣').contains(&c)) {
        return None;
    }
    let mut words: Vec<_> = native.split_whitespace().collect();
    if words.len() < 2 {
        return None;
    }
    let province = match words[0] {
        "서울" => "서울특별시",
        "부산" => "부산광역시",
        "대구" => "대구광역시",
        "인천" => "인천광역시",
        "광주" => "광주광역시",
        "대전" => "대전광역시",
        "울산" => "울산광역시",
        "세종" => "세종특별자치시",
        "경기" => "경기도",
        "강원" => "강원특별자치도",
        "충북" => "충청북도",
        "충남" => "충청남도",
        "전북" => "전북특별자치도",
        "전남" => "전라남도",
        "경북" => "경상북도",
        "경남" => "경상남도",
        "제주" => "제주특별자치도",
        name => name,
    };
    if !province.chars().all(|c| ('가'..='힣').contains(&c))
        || !["특별시", "광역시", "특별자치시", "도"]
            .iter()
            .any(|suffix| province.ends_with(suffix))
    {
        return None;
    }
    let last = words.last()?;
    let numbered = last.starts_with(|c: char| c.is_ascii_digit())
        || last
            .strip_prefix('산')
            .is_some_and(|n| n.starts_with(|c: char| c.is_ascii_digit()));
    if !numbered && !partial {
        return None;
    }
    let mut number = if numbered {
        words.pop()?.to_string()
    } else {
        String::new()
    };
    let mountain = number.starts_with('산') || words.last() == Some(&"산");
    if mountain {
        number = number.trim_start_matches('산').to_string();
        if words.last() == Some(&"산") {
            words.pop();
        }
    }
    let digits: Vec<_> = number.split('-').collect();
    if numbered
        && (digits.len() > 2
            || digits.iter().enumerate().any(|(i, d)| {
                (d.is_empty() && !(partial && i == 1)) || !d.bytes().all(|b| b.is_ascii_digit())
            }))
    {
        return None;
    }
    if mountain {
        number.insert(0, '산');
    }
    let road = words.pop()?;
    if (!partial && !road.ends_with(['로', '길', '동', '리', '가']))
        || !road
            .chars()
            .all(|c| ('가'..='힣').contains(&c) || c.is_ascii_digit())
        || words.is_empty()
        || words.iter().skip(1).any(|w| {
            !w.ends_with(['시', '군', '구', '읍', '면', '동', '리'])
                || !w
                    .chars()
                    .all(|c| ('가'..='힣').contains(&c) || c.is_ascii_digit())
        })
    {
        return None;
    }
    // ponytail: shorthand needs a city/county/district; province-only forms need namesake disambiguation first.
    if province != words[0]
        && words
            .get(1)
            .is_none_or(|w| !w.ends_with(['시', '군', '구']))
    {
        return None;
    }
    words[0] = province;
    Some(Address {
        areas: words,
        road,
        number,
        country,
        open: open && native_last,
        country_first: native_last && country.is_some(),
    })
}

impl Index {
    pub(crate) fn korean_region(&self, q: &str) -> Option<Hit> {
        let (name, country) = q.split_once(',')?;
        if country.contains(',') || self.country(country).as_deref() != Some("kr") {
            return None;
        }
        let key = name_key(name);
        let mut regions = self.regions.values().copied().filter(|&p| {
            let place = &self.places[p as usize];
            place.cc.as_ref() == "kr" && place.named(key)
        });
        let region = regions.next()?;
        if regions.next().is_some() {
            return None;
        }
        let (towns, _) = self.places_exact(name, Some("kr"), None, &|p| {
            matches!(p.subtype.as_ref(), "locality" | "localadmin") && p.named(key)
        });
        towns.is_empty().then_some(Hit::Place(region))
    }

    fn korean_scope(&self, address: &Address<'_>) -> Option<Vec<u32>> {
        let country = match address.country {
            Some(country) => Some(self.country(country)?),
            None => None,
        };
        let mut scope = vec![];
        for (i, area) in address.areas.iter().enumerate() {
            let key = name_key(area);
            let (found, _) = self.places_exact(area, Some("kr"), None, &|p| {
                p.named(key) && (i > 0 || p.subtype.as_ref() == "region")
            });
            if i == 0 && found.is_empty() {
                return None;
            }
            if country.as_deref().is_some_and(|c| c != "kr") {
                return Some(vec![]);
            }
            let next: Vec<_> = found
                .into_iter()
                .filter_map(|h| match h {
                    Hit::Place(id)
                        if scope.is_empty() || self.chain(id).iter().any(|p| scope.contains(p)) =>
                    {
                        Some(id)
                    }
                    _ => None,
                })
                .collect();
            if next.is_empty() {
                return Some(vec![]);
            }
            scope = next;
        }
        Some(scope)
    }

    pub(crate) fn korean_search(&self, q: &str) -> Option<Vec<Hit>> {
        let address = parse(q, false)?;
        let scope = self.korean_scope(&address)?;
        if scope.is_empty() {
            return Some(vec![]);
        }
        let hits = self.korean_houses(&address, &scope)?;
        match district(&address) {
            Some(district) if hits.is_empty() => {
                let scope = self.korean_scope(&district)?;
                self.korean_houses(&district, &scope)
            }
            _ => Some(hits),
        }
    }

    fn korean_houses(&self, address: &Address<'_>, scope: &[u32]) -> Option<Vec<Hit>> {
        let mut cl = self.musts(self.f.names, address.road);
        if cl.is_empty() {
            return Some(vec![]);
        }
        cl.push(must(Box::new(BooleanQuery::new(
            scope
                .iter()
                .map(|id| (Occur::Should, term(self.f.ids, &id.to_string())))
                .collect(),
        ))));
        let top = self.top_of(Kind::Street, Some("kr"), cl, 1000);
        if top.len() == 1000 {
            return Some(vec![]);
        }
        let mut hits = vec![];
        for a in top {
            let doc: tantivy::TantivyDocument = self.searcher.doc(a).ok()?;
            let name = best_name(&doc, self.f.name, address.road);
            if fold(&name) != fold(address.road) {
                continue;
            }
            let sid = self.fast(a, "id") as u32;
            let mut key = sid.to_be_bytes().to_vec();
            key.extend_from_slice(schema::number_key(&address.number).as_bytes());
            let Some(v) = self.numbers[sid as usize % self.numbers.len()].get(&key) else {
                continue;
            };
            let (x, y, own) = schema::unpack(v);
            let postcode = if own > 0 {
                use tantivy::schema::Value as _;
                doc.get_all(self.f.pcode)
                    .nth(own as usize - 1)
                    .and_then(|v| v.as_str())
                    .map(String::from)
            } else {
                None
            };
            hits.push(Hit::Street {
                doc: a,
                sid,
                place: self.fast(a, "place") as u32,
                name,
                x,
                y,
                number: Some(address.number.clone()),
                postcode,
                city: None,
                postal_town: None,
                cc: String::new(),
            });
        }
        hits.sort_by_key(|h| match h {
            Hit::Street { sid, .. } => *sid,
            _ => 0,
        });
        // ponytail: points within roughly 100 m share an answer; building-level disambiguation needs source ids in the number index.
        if let Some(Hit::Street { x, y, .. }) = hits.first()
            && hits.iter().all(
                |h| matches!(h, Hit::Street { x: hx, y: hy, .. } if (hx - x).hypot(hy - y) < 0.001),
            )
        {
            hits.truncate(1);
        }
        Some(hits)
    }

    pub(crate) fn korean_fallback(&self, q: &str) -> Option<Hit> {
        let address = parse(q, false)?;
        let scope = self.korean_scope(&address)?;
        if scope.is_empty() {
            return None;
        }
        let local = |p: &Place| matches!(p.subtype.as_ref(), "locality" | "localadmin");
        if address.road.ends_with(['동', '리', '가']) {
            let key = name_key(address.road);
            let (places, _) = self.places_exact(address.road, Some("kr"), None, &|p| {
                local(p) && p.named(key)
            });
            if let Some(hit) = places.into_iter().find(|h| matches!(h, Hit::Place(id) if self.chain(*id).iter().any(|p| scope.contains(p)))) {
                return Some(hit);
            }
        }
        scope
            .into_iter()
            .find(|&p| local(&self.places[p as usize]))
            .map(Hit::Place)
    }

    pub(crate) fn korean_address_matches(
        &self,
        q: &str,
        lat: f64,
        lon: f64,
        road: &str,
        number: &str,
        good_match: bool,
    ) -> bool {
        let Some(address) = parse(q, false) else {
            return true;
        };
        let key = name_key(address.areas[0]);
        let (regions, _) = self.places_exact(address.areas[0], Some("kr"), None, &|p| {
            p.subtype.as_ref() == "region" && p.named(key)
        });
        if regions.is_empty() {
            return true;
        }
        if let Some(country) = address.country {
            let Some(country) = self.country(country) else {
                return true;
            };
            if country != "kr" {
                return false;
            }
        }
        if !address.house_matches(road, number, good_match, || {
            self.korean_search(q).is_some_and(|hits| hits.len() == 1)
        }) {
            return false;
        }
        self.areas
            .iter()
            .flat_map(|a| a.containing(lat, lon))
            .find(|&(kind, _)| kind == 1)
            .is_none_or(|(_, p)| self.places[p as usize].named(key))
    }

    pub(crate) fn korean_suggest(
        &self,
        text: &str,
        cc: Option<&str>,
        want: usize,
    ) -> Option<Vec<Suggestion>> {
        let address = parse(text, true)?;
        let scope = self.korean_scope(&address)?;
        if scope.is_empty() || cc.is_some_and(|c| !c.eq_ignore_ascii_case("kr")) {
            return Some(vec![]);
        }
        let open = address.open;
        let number = (!address.number.is_empty()).then_some(address.number.as_str());
        let road_open = open && number.is_none();
        let road = fold(address.road);
        let typed = |scope: &[u32]| -> Vec<Suggestion> {
            self.streets_typed(
                &words_open(address.road, road_open),
                road_open,
                Some(scope),
                number,
                open,
                Some("kr"),
                None,
                None,
                want.max(5),
            )
            .into_iter()
            .filter(|s| {
                s.display_name
                    .split(", ")
                    .nth(usize::from(s.kind == "address"))
                    .map(fold)
                    .is_some_and(|name| name == road || road_open && name.starts_with(&road))
            })
            .take(want)
            .collect()
        };
        let found = typed(&scope);
        match district(&address) {
            Some(district) if found.is_empty() && !road_open => {
                Some(typed(&self.korean_scope(&district)?))
            }
            _ => Some(found),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weak_country_first_fallbacks_keep_the_existing_confidence_contract() {
        let first = parse("South Korea, 서울특별시 가상구 새로12길 999", false).unwrap();
        let last = parse("서울특별시 가상구 새로12길 999, South Korea", false).unwrap();
        for road in ["새로12길", "다른길"] {
            assert!(first.house_matches(road, "999", false, || false));
            assert!(!first.house_matches(road, "999", true, || false));
            for good in [false, true] {
                assert!(!last.house_matches(road, "999", good, || false));
            }
        }
        assert!(first.house_matches("새로12길", "999", true, || true));
    }

    #[test]
    fn country_first_keeps_the_native_hierarchy_and_number() {
        for (query, country) in [
            (
                "South Korea, 서울특별시 가상구 새로12길 13-34",
                "South Korea",
            ),
            ("대한민국, 서울특별시 가상구 새로12길 13-34", "대한민국"),
            ("South Korea, 서울 가상구 새로12길 13-34", "South Korea"),
        ] {
            let parsed = parse(query, false).unwrap();
            assert_eq!(parsed.areas, ["서울특별시", "가상구"]);
            assert_eq!((parsed.road, parsed.number.as_str()), ("새로12길", "13-34"));
            assert_eq!(parsed.country, Some(country));
        }
        for (query, open) in [
            ("South Korea, 서울특별시 가상구 새로1", true),
            ("South Korea, 서울특별시 가상구 새로1 ", false),
            ("서울특별시 가상구 새로1, South Korea", false),
            ("서울특별시 가상구 새로1", true),
            ("서울특별시 가상구 새로1,", false),
        ] {
            assert_eq!(parse(query, true).unwrap().open, open, "{query}");
        }
        let lot = parse("대한민국, 경기도 가상시 초월읍 옛리 산 45-6", false).unwrap();
        assert_eq!(lot.number, "산45-6");
        for query in [
            ", 서울특별시 가상구 새로12길 13-34",
            "South Korea, 서울특별시 가상구 새로12길 13-34, Japan",
            "South Korea, 10 Downing Street",
            "South Korea, 서울특별시 가상구 새로12길 13-",
        ] {
            assert!(parse(query, false).is_none(), "{query}");
        }
    }

    #[test]
    fn short_provinces_keep_an_explicit_city_county_or_district() {
        for (short, full) in [
            ("서울", "서울특별시"),
            ("부산", "부산광역시"),
            ("대구", "대구광역시"),
            ("인천", "인천광역시"),
            ("광주", "광주광역시"),
            ("대전", "대전광역시"),
            ("울산", "울산광역시"),
            ("세종", "세종특별자치시"),
            ("경기", "경기도"),
            ("강원", "강원특별자치도"),
            ("충북", "충청북도"),
            ("충남", "충청남도"),
            ("전북", "전북특별자치도"),
            ("전남", "전라남도"),
            ("경북", "경상북도"),
            ("경남", "경상남도"),
            ("제주", "제주특별자치도"),
        ] {
            let query = format!("{short} 가상시 가상구 새로12길 13-34, South Korea");
            let parsed = parse(&query, false).unwrap();
            assert_eq!(parsed.areas, [full, "가상시", "가상구"]);
            assert_eq!((parsed.road, parsed.number.as_str()), ("새로12길", "13-34"));
        }
        let partial = parse("경기 가상시 새로1", true).unwrap();
        assert_eq!(partial.areas, ["경기도", "가상시"]);
        for query in [
            "광주 새로12길 13",
            "제주 제주시 1",
            "경기 가상면 새로12길 13",
            "세종 새로12길 13",
            "가람 가상시 새로12길 13",
        ] {
            assert!(parse(query, false).is_none(), "{query}");
            assert!(parse(query, true).is_none(), "{query}");
        }
    }

    #[test]
    fn native_hierarchy_and_both_numbering_systems() {
        let road = parse("서울특별시 가상구 새로12길 13-34, South Korea", false).unwrap();
        assert_eq!(road.areas, ["서울특별시", "가상구"]);
        assert_eq!((road.road, road.number.as_str()), ("새로12길", "13-34"));
        for query in [
            "경기도 가상시 초월읍 옛리 산 45-6",
            "경기도 가상시 초월읍 옛리 산45-6",
        ] {
            let lot = parse(query, false).unwrap();
            assert_eq!(lot.areas, ["경기도", "가상시", "초월읍"]);
            assert_eq!((lot.road, lot.number.as_str()), ("옛리", "산45-6"));
        }
        for query in [
            "서울특별시 새로12길",
            "서울특별시 가상구 새로12길 1-",
            "서울특별시 가상구 새로12길 1-2-3",
            "서울특별시 rubbish 새로12길 1",
            "10 Downing Street, London",
        ] {
            assert!(parse(query, false).is_none(), "{query}");
        }
    }
}
