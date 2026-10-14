use crate::{
    index::{Index, Parser},
    matching::*,
    metrics::METRICS,
    proto::opengeocoding::*,
};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::{borrow::Cow, path::Path, sync::Arc};

const ORDERS: [(&str, &[&str]); 4] = [
    (
        "default",
        &[
            "country",
            "state",
            "postcode",
            "city",
            "road",
            "house_number",
        ],
    ),
    ("postcode", &["postcode", "city", "country"]),
    ("loose", &["country", "state", "postcode", "city"]),
    ("just_city", &["country", "city"]),
];

pub struct Resolver(Arc<Core>);

/// An accepted candidate with what it can be told apart by: every name it is known by (English
/// and its own languages), for judging how much of the query it explains, and its kind.
struct Chosen {
    found: FullAddress,
    names: Vec<String>,
    kind: String,
    /// How many countries the acceptable candidates are in: a bare name with no country is a
    /// guess when several have one.
    countries: usize,
}

impl Chosen {
    fn explained(&self, query: &str, address: &ParsedAddress) -> (usize, usize) {
        let stated = part(address, "country");
        coverage(
            query,
            self.names
                .iter()
                .map(String::as_str)
                .chain(country_matches_text(&self.found, address).then_some(stated)),
        )
    }

    /// Whether the candidate's own name is one of the stated place names.
    fn named_by(&self, text: &str) -> bool {
        self.found
            .display_name
            .split(',')
            .next()
            .is_some_and(|first| names_agree(text, first.trim()))
    }

    /// A settlement or postcode: the kinds a fragment of a query can name reliably.
    fn anchor(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "city" | "town" | "village" | "hamlet" | "municipality" | "postcode"
        )
    }
}

fn country_matches_text(found: &FullAddress, address: &ParsedAddress) -> bool {
    let stated = expected_country(address);
    !stated.is_empty() && country_matches(&stated, found.address_details.as_option())
}

fn names_of(value: &Value, into: &mut Vec<String>) {
    into.push(string(&value["display_name"]).to_owned());
    for name in value["namedetails"].as_object().into_iter().flatten() {
        into.extend(string(name.1).split(';').map(str::to_owned));
    }
    into.extend(
        value["address"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(_, v)| v.as_str().map(str::to_owned)),
    );
}

/// Everything runs on a blocking thread (libpostal and the index search block), so one request
/// takes one hop to that pool instead of one per parse and search.
struct Core {
    index: Arc<Index>,
    parser: Arc<Parser>,
}

impl Resolver {
    /// Loads the libpostal models and the index in `dir`. Blocks for several seconds.
    pub fn open(dir: &Path) -> Result<Self> {
        // before the models load, which takes seconds
        anyhow::ensure!(
            dir.join("meta.json").is_file(),
            "INDEX_DIR {} holds no build_index output",
            dir.display()
        );
        let cache = match std::env::var("PARSE_CACHE") {
            Ok(v) => v.parse().context("PARSE_CACHE")?,
            Err(_) => Parser::DEFAULT_CACHE,
        };
        let parser = Arc::new(Parser::unloaded(cache));
        // the index doesn't parse while it opens, so the two loads overlap
        let index = std::thread::scope(|scope| {
            let models = scope.spawn(|| parser.load());
            let index = Index::open(dir, parser.clone());
            models.join().expect("libpostal setup thread panicked")?;
            index
        })?;
        let index = Arc::new(index);
        Ok(Self(Arc::new(Core { index, parser })))
    }

    /// Warms the suggestion caches on a thread of its own, so the first people to type a short
    /// prefix do not pay for reading it. Does not delay anything.
    pub fn warm_suggestions(&self) {
        let core = self.0.clone();
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            core.index.warm_suggest();
            tracing::info!(
                seconds = started.elapsed().as_secs_f64(),
                "Suggestion caches warmed"
            );
        });
    }

    pub fn index(&self) -> &Index {
        &self.0.index
    }

    pub fn regions(&self) -> Result<Vec<(String, Region)>> {
        self.0.regions()
    }

    pub async fn parse(&self, query: &str) -> Result<ParsedAddress> {
        let core = self.0.clone();
        let query = query.to_owned();
        tokio::task::spawn_blocking(move || core.parse(&query)).await?
    }

    pub async fn resolve(&self, query: &str) -> Result<FullAddress> {
        let core = self.0.clone();
        let query = query.to_owned();
        tokio::task::spawn_blocking(move || core.resolve(&query)).await?
    }

    /// Completions of a partly typed address; see `Index::suggest`.
    pub async fn suggest(
        &self,
        text: &str,
        limit: usize,
        country_code: &str,
    ) -> Result<Vec<crate::index::Suggestion>> {
        let core = self.0.clone();
        let (text, cc) = (text.to_owned(), country_code.to_owned());
        Ok(tokio::task::spawn_blocking(move || {
            core.index
                .suggest(&text, limit, Some(cc.as_str()).filter(|c| !c.is_empty()))
        })
        .await?)
    }

    pub async fn reverse(&self, lat: f32, lon: f32) -> Result<Option<FullAddress>> {
        let core = self.0.clone();
        tokio::task::spawn_blocking(move || core.reverse(lat, lon)).await?
    }

    /// The addresses nearest a point; see `Index::reverse_search`.
    pub async fn reverse_search(
        &self,
        lat: f32,
        lon: f32,
        radius_m: f64,
        size: usize,
    ) -> Result<Vec<ReverseResult>> {
        let core = self.0.clone();
        tokio::task::spawn_blocking(move || {
            core.index
                .reverse_search(lat as f64, lon as f64, radius_m, size)
                .into_iter()
                .map(|(layer, value, meters)| {
                    let mut address = format_response(&value)?;
                    address.good_match = true;
                    Ok(ReverseResult {
                        address: address.into(),
                        distance_m: meters as f32,
                        layer: layer.into(),
                        ..Default::default()
                    })
                })
                .collect()
        })
        .await?
    }
}

impl Core {
    fn parse(&self, query: &str) -> Result<ParsedAddress> {
        Ok(self.parser.parse(query)?.iter().cloned().collect())
    }

    /// The index's hits and their English rendering.
    fn search(&self, query: &str) -> (Vec<crate::index::Hit>, Vec<Value>) {
        let query = query.trim_matches(whitespace);
        if query.is_empty() {
            return (vec![], vec![]);
        }
        let hits = self.index.search(query);
        let english = hits.iter().map(|h| self.index.render(h, true)).collect();
        (hits, english)
    }

    fn first_acceptable(
        &self,
        query: &str,
        address: &ParsedAddress,
        (country, postcode): (&str, &str),
        tier: &str,
        other_names: bool,
        rank_by: Option<&str>,
    ) -> Result<Option<Chosen>> {
        // A lone name the parser labels a state (California, 京都) still has to name the result.
        let lone_state = part(address, "city").is_empty()
            && !part(address, "state").is_empty()
            && address.keys().all(|key| key == "state" || key == "country");
        let mut named;
        let address = if lone_state {
            named = address.clone();
            named.insert("city".into(), part(address, "state").into());
            &named
        } else {
            address
        };
        let (hits, candidates) = {
            let _timer = METRICS.search.with_label_values(&[tier]).start_timer();
            self.search(query)
        };
        let english = candidates
            .iter()
            .map(format_response)
            .collect::<Result<Vec<_>>>()?;
        let mut judged = Cow::Borrowed(english.as_slice());
        let place_agrees = |f: &FullAddress| {
            city_agrees(address, f.address_details.as_option())
                || postcode_place_agrees(address, postcode, f.address_details.as_option())
        };
        if !part(address, "city").is_empty() && !english.iter().any(place_agrees) {
            // the same hits named in their own languages
            let local: Vec<_> = hits.iter().map(|h| self.index.render(h, false)).collect();
            for (index, candidate) in candidates.iter().enumerate() {
                if let Some(twin) = local.iter().rev().find(|hit| {
                    truthy(&hit["osm_id"])
                        && hit.get("osm_id") == candidate.get("osm_id")
                        && hit.get("osm_type") == candidate.get("osm_type")
                }) {
                    let formatted = format_response(twin)?;
                    if place_agrees(&formatted) {
                        judged.to_mut()[index] = formatted;
                    }
                }
            }
        }
        let city = part(address, "city");
        // Only a bare place name ("Budapest, Hungary") may be a same-named region; in a street
        // address a same-named county or province is a different place.
        let bare = address.keys().all(|key| {
            matches!(
                key.as_str(),
                "city" | "state" | "state_district" | "country_region" | "country"
            )
        });
        if other_names && !city.is_empty() {
            for (index, candidate) in candidates.iter().enumerate() {
                let eligible = match candidate["addresstype"].as_str() {
                    Some("city" | "town" | "village" | "municipality" | "hamlet") => true,
                    Some(
                        "region" | "state" | "province" | "state_district" | "county" | "country",
                    ) => bare,
                    _ => false,
                };
                if !eligible
                    || !matches!(candidate["class"].as_str(), Some("place" | "boundary"))
                    || city_agrees(address, judged[index].address_details.as_option())
                {
                    continue;
                }
                // A place's own names in every language (name:*, alt_name, short_name) corroborate
                // queries written in another language or script than the English response.
                if let Some(name) = own_name(candidate, city) {
                    let details = judged.to_mut()[index]
                        .address_details
                        .get_or_insert_default();
                    details.city = name.into();
                }
            }
        }
        // "Coulsdon, London": the suburb is the more specific part, and it is in the town stated
        // if that town's centre is near. The index doesn't always file it under the town.
        let finer = ["suburb", "city_district"]
            .iter()
            .map(|key| part(address, key))
            .find(|s| !s.is_empty());
        let near_town = finer.filter(|_| !city.is_empty()).and_then(|finer| {
            let mut as_city = address.clone();
            as_city.insert("city".into(), finer.into());
            let town = self.search(city).1.first().map(format_response)?.ok()?;
            Some((as_city, town.geo_coordinates.into_option()?))
        });
        let finer_near = |f: &FullAddress| {
            near_town.as_ref().is_some_and(|(as_city, town)| {
                city_agrees(as_city, f.address_details.as_option())
                    && f.geo_coordinates
                        .as_option()
                        .is_some_and(|g| km_between(town.lat, town.lon, g.lat, g.lon) < 60.0)
            })
        };
        // A town that names no place ("IDX") can't outweigh a house, road and postcode that agree;
        // a real one (the postcode is the mistyped part) can.
        let junk_city = !city.is_empty()
            && !self
                .index
                .names_place(city, Some(country).filter(|c| !c.is_empty()));
        // A place with no locality of its own (a house the data files under no town) says
        // nothing of the stated one; the postcode's own town then has to.
        let postcode_vouches = std::cell::LazyCell::new(|| {
            let hits = self
                .index
                .postcode_hits(postcode, Some(country).filter(|c| !c.is_empty()));
            hits.first()
                .and_then(|h| format_response(&self.index.render(h, true)).ok())
                .is_none_or(|f| {
                    place_agrees(&f)
                        || f.address_details.as_option().is_some_and(|d| {
                            [&d.county, &d.state_district, &d.suburb, &d.city_district]
                                .iter()
                                .any(|name| {
                                    !name.is_empty()
                                        && !part(address, "city").is_empty()
                                        && names_agree(part(address, "city"), name)
                                })
                        })
                })
        });
        let overriding = tier.ends_with("without_stated_country");
        // libpostal files fragments ("province of") as a state district; only a state is vouched
        let stated_state = {
            let mut a = address.clone();
            a.shift_remove("state_district");
            a
        };
        let mut viable: Vec<_> = judged
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                let details = f.address_details.as_option();
                (!overriding || state_consistent(&stated_state, details))
                    && (postcode_matches(postcode, details) || house_road_city(address, details))
                    && (part(address, "city").is_empty()
                        || city_agrees(address, details)
                        || (same_postcode_country(country, postcode, details)
                            && (locality(details) || *postcode_vouches))
                        || postcode_place_agrees(address, postcode, details)
                        || (house_road_postcode(address, postcode, details) && junk_city)
                        || house_road_city(address, details)
                        || finer_near(f))
            })
            .collect();
        if !lone_state
            && !city.is_empty()
            && ["road", "house_number", "postcode"]
                .iter()
                .all(|key| part(address, key).is_empty())
        {
            // A bare place name means the settlement, not a same-named quarter, county or region
            // that the index may rank higher.
            viable.sort_by_key(|(i, _)| match candidates[*i]["addresstype"].as_str() {
                Some("city" | "town") => 0,
                Some("municipality") => 1,
                Some("village") => 2,
                _ => 3,
            });
        }
        let values: Vec<_> = viable.iter().map(|(_, f)| (*f).clone()).collect();
        let chosen = choose_candidate(address, country, postcode, &values).or_else(|| {
            (tier == "raw")
                .then(|| {
                    unpostcoded_candidate(address, country, postcode, &values, |cc| {
                        self.index
                            .knows_postcode(postcode, Some(cc).filter(|c| !c.is_empty()))
                    })
                })
                .flatten()
                .map(|i| (i, "country"))
        });
        let Some((mut index, mut matched)) = chosen else {
            if !candidates.is_empty() {
                tracing::warn!(count = candidates.len(), tier, "No acceptable candidate");
            }
            return Ok(None);
        };
        // Same-named places ("Chencun" in two provinces): the one the rest of the query names.
        if let Some(full) = rank_by {
            let names_at = |at: usize| {
                let mut names = vec![];
                names_of(&candidates[at], &mut names);
                names_of(&self.index.render(&hits[at], false), &mut names);
                names
            };
            let best = (0..values.len())
                .filter_map(|i| {
                    let own = choose_candidate(address, country, postcode, &values[i..=i])?;
                    let names = names_at(viable[i].0);
                    let explained = coverage(full, names.iter().map(String::as_str)).0;
                    Some((explained, std::cmp::Reverse(i), own.1))
                })
                .max();
            if let Some((_, std::cmp::Reverse(i), own)) = best {
                (index, matched) = (i, own);
            }
        }
        if matched != "country" {
            tracing::info!(
                stated_country = country,
                accepted_country = english[viable[index].0].address_details.country_code,
                matched,
                "Accepting corroborated country override"
            );
        }
        let at = viable[index].0;
        let mut names = vec![];
        names_of(&candidates[at], &mut names);
        names_of(&self.index.render(&hits[at], false), &mut names);
        Ok(Some(Chosen {
            found: english[at].clone(),
            names,
            kind: string(&candidates[at]["addresstype"]).to_owned(),
            countries: viable
                .iter()
                .filter_map(|(_, f)| f.address_details.as_option().map(|d| &d.country_code))
                .collect::<std::collections::HashSet<_>>()
                .len(),
        }))
    }

    fn resolve(&self, query: &str) -> Result<FullAddress> {
        if query.is_empty() || query.chars().count() > 256 {
            return Ok(FullAddress::default());
        }
        // Tabs and ASCII record separators between fields read as spaces to both parsers.
        let query = &query.replace(|ch: char| whitespace(ch) && ch != ' ', " ");
        if PLACEHOLDERS.contains(&query.trim_matches(whitespace).to_lowercase().as_str()) {
            return Ok(FullAddress::default());
        }
        let mut address = self.parse(query)?;
        if part(&address, "country").is_empty() {
            let parts: Vec<_> = query
                .split(',')
                .map(|s| s.trim_matches(whitespace))
                .collect();
            if parts.len() >= 3 {
                let rotated_query = parts[2..]
                    .iter()
                    .chain(parts[..2].iter())
                    .copied()
                    .collect::<Vec<_>>()
                    .join(", ");
                let rotated = self.parse(&rotated_query)?;
                if !part(&rotated, "country").is_empty() {
                    address = rotated;
                }
            }
        }
        // Repeated fields ("Karnataka, India, India") reach the parser as one "india india".
        if let Some(stated) = address.get_mut("country") {
            let words: Vec<_> = stated.split(' ').collect();
            let half = words.len() / 2;
            if half > 0 && words.len() % 2 == 0 && words[..half] == words[half..] {
                *stated = words[..half].join(" ");
            }
        }
        let country = expected_country(&address);
        let postcode = if part(&address, "postcode").is_empty() {
            eircode(query)
        } else {
            part(&address, "postcode").into()
        };
        // A country the parser missed ("Kingdom of Bahrain" as a house) keeps a wrong place in
        // another country from answering, but it doesn't license a country-level fallback.
        let tailed = part(&address, "country").is_empty().then(|| {
            country_in_tail(query, &address).map(|stated| {
                let mut tailed = address.clone();
                tailed.insert("country".into(), stated);
                tailed
            })
        });
        let tailed = tailed.flatten();
        let (stated_address, stated_country) = match &tailed {
            Some(tailed) => (tailed, expected_country(tailed)),
            None => (&address, country.clone()),
        };
        let mut weak = None;
        if let Some(chosen) = self.first_acceptable(
            query,
            stated_address,
            (&stated_country, &postcode),
            "raw",
            true,
            None,
        )? {
            let (explained, total) = chosen.explained(query, stated_address);
            // A place that names a few words of a long query is a guess: "Twin City Shopping
            // Centre, Burgersfort" is not Thuin. A street or house answer was checked against its
            // road.
            let details = chosen.found.address_details.as_option();
            let named = ["city", "city_district", "suburb"]
                .iter()
                .any(|key| chosen.named_by(part(stated_address, key)));
            let guess = details.is_none_or(|d| d.road.is_empty())
                && !postcode_agrees(&postcode, details)
                && total >= 3
                && explained * 2 < total
                && !(named && chosen.countries <= 1);
            if !guess {
                return Ok(accept(chosen.found, &address, &postcode, "raw", true));
            }
            weak = Some(chosen);
        }
        if weak.is_none()
            && let Some(found) = self.fallbacks(query, &address, &country, &postcode)?
        {
            return Ok(found);
        }
        if let Some(chosen) =
            self.search_parts(query, stated_address, &stated_country, weak.as_ref())?
        {
            // a part that accounts for every word of the query is no fallback
            let (explained, total) = chosen.explained(query, stated_address);
            let whole = total > 0 && explained == total;
            return Ok(accept(chosen.found, &address, &postcode, "part", whole));
        }
        if let Some(chosen) = weak {
            return Ok(accept(chosen.found, &address, &postcode, "weak", false));
        }
        METRICS.tiers.with_label_values(&["unresolved"]).inc();
        Ok(FullAddress::default())
    }

    /// Structured re-queries of the parsed fields, loosest last.
    fn fallbacks(
        &self,
        query: &str,
        address: &ParsedAddress,
        country: &str,
        postcode: &str,
    ) -> Result<Option<FullAddress>> {
        for (tier, order) in ORDERS {
            let structured = create_query(address, order);
            if !useful_query(&structured, postcode, country) {
                continue;
            }
            if let Some(chosen) =
                self.first_acceptable(&structured, address, (country, postcode), tier, true, None)?
            {
                return Ok(Some(accept(chosen.found, address, postcode, tier, false)));
            }
        }
        let stripped = without_country(query, address);
        if !stripped.is_empty()
            && let Some(chosen) = self.first_acceptable(
                &stripped,
                address,
                (country, postcode),
                "without_stated_country",
                country.is_empty(),
                None,
            )?
        {
            return Ok(Some(accept(
                chosen.found,
                address,
                postcode,
                "without_stated_country",
                true,
            )));
        }
        // Overriding a recognized stated country needs the place's primary fields to agree;
        // alternative names alone matched same-named regions abroad.
        if !part(address, "country").is_empty() {
            let mut without = address.clone();
            without.shift_remove("country");
            for (tier, order) in ORDERS {
                let tier = format!("{tier}_without_stated_country");
                let structured = create_query(&without, order);
                if !useful_query(&structured, postcode, "") {
                    continue;
                }
                if let Some(chosen) = self.first_acceptable(
                    &structured,
                    &without,
                    ("", postcode),
                    &tier,
                    country.is_empty(),
                    None,
                )? {
                    return Ok(Some(accept(chosen.found, address, postcode, &tier, true)));
                }
            }
        }
        Ok(None)
    }

    /// Last resort for a query the parser garbled or that carries words no place has: each
    /// comma-separated part searched on its own. Only a settlement or postcode that accounts for
    /// at least two of the query's words answers, the one explaining most (a later part
    /// breaking ties: addresses end with the larger place), and only when it beats `weak`.
    fn search_parts(
        &self,
        query: &str,
        address: &ParsedAddress,
        country: &str,
        weak: Option<&Chosen>,
    ) -> Result<Option<Chosen>> {
        let parts: Vec<&str> = query
            .split([',', ';'])
            .map(|p| p.trim_matches(whitespace))
            .filter(|p| p.chars().filter(|c| c.is_alphanumeric()).count() >= 3)
            .collect();
        if parts.len() < 2 {
            return Ok(None);
        }
        let stated = normalize_place(part(address, "country"));
        // "Clermont, FL" is not the Clermont in France
        let country = if country.is_empty() {
            implied_country(address)
        } else {
            country.to_owned()
        };
        let country = country.as_str();
        let mut found: Vec<(usize, bool, usize, Chosen)> = vec![];
        // A weak guess that no stated name matches (Thuin, reached through the alternative name
        // "Twin") yields to a settlement named exactly as a part says.
        let guess = weak.is_some_and(|w| {
            w.countries > 1
                || !["city", "city_district", "suburb", "house", "state_district"]
                    .iter()
                    .any(|key| w.named_by(part(address, key)))
        });
        // Turning no answer into one takes more of the query behind it than replacing a guess.
        let floor = weak.map_or(3, |w| w.explained(query, address).0 + 1);
        for (position, text) in parts.iter().enumerate() {
            if normalize_place(text) == stated {
                continue;
            }
            let parsed = self.parse(text)?;
            let postcode = part(&parsed, "postcode").to_owned();
            // a bare postcode or name is searched worldwide: say which country it is in
            let scoped = match self
                .index
                .country_name(country)
                .filter(|_| part(&parsed, "country").is_empty())
            {
                Some(name) => format!("{text}, {name}"),
                None => text.to_string(),
            };
            let Some(chosen) = self.first_acceptable(
                &scoped,
                &parsed,
                (country, &postcode),
                "part",
                true,
                Some(query),
            )?
            else {
                continue;
            };
            let (explained, _) = chosen.explained(query, address);
            if explained == 0
                || !state_consistent(address, chosen.found.address_details.as_option())
            {
                continue;
            }
            let named = chosen.named_by(text);
            found.push((position, named, explained, chosen));
        }
        // Parts that name places near each other agree: "Ancol" in the Jakarta of "Kota Jkt
        // Utara", not the one elsewhere.
        let spot = |c: &Chosen| c.found.geo_coordinates.as_option().map(|g| (g.lat, g.lon));
        let support = |at: usize| -> usize {
            let Some((lat, lon)) = spot(&found[at].3) else {
                return 0;
            };
            found
                .iter()
                .enumerate()
                .filter(|&(i, (p, ..))| i != at && *p != found[at].0)
                .filter_map(|(_, (.., c))| spot(c))
                .filter(|&(la, lo)| km_between(lat, lon, la, lo) < 60.0)
                .count()
        };
        let mut best: Option<((usize, bool, usize, usize), usize)> = None;
        for (at, (position, named, explained, chosen)) in found.iter().enumerate() {
            // "Nr. 2" is not the postcode NR2: with no country to place one, a postcode needs
            // more of the query behind it
            let unplaced_postcode =
                chosen.kind == "postcode" && country.is_empty() && *explained < 3;
            if !chosen.anchor()
                || unplaced_postcode
                || (*explained < floor && !(guess && *named && *explained >= 1))
            {
                continue;
            }
            let key = (support(at), *named, *explained, *position);
            if best.is_none_or(|(k, _)| key > k) {
                best = Some((key, at));
            }
        }
        Ok(best.map(|(_, at)| found.swap_remove(at).3))
    }

    fn reverse(&self, lat: f32, lon: f32) -> Result<Option<FullAddress>> {
        let value = self.index.reverse(lat as f64, lon as f64);
        if !truthy(&value["address"]) {
            return Ok(None);
        }
        let mut formatted = format_response(&value)?;
        formatted.good_match = true;
        Ok(Some(formatted))
    }

    /// Each ISO 3166-2 code the index knows with its region, as a search for `<cc>, <code>`
    /// renders it.
    fn regions(&self) -> Result<Vec<(String, Region)>> {
        let mut regions = vec![];
        for (code, place) in self.index.region_places() {
            let result = self.index.render(&crate::index::Hit::Place(place), true);
            let details = self.index.details(place.into());
            let names = &details["names"];
            let en_name = ["name:en", "_place_name:en", "name"]
                .iter()
                .find_map(|name| names[*name].as_str().filter(|s| !s.is_empty()))
                .unwrap_or("");
            let Some((prefix, admin_code)) = code.split_once('-') else {
                continue;
            };
            // A territory's own code names its sovereign (US-PR), not the territory.
            let country_code = prefix.to_lowercase();
            let country_name = if country_code == string(&details["country_code"]) {
                string(&result["address"]["country"])
            } else {
                self.index
                    .country_name(&country_code)
                    .unwrap_or_else(|| string(&result["address"]["country"]))
            };
            regions.push((
                code.to_uppercase(),
                Region {
                    country_code,
                    country_name: country_name.into(),
                    admin_level: integer(&details["admin_level"])?,
                    local_name: string(&details["localname"]).into(),
                    en_name: en_name.into(),
                    admin_code: admin_code.into(),
                    ..Default::default()
                },
            ));
        }
        Ok(regions)
    }
}

fn locality(details: Option<&AddressDetails>) -> bool {
    details.is_some_and(|d| {
        [&d.city, &d.town, &d.village, &d.municipality, &d.suburb]
            .iter()
            .any(|s| !s.is_empty())
    })
}

fn km_between(lat1: f32, lon1: f32, lat2: f32, lon2: f32) -> f32 {
    let (lat1, lat2) = (lat1.to_radians(), lat2.to_radians());
    let a = ((lat2 - lat1) / 2.0).sin().powi(2)
        + lat1.cos() * lat2.cos() * ((lon2 - lon1).to_radians() / 2.0).sin().powi(2);
    12742.0 * a.sqrt().asin()
}

fn own_name<'a>(candidate: &'a Value, city: &str) -> Option<&'a str> {
    let names = candidate["namedetails"].as_object().into_iter().flatten();
    names
        .filter_map(|(_, v)| v.as_str())
        .flat_map(|v| v.split(';'))
        .chain(string(&candidate["display_name"]).split(',').next())
        .find(|name| names_agree(city, name))
}

/// Missing-data placeholders that would otherwise match to places named "n/a".
const PLACEHOLDERS: &[&str] = &[
    "n/a", "n.a.", "na", "none", "null", "nil", "unknown", "-", "?",
];

/// A structured query of one character, or of a bare postcode with no country to place it in,
/// matches arbitrary places worldwide.
fn useful_query(query: &str, postcode: &str, country: &str) -> bool {
    query.chars().filter(|ch| ch.is_alphanumeric()).count() > 1
        && (!country.is_empty() || normalize_postcode(query) != normalize_postcode(postcode))
}

fn accept(
    mut found: FullAddress,
    address: &ParsedAddress,
    postcode: &str,
    tier: &str,
    good: bool,
) -> FullAddress {
    // A street without the requested house number is a fallback, not a match.
    let house_found = part(address, "house_number").is_empty()
        || found
            .address_details
            .as_option()
            .is_some_and(|d| !d.house_number.is_empty());
    // Accepted although its postcode disagrees: the input contradicts itself.
    let postcode_found = postcode_matches(postcode, found.address_details.as_option());
    if postcode.chars().any(|c| c.is_ascii_digit()) {
        let details = found.address_details.get_or_insert_default();
        if details.postcode.is_empty() {
            details.postcode = postcode.into();
        }
    }
    if found
        .address_details
        .as_option()
        .is_some_and(|d| !d.postcode.is_empty())
    {
        let details = found.address_details.get_or_insert_default();
        let formatted = format_postcode(&details.country_code, &details.postcode);
        if formatted != details.postcode {
            found.display_name = found.display_name.replace(&details.postcode, &formatted);
            details.postcode = formatted;
        }
    }
    // a P.O. box is a counter, not a place: the town is the best that can be said
    let located = !is_po_box(part(address, "po_box"));
    found.good_match = good && house_found && postcode_found && located;
    METRICS.tiers.with_label_values(&[tier]).inc();
    found
}

pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::Number(n) => n.as_f64().is_some_and(|v| v != 0.),
    }
}

fn string(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}

fn integer(value: &Value) -> Result<u32> {
    match value {
        Value::String(s) => s.parse().context("integer field"),
        _ => value
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .context("integer field"),
    }
}

fn float(value: &Value) -> Result<f32> {
    match value {
        Value::String(s) => s.parse().context("coordinate field"),
        _ => value.as_f64().map(|v| v as f32).context("coordinate field"),
    }
}

pub fn format_response(value: &Value) -> Result<FullAddress> {
    let bounding = if truthy(&value["boundingbox"]) {
        Some(BoundingBox {
            min_lat: float(&value["boundingbox"][0])?,
            max_lat: float(&value["boundingbox"][1])?,
            min_lon: float(&value["boundingbox"][2])?,
            max_lon: float(&value["boundingbox"][3])?,
            ..Default::default()
        })
    } else {
        None
    };
    let mut formatted = FullAddress {
        display_name: string(&value["display_name"]).into(),
        ..Default::default()
    };
    if truthy(&value["lat"]) && truthy(&value["lon"]) {
        formatted.geo_coordinates = GeoCoordinates {
            lat: float(&value["lat"])?,
            lon: float(&value["lon"])?,
            bounding_box: bounding.into(),
            ..Default::default()
        }
        .into();
    }
    if !truthy(&value["address"]) {
        return Ok(formatted);
    }
    let address = value["address"]
        .as_object()
        .context("address metadata must be an object")?;
    let mut regular = address.clone();
    let mut codes = vec![];
    for (key, code) in address.iter().filter(|(key, _)| key.contains("ISO3166-2")) {
        regular.remove(key);
        if code == "RS-KM" {
            continue;
        }
        let level = key.replace("ISO3166-2-lvl", "");
        let level: u32 = if level.is_empty() { 2 } else { level.parse()? };
        if level > 10 {
            continue;
        }
        codes.push(json!({"admin_level": level, "code": code}));
    }
    regular.insert("region_codes".into(), json!(codes));
    let details: AddressDetails = serde_json::from_value(Value::Object(regular))?;
    formatted.address_details = details.into();
    Ok(formatted)
}
