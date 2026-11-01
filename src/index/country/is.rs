use super::super::*;
use tantivy::schema::Value as _;

fn post_town(name: &str) -> String {
    fold(name.split(',').next().unwrap_or_default().trim())
}

pub(crate) fn numberless(index: &Index, query: &str) -> Option<Vec<Hit>> {
    let parts: Vec<_> = query.split(',').map(str::trim).collect();
    let [name, postal, country] = parts.as_slice() else {
        return None;
    };
    if index.country(country).as_deref() != Some("is")
        || name.is_empty()
        || name.chars().any(char::is_numeric)
    {
        return None;
    }
    let (postcode, city) = postal.split_once(' ')?;
    if postcode.len() != 3
        || !postcode.bytes().all(|b| b.is_ascii_digit())
        || city.trim().is_empty()
    {
        return None;
    }
    let field = index.searcher.schema().get_field("numberless").ok()?;
    let mut cl = index.musts(index.f.names, name);
    cl.push(must(term(index.f.pcode, postcode)));
    let docs = index.top_of(Kind::Street, Some("is"), cl, 1000);
    if docs.len() == 1000 {
        return Some(vec![]);
    }
    let mut hits = vec![];
    let mut supported = false;
    for a in docs {
        let doc = index.document(a).ok()?;
        if !doc
            .get_all(index.f.name)
            .filter_map(|v| v.as_str())
            .any(|n| fold(n) == fold(name))
        {
            continue;
        }
        for value in doc.get_all(field).filter_map(|v| v.as_str()) {
            let (pc, x, y): (String, f64, f64) = serde_json::from_str(value).ok()?;
            if pc != postcode {
                continue;
            }
            supported = true;
            let postal_town = postal_town(&doc, index.f.postal_town, "", Some(&pc));
            let sid = index.fast(a, "id") as u32;
            let hit = Hit::Street {
                doc: a,
                sid,
                place: index.fast(a, "place") as u32,
                name: stored(&doc, index.f.name),
                number: None,
                postcode: Some(pc),
                postal_town: postal_town.clone(),
                city: None,
                cc: "is".into(),
                x,
                y,
            };
            if (postal_town.is_some_and(|p| post_town(&p) == post_town(city))
                || index.street_locality_named(&hit, city))
                && !hits
                    .iter()
                    .any(|h| matches!(h, Hit::Street { sid: hs, x: hx, y: hy, .. } if *hs == sid && *hx == x && *hy == y))
            {
                hits.push(hit);
            }
        }
    }
    supported.then_some(hits)
}

pub(crate) fn country_conflict(index: &Index, query: &str, country: &str) -> bool {
    if country != "is"
        || index
            .country(query.rsplit(',').next().unwrap_or_default())
            .is_none_or(|c| c == "is")
    {
        return false;
    }
    let first = query.split(',').next().unwrap_or_default().trim();
    let name = first
        .rsplit_once(' ')
        .filter(|(_, n)| n.starts_with(|c: char| c.is_ascii_digit()))
        .map_or(first, |(name, _)| name);
    let Ok(field) = index.searcher.schema().get_field("iceland_alias") else {
        return false;
    };
    !index
        .top_of(
            Kind::Street,
            Some("is"),
            vec![must(term(field, &fold(name)))],
            1,
        )
        .is_empty()
}

pub(crate) fn alias_conflict(index: &Index, query: &str) -> bool {
    let parts: Vec<_> = query.split(',').map(str::trim).collect();
    if !matches!(parts.len(), 2 | 3)
        || index.country(parts[parts.len() - 1]).as_deref() != Some("is")
    {
        return false;
    }
    let (name, number) = parts[0]
        .rsplit_once(' ')
        .filter(|(_, n)| n.starts_with(|c: char| c.is_ascii_digit()))
        .map_or((parts[0], None), |(name, n)| (name, Some(n)));
    let scope = if parts.len() == 3 {
        let Some((pc, city)) = parts[1].split_once(' ') else {
            return false;
        };
        if pc.len() != 3 || !pc.bytes().all(|b| b.is_ascii_digit()) || city.trim().is_empty() {
            return false;
        }
        Some((pc, city))
    } else {
        None
    };
    let Ok(field) = index.searcher.schema().get_field("iceland_alias") else {
        return false;
    };
    let mut cl = vec![must(term(field, &fold(name)))];
    if let Some((pc, _)) = scope {
        cl.push(must(term(index.f.pcode, pc)));
    }
    let docs = index.top_of(Kind::Street, Some("is"), cl, 1000);
    if docs.len() == 1000 {
        return true;
    }
    let mut point = None;
    let mut contradicted = false;
    for a in docs {
        let hits = if let Some(number) = number {
            let sid = index.fast(a, "id") as u32;
            let Some((number, v)) = index.house_record(sid, &schema::number_key(number)) else {
                continue;
            };
            let key = [sid.to_be_bytes().as_slice(), number.as_bytes()].concat();
            let far = index.far[sid as usize % index.numbers.len()]
                .as_ref()
                .and_then(|m| m.get(&key));
            std::iter::once(v)
                .chain(far)
                .filter_map(|v| {
                    let (x, y, pc) = schema::unpack(v);
                    index.house_hit(a, number.clone(), x, y, pc)
                })
                .collect::<Vec<_>>()
        } else {
            vec![Hit::Street {
                doc: a,
                sid: index.fast(a, "id") as u32,
                place: index.fast(a, "place") as u32,
                name: String::new(),
                number: None,
                postcode: scope.map(|(pc, _)| pc.into()),
                postal_town: index.document(a).ok().and_then(|d| {
                    postal_town(&d, index.f.postal_town, "", scope.map(|(pc, _)| pc))
                }),
                city: None,
                cc: "is".into(),
                x: index.fast_f64(a, "x"),
                y: index.fast_f64(a, "y"),
            }]
        };
        for hit in hits {
            let Hit::Street {
                sid,
                postcode,
                postal_town,
                x,
                y,
                ..
            } = &hit
            else {
                continue;
            };
            if let Some((pc, city)) = scope {
                if postcode.as_deref() != Some(pc) {
                    continue;
                }
                if !(postal_town
                    .as_ref()
                    .is_some_and(|p| post_town(p) == post_town(city))
                    || index.street_locality_named(&hit, city))
                {
                    contradicted = true;
                    continue;
                }
            }
            if point.is_some_and(|p| p != (*sid, *x, *y)) {
                return true;
            }
            point = Some((*sid, *x, *y));
        }
    }
    point.is_none() && contradicted
}
