use super::*;

pub(super) struct Place {
    pub(super) subtype: Box<str>,
    pub(super) class: Box<str>,
    pub(super) cc: Box<str>,
    pub(super) region: Box<str>,
    pub(super) population: u32,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) bbox: [f32; 4],
    pub(super) parent: u32,
    pub(super) primary: Box<str>,
    /// `language=name` pairs joined by SEP.
    pub(super) names: Box<str>,
    /// Where the English name starts in `names` (NONE: none): a country's names run to
    /// thousands of bytes, and every rendered address names its country.
    pub(super) en: u32,
    /// Sorted `name_key` hashes of all names, so exact-name checks don't re-fold them.
    pub(super) keys: Box<[u64]>,
    /// A county that is a city in all but name: Overture has Birmingham only as a county with
    /// a same-named neighbourhood inside, and no locality.
    pub(super) city_like: bool,
    pub(super) settlement_twin: bool,
}

impl Place {
    pub(super) fn named(&self, key: u64) -> bool {
        self.keys.binary_search(&key).is_ok()
    }

    pub(super) fn name(&self, en: bool) -> &str {
        match self.en {
            at if en && at != NONE => self.names[at as usize..].split(SEP).next().unwrap_or(""),
            _ => &self.primary,
        }
    }

    pub(super) fn english(names: &str) -> u32 {
        let mut at = 0;
        for pair in names.split(SEP) {
            if pair.split_once('=').is_some_and(|(l, _)| l == "en") {
                return (at + "en=".len()) as u32;
            }
            at += pair.len() + SEP.len_utf8();
        }
        NONE
    }

    pub(super) fn pairs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.names.split(SEP).filter_map(|p| p.split_once('='))
    }

    pub(super) fn all_names(&self) -> impl Iterator<Item = &str> {
        std::iter::once(&*self.primary).chain(self.pairs().map(|(_, n)| n))
    }

    /// Nominatim's address key for this place.
    pub(super) fn key(&self) -> &'static str {
        match &*self.subtype {
            "country" | "dependency" => "country",
            "region" | "macroregion" => "state",
            "county" if self.city_like => "city",
            "county" => "county",
            "localadmin" => "municipality",
            "locality" => match &*self.class {
                "megacity" | "city" => "city",
                "town" => "town",
                "village" | "hamlet" => "village",
                _ if self.population >= 100_000 => "city",
                _ if self.population >= 10_000 => "town",
                _ => "village",
            },
            "macrohood" | "neighborhood" => "suburb",
            _ => "neighbourhood",
        }
    }

    pub(super) fn admin_level(&self) -> u32 {
        match &*self.subtype {
            "country" | "dependency" => 2,
            "region" => 4,
            "macroregion" => 5,
            "county" => 6,
            "localadmin" => 8,
            "locality" => 8,
            _ => 10,
        }
    }
}

/// Place number and ISO 3166-2 code of the places that stand for a subdivision.
type OwnCodes = Vec<(u32, String)>;

/// ISO 3166-2 code by Wikidata item, from one list of `data/reference.json`: `iso3166_2`, the
/// subdivisions that Overture's `region` column lacks (Ugandan districts, overseas territories)
/// and that consumers expect, or `iso3166_2_lvl6`, those below a region (French départements).
pub(super) fn iso_by_item<'a>(reference: &'a Value, list: &str) -> HashMap<&'a str, &'a str> {
    reference[list]
        .as_object()
        .into_iter()
        .flatten()
        .flat_map(|(code, items)| {
            let items = items.as_array().into_iter().flatten();
            items
                .filter_map(Value::as_str)
                .map(move |item| (item, code.as_str()))
        })
        .collect()
}

/// Reads `places.tsv` on all cores (about 5 million lines for the world), hashing each place's
/// names as it goes, and the places whose Wikidata item (the last column) has an ISO 3166-2
/// code in `iso`, by place number.
pub(super) fn load_places(
    path: &Path,
    iso: &HashMap<&str, &str>,
) -> Result<(Vec<Place>, OwnCodes, history::History)> {
    use rayon::prelude::*;
    let text = fs::read_to_string(path).with_context(|| path.display().to_string())?;
    let lines: Vec<&str> = text.lines().collect();
    let settlements: std::collections::HashSet<(&str, &str, &str)> = lines
        .par_iter()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            if !matches!(fields.next()?, "locality" | "localadmin") {
                return None;
            }
            let country = fields.nth(1)?;
            let region = fields.next()?;
            let item = fields.nth(11)?;
            (!region.is_empty() && !item.is_empty()).then_some((country, region, item))
        })
        .collect();
    let own = lines
        .par_iter()
        .enumerate()
        .filter_map(|(i, l)| {
            let code = iso.get(l.split('\t').nth(15)?)?;
            Some((i as u32, code.to_string()))
        })
        .collect();
    let places: Vec<Place> = lines
        .par_iter()
        .map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            let f = |i: usize| c.get(i).copied().unwrap_or("");
            let num = |i: usize| f(i).parse::<f32>().unwrap_or(f32::NAN);
            let mut place = Place {
                subtype: f(0).into(),
                class: f(1).into(),
                cc: f(2).to_lowercase().into(),
                region: f(3).into(),
                population: f(5).parse().unwrap_or(0),
                x: num(6),
                y: num(7),
                bbox: [num(8), num(9), num(10), num(11)],
                parent: f(12).parse().unwrap_or(NONE),
                primary: f(13).into(),
                names: f(14).into(),
                en: Place::english(f(14)),
                keys: Box::default(),
                city_like: false,
                settlement_twin: matches!(f(0), "county" | "region" | "macroregion")
                    && settlements.contains(&(f(2), f(3), f(15))),
            };
            let mut keys: Vec<u64> = place.all_names().map(name_key).collect();
            keys.sort_unstable();
            keys.dedup();
            place.keys = keys.into();
            place
        })
        .collect();
    let history = history::History::load(&places, &lines)?;
    Ok((places, own, history))
}

/// Nominatim-style keys (`name:fr`, `alt_name`, `short_name:ja`); repeated keys join with ';'.
/// A CJK name with an administrative suffix also gives its stem as a short name (東京都 -> 東京).
pub(super) fn namedetails(pl: &Place, names: &mut Map<String, Value>) {
    use serde_json::map::Entry;
    let mut add = |k: String, n: &str| match names.entry(k) {
        Entry::Occupied(mut e) => {
            if let Value::String(v) = e.get_mut()
                && !v.split(';').any(|x| x == n)
            {
                v.push(';');
                v.push_str(n);
            }
        }
        Entry::Vacant(e) => {
            e.insert(n.into());
        }
    };
    for (l, n) in pl.pairs() {
        add(
            if l.contains("_name") {
                l.to_string()
            } else {
                format!("name:{l}")
            },
            n,
        );
    }
    for n in pl.all_names() {
        if let Some(stem) = cjk_stem(n) {
            add("short_name".into(), stem);
        }
    }
}

impl Index {
    pub(super) fn country(&self, text: &str) -> Option<String> {
        // The analyzer drops tokens of 40+ bytes; losing an address must not leave a bare country.
        if text.len() >= 40
            && text
                .split(|c: char| !c.is_alphanumeric())
                .any(|word| word.len() >= 40)
        {
            return None;
        }
        let folded = fold(text);
        if folded.is_empty() {
            return None;
        }
        if let Some(c) = self.countries.get(&folded) {
            return Some(c.clone());
        }
        // "india india": a repeated field the parser joined
        let words: Vec<&str> = folded.split(' ').collect();
        let half = words.len() / 2;
        (half > 0 && words.len().is_multiple_of(2) && words[..half] == words[half..])
            .then(|| self.countries.get(&words[..half].join(" ")).cloned())
            .flatten()
    }

    /// The countries a stated region may be in: those with an ISO 3166-2 code that ends in a
    /// written code ("PA", "BCS"), or the one whose region has that name.
    pub(super) fn region_countries(&self, region: &str) -> Vec<String> {
        let code: String = region
            .chars()
            .filter(|c| c.is_ascii_alphabetic())
            .collect::<String>()
            .to_uppercase();
        let mut found = vec![];
        if (2..=3).contains(&code.len())
            && code.len() == region.chars().filter(|c| *c != '.').count()
        {
            let suffix = format!("-{code}");
            found.extend(
                self.regions
                    .keys()
                    .filter_map(|k| k.strip_suffix(&suffix)?.to_lowercase().into()),
            );
        }
        let named = crate::matching::implied_country(
            &[("state".to_string(), region.to_string())]
                .into_iter()
                .collect(),
        );
        if !named.is_empty() {
            found.push(named);
        }
        found.sort();
        found.dedup();
        found
    }

    pub(super) fn chain(&self, mut p: u32) -> Vec<u32> {
        let mut out = vec![];
        while p != NONE && out.len() < 16 {
            out.push(p);
            p = self.places[p as usize].parent;
        }
        out
    }

    /// Places named `text`: exact name matches first, then those under a place named `within`,
    /// then by importance.
    pub(super) fn places(
        &self,
        text: &str,
        cc: Option<&str>,
        within: Option<&str>,
        ok: &dyn Fn(&Place) -> bool,
    ) -> Vec<Hit> {
        self.places_exact(text, cc, within, ok).0
    }

    /// `places`, and whether the first one is named exactly `text`; failing that, the same for
    /// `text` without an administrative word ("Vas County" is named "Vas").
    pub(super) fn places_exact(
        &self,
        text: &str,
        cc: Option<&str>,
        within: Option<&str>,
        ok: &dyn Fn(&Place) -> bool,
    ) -> (Vec<Hit>, bool) {
        let found = self.places_named(text, cc, within, ok, false);
        if found.0.is_empty()
            && let Some(bare) = without_admin_word(text)
        {
            // "Kreisfreie Stadt München" names a city; "Vas County" an area
            let city_word = ["city", "stadt", "kreisfreie", "township", "borough", "town"]
                .iter()
                .any(|w| text.split_whitespace().any(|t| t.to_lowercase() == *w));
            return self.places_named(&bare, cc, within, ok, !city_word);
        }
        found
    }

    pub(super) fn places_named(
        &self,
        text: &str,
        cc: Option<&str>,
        within: Option<&str>,
        ok: &dyn Fn(&Place) -> bool,
        area_named: bool,
    ) -> (Vec<Hit>, bool) {
        let cl = self.musts(self.f.names, text);
        if cl.is_empty() {
            return (vec![], false);
        }
        let within_text = within;
        let want = name_key(text);
        let long = long_words(text);
        // "Cauca Department" is named "Cauca"
        // a region written as its code ("Elmwood Park, NJ"): the "-NJ" of US-NJ
        let within_code = within
            .map(str::trim)
            .filter(|w| (2..=3).contains(&w.len()) && w.chars().all(|c| c.is_ascii_alphabetic()))
            .map(|w| format!("-{}", w.to_uppercase()));
        let within: Vec<u64> = within
            .filter(|w| !fold(w).is_empty())
            .into_iter()
            .flat_map(|w| std::iter::once(w.to_string()).chain(without_admin_word(w)))
            .map(|w| name_key(&w))
            .collect();
        let mut ranked = self.top_of(Kind::Place, cc, cl, 100);
        // The stated region places a small town among many of its name ("Temple, PA"): when
        // none of the top 100 is inside it, look in the countries that have such a region too.
        let is_inside = |a: &DocAddress| {
            let id = self.fast(*a, "id") as u32;
            let pl = &self.places[id as usize];
            within.iter().any(|&w| {
                self.chain(id)
                    .iter()
                    .skip(1)
                    .any(|&x| self.places[x as usize].named(w))
            }) || within_code
                .as_ref()
                .is_some_and(|c| pl.region.ends_with(c.as_str()))
        };
        if cc.is_none()
            && let Some(region) = within_text
            && !ranked.iter().any(is_inside)
        {
            for country in self.region_countries(region) {
                for a in self.top_of(
                    Kind::Place,
                    Some(&country),
                    self.musts(self.f.names, text),
                    100,
                ) {
                    if !ranked.contains(&a) {
                        ranked.push(a);
                    }
                }
            }
        }
        // The provinces made at startup are in no search index; like `<cc>, <code>`, they need
        // their country stated.
        let provinces = self.provinces.values().copied().filter(|&p| {
            let pl = &self.places[p as usize];
            cc.is_some_and(|cc| pl.cc.eq_ignore_ascii_case(cc)) && pl.named(want)
        });
        let ids: Vec<(usize, u32)> = provinces
            .map(|p| (0, p))
            .chain(
                ranked
                    .into_iter()
                    .enumerate()
                    .map(|(rank, a)| (rank, self.fast(a, "id") as u32)),
            )
            .filter(|(_, id)| ok(&self.places[*id as usize]))
            .filter(|(_, id)| {
                long.is_empty() || names_hold(self.places[*id as usize].all_names(), &long)
            })
            .collect();
        // The most important exact match among administrative areas, and the population of the
        // largest exact-named settlement: only that settlement may outrank the area.
        let area = ids
            .iter()
            .map(|&(_, id)| &self.places[id as usize])
            .position(|p| {
                p.named(want)
                    && matches!(
                        &*p.subtype,
                        "region" | "macroregion" | "county" | "macrocounty"
                    )
            })
            .map(|k| ids[k].1)
            .filter(|_| !area_named);
        let largest = ids
            .iter()
            .map(|&(_, id)| &self.places[id as usize])
            .filter(|p| p.subtype.as_ref() == "locality" && p.named(want))
            .map(|p| p.population)
            .max()
            .unwrap_or(0);
        let mut found: Vec<(bool, bool, bool, usize, u32)> = ids
            .into_iter()
            .map(|(rank, id)| {
                let pl = &self.places[id as usize];
                let exact = pl.named(want);
                let chain = self.chain(id);
                let inside = within.iter().any(|&w| {
                    chain
                        .iter()
                        .skip(1)
                        .any(|&a| self.places[a as usize].named(w))
                }) || within_code
                    .as_ref()
                    .is_some_and(|c| pl.region.ends_with(c.as_str()));
                // A major city outranks a same-named region, as with Nominatim (Tokyo, New York),
                // and so does a town inside a same-named area (Brasília in the Federal District
                // also called Brasília, Yalova in Yalova Province).
                let town = pl.subtype.as_ref() == "locality"
                    && (matches!(&*pl.class, "city" | "town") || pl.population >= 10_000)
                    && [&*pl.primary, pl.name(true)]
                        .iter()
                        .any(|n| exact_key(n) == exact_key(text))
                    && pl.population >= largest;
                let big = pl.subtype.as_ref() == "locality" && pl.population >= 500_000
                    || town && area.is_some_and(|a| chain.contains(&a))
                    || area_named && !matches!(&*pl.subtype, "locality" | "localadmin");
                (!exact, !inside, !big, rank, id)
            })
            .collect();
        found.sort();
        // Places outside the stated region ("big bend, co") only when none is inside it.
        if found.first().is_some_and(|f| !f.1) {
            found.retain(|f| !f.1);
        }
        let exact = found.first().is_some_and(|f| !f.0);
        (
            found
                .into_iter()
                .take(10)
                .map(|(.., id)| Hit::Place(id))
                .collect(),
            exact,
        )
    }

    /// `text`, or its most specific part, if it names a place: words dropped from the front
    /// ("fra seynod" -> "seynod"), or for CJK the administrative parts ("南京市江宁区" ->
    /// "江宁区", "南京市").
    pub(super) fn resolve_place(&self, text: &str, cc: Option<&str>) -> Option<String> {
        let local = |p: &Place| !matches!(&*p.subtype, "country" | "dependency");
        let mut tries: Vec<String> = vec![text.to_string()];
        let words: Vec<&str> = text.split_whitespace().collect();
        tries.extend((1..words.len()).map(|i| words[i..].join(" ")));
        let mut part = String::new();
        let mut parts = vec![];
        for c in text.chars() {
            part.push(c);
            if "市区县省都道府県町村".contains(c) {
                parts.push(std::mem::take(&mut part));
            }
        }
        if parts.len() > 1 {
            tries.extend(parts.into_iter().rev());
        }
        let (found, outer) = tries.into_iter().find_map(|t| {
            let places = self.places(&t, cc, None, &local);
            (!places.is_empty()).then_some((t, places))
        })?;
        // "cabanes castelló/castellón": a town written before the province it is in is the town.
        let head = text.strip_suffix(found.as_str()).map(str::trim);
        if let Some(head) = head.filter(|h| !h.is_empty())
            && let (towns, true) = self.places_exact(head, cc, None, &local)
            && let Some(Hit::Place(town)) = towns.first()
            && outer
                .iter()
                .any(|h| matches!(h, Hit::Place(p) if p != town && self.chain(*town).contains(p)))
        {
            return Some(head.to_string());
        }
        Some(found)
    }

    /// Whether a place named exactly `name` (in country `cc`, if given) has its centre within
    /// `km` of the point.
    /// "Maplewood Township" may be named "Maplewood".
    pub fn place_near(&self, name: &str, cc: Option<&str>, lat: f32, lon: f32, km: f32) -> bool {
        std::iter::once(name.to_string())
            .chain(without_admin_word(name))
            .any(|name| {
                let want = name_key(&name);
                let local =
                    |p: &Place| !matches!(&*p.subtype, "country" | "dependency") && p.named(want);
                self.places(&name, cc, None, &local)
                    .iter()
                    .any(|h| match h {
                        Hit::Place(id) => {
                            let p = &self.places[*id as usize];
                            let dx = (p.x - lon) * lat.to_radians().cos();
                            dx.hypot(p.y - lat) * 111.2 < km
                        }
                        _ => false,
                    })
            })
    }

    /// The English name of a country's place.
    pub fn country_name(&self, cc: &str) -> Option<&str> {
        let place = self.country_places.get(cc)?;
        Some(self.places[*place as usize].name(true))
    }

    /// Every ISO 3166-2 code that names a region place or a subdivision below one, with that
    /// place.
    pub fn region_places(&self) -> impl Iterator<Item = (&str, u32)> {
        self.regions
            .iter()
            .chain(&self.subdivisions)
            .chain(&self.provinces)
            .map(|(code, &place)| (code.as_str(), place))
    }

    /// The code of the region a subdivision's place lies in.
    pub fn parent_region_code(&self, place: u32) -> Option<&str> {
        self.region_code(self.chain(place).get(1..)?)
    }

    /// `query` with a part that is a region's ISO 3166-2 code ("B.C.S" for Baja California Sur)
    /// replaced by the region's name; see [`crate::matching::with_region_names`].
    pub fn with_region_names(&self, query: &str) -> Option<String> {
        crate::matching::with_region_names(query, |code| {
            if self.country(code).is_some() {
                return None;
            }
            let suffix = format!("-{code}");
            let mut named = self.regions.iter().filter(|(k, _)| k.ends_with(&suffix));
            match (named.next(), named.next()) {
                (Some((_, &id)), None) => Some(self.places[id as usize].primary.to_string()),
                _ => None,
            }
        })
    }

    /// A country/subdivision/name slug. Some(empty) keeps a conflicting code or name from
    /// falling through to a less constrained interpretation.
    pub fn place_slug(&self, query: &str) -> Option<Vec<Hit>> {
        let mut parts = query.trim().splitn(3, '-');
        let (cc, division, town) = (parts.next()?, parts.next()?, parts.next()?);
        if cc.len() != 2
            || !cc.bytes().all(|c| c.is_ascii_alphabetic())
            || !(1..=3).contains(&division.len())
            || !division.bytes().all(|c| c.is_ascii_alphanumeric())
            || town.is_empty()
            || town.split('-').any(|word| word.is_empty())
            || !town.chars().all(|c| c.is_alphabetic() || c == '-')
        {
            return None;
        }
        let cc = self.country(cc)?;
        let code = format!("{}-{}", cc.to_uppercase(), division.to_uppercase());
        let Some(region) = self.region_query(&format!("{cc}, {code}")) else {
            return Some(vec![]);
        };
        let town = town.replace('-', " ");
        let key = name_key(&town);
        Some(
            self.places_named(
                &town,
                Some(&cc),
                Some(self.places[region as usize].name(true)),
                &|p| matches!(p.subtype.as_ref(), "locality" | "localadmin") && p.named(key),
                false,
            )
            .0
            .into_iter()
            .filter(|hit| matches!(hit, Hit::Place(id) if self.chain(*id).contains(&region)))
            .collect(),
        )
    }

    /// Reviewed membership observations, only after current lookups have abstained.
    pub fn historical_place(&self, query: &str) -> Option<Hit> {
        if query.chars().count() > 256 {
            return None;
        }
        let parts: Vec<_> = query.split(',').map(str::trim).collect();
        let [town, ancestor, country] = parts.as_slice() else {
            return None;
        };
        let cc = self.country(country)?;
        if self.country(ancestor).is_some() || self.country(town).is_some_and(|other| other != cc) {
            return None;
        }
        self.history
            .find(&self.places, town, ancestor, &cc)
            .map(Hit::Place)
    }

    /// `<cc>, <ISO 3166-2 code>`, as a query for a region by its code.
    pub(super) fn region_query(&self, q: &str) -> Option<u32> {
        let (cc, code) = q.split_once(", ")?;
        let code = code.trim().to_uppercase();
        (cc.trim().len() == 2 && code.starts_with(&format!("{}-", cc.trim().to_uppercase())))
            .then(|| {
                let found = self.regions.get(&code).or(self.subdivisions.get(&code));
                found.or(self.provinces.get(&code)).copied()
            })
            .flatten()
    }

    /// The ISO 3166-2 code of a place chain: the lowest place with a code of its own or coded
    /// region, so a district beats its region (UG-108 over UG-C) and a region its coded
    /// territory (WF-UV over FR-WF).
    pub(super) fn region_code(&self, chain: &[u32]) -> Option<&str> {
        // A child's inherited region column can disagree with its explicit region parent.
        chain
            .iter()
            .find_map(|p| {
                let pl = &self.places[*p as usize];
                self.own_codes.get(p).map(|c| &**c).or_else(|| {
                    (pl.subtype.as_ref() == "region" && !pl.region.is_empty())
                        .then_some(&*pl.region)
                })
            })
            .or_else(|| {
                chain
                    .iter()
                    .map(|&p| &*self.places[p as usize].region)
                    .find(|r| !r.is_empty())
            })
    }
}
