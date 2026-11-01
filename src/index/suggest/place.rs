use super::*;

/// Places lighter than this are left to the term dictionary: only an exact word finds them.
const MIN_WEIGHT: u64 = 5_000;

/// Candidates kept for a prefix: enough for a qualifier ("Springfield, IL") to pick from.
pub(in crate::index) const CANDIDATES: usize = 400;

/// Of those, how many a qualifier is checked against: each reads its places' ancestors.
const EXAMINED: usize = 150;

/// How much a name's fit counts: the name itself, its main name starts with the text, another
/// language's does, a name has it inside (or a later word of the main name starts with it).
const FIT: [u64; 4] = [100, 20, 5, 1];

/// The most a country's name in a language that is not its main one is worth.
const OTHER_NAME_COUNTRY: u64 = 2_000_000;

/// How likely a place is the one wanted: its population, or for places the data gives none (most
/// hamlets, many regions) a guess by kind. Countries are all alike, and an area is not more
/// likely than the city it is named for (Tokyo, New York).
pub(in crate::index) fn weight(p: &Place) -> u64 {
    let pop = p.population as u64;
    match &*p.subtype {
        "country" | "dependency" => 50_000_000,
        "region" | "macroregion" => pop.clamp(300_000, 1_000_000),
        "county" | "macrocounty" => pop.clamp(10_000, 100_000),
        "localadmin" => pop.max(10_000),
        // a place the builder added from GeoNames weighs like a locality Overture gives no size,
        // however populous: it fills a gap and must not crowd out Overture's places
        "locality" if &*p.class == "geonames" => 100,
        // and one below cities500's bar after every other locality of its name
        "locality" if &*p.class == schema::SMALL_GEONAMES => 50,
        "locality" => pop.max(match &*p.class {
            "megacity" => 10_000_000,
            "city" => 50_000,
            "town" => 5_000,
            _ => 100,
        }),
        _ => pop.max(500),
    }
}

fn place_kind(p: &Place) -> &'static str {
    match p.key() {
        "state" => "region",
        "suburb" | "neighbourhood" => "neighbourhood",
        other => other,
    }
}

/// Folded name, a NUL, whether it is the primary or English name (0), another (1) or a main
/// name from its second word or a later one on (2: "york" of "new york"), and the place number
/// -> the place's weight. One entry per place and name, so places sharing a name have distinct
/// keys and a prefix is a range.
pub(in crate::index) fn place_names(places: &[Place]) -> Result<tantivy_fst::Map<Vec<u8>>> {
    use rayon::prelude::*;
    let mut keys: Vec<(Vec<u8>, u64)> = places
        .par_iter()
        .enumerate()
        .flat_map_iter(|(id, p)| {
            let w = weight(p);
            let mut names: Vec<(String, u8)> = vec![];
            if w >= MIN_WEIGHT {
                let main = [&*p.primary, p.name(true)];
                for n in p.all_names() {
                    let flag = u8::from(!main.contains(&n));
                    let n = fold(n);
                    match names.iter_mut().find(|(m, _)| *m == n) {
                        Some(m) => m.1 = m.1.min(flag),
                        None if !n.is_empty() => names.push((n, flag)),
                        None => {}
                    }
                }
                let inside: Vec<String> = names
                    .iter()
                    .filter(|(_, flag)| *flag == 0 && w >= CITY)
                    .flat_map(|(n, _)| {
                        let words: Vec<&str> = n.split(' ').collect();
                        (1..words.len())
                            .filter(|&i| words[i].chars().count() >= MIN_INSIDE)
                            .map(|i| words[i..].join(" "))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                for n in inside {
                    if !names.iter().any(|(m, _)| *m == n) {
                        names.push((n, 2));
                    }
                }
            }
            names.into_iter().map(move |(n, flag)| {
                let mut key = n.into_bytes();
                key.extend_from_slice(&[0, flag]);
                key.extend_from_slice(&(id as u32).to_be_bytes());
                (key, w)
            })
        })
        .collect();
    keys.par_sort_unstable();
    let mut build = tantivy_fst::MapBuilder::memory();
    for (key, w) in keys {
        build.insert(key, w)?;
    }
    Ok(tantivy_fst::Map::from_bytes(build.into_inner()?)?)
}

impl Index {
    /// Country codes of the countries a qualifier names, or starts to.
    pub(super) fn countries_named(&self, q: &str, open: bool) -> Vec<&str> {
        let names = self.suggest.country_names.get_or_init(|| {
            let mut all: Vec<_> = self
                .countries
                .iter()
                .map(|(n, cc)| (n.clone(), cc.clone()))
                .collect();
            all.sort_unstable();
            all
        });
        let from = names.partition_point(|(n, _)| n.as_str() < q);
        let mut out: Vec<&str> = names[from..]
            .iter()
            .take_while(|(n, _)| n == q || (open && q.len() >= 3 && n.starts_with(q)))
            .map(|(_, cc)| cc.as_str())
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// A place's folded name and folded English name.
    pub(super) fn folded_names(&self, id: u32) -> Arc<[String; 2]> {
        remember(&self.suggest.folded, id, || {
            let pl = &self.places[id as usize];
            Arc::new([fold(&pl.primary), fold(pl.name(true))])
        })
    }

    /// The place's name and what it lies in, as an address line: "Paris, Ile-de-France, France".
    pub(in crate::index) fn label(&self, id: u32) -> Arc<str> {
        remember(&self.suggest.labels, id, || {
            let mut keys: Vec<&str> = vec![];
            let mut parts: Vec<&str> = vec![];
            for p in self.chain(id) {
                let pl = &self.places[p as usize];
                if !keys.contains(&pl.key()) {
                    keys.push(pl.key());
                    parts.push(pl.name(true));
                }
            }
            parts.join(", ").into()
        })
    }

    /// Whether `place` lies in what each qualifier names: a country, a region or province by name
    /// or code ("Rho (MI)"), or any larger place around it.
    pub(super) fn qualifies(
        &self,
        place: u32,
        quals: &[(String, Vec<&str>)],
        open_last: bool,
    ) -> bool {
        let pl = &self.places[place as usize];
        let chain = self.chain(place);
        let country_named = quals.iter().any(|(_, ccs)| {
            ccs.contains(&ps::country(&pl.cc))
                || super::sovereign(&pl.cc).is_some_and(|cc| ccs.contains(&cc))
        });
        let matches =
            |n: &str, q: &str, open: bool| n == q || (open && q.len() >= 3 && n.starts_with(q));
        quals.iter().enumerate().all(|(i, (q, ccs))| {
            let open = open_last && i + 1 == quals.len();
            let code = format!("-{}", q.to_uppercase());
            let key = std::cell::LazyCell::new(|| super::name_key(q));
            ccs.contains(&ps::country(&pl.cc))
                || (q.len() <= 3 && pl.region.ends_with(&code))
                || chain.iter().skip(1).any(|&a| {
                    let names = self.folded_names(a);
                    matches(&names[0], q, open)
                        || matches(&names[1], q, open)
                        || ((ccs.is_empty() || country_named)
                            && self.places[a as usize].named(*key))
                        || (q.len() <= 3
                            && (self.places[a as usize].region.ends_with(&code)
                                || self
                                    .subdivision_codes
                                    .get(&a)
                                    .is_some_and(|c| c.ends_with(&code))))
                        || (pl.cc.eq_ignore_ascii_case("it")
                            && it::uncoded_province(q).is_some_and(|name| {
                                self.places[a as usize].named(super::name_key(name))
                            }))
                })
        })
    }

    /// The English or primary name of the best-known place whose name is one edit from `text`
    /// ("Mississauaga", "Soihull") when `text` names nothing, with its first two letters right.
    /// None when no place, or several equally well known, is that close.
    pub fn similar_place(&self, text: &str, cc: Option<&str>) -> Option<String> {
        let key = fold(text);
        if key.chars().count() < 5 || !key.chars().all(|c| c.is_alphabetic() || c == ' ') {
            return None;
        }
        let head: String = key.chars().take(2).collect();
        let mut to = head.as_bytes().to_vec();
        *to.last_mut()? += 1;
        let mut best: Option<(u64, u32)> = None;
        let mut tied = false;
        let dfa = ONE_EDIT.build_dfa(&key);
        let mut range = self
            .place_names
            .search(NameWithin(&dfa))
            .ge(head.as_bytes())
            .lt(&to)
            .into_stream();
        while let Some((k, w)) = range.next() {
            let at = k.len() - 6;
            let id = u32::from_be_bytes(k[at + 2..].try_into().ok()?);
            let Ok(name) = std::str::from_utf8(&k[..at]) else {
                continue;
            };
            if k[at + 1] == 2
                || name == key
                || !super::one_edit(&key, name)
                || !cc.is_none_or(|c| self.places[id as usize].cc.eq_ignore_ascii_case(c))
            {
                continue;
            }
            match best {
                Some((bw, bid)) if bid != id && w == bw => tied = true,
                Some((bw, _)) if w <= bw => {}
                _ => {
                    best = Some((w, id));
                    tied = false;
                }
            }
        }
        let (_, id) = best.filter(|_| !tied)?;
        Some(self.places[id as usize].name(true).to_owned())
    }

    /// The places of `place_names` whose name is `text`, or starts with it when `open`, best
    /// first: place, fit (0 the name itself, 1 its main name starts with the text, 2 another
    /// language's does, `INSIDE` a later word of its main name does) and weight. With `typo`,
    /// the names one edit from `text` (or starting so when `open`) and with its first letter, all
    /// of fit `TYPO`. Reading a short prefix's range is the cost, so it is kept.
    pub(in crate::index) fn name_candidates(
        &self,
        text: &str,
        open: bool,
        typo: bool,
    ) -> Candidates {
        if text.is_empty() {
            return Arc::new([]);
        }
        let key = (Box::<str>::from(text), open, typo);
        if let Some(hit) = self.place_prefixes.lock().unwrap().get(&key) {
            return hit.clone();
        }
        let inside = text.chars().count() >= MIN_INSIDE;
        let mut best: HashMap<u32, (u64, u8)> = HashMap::new();
        let mut add = |k: &[u8], w: u64| {
            let at = k.len() - 6;
            let id = u32::from_be_bytes(k[at + 2..].try_into().unwrap());
            let flag = k[at + 1];
            if flag == 2 && !inside {
                return;
            }
            let fit = match flag {
                _ if typo => TYPO,
                2 => INSIDE,
                _ if at == text.len() => 0,
                _ => 1 + flag,
            };
            // a country's weight is for its own name: another language's word that happens to
            // be "lagos" or start "lima" is not worth a country's
            let w = match &*self.places[id as usize].subtype {
                "country" | "dependency" if flag >= 1 => w.min(OTHER_NAME_COUNTRY),
                _ => w,
            };
            let score = w * FIT[if typo { 1 + flag } else { fit.min(3) } as usize];
            let e = best.entry(id).or_insert((score, fit));
            if score > e.0 {
                *e = (score, fit);
            }
        };
        if typo {
            let mut first = [0; 4];
            let from = text
                .chars()
                .next()
                .map_or(&[][..], |c| c.encode_utf8(&mut first).as_bytes());
            let mut to = from.to_vec();
            if let Some(last) = to.last_mut() {
                *last += 1;
            }
            let dfa = if open {
                ONE_EDIT.build_prefix_dfa(text)
            } else {
                ONE_EDIT.build_dfa(text)
            };
            let mut found = self
                .place_names
                .search(NameWithin(&dfa))
                .ge(from)
                .lt(&to)
                .into_stream();
            while let Some((k, w)) = found.next() {
                add(k, w);
            }
        } else {
            let (from, to) = if open {
                let mut to = text.as_bytes().to_vec();
                *to.last_mut().unwrap() += 1;
                (text.as_bytes().to_vec(), to)
            } else {
                (
                    [text.as_bytes(), &[0]].concat(),
                    [text.as_bytes(), &[1]].concat(),
                )
            };
            let mut range = self.place_names.range().ge(&from).lt(&to).into_stream();
            while let Some((k, w)) = range.next() {
                add(k, w);
            }
        }
        let mut all: Vec<(u32, u8, u64)> = best
            .into_iter()
            .map(|(id, (s, fit))| (id, fit, s))
            .collect();
        all.sort_unstable_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
        all.truncate(CANDIDATES);
        let all: Candidates = all.into();
        let mut cache = self.place_prefixes.lock().unwrap();
        // ponytail: dropped whole when full, not LRU
        if cache.len() >= MAX_CACHED {
            cache.clear();
        }
        cache.insert(key, all.clone());
        all
    }

    /// Places named by `parts` (the first part's name, then what it lies in), best first, with
    /// how well each name fits: 0 is a name of the place, 1 its main name (local or English)
    /// starts with the text, 2 a name in another language does, 3 a name has it inside,
    /// `INSIDE` a later word of its main name starts with it (only with `Names::Inside`), `TYPO`
    /// a name is one edit from it (only with `Names::Typo`).
    pub(super) fn suggest_places(
        &self,
        parts: &[String],
        open: bool,
        cc: Option<&str>,
        focus: Option<&Focus>,
        names: Names,
        want: usize,
    ) -> Vec<(u32, u8)> {
        let Some(first) = parts.first() else {
            return vec![];
        };
        let all = words_open(first, open && parts.len() == 1);
        // "paris fra": the whole text names a place, or its front does and its end says where
        let splits: Vec<usize> = if parts.len() == 1 && names != Names::Typo {
            std::iter::once(all.len())
                .chain((1..all.len()).rev().take(3))
                .collect()
        } else {
            vec![all.len()]
        };
        let typo = names == Names::Typo;
        let mut found: Vec<(u32, u8)> = vec![];
        for (n, split) in splits.into_iter().enumerate() {
            if n > 0 && found.len() >= want {
                break;
            }
            let quals: Vec<String> = if split < all.len() {
                vec![all[split..].join(" ")]
            } else {
                // "Portland, ME 04101": the region, not its postcode, qualifies the town
                parts[1..]
                    .iter()
                    .map(|p| {
                        let q = fold(p);
                        let named: Vec<&str> = q
                            .split(' ')
                            .filter(|w| !w.bytes().all(|b| b.is_ascii_digit()))
                            .collect();
                        if named.is_empty() { q } else { named.join(" ") }
                    })
                    .collect()
            };
            // a qualifier of one letter says nothing yet, and no place matches it
            let quals: Vec<String> = quals
                .into_iter()
                .filter(|q| !(open && q.chars().count() < 2))
                .collect();
            let head = &all[..split];
            let head_open = open && quals.is_empty();
            let pool = if quals.is_empty() {
                want.max(10) * 4
            } else {
                POOL
            };
            let quals: Vec<(String, Vec<&str>)> = quals
                .into_iter()
                .map(|q| {
                    let ccs = self.countries_named(&q, open);
                    (q, ccs)
                })
                .collect();
            let text = head.join(" ");
            if typo
                && (text.chars().count() < MIN_TYPO
                    || !text.chars().all(|c| c.is_alphabetic() || c == ' '))
            {
                return vec![];
            }
            let acceptable = |id: u32| {
                let pl = &self.places[id as usize];
                pl.subtype.as_ref() != "microhood"
                    && (pl.class.as_ref() == schema::SMALL_GEONAMES) == (names == Names::Small)
                    && cc.is_none_or(|c| pl.cc.eq_ignore_ascii_case(c))
                    && (quals.is_empty() || self.qualifies(id, &quals, open))
            };
            let mut ranked: Vec<(Reverse<u64>, u8, u32)> = vec![];
            let forms = if names == Names::Small {
                vec![]
            } else if head_open && !typo {
                open_forms(head)
            } else {
                vec![text.clone()]
            };
            let mut candidates: Vec<(u32, u8, u64)> = vec![];
            for form in &forms {
                candidates.extend(self.name_candidates(form, head_open, typo).iter());
            }
            // "koeln" is read as "köln" only when no place is named as typed
            let umlaut = umlaut_form(&text).filter(|_| !typo && names != Names::Small);
            for n in 0..2 {
                if n == 1 {
                    match &umlaut {
                        Some(alt) if ranked.is_empty() => {
                            candidates = self.name_candidates(alt, head_open, false).to_vec()
                        }
                        _ => break,
                    }
                }
                if let Some(focus) = focus {
                    for c in &mut candidates {
                        c.2 = self.focused(c.2, c.0, focus);
                    }
                }
                candidates.sort_unstable_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
                // candidates come best first, so the first `want` that qualify are the best
                for (examined, &(id, fit, score)) in candidates.iter().enumerate() {
                    if ranked.len() >= want || examined >= EXAMINED {
                        break;
                    }
                    if (names == Names::Inside || fit != INSIDE)
                        && !found.iter().any(|f| f.0 == id)
                        && !ranked.iter().any(|r| r.2 == id)
                        && acceptable(id)
                    {
                        ranked.push((Reverse(score), fit, id));
                    }
                }
            }
            // The rest of the places, by the words the term dictionary holds: those as typed
            // first, for the places a prefix adds ("paris" -> "parish") are many.
            if ranked.len() < want && !typo {
                let want_name = name_key(&text);
                let mut ids: Vec<u32> = vec![];
                for as_prefix in [false, true] {
                    if as_prefix && !head_open {
                        break;
                    }
                    let Some((cl, rarest)) = self.typed_words(self.f.names, head, as_prefix) else {
                        continue;
                    };
                    if as_prefix && rarest > MAX_PLACE_WORK {
                        continue;
                    }
                    let kind = if names == Names::Small {
                        Kind::Small
                    } else {
                        Kind::Searched
                    };
                    for a in self.top_of(kind, cc, cl, pool) {
                        let id = self.fast(a, "id") as u32;
                        if !ids.contains(&id)
                            && !found.iter().any(|f| f.0 == id)
                            && !ranked.iter().any(|r| r.2 == id)
                        {
                            ids.push(id);
                        }
                    }
                }
                for id in ids.into_iter().filter(|&id| acceptable(id)) {
                    let pl = &self.places[id as usize];
                    let starts = |n: &str| fold(n).starts_with(&text);
                    let fit = if pl.named(want_name) {
                        0
                    } else if starts(&pl.primary) || starts(pl.name(true)) {
                        1
                    } else if pl.all_names().any(starts) {
                        2
                    } else {
                        3
                    };
                    // the name fits less as less of it is typed
                    let score = weight(pl) * FIT[fit as usize];
                    let score = focus.map_or(score, |f| self.focused(score, id, f));
                    ranked.push((Reverse(score), fit, id));
                }
            }
            ranked.sort_unstable();
            found.extend(ranked.into_iter().map(|(_, fit, id)| (id, fit)));
        }
        found.truncate(want);
        found
    }

    pub(in crate::index) fn focused(&self, score: u64, id: u32, focus: &Focus) -> u64 {
        let pl = &self.places[id as usize];
        (score as f64 * focus.boost(pl.y as f64, pl.x as f64)) as u64
    }

    pub(super) fn place_suggestion(&self, id: u32) -> Suggestion {
        let pl = &self.places[id as usize];
        Suggestion {
            id: format!("p{id}"),
            retrieval_id: format!("{}/p{id}", self.suggestion_scope),
            display_name: self.label(id).to_string(),
            unverified_suffix: String::new(),
            kind: place_kind(pl),
            category: "",
            country_code: ps::country(&pl.cc).to_string(),
            x: pl.x as f64,
            y: pl.y as f64,
            // most towns' boxes are the point itself, which zooms a map no better than the point
            bbox: (pl.bbox.iter().all(|v| v.is_finite())
                && (pl.bbox[2] - pl.bbox[0]).max(pl.bbox[3] - pl.bbox[1]) > 1e-3)
                .then_some(pl.bbox),
        }
    }
}
