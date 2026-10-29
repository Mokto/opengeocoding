use super::*;

impl Index {
    /// Candidates for a free-text query, best first; at most ten. Blocks: call it off the
    /// async executor.
    pub fn search(&self, q: &str) -> Vec<Hit> {
        self.search_with(q, true)
    }

    /// `search`, and with `territories` a query of only a dependency's name (Guernsey) is in the
    /// dependency: the place of its name there the search finds first (Gibraltar's town, Saint
    /// Helena's island), else the dependency.
    fn search_with(&self, q: &str, territories: bool) -> Vec<Hit> {
        let q = q.trim();
        if q.is_empty() {
            return vec![];
        }
        if let Some(hits) = self.korean_search(q)
            && !hits.is_empty()
        {
            return hits;
        }
        // Japanese block addresses: libpostal splits them, and the data names the chōme
        // ("芝公園四丁目", number "2-8"), so they're read here.
        let mut chome = None;
        for (city, street, number) in jp::japanese(q) {
            let city = self.resolve_place(&city, Some("jp")).unwrap_or(city);
            let hits = self.streets(&street, Some(&number), Some(&city), None, Some("jp"), false);
            let housed = |h: &Hit| {
                matches!(
                    h,
                    Hit::Street {
                        number: Some(_),
                        ..
                    }
                )
            };
            if let Some(h) = hits.iter().find(|h| housed(h)) {
                return vec![h.clone()];
            }
            if chome.is_none() && street.ends_with("丁目") {
                chome = hits.into_iter().next();
            }
        }
        // the block's chōme when the house isn't in the data
        if let Some(h) = chome {
            return vec![h];
        }
        if let Some(p) = self.region_query(q) {
            return vec![Hit::Place(p)];
        }
        // "Birmingham, Birmingham, United Kingdom, United Kingdom": repeated parts parse as one
        // doubled value ("united kingdom united kingdom") that names nothing.
        let mut seen = std::collections::HashSet::new();
        let deduped: Vec<&str> = q
            .split(',')
            .map(str::trim)
            .filter(|p| seen.insert(p.to_lowercase()))
            .collect();
        let repeated = deduped.len() < q.split(',').count();
        let deduped = deduped.join(", ");
        let q = deduped.as_str();
        let parsed = self.parse(q);
        if let Ok(Some((_, hit))) = self.nz_comma_house(q, &parsed) {
            return vec![hit];
        }
        let get = |k: &str| parsed.iter().find(|(l, _)| l == k).map(|(_, v)| v.as_str());
        let country_text = get("country");
        let cc = country_text.and_then(|c| self.country(c));
        let cc = cc.as_deref();
        let city = get("city")
            .or(get("city_district"))
            .or(get("suburb"))
            .or(get("island"));
        let state = get("state")
            .or(get("state_district"))
            .or(get("country_region"));
        let local = |p: &Place| !matches!(&*p.subtype, "country" | "dependency");
        // A query that only names a country ("Malta", "North Macedonia") means the country,
        // unless it was repeated and the country has a town of that name ("Luxembourg,
        // Luxembourg", "Djibouti, Djibouti").
        let town = |p: &Place| matches!(&*p.subtype, "locality" | "localadmin");
        if let Some(c) = self.country(q)
            && let Some(&p) = (self.country_places.get(&c))
                .or_else(|| territories.then(|| self.dependencies.get(&c)).flatten())
            // "Lagos" is Laos in some language the data names it in, and a city to everyone: a
            // name the reference data doesn't give the country yields to a town of that name.
            // Its own or English name (Bonaire, Svalbard) is not such a name.
            && (q.contains(',')
                || !crate::matching::country_named(q).is_empty()
                || [&*self.places[p as usize].primary, self.places[p as usize].name(true)]
                    .iter()
                    .any(|name| name_key(name) == name_key(q))
                || self.places_exact(q, None, None, &town).0.is_empty())
        {
            if repeated && let (places, true) = self.places_exact(q, Some(&c), None, &town) {
                return places.into_iter().take(1).collect();
            }
            let territory = &self.places[p as usize];
            if territory.subtype.as_ref() == "dependency" {
                let found = self.search_with(q, false);
                if found
                    .first()
                    .is_some_and(|hit| self.hit_country(hit) == &*territory.cc)
                {
                    return found
                        .into_iter()
                        .filter(|hit| self.hit_country(hit) == &*territory.cc)
                        .collect();
                }
            }
            return vec![Hit::Place(p)];
        }
        if let Some(hit) = self.hk_chinese(q) {
            return vec![hit];
        }
        // Chinese road addresses ("上海市虹口区中山北一路295号"): libpostal splits the road
        // from its number; read them here, scoped by the written district or city.
        if let Some((units, road, number)) = cn_tw::chinese(q) {
            let hits = self.chinese_streets(&units, &road, &number, city, cc);
            if !hits.is_empty() {
                return hits;
            }
        }
        let mut hits = vec![];
        let mut roads: Vec<&str> = parsed
            .iter()
            .filter(|(l, _)| l == "road")
            .map(|(_, v)| v.as_str())
            .collect();
        // The street is the road written next to the house number ("Pte.Roca 731, Rosario"
        // gives roads "pte.roca" and "rosario"), else the last road.
        if let Some(at) = parsed.iter().position(|(l, _)| l == "house_number") {
            let next_to = [at.checked_sub(1), Some(at + 1)]
                .into_iter()
                .flatten()
                .find(|&i| parsed.get(i).is_some_and(|(l, _)| l == "road"));
            if let Some(i) = next_to {
                let street = parsed[i].1.as_str();
                roads.retain(|r| *r != street);
                roads.push(street);
            }
        }
        if let Some(road) = roads.last().map(|r| without_number_mark(r)) {
            let exact = cc == Some("nz")
                && matches!(q.split(',').count(), 3 | 4)
                && q.rsplit(',')
                    .next()
                    .and_then(|c| self.country(c))
                    .as_deref()
                    == Some("nz")
                && self
                    .parser
                    .parse(q.split(',').next().unwrap())
                    .is_ok_and(|head| {
                        head_aligns(&head)
                            && one_house(&head)
                            && ["road", "house_number", "unit"]
                                .iter()
                                .all(|key| field(&head, key) == field(&parsed, key))
                    });
            // libpostal often puts the town in another part: "zürich - birchstrasse 160" gives
            // house "zürich", "fra - seynod - 26 rue de sansy" a first road "fra seynod". The
            // street is the last road; the town, if none is labelled, the first other part that
            // names a place.
            let resolved_city = match city {
                Some(c) => Some(self.resolve_place(c, cc).unwrap_or_else(|| c.to_string())),
                None => get("house")
                    .into_iter()
                    .chain(roads[..roads.len() - 1].iter().copied())
                    .find_map(|t| self.resolve_place(t, cc)),
            };
            hits = self.streets_in(
                road,
                get("house_number").map(house_number),
                resolved_city.as_deref(),
                es::suburb(cc, get("city").and(get("suburb"))),
                get("postcode"),
                cc,
                exact,
            );
            if let Some(found) = self.hk_house_anywhere(
                &hits,
                road,
                get("house_number").map(house_number),
                city,
                get("postcode"),
                cc,
                exact,
            ) {
                hits = found;
            }
            let housed = |h: &Hit| {
                matches!(
                    h,
                    Hit::Street {
                        number: Some(_),
                        ..
                    }
                )
            };
            if cc == Some("ee")
                && !hits.iter().any(housed)
                && let Some(bare) = ee::without_initials(road)
            {
                let found: Vec<Hit> = self
                    .streets_in(
                        &bare,
                        get("house_number").map(house_number),
                        resolved_city.as_deref(),
                        None,
                        get("postcode"),
                        cc,
                        false,
                    )
                    .into_iter()
                    .filter(|h| {
                        matches!(h, Hit::Street { number: Some(_), name, .. }
                            if crate::matching::roads_agree(road, name))
                    })
                    .collect();
                if !found.is_empty() {
                    hits = found;
                }
            }
            // "70 N Professor St, Oberlin 44074" parses one road "n professor st oberlin": with
            // no town labelled, the road's last words may be it.
            if hits.is_empty() && city.is_none() && (cc.is_some() || get("postcode").is_some()) {
                let words: Vec<&str> = road.split_whitespace().collect();
                // a street-type word ("road", "st") or a short one is no town
                let kind = |w: &str| {
                    let f = fold(w);
                    f.len() < 3 || canon::canon(&f).is_some() || canon::is_canonical(&f)
                };
                for k in 1..words.len().min(3) {
                    let (street, town) = words.split_at(words.len() - k);
                    if town.iter().any(|w| kind(w)) || street.iter().all(|w| fold(w).len() < 3) {
                        continue;
                    }
                    let town = town.join(" ");
                    if let Some(town) = self.resolve_place(&town, cc) {
                        hits = self.streets(
                            &street.join(" "),
                            get("house_number").map(house_number),
                            Some(&town),
                            get("postcode"),
                            cc,
                            false,
                        );
                        if !hits.is_empty() {
                            break;
                        }
                    }
                }
            }
        }
        // "1516 Ruth Avenue, OH": a house with only its US state to place it, answered when the
        // state holds that number on one street of that name.
        if hits.is_empty()
            && city.is_none()
            && get("postcode").is_none()
            && cc.is_none_or(|c| c == "us")
            && let (Some(road), Some(n), Some(state)) =
                (roads.last(), get("house_number").map(house_number), state)
            && let Some(region) = self.us_state(state)
            && self.house_unique_in(without_number_mark(road), n, region)
        {
            let name = self.places[region as usize].primary.to_string();
            let found: Vec<Hit> = self
                .streets_by(
                    without_number_mark(road),
                    Some(n),
                    Some(&name),
                    &[region],
                    None,
                    Some("us"),
                    false,
                    false,
                    None,
                )
                .into_iter()
                .filter(|h| {
                    matches!(
                        h,
                        Hit::Street {
                            number: Some(_),
                            ..
                        }
                    )
                })
                .collect();
            hits = found.into_iter().take(1).collect();
        }
        // Village addresses ("Libá 156, 351 31 Libá"): the number belongs to the village,
        // indexed as a street of that name, not another the search returns ("Maleny Vista");
        // an unspaced name ("帯屋町二丁目" for "帯屋町") is taken as found. Only an exact house
        // answers.
        if roads.is_empty()
            && let (Some(n), Some(town)) = (get("house_number").map(house_number), city)
        {
            let found = self.streets(town, Some(n), Some(town), get("postcode"), cc, false);
            if let Some(h) = found.into_iter().find(|h| {
                matches!(
                    h,
                    Hit::Street {
                        number: Some(_),
                        name,
                        ..
                    } if fold(name) == fold(town) || !name.contains(' ')
                )
            }) {
                return vec![h];
            }
        }
        // libpostal reads a numbered street or village with no street type as a venue ("Świtały
        // 12, 76-231" and "św. Jana 10, 36-050 Sokołów" give a house and no road), and a street
        // before its number too ("Panorama Place 3, Maleny"): only an exact house on a street of
        // that name answers. So with a number it leaves in the road ("Rakowice Małe 20BS,
        // 59-600"), or in a venue that ends in a street type ("Railway Terrace 9, Mount
        // Pleasant").
        let venue = roads
            .is_empty()
            .then(|| get("house").zip(get("house_number").map(house_number)))
            .flatten();
        let street_type = |name: &str| {
            name.split_whitespace().last().is_some_and(|w| {
                let f = fold(w);
                canon::canon(&f).is_some() || canon::is_canonical(&f)
            })
        };
        let in_road = (hits.is_empty() && get("house_number").is_none())
            .then(|| match roads.last() {
                Some(r) => r.rsplit_once(' '),
                None => get("house")
                    .and_then(|h| h.rsplit_once(' '))
                    .filter(|(name, _)| street_type(name)),
            })
            .flatten()
            .filter(|(_, n)| n.starts_with(|c: char| c.is_ascii_digit()));
        if let Some((name, n)) = venue.or(in_road)
            && (city.is_some() || get("postcode").is_some())
        {
            let town = city.map(|c| self.resolve_place(c, cc).unwrap_or_else(|| c.to_string()));
            let found = self.streets(name, Some(n), town.as_deref(), get("postcode"), cc, false);
            if let Some(h) = found.into_iter().find(|h| {
                matches!(
                    h,
                    Hit::Street {
                        number: Some(_),
                        ..
                    }
                )
            }) {
                return vec![h];
            }
        }
        // "Santiago, Metropolitana, Chile" parses "santiago" as a road: without a house number,
        // an unmatched road is tried as a place name too.
        let road_as_place = hits.is_empty() && get("house_number").is_none();
        if get("road").is_none() || road_as_place {
            // libpostal's labels for place-only text are unreliable ("Doha, Ad Dawhah, Qatar"
            // gives house "doha", city "ad dawhah"): try each place-like part in turn.
            const PLACE_LABELS: [&str; 8] = [
                "city",
                "city_district",
                "suburb",
                "island",
                "house",
                "state_district",
                "state",
                "country_region",
            ];
            // in the order written: the first part is usually the most specific
            let texts: Vec<&str> = parsed
                .iter()
                .filter(|(l, _)| {
                    PLACE_LABELS.contains(&l.as_str()) || (road_as_place && l == "road")
                })
                .map(|(_, v)| v.as_str())
                // The country's own text, or another of its names, unless that also names a place
                // there: Finnish calls Brazil "Brasilia", and some languages Tunisia "Tunis".
                .filter(|t| {
                    !(Some(*t) == country_text
                        || (cc.is_some()
                            && self.country(t).as_deref() == cc
                            && !self.places_exact(t, cc, None, &local).1))
                })
                .collect();
            if let Some(pc) = get("postcode") {
                let found = self.postcodes(pc, cc);
                let countries: std::collections::HashSet<&str> = found
                    .iter()
                    .filter_map(|h| match h {
                        Hit::Postcode { place, .. } if *place != NONE => {
                            Some(&*self.places[*place as usize].cc)
                        }
                        _ => None,
                    })
                    .collect();
                // A bare postcode that several countries use is a guess without a country.
                if cc.is_some() || !texts.is_empty() || countries.len() <= 1 {
                    hits = found;
                }
            }
            let mut matched = false;
            // all parts together first: "région bruxelles-capitale" is one name, and "région"
            // alone matches every region
            let joined = texts.join(" ");
            // ...but not with a code ("Stratford, CT" is not "Stratford Court")
            let tries = (texts.len() > 1 && texts.iter().all(|t| t.trim().len() > 3))
                .then_some(joined.as_str())
                .into_iter()
                .chain(texts.iter().copied());
            // the first exact match, else the first match
            let mut best: Option<Vec<Hit>> = None;
            for text in tries {
                let within = state.filter(|s| *s != text);
                let (places, exact) = self.places_exact(text, cc, within, &local);
                if exact {
                    best = Some(places);
                    break;
                }
                // Without a country, a partial name ("beach gardens" in Palm Beach Gardens) is a
                // guess anywhere in the world.
                if best.is_none() && !places.is_empty() && cc.is_some() {
                    best = Some(places);
                }
            }
            if let Some(places) = best {
                // a postcode elsewhere ("00871" is Warsaw's; Lakeport is American) doesn't
                // belong to this place
                let place_cc = |h: &Hit| match h {
                    Hit::Place(p) | Hit::Postcode { place: p, .. } if *p != NONE => {
                        Some(self.places[*p as usize].cc.clone())
                    }
                    _ => None,
                };
                let found: std::collections::HashSet<_> =
                    places.iter().filter_map(place_cc).collect();
                hits.retain(|h| place_cc(h).is_some_and(|c| found.contains(&c)));
                hits.extend(places);
                matched = true;
            }
            if texts.is_empty()
                && hits.is_empty()
                && let Some(text) = country_text
            {
                hits.extend(self.places(text, None, None, &|p| {
                    matches!(&*p.subtype, "country" | "dependency")
                }));
            }
            if !matched && !texts.is_empty() {
                // Like Nominatim, every stated part has to match: "Qwertyuiop 12345" is not
                // postcode 12345.
                hits.clear();
            }
            if !matched
                && !texts.is_empty()
                && get("house_number").is_none()
                && get("postcode").is_none()
            {
                // "Dubai, UAE" parses as one "house": the first comma part as a place, in the
                // last part's country if it names one.
                let parts: Vec<&str> = q
                    .split(',')
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .collect();
                let last_cc = (parts.len() > 1)
                    .then(|| self.country(parts[parts.len() - 1]))
                    .flatten();
                if let (Some(c), Some(first)) = (&last_cc, parts.first()) {
                    hits = self.places(first, Some(c), None, &local);
                }
            }
        }
        // "Gaza Strip, Palestine": the strip is its own country in the data (xg) and Palestine
        // is not a country place, so a search scoped to PS finds nothing. A user-assigned code
        // the reference files under the stated country is that place. "Georgia" is not one: its
        // code is a real country's, and the US state was found above.
        if hits.is_empty()
            && get("house_number").is_none()
            && roads.is_empty()
            && let Some(text) = state.or(city)
            && let Some(code) = self.country(text)
            && code.starts_with('x')
            && let Some(stated) = cc
        {
            let associated = crate::matching::implied_country(
                &[("state".to_string(), text.to_string())]
                    .into_iter()
                    .collect(),
            );
            if stated == associated
                && let Some(&p) = self.country_places.get(&code)
            {
                return vec![Hit::Place(p)];
            }
        }
        hits.truncate(10);
        hits
    }

    /// Whether every street named `road` in place `within` that holds house `number` holds it
    /// within about a kilometre of the others: one house, perhaps on several segments.
    fn house_unique_in(&self, road: &str, number: &str, within: u32) -> bool {
        const MAX: usize = 1000;
        let mut cl = self.musts(self.f.names, road);
        if cl.is_empty() {
            return false;
        }
        cl.push(must(term(self.f.ids, &within.to_string())));
        let top = self.top_of(Kind::Street, None, cl, MAX);
        let number = schema::number_key(number);
        let houses: Vec<(f64, f64)> = top
            .iter()
            .filter(|&&a| {
                let sid = self.fast(a, "id") as u32;
                self.house_record(sid, &number).is_some()
            })
            .map(|&a| (self.fast_f64(a, "x"), self.fast_f64(a, "y")))
            .collect();
        top.len() < MAX
            && houses
                .first()
                .is_some_and(|&(x0, y0)| houses.iter().all(|&(x, y)| (x - x0).hypot(y - y0) < 0.01))
    }

    pub(super) fn house_chain(&self, chain: &[u32], x: f64, y: f64) -> Option<Vec<u32>> {
        let region = |chain: &[u32]| {
            chain
                .iter()
                .copied()
                .find(|&p| self.places[p as usize].subtype.as_ref() == "region")
        };
        let named = region(chain)?;
        let spatial = self.chain(self.area_at(y, x).0?);
        let containing = region(&spatial)?;
        if named == containing
            || self.places[named as usize].cc != self.places[containing as usize].cc
        {
            return None;
        }
        Some(
            chain
                .iter()
                .copied()
                .take_while(|&p| {
                    !matches!(
                        self.places[p as usize].subtype.as_ref(),
                        "localadmin"
                            | "county"
                            | "region"
                            | "macroregion"
                            | "dependency"
                            | "country"
                    ) && !self.own_codes.contains_key(&p)
                })
                .chain(spatial)
                .collect(),
        )
    }

    pub(super) fn address(
        &self,
        chain: &[u32],
        en: bool,
        a: &mut Map<String, Value>,
        display: &mut Vec<String>,
    ) {
        let mut settled = false;
        for &p in chain {
            let pl = &self.places[p as usize];
            let key = if pl.agglomeration && settled {
                "municipality"
            } else {
                pl.key()
            };
            settled |= pl.subtype.as_ref() == "locality";
            if !a.contains_key(key) {
                a.insert(key.into(), pl.name(en).into());
                display.push(pl.name(en).into());
            }
            if !pl.cc.is_empty() {
                a.entry("country_code")
                    .or_insert_with(|| pl.cc.to_string().into());
            }
        }
        // Nominatim lists a dependency (Svalbard, Puerto Rico) as a state as well.
        if let Some(&dep) = chain
            .iter()
            .find(|&&p| self.places[p as usize].subtype.as_ref() == "dependency")
        {
            a.entry("state")
                .or_insert_with(|| self.places[dep as usize].name(en).into());
        }
        let sovereign = a
            .get("country_code")
            .and_then(Value::as_str)
            .filter(|cc| part_of_sovereign(cc))
            .and_then(sovereign)
            .and_then(|cc| Some((cc, self.country_name(cc)?)));
        if let Some((cc, name)) = sovereign {
            a.insert("country_code".into(), cc.into());
            a.insert("country".into(), name.into());
            display.push(name.into());
        }
        // The territory stays a region, so the answer is not only the country.
        if a.get("country_code")
            .and_then(Value::as_str)
            .is_some_and(country::ps::territory)
        {
            let territory = a.get("country").cloned();
            a.insert("country_code".into(), "ps".into());
            a.insert("country".into(), "Palestine".into());
            if let Some(name) = territory {
                a.entry("state").or_insert(name);
            }
            display.push("Palestine".into());
        }
        if let Some(code) = self.region_code(chain) {
            if let Some(province) = ie::irish_province(code) {
                a.insert("ISO3166-2-lvl5".into(), province.into());
            }
            a.insert("ISO3166-2-lvl4".into(), code.into());
        }
        if let Some(code) = chain.iter().find_map(|p| self.subdivision_codes.get(p)) {
            a.insert("ISO3166-2-lvl6".into(), code.to_string().into());
        }
        if let Some((code, _)) = self
            .provinces
            .iter()
            .find(|(_, p)| chain.first() == Some(*p))
        {
            a.insert("ISO3166-2-lvl5".into(), code.clone().into());
        }
    }

    /// The country code of a hit's place, as the data files it (re for Réunion).
    fn hit_country<'a>(&'a self, hit: &'a Hit) -> &'a str {
        match hit {
            Hit::Place(p) => &self.places[*p as usize].cc,
            Hit::Street { place, cc, .. } | Hit::Postcode { place, cc, .. } => {
                match self.places.get(*place as usize) {
                    Some(pl) if *place != NONE => &pl.cc,
                    _ => cc,
                }
            }
        }
    }

    /// A candidate as Nominatim-style JSON, with English names when `en`.
    pub fn render(&self, hit: &Hit, en: bool) -> Value {
        let mut a = Map::new();
        let mut display: Vec<String> = vec![];
        let (id, osm_type, class, typ, addresstype, x, y, bbox, namedetails) = match hit {
            Hit::Place(p) => {
                let pl = &self.places[*p as usize];
                let mut names = Map::new();
                names.insert("name".into(), pl.primary.to_string().into());
                namedetails(pl, &mut names);
                self.address(&self.chain(*p), en, &mut a, &mut display);
                let admin = matches!(
                    &*pl.subtype,
                    "country" | "dependency" | "region" | "macroregion" | "county" | "localadmin"
                );
                let (class, typ) = if admin {
                    ("boundary", "administrative")
                } else {
                    ("place", pl.key())
                };
                (
                    *p as u64,
                    "relation",
                    class,
                    typ,
                    pl.key(),
                    pl.x as f64,
                    pl.y as f64,
                    pl.bbox,
                    names,
                )
            }
            Hit::Street {
                sid,
                place,
                name,
                x,
                y,
                number,
                postcode,
                city,
                postal_town,
                cc,
                ..
            } => {
                if let Some(town) = postal_town {
                    a.insert("postal_town".into(), town.clone().into());
                }
                if let Some(pc) = postcode {
                    a.insert("postcode".into(), pc.clone().into());
                }
                if let Some(n) = number {
                    a.insert("house_number".into(), n.clone().into());
                    display.push(n.clone());
                }
                a.insert("road".into(), name.clone().into());
                display.push(name.clone());
                if *place != NONE {
                    let chain = self.chain(*place);
                    let chain = number
                        .as_ref()
                        .and_then(|_| self.house_chain(&chain, *x, *y))
                        .unwrap_or(chain);
                    self.address(&chain, en, &mut a, &mut display);
                } else if let Some(country) = self.country_name(cc) {
                    a.insert("country_code".into(), cc.clone().into());
                    a.insert("country".into(), country.into());
                    display.push(country.into());
                }
                if let Some(c) = city {
                    // Nominatim names the postal city and lists the smaller locality as suburb.
                    for key in ["city", "town", "village"] {
                        if let Some(v) = a.remove(key) {
                            a.entry("suburb").or_insert(v);
                        }
                    }
                    a.insert("city".into(), c.clone().into());
                }
                let id = match number {
                    Some(n) => {
                        HOUSE
                            | (*sid as u64) << 8
                                ^ n.bytes()
                                    .fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64))
                                    & 0xff_ffff_ffff
                    }
                    None => STREET | *sid as u64,
                };
                let d = 0.001;
                let (class, typ, at) = if number.is_some() {
                    ("place", "house", "place")
                } else {
                    ("highway", "residential", "road")
                };
                let names = Map::from_iter([("name".to_string(), Value::from(name.clone()))]);
                (
                    id,
                    if number.is_some() { "node" } else { "way" },
                    class,
                    typ,
                    at,
                    *x,
                    *y,
                    [*x as f32 - d, *y as f32 - d, *x as f32 + d, *y as f32 + d],
                    names,
                )
            }
            Hit::Postcode {
                display: pc,
                cc,
                place,
                x,
                y,
            } => {
                a.insert("postcode".into(), pc.clone().into());
                display.push(pc.clone());
                if *place != NONE {
                    self.address(&self.chain(*place), en, &mut a, &mut display);
                }
                if !cc.is_empty() {
                    a.entry("country_code").or_insert_with(|| cc.clone().into());
                }
                let id = POSTCODE
                    | pc.bytes()
                        .fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64))
                        & 0xff_ffff_ffff;
                let d = 0.01;
                (
                    id,
                    "relation",
                    "place",
                    "postcode",
                    "postcode",
                    *x,
                    *y,
                    [*x as f32 - d, *y as f32 - d, *x as f32 + d, *y as f32 + d],
                    Map::new(),
                )
            }
        };
        let [x0, y0, x1, y1] = bbox;
        display.dedup();
        json!({
            "place_id": id,
            "osm_type": osm_type,
            "osm_id": id,
            "lat": y.to_string(),
            "lon": x.to_string(),
            "class": class,
            "type": typ,
            "addresstype": addresstype,
            "display_name": display.join(", "),
            "address": a,
            "namedetails": namedetails,
            "boundingbox": [y0.to_string(), y1.to_string(), x0.to_string(), x1.to_string()],
        })
    }

    pub(crate) fn house_locality_named(&self, hit: &Hit, name: &str) -> bool {
        matches!(
            hit,
            Hit::Street {
                number: Some(_),
                ..
            }
        ) && self.street_locality_named(hit, name)
    }

    /// The house's source names its town so (New Zealand's town_city, around its locality).
    pub(super) fn house_town_named(&self, hit: &Hit, name: &str) -> bool {
        use tantivy::schema::Value as _;
        let Hit::Street {
            doc,
            number: Some(number),
            ..
        } = hit
        else {
            return false;
        };
        let Ok(doc) = self.searcher.doc::<tantivy::TantivyDocument>(*doc) else {
            return false;
        };
        let number = schema::number_key(number);
        doc.get_all(self.f.postal_town).any(|value| {
            value
                .as_str()
                .and_then(|v| serde_json::from_str::<(&str, &str, &str, &str)>(v).ok())
                .is_some_and(|(_, house, town, source)| {
                    source == schema::TOWN_CITY
                        && house == number
                        && crate::matching::names_agree(name, town)
                })
        })
    }

    /// A town or municipality above the street is named so.
    pub(crate) fn street_locality_named(&self, hit: &Hit, name: &str) -> bool {
        let Hit::Street { place, .. } = hit else {
            return false;
        };
        if *place == NONE || name.chars().count() <= 2 {
            return false;
        }
        let key = name_key(name);
        self.chain(*place).iter().any(|&id| {
            let place = &self.places[id as usize];
            matches!(place.subtype.as_ref(), "locality" | "localadmin") && place.named(key)
        })
    }

    pub(crate) fn house_admin_locality_named(&self, hit: &Hit, name: &str) -> bool {
        let Hit::Street {
            place,
            number: Some(_),
            ..
        } = hit
        else {
            return false;
        };
        if *place == NONE || name.chars().count() <= 2 {
            return false;
        }
        let key = name_key(name);
        self.chain(*place).iter().any(|&id| {
            let place = &self.places[id as usize];
            place.settlement_twin && place.named(key)
        })
    }

    /// A county above the house is named `name`, and so is a town inside that county, its seat:
    /// Libreville's department holds the houses outside the town's own area, the City of
    /// Belgrade those of its municipalities. A `state`, if stated, names a region above them.
    pub(crate) fn house_seat_named(&self, hit: &Hit, name: &str, state: &str) -> bool {
        let Hit::Street {
            place,
            number: Some(_),
            ..
        } = hit
        else {
            return false;
        };
        if *place == NONE || name.chars().count() <= 2 {
            return false;
        }
        let key = name_key(name);
        let chain = self.chain(*place);
        let subtype = |id: u32| self.places[id as usize].subtype.as_ref();
        let Some(&county) = chain
            .iter()
            .find(|&&id| subtype(id) == "county" && self.places[id as usize].named(key))
        else {
            return false;
        };
        if !state.is_empty()
            && !chain.iter().any(|&id| {
                matches!(subtype(id), "region" | "macroregion")
                    && self.places[id as usize].named(name_key(state))
            })
        {
            return false;
        }
        let cc = &self.places[county as usize].cc;
        self.places(name, Some(cc), None, &|p| {
            p.subtype.as_ref() == "locality" && p.named(key)
        })
        .iter()
        .any(|h| matches!(h, Hit::Place(id) if self.chain(*id).contains(&county)))
    }

    pub(crate) fn isolated_house_without_postcode(
        &self,
        hit: &Hit,
        postcode: &str,
        country: &str,
    ) -> bool {
        use tantivy::schema::Value as _;
        let Hit::Street {
            name,
            number: Some(number),
            postcode: Some(actual),
            x,
            y,
            ..
        } = hit
        else {
            return false;
        };
        let (wanted, actual) = (pc_norm(postcode), pc_norm(actual));
        let numeric_typo = crate::matching::postcode_shape(country, &wanted) == Some(true)
            && wanted.len() == actual.len()
            && wanted
                .bytes()
                .chain(actual.bytes())
                .all(|b| b.is_ascii_digit())
            && wanted
                .bytes()
                .zip(actual.bytes())
                .filter(|(a, b)| a != b)
                .count()
                == 1;
        if country.is_empty() || !(numeric_typo || self.knows_postcode(postcode, Some(country))) {
            return false;
        }
        let terms = self.musts(self.f.names, name);
        if terms.is_empty() {
            return false;
        }
        let streets = self.top_of(Kind::Street, Some(country), terms, 1000);
        if streets.len() == 1000 {
            return false;
        }
        let mut found = false;
        for a in streets {
            let doc: tantivy::TantivyDocument = self.searcher.doc(a).unwrap();
            if doc.get_all(self.f.pcode).any(|value| {
                value
                    .as_str()
                    .is_some_and(|pc| crate::matching::postcodes_compatible(postcode, pc))
            }) {
                return false;
            }
            let sid = self.fast(a, "id") as u32;
            if let Some((_, value)) = self.house_record(sid, &schema::number_key(number)) {
                let (lon, lat, _) = schema::unpack(value);
                if ((lon - x) * y.to_radians().cos()).hypot(lat - y) * 111_195. > 100. {
                    return false;
                }
                found = true;
            }
        }
        found
    }

    pub fn details(&self, id: u64) -> Value {
        let Some(pl) = (id < STREET)
            .then(|| self.places.get(id as usize))
            .flatten()
        else {
            return json!({});
        };
        let mut names = Map::new();
        names.insert("name".into(), pl.primary.to_string().into());
        namedetails(pl, &mut names);
        json!({
            "place_id": id,
            "country_code": pl.cc,
            "admin_level": pl.admin_level(),
            "localname": pl.primary,
            "names": names,
            "addresstype": pl.key(),
            "centroid": {"type": "Point", "coordinates": [pl.x, pl.y]},
        })
    }
}

/// The territories an answer files under their sovereign's country: France's overseas
/// departments, Svalbard and the Caribbean Netherlands are parts of that country. The others
/// with an ISO 3166-1 code of their own (Puerto Rico, Hong Kong, Curaçao, ...) answer with it,
/// and, in the index, their sovereign's ISO 3166-2 code (US-PR) as their region, which
/// SearchAddress leaves out.
pub fn part_of_sovereign(territory: &str) -> bool {
    matches!(territory, "re" | "gp" | "mq" | "gf" | "yt" | "sj" | "bq")
}
