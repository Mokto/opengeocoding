use super::*;

pub(super) struct Address<'a> {
    pub areas: Vec<&'a str>,
    pub road: &'a str,
    pub number: String,
    pub country: Option<&'a str>,
}

pub(super) fn parse(q: &str, partial: bool) -> Option<Address<'_>> {
    let q = q.trim();
    if !q.starts_with(|c| ('가'..='힣').contains(&c)) || q.chars().take(257).count() > 256 {
        return None;
    }
    let mut parts = q.split(',').map(str::trim);
    let mut words: Vec<_> = parts.next()?.split_whitespace().collect();
    let country = parts.next().filter(|p| !p.is_empty());
    if parts.next().is_some() || words.len() < 2 {
        return None;
    }
    let province = words[0];
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
    Some(Address {
        areas: words,
        road,
        number,
        country,
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

    pub(super) fn korean_scope(&self, address: &Address<'_>) -> Option<Vec<u32>> {
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
        if (!road.is_empty() && fold(road) != fold(address.road))
            || (!number.is_empty() && !self.korean_search(q).is_some_and(|hits| hits.len() == 1))
        {
            return false;
        }
        self.areas
            .iter()
            .flat_map(|a| a.containing(lat, lon))
            .find(|&(kind, _)| kind == 1)
            .is_none_or(|(_, p)| self.places[p as usize].named(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
