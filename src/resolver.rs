use crate::index::country::{au, ca, de, dk, ee, gr, hk_mo, ie, jp, ps, pt, si, tw};
use crate::{
    index::{
        Index, Parser,
        canon::{canon, is_canonical},
    },
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

thread_local! {
    /// Whether the last address `Core::resolve_once` parsed on this thread had a house number:
    /// `Core::suggested_house` looks further for a house the parse lost than for one the index
    /// lacks.
    static PARSED_NUMBER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A rewrite of a query, None when it doesn't apply.
type Rewrite<'a> = dyn Fn(&str) -> Option<String> + 'a;

/// How near a stated region must come to a town in another region for a place list to answer
/// with the town: the region it was in before a split (Arica, Tarapacá).
const NEIGHBOUR_KM: f64 = 75.0;

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

    /// Whether the candidate's own name is one of the stated place names. A name in another
    /// language ("Sevilla" for Seville) is its own too.
    fn named_by(&self, text: &str) -> bool {
        let agrees = agrees_with(text);
        self.found
            .display_name
            .split(',')
            .next()
            .is_some_and(|first| agrees(first.trim()))
            // "ny" agrees with a code or a fragment ("ct/ ny" became Niamey). A full name
            // in another language ("Sevilla") still does.
            || (text.chars().count() > 2 && self.names.iter().any(|name| agrees(name)))
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
    // A numbered region code ("FR-17") names no word of a query: "route 17" is not
    // Charente-Maritime.
    into.extend(
        value["address"]
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(key, v)| {
                !key.contains("ISO3166")
                    || !v
                        .as_str()
                        .is_some_and(|v| v.chars().any(|c| c.is_ascii_digit()))
            })
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
            METRICS.suggest_warm.set(1);
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
        tokio::task::spawn_blocking(move || core.parse(&query).map(Option::unwrap_or_default))
            .await?
    }

    pub async fn resolve(&self, query: &str) -> Result<FullAddress> {
        let core = self.0.clone();
        let query = query.to_owned();
        let span = tracing::Span::current();
        tokio::task::spawn_blocking(move || {
            let _entered = span.enter();
            core.index.memoized(|| {
                // only the query as written: `resolve` also answers the forms it rewrites it to
                let found = core.resolve(&query)?;
                let mut found = core.named_locality(&query, found)?;
                if found.geo_coordinates.as_option().is_some_and(|g| {
                    let (road, number) = found
                        .address_details
                        .as_option()
                        .map_or(("", ""), |d| (d.road.as_str(), d.house_number.as_str()));
                    !core.index.korean_address_matches(
                        &query,
                        g.lat as f64,
                        g.lon as f64,
                        road,
                        number,
                    )
                }) {
                    found = FullAddress::default();
                }
                if found
                    .address_details
                    .as_option()
                    .is_none_or(|d| d.house_number.is_empty() && d.road.is_empty())
                    && let Some(hit) = core.index.korean_fallback(&query)
                {
                    found = format_response(&core.index.render(&hit, true))?;
                    METRICS.tiers.with_label_values(&["korean_locality"]).inc();
                }
                own_regions(&mut found);
                // Rewrites and direct fallbacks must also respect the original geographic context.
                if !numeric_place_supported(&query, &found) {
                    found = FullAddress::default();
                }
                Ok(found)
            })
        })
        .await?
    }

    /// Completions of a partly typed address; see `Index::suggest`.
    pub async fn suggest(
        &self,
        text: &str,
        limit: usize,
        country_code: &str,
        focus: Option<crate::index::Focus>,
        landmarks: bool,
    ) -> Result<Vec<crate::index::Suggestion>> {
        let core = self.0.clone();
        let (text, cc) = (text.to_owned(), country_code.to_owned());
        Ok(tokio::task::spawn_blocking(move || {
            let cc = Some(cc.as_str()).filter(|c| !c.is_empty());
            core.index.memoized(|| {
                core.index
                    .suggest(&text, limit, cc, focus.as_ref(), landmarks)
            })
        })
        .await?)
    }

    pub async fn retrieve_suggestion(&self, id: &str) -> Result<Option<crate::index::Suggestion>> {
        let core = self.0.clone();
        let id = id.to_owned();
        Ok(tokio::task::spawn_blocking(move || core.index.retrieve_suggestion(&id)).await?)
    }

    /// The address that names a point's region; see `Index::reverse_region`.
    pub async fn reverse_region(&self, lat: f32, lon: f32) -> Result<Option<FullAddress>> {
        let core = self.0.clone();
        tokio::task::spawn_blocking(move || core.reverse_region(lat, lon)).await?
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
                    own_regions(&mut address);
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
    fn parse(&self, query: &str) -> Result<Option<ParsedAddress>> {
        let Some(parsed) = self.index.parse_address(query, true)? else {
            return Ok(None);
        };
        let address = parsed.iter().cloned().collect();
        tracing::debug!(target: "opengeocoding::address_log", ?query, parsed = ?address, "Address parsed");
        Ok(Some(address))
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
        let named_in_english =
            !part(address, "city").is_empty() && english.iter().any(place_agrees);
        if !part(address, "city").is_empty() && !named_in_english {
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
                let details = judged[index].address_details.as_option();
                if !named_in_english
                    && country_matches(country, details)
                    && road_agrees(address, details)
                {
                    let postal_town = candidate["address"]["postal_town"]
                        .as_str()
                        .is_some_and(|town| names_agree(city, town))
                        && (postcode_matches(postcode, details)
                            || (tier == "raw"
                                && self.index.isolated_house_without_postcode(
                                    &hits[index],
                                    postcode,
                                    country,
                                )));
                    let administrative_twin =
                        self.index.house_admin_locality_named(&hits[index], city)
                            && (postcode.is_empty() || postcode_agrees(postcode, details));
                    // the county's seat, unless the house's postal town is another
                    let seat = candidate["address"]["postal_town"]
                        .as_str()
                        .is_none_or(|town| names_agree(city, town))
                        && self
                            .index
                            .house_seat_named(&hits[index], city, part(address, "state"))
                        && (postcode.is_empty() || postcode_agrees(postcode, details));
                    let territory = details.is_some_and(|d| {
                        hk_mo::territory_as_town(&d.country_code)
                            && self.index.hk_house_alone(&hits[index])
                            && (country_named(city) == d.country_code
                                || (!d.house_number.is_empty()
                                    && judged[index].geo_coordinates.as_option().is_some_and(
                                        |g| {
                                            hk_mo::area_near(
                                                &self.index,
                                                city,
                                                &d.country_code,
                                                g.lat.into(),
                                                g.lon.into(),
                                            )
                                        },
                                    )))
                    });
                    if self.index.house_locality_named(&hits[index], city)
                        || seat
                        || territory
                        || ((postal_town || administrative_twin)
                            && state_consistent(address, details)
                            && !state_code_conflict(address, details))
                    {
                        judged.to_mut()[index]
                            .address_details
                            .get_or_insert_default()
                            .city = city.into();
                        continue;
                    }
                }
                // A street the data files under a district of the town stated (Warsaw's
                // Śródmieście) is in that town.
                if !named_in_english
                    && country_matches(country, details)
                    && details.is_some_and(|d| {
                        d.house_number.is_empty()
                            && !d.road.is_empty()
                            && roads_agree(part(address, "road"), &d.road)
                    })
                    && self.index.street_locality_named(&hits[index], city)
                {
                    judged.to_mut()[index]
                        .address_details
                        .get_or_insert_default()
                        .city = city.into();
                    continue;
                }
                let eligible = match candidate["addresstype"].as_str() {
                    Some("city" | "town" | "village" | "municipality" | "hamlet") => true,
                    Some(
                        "region" | "state" | "province" | "state_district" | "county" | "country",
                    ) => bare,
                    _ => false,
                };
                if !eligible
                    || !matches!(candidate["class"].as_str(), Some("place" | "boundary"))
                    || city_agrees(address, details)
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
        // "Petit Soufrière" parses as a house "petit" in a town "soufrière": the place the whole
        // first part names, a word end cut short ("Petite Soufriere"), is the one meant
        let first_part = (!city.is_empty()
            && !part(address, "house").is_empty()
            && ["road", "house_number", "postcode"]
                .iter()
                .all(|key| part(address, key).is_empty()))
        .then(|| query.split(',').next())
        .flatten();
        let spelt_out = |f: &FullAddress| {
            first_part.is_some_and(|written| {
                f.display_name
                    .split(',')
                    .next()
                    .is_some_and(|name| cut_short(written, name))
            })
        };
        for index in 0..judged.len() {
            if spelt_out(&judged[index])
                && country_matches(country, judged[index].address_details.as_option())
            {
                judged.to_mut()[index]
                    .address_details
                    .get_or_insert_default()
                    .city = city.into();
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
            let vouches = |h| {
                format_response(&self.index.render(h, true))
                    .ok()
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
            };
            // a postcode spans several places ("14727" is Cuba's and Hinsdale's): any may be
            // the postal town
            hits.is_empty() || hits.iter().any(vouches)
        });
        let overriding = tier.ends_with("without_stated_country");
        // libpostal files fragments ("province of") as a state district; only a state is vouched
        let stated_state = {
            let mut a = address.clone();
            a.shift_remove("state_district");
            a
        };
        let stated_region = Some(part(address, "state"))
            .filter(|s| !s.is_empty() && !country.is_empty())
            .and_then(|s| self.index.region_named(s, country));
        let other_region = |details: Option<&AddressDetails>| {
            stated_region.as_deref().is_some_and(|code| {
                details.is_some_and(|d| {
                    let cc = code.split('-').next().unwrap_or("");
                    let own: Vec<_> = d
                        .region_codes
                        .iter()
                        .filter(|c| c.code.split('-').next() == Some(cc))
                        .collect();
                    !own.is_empty()
                        && !own.iter().any(|c| c.code.eq_ignore_ascii_case(code))
                        && !region_agrees(address, Some(d))
                })
            })
        };
        let mut viable: Vec<_> = judged
            .iter()
            .enumerate()
            .filter(|(i, f)| {
                let details = f.address_details.as_option();
                (query.chars().any(char::is_alphabetic)
                    || candidates[*i]["addresstype"] == "postcode")
                    && numeric_place_supported(query, &english[*i])
                    && !(candidates[*i]["addresstype"] == "country"
                    && !part(address, "road").is_empty()
                    && !part(address, "house_number").is_empty())
                    && (!overriding || state_consistent(&stated_state, details))
                    && (!(part(address, "state").ends_with('市')
                        && part(address, "city").ends_with('區'))
                        || (state_consistent(address, details)
                            && details.is_some_and(|d| {
                                (d.road.is_empty() || roads_agree(part(address, "road"), &d.road))
                                    && (d.house_number.is_empty() || road_agrees(address, details))
                            })))
                    // "El Oasis, Coahuila de Zaragoza" is not the El Oasis in Chihuahua
                    && (!other_region(details)
                        || house_road_city(address, details)
                        || postcode_agrees(postcode, details))
                    // a road alone doesn't outweigh the state: "10 Elm Street, MA" is not Newark's;
                    // nor, with no country stated, do a road and town in a country with no region
                    // of that code ("115 Somerville Rd, Somerville, NB" is not Auckland's)
                    && (!state_code_conflict(address, details)
                        || (road_agrees(address, details)
                            && city_agrees(address, details)
                            && (!country.is_empty()
                                || details.is_some_and(|d| {
                                    self.index
                                        .region_countries(part(address, "state"))
                                        .iter()
                                        .any(|cc| cc == filed_country(&d.country_code))
                                })))
                        || postcode_agrees(postcode, details))
                    && (postcode_matches(postcode, details) || house_road_city(address, details))
                    && !au::municipal_conflict(&self.index, &candidates[*i], address, postcode)
                    && !ee::municipality_conflict(address, details)
                    && !si::municipality_conflict(address, details)
                    && (part(address, "city").is_empty()
                        || city_agrees(address, details)
                        || details.is_some_and(|d| {
                            road_agrees(address, details) && si::municipality_agrees(address, d)
                        })
                        || named_in_region(query, details)
                        || (same_postcode_country(country, postcode, details)
                            && (locality(details) || *postcode_vouches))
                        || postcode_place_agrees(address, postcode, details)
                        || (house_road_postcode(address, postcode, details)
                            && (junk_city || *postcode_vouches))
                        || house_road_city(address, details)
                        || finer_near(f))
            })
            .collect();
        // one candidate needs no ordering
        if viable.len() > 1 && !city.is_empty() {
            // The stated town as a candidate's own name comes before a suburb filed under it.
            let bare = !lone_state
                && ["road", "house_number", "postcode"]
                    .iter()
                    .all(|key| part(address, key).is_empty());
            // Without a country, preserve the index's importance order between regions and
            // villages: a tiny foreign namesake must not displace a region.
            viable.sort_by_cached_key(|(i, f)| {
                (
                    !spelt_out(f),
                    !town_named(address, f.address_details.as_option()),
                    if bare {
                        match candidates[*i]["addresstype"].as_str() {
                            Some("city" | "town") => 0,
                            Some("municipality") => 1,
                            Some("region" | "state" | "province")
                                if country.is_empty() && tier == "raw" =>
                            {
                                2
                            }
                            Some("village") => 2,
                            _ => 3,
                        }
                    } else {
                        0
                    },
                )
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
                .filter_map(|(_, f)| {
                    f.address_details
                        .as_option()
                        .map(|d| filed_country(&d.country_code))
                })
                .collect::<std::collections::HashSet<_>>()
                .len(),
        }))
    }

    /// "Madgoul, Djibouti" answered with no more than the country, or not at all, with a county
    /// or region of that name ("Kolahun, Liberia"), or with a town, street or other place below
    /// a region that no part of the answer is named like ("Keskese Street" in Asmara, the city
    /// of Djibouti): the locality of that name in that country (in that county or region), if
    /// there is one. Refining an empty, country-only or area answer requires nearby source
    /// addresses naming the locality, or GeoNames corroborating its type-qualified primary name
    /// as a mapped village/hamlet. A region of another name stays. A locality added from GeoNames
    /// below `cities500`'s bar answers nothing else: often no one is recorded living there and
    /// only GeoNames has it, the weakest evidence the index holds.
    pub(crate) fn named_locality(&self, query: &str, found: FullAddress) -> Result<FullAddress> {
        let Some((name, country)) = query.split_once(',') else {
            return Ok(found);
        };
        let name = name.trim_matches(whitespace);
        let cc = named_country(country);
        if country.contains(',') || cc.is_empty() || !named_country(name).is_empty() {
            return Ok(found);
        }
        use crate::index::schema::fold;
        let key = fold(name);
        let details = found.address_details.as_option();
        let below = |d: &AddressDetails| {
            [
                &d.city,
                &d.town,
                &d.village,
                &d.suburb,
                &d.hamlet,
                &d.road,
                &d.neighbourhood,
                &d.city_district,
                &d.quarter,
                &d.municipality,
            ]
            .into_iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<Vec<String>>()
        };
        let finer = details.map(below).unwrap_or_default();
        // "Afiamalu Sasa'e" is named so; a street is not ("Keskese Street")
        let words = format!(" {key} ");
        let named = details.is_some_and(|d| {
            fold(&d.road) == key
                || finer
                    .iter()
                    .filter(|s| **s != d.road)
                    .chain([&d.state, &d.region, &d.county])
                    .any(|s| format!(" {} ", fold(s)).contains(&words))
        });
        // A territory filed under its sovereign is the answer's state ("American Samoa"), with
        // only the sovereign's code for it (US-AS), which the response drops (`own_regions`).
        let coarse = details.is_none_or(|d| {
            let own = format!("{}-", d.country_code.to_uppercase());
            d.county.is_empty()
                && !d
                    .region_codes
                    .iter()
                    .any(|r| r.code.to_uppercase().starts_with(&own))
        });
        // a county, or a region with none, named as the query is no more than the town of that
        // name in it: "Kolahun, Liberia", "Bel Ombre, Seychelles"
        let area = details
            .map(|d| {
                if d.county.is_empty() {
                    &d.state
                } else {
                    &d.county
                }
            })
            .filter(|a| finer.is_empty() && !a.is_empty() && fold(a) == key);
        let elsewhere = !finer.is_empty() && !named;
        if !(coarse || area.is_some() || elsewhere) {
            return Ok(found);
        }
        // what only GeoNames has fills a gap, never replaces an answer: "Koksan" is a hamlet
        // there and, in Cyrillic only, the Goksan-eup the query found
        let Some(hit) = self.index.locality_named(name, &cc, !elsewhere) else {
            return Ok(found);
        };
        if !elsewhere && !self.index.locality_supported(&hit, name) {
            return Ok(found);
        }
        let mut place = format_response(&self.index.render(&hit, true))?;
        if let Some(area) = area
            && place
                .address_details
                .as_option()
                .is_none_or(|d| d.county != *area && d.state != *area)
        {
            return Ok(found);
        }
        place.good_match = true;
        METRICS.tiers.with_label_values(&["named_locality"]).inc();
        Ok(place)
    }

    fn resolve(&self, query: &str) -> Result<FullAddress> {
        let sar = hk_mo::without_sar(query);
        let query = sar.as_deref().unwrap_or(query);
        let palestine = ps::as_palestine(query);
        let query = palestine.as_deref().unwrap_or(query);
        let eircode = ie::compact_eircode(query);
        let query = eircode.as_deref().unwrap_or(query);
        let house_first = ee::county_first(query);
        let query = house_first.as_deref().unwrap_or(query);
        // "Palestine" is the country, not Palestine, Texas
        let cc = country_named(query);
        if !cc.is_empty()
            && !query.contains(',')
            && query.trim_matches(whitespace).chars().count() > 3
            && let Some(hit) = self.index.country_place(&cc)
        {
            let mut found = format_response(&self.index.render(&hit, true))?;
            found.good_match = true;
            METRICS.tiers.with_label_values(&["country_name"]).inc();
            return Ok(found);
        }
        if let Some(found) = self.numbered_place(query)? {
            return Ok(found);
        }
        let found = self.resolve_address(query)?;
        if found
            .address_details
            .as_option()
            .is_some_and(|d| jp::conflicting_prefecture(&self.index, query, d))
        {
            return Ok(self
                .stated_town(query)?
                .filter(|f| {
                    f.address_details.as_option().is_some_and(|d| {
                        d.country_code == "jp" && !jp::conflicting_prefecture(&self.index, query, d)
                    })
                })
                .unwrap_or_default());
        }
        let ignored = self.stated_country_ignored(query, &found);
        if (found.display_name.is_empty() || ignored)
            && let Some(degraded) = self.degraded(query)?
        {
            return self.named_settlement(query, degraded);
        }
        if ignored {
            return Ok(FullAddress::default());
        }
        // a weak answer above the street to a query with a number may hide the stated house
        let above_street = !found.display_name.is_empty()
            && !found.good_match
            && query.chars().any(|c| c.is_ascii_digit())
            && found
                .address_details
                .as_option()
                .is_some_and(|d| d.road.is_empty() && d.house_number.is_empty());
        if (found.display_name.is_empty() || above_street)
            && let Some(mended) = self.mended(query, above_street)?
        {
            return Ok(mended);
        }
        self.named_settlement(query, found)
    }

    fn numbered_place(&self, query: &str) -> Result<Option<FullAddress>> {
        if query
            .split(',')
            .filter(|p| !p.trim().is_empty() && p.trim().chars().all(char::is_numeric))
            .count()
            != 1
        {
            return Ok(None);
        }
        for number in query
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty() && p.chars().all(char::is_numeric))
        {
            for value in self.search(number).1 {
                if value["addresstype"] == "postcode"
                    || !matches!(value["class"].as_str(), Some("place" | "boundary"))
                    || !own_name(&value, number).is_some_and(|n| n == number)
                {
                    continue;
                }
                let found = format_response(&value)?;
                let mut names = vec![];
                names_of(&value, &mut names);
                let (explained, total) = coverage(query, names.iter().map(String::as_str));
                if explained == total && numeric_place_supported(query, &found) {
                    return Ok(Some(accept(
                        found,
                        &ParsedAddress::new(),
                        "",
                        "numbered_place",
                        true,
                    )));
                }
            }
        }
        Ok(None)
    }

    /// For a query nothing answers: the street, house or written postcode found once what may
    /// have garbled its parse is mended, each mend kept for the next ("…, EE. UU." as the
    /// country's name, "Taubaté/SP" as two parts, "Hamburg-Barmbek" as Hamburg, a Danish postal
    /// district or a German town's long name as the town, a place repeated before the street, a
    /// trailing label or phone fragment, a number-only part, a town written first); else the
    /// town it names. With `house_only`, only a house found as a good match.
    fn mended(&self, query: &str, house_only: bool) -> Result<Option<FullAddress>> {
        let is_place = |text: &str| self.index.names_place(text.trim(), None);
        let stated = named_country(query.rsplit(',').next().unwrap_or(""));
        let postcoded = |digits: usize| {
            query.split(',').any(|p| {
                let code = p.trim().split(whitespace).next().unwrap_or("");
                code.len() == digits && code.bytes().all(|b| b.is_ascii_digit())
            })
        };
        let danish = stated == "dk" || (stated.is_empty() && postcoded(4));
        let german = stated == "de" || (stated.is_empty() && postcoded(5));
        let steps: [&Rewrite; 10] = [
            &english_country,
            &slash_as_comma,
            &|q| with_towns(q, de::town_without_district).filter(|_| german),
            &|q| {
                with_towns(q, |t| dk::postal_district_town(t).map(str::to_owned)).filter(|_| danish)
            },
            &|q| with_towns(q, de::town_without_qualifier).filter(|_| german),
            &without_leading_place,
            &without_trailing_words,
            &|q| {
                without_trailing_part(q, |part| {
                    is_place(part) || !self.index.region_countries(part).is_empty()
                })
            },
            &|q| town_last(q, is_place),
            &|q| without_number_parts(q, is_place),
        ];
        let written = normalize_postcode(query);
        let mut mended = query.to_owned();
        for (at, step) in steps.iter().enumerate() {
            let Some(next) = step(&mended) else {
                continue;
            };
            mended = next;
            let mut found = self.resolve_address(&mended)?;
            // a dropped postcode contradicted the answer, or the data lacks it
            if at == steps.len() - 1 {
                found.good_match = false;
            }
            // the resolver may place what is left of the query abroad
            if !stated.is_empty() && !country_matches(&stated, found.address_details.as_option()) {
                continue;
            }
            if found.address_details.as_option().is_some_and(|d| {
                if house_only {
                    return found.good_match && !d.house_number.is_empty();
                }
                !d.road.is_empty()
                    || !d.house_number.is_empty()
                    || (normalize_postcode(&d.postcode).len() >= 4
                        && written.contains(&normalize_postcode(&d.postcode)))
            }) {
                METRICS.tiers.with_label_values(&["mended"]).inc();
                return Ok(Some(found));
            }
        }
        if house_only {
            return Ok(None);
        }
        self.stated_town(query)
    }

    /// The town a query names when its street, house and postcode found nothing: the chōme,
    /// ward or city of a Japanese address; else the town written first before the street
    /// ("Galati, Str. Brailei, Nr. 32"), or the one part after the street that names no region
    /// or district, within the region stated after it ("Kitakyushu, Fukuoka"). A region written
    /// with a postal district and nothing else after the street is the town ("Dublin 1"). Only a
    /// city, or a place the stated postcode lies in, answers, and only for an address with a
    /// street or house, or a list of names; never a good match.
    fn stated_town(&self, query: &str) -> Result<Option<FullAddress>> {
        let (prefecture, tries) = jp::town_queries(query);
        let prefecture = Some(prefecture)
            .filter(|p| !p.is_empty())
            .and_then(|p| self.index.region_named(&p, "jp"));
        for tried in tries {
            let found = self.resolve_written(&tried)?;
            if found.address_details.as_option().is_some_and(|d| {
                d.country_code == "jp"
                    && d.road.is_empty()
                    && prefecture.as_deref().is_none_or(|code| {
                        d.region_codes
                            .iter()
                            .any(|r| r.code.eq_ignore_ascii_case(code))
                    })
            }) {
                METRICS.tiers.with_label_values(&["stated_town"]).inc();
                return Ok(Some(accept(
                    found,
                    &ParsedAddress::new(),
                    "",
                    "stated_town",
                    false,
                )));
            }
        }
        let parsed = self.parse(query)?.unwrap_or_default();
        // "PO Box 217, Apia": no street to have missed
        if query.chars().any(char::is_numeric)
            && part(&parsed, "road").is_empty()
            && part(&parsed, "house_number").is_empty()
        {
            return Ok(None);
        }
        let mut body: Vec<&str> = query
            .split(',')
            .map(|p| p.trim_matches(whitespace))
            .filter(|p| !p.is_empty())
            .collect();
        let cc = body
            .last()
            .map_or(String::new(), |last| named_country(last));
        let country = if cc.is_empty() { None } else { body.pop() };
        let regions_of = [
            cc.clone(),
            expected_country(&parsed),
            implied_country(&parsed),
        ]
        .into_iter()
        .find(|c| !c.is_empty());
        let name_of = |p: &str| {
            p.split(whitespace)
                .filter(|w| !w.is_empty() && !w.chars().any(char::is_numeric))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let word_is = |p: &str, f: &dyn Fn(&str) -> bool| {
            p.split(whitespace)
                .map(normalize_place)
                .any(|w| w.len() > 1 && f(&w))
        };
        let street = |p: &str| word_is(p, &|w| canon(w).is_some() || is_canonical(w));
        let district = |p: &str| word_is(p, &|w| matches!(w, "dist" | "district" | "ward"));
        let region = |name: &str| {
            !name.to_lowercase().ends_with(" city")
                && regions_of
                    .as_deref()
                    .is_some_and(|cc| self.index.region_named(name, cc).is_some())
        };
        let mut state = None;
        let town = if let [first, rest @ ..] = &body[..]
            && !rest.is_empty()
            && !first.chars().any(char::is_numeric)
            && !street(first)
            && rest
                .iter()
                .all(|p| street(p) || p.chars().any(char::is_numeric))
        {
            Some(name_of(first))
        } else {
            // the first part of an address is its street, house or building
            let address = query.chars().any(char::is_numeric);
            let mut places = vec![];
            let mut postal_region = None;
            for (i, p) in body.iter().enumerate().rev() {
                if street(p) || (i == 0 && address) {
                    break;
                }
                let name = name_of(p);
                if district(p) || name.chars().filter(|c| c.is_alphabetic()).count() < 3 {
                    continue;
                }
                if region(&name) {
                    if p.chars().any(char::is_numeric) {
                        postal_region.get_or_insert(name.clone());
                    }
                    state.get_or_insert(name);
                    continue;
                }
                places.push(name);
            }
            // several places, of which the data knows none as written: which is the town?
            match &places[..] {
                [only] => Some(only.clone()),
                [] => postal_region.inspect(|_| state = None),
                _ => None,
            }
        };
        let Some(name) = town else {
            return Ok(None);
        };
        let postcode = part(&parsed, "postcode").to_owned();
        let in_country = Some(cc.as_str()).filter(|c| !c.is_empty());
        let state_code = state
            .as_deref()
            .zip(regions_of.as_deref())
            .and_then(|(state, cc)| self.index.region_named(state, cc));
        // "Hamburg-Barmbek", "København N", "Kehl am Rhein", "Taipei City": the town without
        // its district or qualifier too
        let forms = [
            Some(name.clone()),
            de::town_without_district(&name).filter(|_| matches!(cc.as_str(), "de" | "")),
            dk::postal_district_town(&name)
                .filter(|_| matches!(cc.as_str(), "dk" | ""))
                .map(str::to_owned),
            de::town_without_qualifier(&name).filter(|_| matches!(cc.as_str(), "de" | "")),
            name.rsplit_once(' ')
                .filter(|(_, last)| last.eq_ignore_ascii_case("city"))
                .map(|(head, _)| head.to_owned()),
        ];
        for form in forms.into_iter().flatten() {
            // only a place of that name, not one a letter off
            if !self.index.names_place(&form, in_country) {
                continue;
            }
            let mut place = ParsedAddress::new();
            place.insert("city".into(), form.clone());
            if let Some(country) = country {
                place.insert("country".into(), country.to_owned());
            }
            let Some(found) = self.settlement(&form, &place, &cc, &postcode)? else {
                continue;
            };
            let details = found.address_details.as_option();
            let city = details.is_some_and(|d| !d.city.is_empty() && names_agree(&form, &d.city));
            let in_state = state_code.as_deref().is_none_or(|code| {
                details.is_some_and(|d| {
                    d.region_codes
                        .iter()
                        .any(|r| r.code.eq_ignore_ascii_case(code))
                })
            });
            // a town's or village's name is often another's too ("Beni" in Myagdi and Mustang)
            if in_state && (city || self.postcode_near(&postcode, &found, true) == Some(true)) {
                METRICS.tiers.with_label_values(&["stated_town"]).inc();
                return Ok(Some(found));
            }
        }
        Ok(None)
    }

    /// An area answer whose region holds a city, town or village named as the query's first
    /// part is that settlement: "Drammen, Viken, Norway" is the town, not its municipality, and
    /// "Bogotá, Bogota, Colombia" the city, not its district.
    fn named_settlement(&self, query: &str, found: FullAddress) -> Result<FullAddress> {
        let Some(d) = found.address_details.as_option() else {
            return Ok(found);
        };
        let area = [
            &d.road,
            &d.house_number,
            &d.city,
            &d.town,
            &d.village,
            &d.suburb,
            &d.city_district,
            &d.neighbourhood,
            &d.hamlet,
            &d.quarter,
        ]
        .iter()
        .all(|s| s.is_empty());
        let parts: Vec<&str> = query
            .split(',')
            .map(|p| p.trim_matches(whitespace))
            .filter(|p| !p.is_empty())
            .collect();
        let Some((&first, mut rest)) = parts.split_first() else {
            return Ok(found);
        };
        if let Some((&last, before)) = rest.split_last()
            && !named_country(last).is_empty()
        {
            rest = before;
        }
        let Some(code) = d.region_codes.iter().find(|r| r.admin_level == 4) else {
            return Ok(found);
        };
        // "Banten, Indonesia" is the province, and "Niulakita, Tuvalu" may be the island's
        // council: only a place list with a region after the town ("Bogotá, Bogota, Colombia")
        // names the town.
        if !area || rest.is_empty() || !named_country(first).is_empty() {
            return Ok(found);
        }
        let Some(hit) = self.index.settlement_in(first, &code.code) else {
            return Ok(found);
        };
        let mut settled = format_response(&self.index.render(&hit, true))?;
        settled.good_match = found.good_match;
        METRICS.tiers.with_label_values(&["named_settlement"]).inc();
        Ok(settled)
    }

    /// A place-only query ("Srinagar, Jammu and Kashmir, Pakistan") answered outside the country
    /// it ends with. A street or postcode may place an address abroad; a list of place names
    /// may not, nor may the country read as a town ("Rue 2 1846, الفيدار, Lebanon" is not
    /// Lebanon, Tennessee). The country's name as the answer's region ("Atlanta, Georgia") is
    /// not a country.
    fn stated_country_ignored(&self, query: &str, found: &FullAddress) -> bool {
        let Some(details) = found.address_details.as_option() else {
            return false;
        };
        let Some((rest, last)) = query.rsplit_once(',') else {
            return false;
        };
        let stated = named_country(last);
        let town_named_last = [&details.city, &details.town, &details.village]
            .iter()
            .any(|name| names_agree(last, name));
        !stated.is_empty()
            && !rest.trim_matches(whitespace).is_empty()
            && (!query.chars().any(char::is_numeric) || town_named_last)
            && details.road.is_empty()
            && details.house_number.is_empty()
            && !country_matches(&stated, Some(details))
            && ![
                &details.state,
                &details.province,
                &details.region,
                &details.county,
            ]
            .iter()
            .any(|name| names_agree(last, name))
    }

    /// For a query no lookup answers, or answers outside the country it ends with: what its
    /// parts still name in that country, never as a good match. A district of the first part's
    /// name in the stated region ("North 24 Parganas, West Bengal, India"); the town with the
    /// country, unless it lies in another region than the one stated, farther than
    /// `NEIGHBOUR_KM` from it; else the stated region, matched
    /// by its loose spelling too ("Al Iskandariyah", "North Jutland", "Mun. Chişinău"); else a
    /// town one edit off or the first of a hyphenated name ("Fes-Ville-Nouvelle"); else the
    /// country. A lone name with no country loses its last word or what follows a hyphen
    /// ("San Francisco Bay", "Dallas-Fort Worth Metroplex") and answers only as a settlement.
    fn degraded(&self, query: &str) -> Result<Option<FullAddress>> {
        let parts: Vec<&str> = query
            .split(',')
            .map(|p| p.trim_matches(whitespace))
            .filter(|p| !p.is_empty())
            .collect();
        let Some((&last, rest)) = parts.split_last() else {
            return Ok(None);
        };
        let weak = |mut found: FullAddress, tier: &str| {
            found.good_match = false;
            METRICS.tiers.with_label_values(&[tier]).inc();
            Some(found)
        };
        let cc = named_country(last);
        // an address with a number has a street, a house or a postcode to place it by, unless the
        // number is in a place's name ("North 24 Parganas")
        if parts.iter().any(|p| {
            p.chars().any(char::is_numeric) && (cc.is_empty() || !self.index.place_named(p, &cc))
        }) {
            return Ok(None);
        }
        // Réunion's places answer as France's, and a shortened name there finds another hamlet
        // ("Jabrun-du-Sud" is not Jabrun)
        if crate::index::part_of_sovereign(&cc) {
            return Ok(None);
        }
        if cc.is_empty() {
            if !rest.is_empty() {
                return Ok(None);
            }
            for shorter in [
                last.split_once('-').map(|(head, _)| head.trim().to_owned()),
                last.rsplit_once(whitespace)
                    .map(|(head, _)| head.trim().to_owned()),
            ]
            .into_iter()
            .flatten()
            .filter(|s| s.chars().filter(|c| c.is_alphabetic()).count() >= 3)
            {
                let found = self.resolve_written(&shorter)?;
                if found
                    .address_details
                    .as_option()
                    .is_some_and(|d| d.road.is_empty() && locality(Some(d)))
                {
                    return Ok(weak(found, "degraded_shortened"));
                }
            }
            return Ok(None);
        }
        let in_country = |f: &FullAddress| {
            f.address_details
                .as_option()
                .is_some_and(|d| !d.country_code.is_empty() && d.road.is_empty())
                && country_matches(&cc, f.address_details.as_option())
        };
        let below_country = |f: &FullAddress| {
            in_country(f)
                && f.address_details.as_option().is_some_and(|d| {
                    locality(Some(d))
                        || !d.region_codes.is_empty()
                        || !d.county.is_empty()
                        || !d.state.is_empty()
                })
        };
        let level4 = |f: &FullAddress| {
            f.address_details.as_option().and_then(|d| {
                d.region_codes
                    .iter()
                    .find(|r| r.admin_level == 4)
                    .map(|r| r.code.to_uppercase())
            })
        };
        let (town, regions) = match rest {
            [] => (None, &[][..]),
            [only] => (None, std::slice::from_ref(only)),
            [town, regions @ ..] => (Some(*town), regions),
        };
        // and whether it is a region: a town the parts also name doesn't rule out another town
        let mut region = None;
        for part in regions {
            if let Some(place) = self.index.region_spelt(part, &cc) {
                let hit = crate::index::Hit::Place(place);
                region = Some((format_response(&self.index.render(&hit, true))?, true));
                break;
            }
            // a lone part with the country is the query, which answered nothing
            if rest.len() == 1 {
                break;
            }
            let found = self.resolve_written(&format!("{part}, {last}"))?;
            if below_country(&found) {
                let area = !locality(found.address_details.as_option());
                region = Some((found, area));
                break;
            }
        }
        // for a lone part, its town reading comes after its region reading
        let town = town.or(if rest.len() == 1 && region.is_none() {
            Some(rest[0])
        } else {
            None
        });
        let stated = region
            .as_ref()
            .filter(|(_, area)| *area)
            .and_then(|(found, _)| level4(found));
        if let Some(town) = town {
            if let Some(code) = &stated
                && let Some(hit) = self.index.district_in(town, code)
            {
                let district = format_response(&self.index.render(&hit, true))?;
                return Ok(weak(district, "degraded_district"));
            }
            let mut tries = vec![];
            if rest.len() > 1 {
                tries.push(town.to_owned());
            }
            tries.extend(self.index.similar_place(town, Some(&cc)));
            // "Taipei City", "Fes-Ville-Nouvelle": the name without what qualifies it, unless a
            // region is stated ("Athens-Clarke County" is not Athens, Georgia's namesake)
            let head = town
                .split_once('-')
                .map(|(head, _)| head)
                .or_else(|| town.strip_suffix(" city"))
                .map(str::trim)
                .filter(|head| {
                    region.is_none()
                        && !head.contains(' ')
                        && head.chars().filter(|c| c.is_alphabetic()).count() >= 3
                });
            tries.extend(head.map(str::to_owned));
            for tried in tries {
                let found = self.resolve_written(&format!("{tried}, {last}"))?;
                let near = |code: &str| {
                    found.geo_coordinates.as_option().is_some_and(|g| {
                        self.index
                            .near_region(g.lat as f64, g.lon as f64, code, NEIGHBOUR_KM)
                    })
                };
                let elsewhere = stated.as_ref().is_some_and(|stated| {
                    level4(&found).is_some_and(|code| code != *stated) && !near(stated)
                });
                if below_country(&found)
                    && locality(found.address_details.as_option())
                    && !elsewhere
                {
                    return Ok(weak(found, "degraded_town"));
                }
            }
        }
        if let Some((region, _)) = region {
            return Ok(weak(region, "degraded_region"));
        }
        let country = match self.index.country_place(&cc) {
            Some(hit) => format_response(&self.index.render(&hit, true))?,
            // a territory without a country place of its own, not a town of its name
            None => Some(self.resolve_written(last)?)
                .filter(|f| {
                    f.address_details.as_option().is_some_and(|d| {
                        !locality(Some(d))
                            && d.county.is_empty()
                            && (d.state.is_empty() || d.state == d.country)
                    })
                })
                .unwrap_or_default(),
        };
        Ok(in_country(&country)
            .then(|| weak(country, "degraded_country"))
            .flatten())
    }

    fn resolve_address(&self, query: &str) -> Result<FullAddress> {
        let found = self.resolve_written(query)?;
        let has_house = |found: &FullAddress| {
            found
                .address_details
                .as_option()
                .is_some_and(|d| !d.house_number.is_empty())
        };
        if has_house(&found) {
            return Ok(found);
        }
        if let Some((turned, number)) = crate::index::number_last(query) {
            let alt = self.resolve_written(&turned)?;
            if alt.good_match && same_house_number(&alt, &number) {
                return Ok(alt);
            }
        }
        // Some Australian sources hold "unit/number" houses; elsewhere the building's house
        // answers a unit the index lacks.
        if let Some(building) = au::without_australian_unit(query) {
            let building = self.resolve_written(&building)?;
            if has_house(&building) {
                return Ok(building);
            }
        }
        if let Some((spaced, number)) = spaced_house_suffix(query) {
            let alt = self.resolve_written(&spaced)?;
            if alt.good_match && same_house_number(&alt, &number) {
                return Ok(alt);
            }
        }
        if let Some((undirected, number, postcode)) = ca::without_street_direction(query) {
            let alt = self.resolve_written(&undirected)?;
            if alt.good_match
                && same_house_number(&alt, &number)
                && alt.address_details.as_option().is_some_and(|d| {
                    d.country_code == "ca" && d.postcode.replace(' ', "") == postcode
                })
            {
                return Ok(alt);
            }
        }
        // A Danish supplementary place the index files differently breaks the parse of the whole
        // address; the house, road and postcode still name it. The place is unverified unless
        // the answer names it.
        if let Some((kept, place)) = dk::without_danish_place(query) {
            let mut alt = self.resolve_written(&kept)?;
            let postcode = query
                .split(',')
                .find_map(|p| {
                    p.trim()
                        .split_once(' ')
                        .filter(|(c, _)| c.len() == 4 && c.bytes().all(|b| b.is_ascii_digit()))
                })
                .map_or("", |(code, _)| code);
            if has_house(&alt)
                && alt
                    .address_details
                    .as_option()
                    .is_some_and(|d| d.postcode.is_empty() || d.postcode == postcode)
            {
                if !normalize_place(&alt.display_name).contains(&normalize_place(&place)) {
                    alt.good_match = false;
                }
                return Ok(alt);
            }
        }
        if let Some(mut house) = self.suggested_house(query, &found)? {
            if let Some((_, code)) = without_calling_code(query)
                && house
                    .address_details
                    .as_option()
                    .is_some_and(|d| calling_code(&d.postcode, &code))
            {
                without_postcode(&mut house);
            }
            return Ok(house);
        }
        if let Some(d) = found.address_details.as_option()
            && d.country_code == "tw"
            && d.house_number.is_empty()
            && !d.road.is_empty()
            && let Some(at) = found.geo_coordinates.as_option()
            && let Some(region) = d.region_codes.iter().find(|r| r.admin_level == 4)
            && let Some(address) = self.parse(query)?
            && !part(&address, "house_number").is_empty()
            && normalize_place(part(&address, "road")) == normalize_place(&d.road)
            && let Some(house) = self.index.house_across_township(
                &d.road,
                part(&address, "house_number"),
                (at.lat as f64, at.lon as f64),
                &region.code,
            )
        {
            return Ok(accept(
                format_response(&house)?,
                &ParsedAddress::new(),
                "",
                "house_across_township",
                false,
            ));
        }
        Ok(found)
    }

    /// A house the parse missed ("D-112 Shastri Nagar, 342003, India" reads as one road, "Plot
    /// No.14" is one number) that the query still names in full: its number, every word of its
    /// street and its postcode or town. Found by the completion lookup, which reads no parse, or
    /// near the answer by a number of several words; one such house or none.
    fn suggested_house(&self, query: &str, found: &FullAddress) -> Result<Option<FullAddress>> {
        use crate::index::schema::{fold, number_key, pc_norm};
        // A street or postcode answer, or any when the parse lost the number ("L5-B6 Lake
        // Nauhan Street, Cavinti"), states what the house is checked against; a town or nothing
        // with a parsed number does not.
        let parsed_number = PARSED_NUMBER.get();
        let answered = found.address_details.as_option();
        let stated =
            answered.filter(|d| !d.road.is_empty() || !d.postcode.is_empty() || !parsed_number);
        let stated_postcode = stated.map_or(String::new(), |d| pc_norm(&d.postcode));
        let tokens: Vec<&str> = query
            .split(|c: char| c == ',' || whitespace(c))
            .filter(|t| !t.is_empty())
            .collect();
        let written: std::collections::HashSet<String> = tokens
            .iter()
            .map(|t| pc_norm(t))
            .chain(query.split(',').map(pc_norm))
            .collect();
        let numbered =
            |t: &str| t.chars().any(|c| c.is_ascii_digit()) && pc_norm(t) != stated_postcode;
        // up to four words of one part of the query
        let mut numbers = vec![];
        for part in query.split(',') {
            let words: Vec<&str> = part.split(whitespace).filter(|w| !w.is_empty()).collect();
            for len in 1..=4 {
                for run in words.windows(len) {
                    // a house number never holds the postcode ("TX 75460")
                    if !stated_postcode.is_empty()
                        && run.iter().any(|w| pc_norm(w) == stated_postcode)
                    {
                        continue;
                    }
                    let run = number_key(&run.join(" "));
                    if numbered(&run) && !numbers.contains(&run) {
                        numbers.push(run);
                    }
                }
            }
        }
        if numbers.is_empty() {
            return Ok(None);
        }
        let words: std::collections::HashSet<String> =
            fold(query).split(' ').map(String::from).collect();
        let named = |name: &str| {
            let name = fold(name);
            !name.is_empty() && name.split(' ').all(|w| words.contains(w))
        };
        let cc = answered
            .map(|d| filed_country(&d.country_code))
            .filter(|c| !c.is_empty());
        let mut houses = vec![];
        // A label and a number ("Plot No.14", "Lot 25", "367 หมู่ 1") make a number rare enough
        // to look for on the answer's postcode; words of the road do not make one.
        let road: Vec<String> = stated.map_or(vec![], |d| {
            fold(&d.road).split(' ').map(String::from).collect()
        });
        let unroaded: Vec<String> = numbers
            .iter()
            .filter(|n| {
                n.split(' ')
                    .filter(|w| !w.chars().any(|c| c.is_ascii_digit()))
                    .all(|w| fold(w).split(' ').all(|f| !road.iter().any(|r| *r == f)))
            })
            .cloned()
            .collect();
        let spelt: Vec<String> = unroaded
            .iter()
            .filter(|n| n.contains(' '))
            .cloned()
            .collect();
        let at = found
            .geo_coordinates
            .as_option()
            .map_or((0.0, 0.0), |g| (g.lat as f64, g.lon as f64));
        let lookup = |around: bool| match stated {
            // A street without the house: its namesakes may hold it, those nearby too when the
            // parse lost the number.
            Some(d) if !around && !d.road.is_empty() => self.index.houses_numbered(
                &unroaded,
                &d.postcode,
                &d.road,
                !parsed_number,
                cc,
                at,
                named,
            ),
            // The postcode or town without the street: the completion lookup, which reads no
            // parse.
            _ if !around => self
                .index
                .suggest_houses(query, cc, 5)
                .iter()
                .filter_map(|s| self.index.suggested_house(s))
                .collect::<Vec<_>>(),
            Some(d)
                if around && d.road.is_empty() && !d.postcode.is_empty() && !spelt.is_empty() =>
            {
                self.index
                    .houses_numbered(&spelt, &d.postcode, "", false, cc, at, named)
            }
            _ => vec![],
        };
        // the postcode's houses only when the first lookup names none
        for around in [false, true] {
            if around && !houses.is_empty() {
                break;
            }
            for value in lookup(around) {
                let house = format_response(&value)?;
                let Some(d) = house.address_details.as_option() else {
                    continue;
                };
                let postcode = pc_norm(&d.postcode);
                let number = number_key(&d.house_number);
                // The words written beside the number belong to its street or its places:
                // "Heyrovského č.p. 470, Olomouc" is not house č.p. 470 of the street Olomouc.
                let places = [
                    &d.road,
                    &d.city,
                    &d.town,
                    &d.village,
                    &d.municipality,
                    &d.suburb,
                    &d.city_district,
                    &d.hamlet,
                    &d.county,
                    &d.state_district,
                    &d.state,
                    &d.country,
                    &d.postcode,
                ];
                let street: Vec<String> = fold(&d.road).split(' ').map(String::from).collect();
                let street: Vec<&str> = street.iter().map(String::as_str).collect();
                let known: std::collections::HashSet<String> = places
                    .iter()
                    .flat_map(|p| fold(p).split(' ').map(String::from).collect::<Vec<_>>())
                    .collect();
                let beside = query.split(',').any(|part| {
                    let ws: Vec<&str> = part.split(whitespace).filter(|w| !w.is_empty()).collect();
                    (1..=4.min(ws.len())).any(|len| {
                        (0..=ws.len() - len).any(|i| {
                            let rest = fold(&[&ws[..i], &ws[i + len..]].concat().join(" "));
                            number_key(&ws[i..i + len].join(" ")) == number
                                && rest.split(' ').all(|w| w.is_empty() || known.contains(w))
                                // a number beside its town alone is a district ("Dublin 4")
                                && (rest.is_empty()
                                    || rest.split(' ').any(|w| street.contains(&w)))
                        })
                    })
                });
                // and no other number of the query (a postcode, another house) goes unexplained
                let explained = |w: &&str| {
                    let w = number_key(w);
                    !numbered(&w)
                        || number.split(' ').any(|n| n == w)
                        || pc_norm(&w) == postcode
                        || fold(&w).split(' ').all(|f| known.contains(f))
                };
                // Without a street or postcode in the answer, the parse found nothing to check
                // the house by: a part it doesn't explain has to name a place ("Missing Bay"
                // may be another address's suburb).
                let parts_named = || {
                    query.split(',').all(|part| {
                        fold(part)
                            .split(' ')
                            .all(|w| w.is_empty() || known.contains(w) || numbered(w))
                            || !named_country(part).is_empty()
                            || self.index.names_place(part.trim(), cc)
                    })
                };
                let verified = numbers.contains(&number)
                    && beside
                    && tokens.iter().all(explained)
                    && named(&d.road)
                    && (stated_postcode.is_empty()
                        || postcode.is_empty()
                        || postcode == stated_postcode)
                    && match cc {
                        Some(cc) => filed_country(&d.country_code) == cc,
                        None => named(&d.country),
                    }
                    && (!postcode.is_empty() && written.contains(&postcode)
                        || [
                            &d.city,
                            &d.town,
                            &d.village,
                            &d.municipality,
                            &d.suburb,
                            &d.city_district,
                            &d.hamlet,
                        ]
                        .into_iter()
                        .any(|p| named(p))
                        // a district named for its seat ("Karachi Division")
                        || [&d.county, &d.state_district].into_iter().any(|p| {
                            p.rsplit_once(' ').is_some_and(|(name, word)| {
                                matches!(word.to_lowercase().as_str(), "division" | "district")
                                    && named(name)
                            })
                        })
                        // or the seat's own name, for a house of no town (Libreville's
                        // department)
                        || (!locality(Some(d)) && d.hamlet.is_empty() && named(&d.county)))
                    && (stated.is_some() || parts_named());
                if verified
                    && !(d.country_code == "au"
                        && self.parse(query)?.is_some_and(|parsed| {
                            au::municipal_conflict(
                                &self.index,
                                &value,
                                &parsed,
                                part(&parsed, "postcode"),
                            )
                        }))
                {
                    houses.push(house);
                }
            }
        }
        let point = |f: &FullAddress| f.geo_coordinates.as_option().map(|g| (g.lat, g.lon));
        let Some(first) = houses.first().and_then(point) else {
            return Ok(None);
        };
        let identity = |f: &FullAddress| {
            f.address_details
                .as_option()
                .map(|d| (number_key(&d.house_number), fold(&d.road)))
        };
        if houses.iter().any(|h| {
            identity(h) != identity(&houses[0])
                || point(h).is_none_or(|(lat, lon)| km_between(first.0, first.1, lat, lon) > 0.2)
        }) {
            return Ok(None);
        }
        Ok(Some(accept(
            houses.swap_remove(0),
            &ParsedAddress::new(),
            "",
            "suggested_house",
            true,
        )))
    }

    /// Reviewed historical memberships answer only where the current lookups found no good match.
    fn resolve_written(&self, query: &str) -> Result<FullAddress> {
        let found = self.resolve_current(query)?;
        if found.good_match {
            return Ok(found);
        }
        match self.index.historical_place(query) {
            Some(hit) => Ok(accept(
                format_response(&self.index.render(&hit, true))?,
                &ParsedAddress::new(),
                "",
                "historical_place",
                true,
            )),
            None => Ok(found),
        }
    }

    fn resolve_current(&self, query: &str) -> Result<FullAddress> {
        if query.chars().count() <= 256
            && let Some(hits) = self.index.place_slug(query)
        {
            let Some(hit) = hits.first() else {
                METRICS.tiers.with_label_values(&["unresolved"]).inc();
                return Ok(FullAddress::default());
            };
            return Ok(accept(
                format_response(&self.index.render(hit, true))?,
                &ParsedAddress::new(),
                "",
                "slug",
                true,
            ));
        }
        // The unit of a Danish address is not modelled: its building's house answers, and a
        // query whose building is not found as a house is resolved as written.
        if let Some(building) = dk::without_danish_unit(query) {
            let found = self.resolve(&building)?;
            if found
                .address_details
                .as_option()
                .is_some_and(|d| !d.house_number.is_empty())
            {
                return Ok(found);
            }
        }
        // A repeated part ("Kyiv, Kyiv, Ukraine", "Madrid, ES, Spain") answers as the query
        // without it.
        let written = query;
        let collapsed = without_redundant_parts(query);
        // "Luxembourg, Luxembourg" is the city; the repeat only goes when the query as written
        // finds nothing ("Chile, Chile")
        if collapsed.is_some() && is_repeated_country(query) {
            let found = self.resolve_once(query)?;
            if !found.display_name.is_empty() {
                return Ok(found);
            }
        }
        let query = collapsed.as_deref().unwrap_or(query);
        let coded = query;
        let named = self.index.with_region_names(query);
        let query = named.as_deref().unwrap_or(query);
        let mut found = self.resolve_once(query)?;
        // A region's code is no place by itself: with no country stated and nothing else in the
        // query naming the answer ("Uhlandstraße, 95 Wohnungen, BGF" is not Bangui), the query
        // is read as written.
        if named.is_some() && coded.split(',').all(|p| named_country(p).is_empty()) {
            let rest: Vec<&str> = coded
                .split(',')
                .zip(query.split(','))
                .filter(|(before, after)| before.trim() == after.trim())
                .map(|(before, _)| before)
                .collect();
            let mut names = vec![found.display_name.as_str()];
            if let Some(d) = found.address_details.as_option() {
                names.extend([
                    d.road.as_str(),
                    d.city.as_str(),
                    d.town.as_str(),
                    d.village.as_str(),
                ]);
            }
            if coverage(&rest.join(","), names).0 == 0 {
                found = self.resolve_once(coded)?;
            }
        }
        if found
            .address_details
            .as_option()
            .is_none_or(|d| d.house_number.is_empty())
            && let Some((building, number)) = ca::canadian_building(query)
        {
            let recovered = self.resolve(building)?;
            if recovered.good_match
                && recovered.address_details.as_option().is_some_and(|d| {
                    d.country_code == "ca"
                        && crate::index::schema::number_key(&d.house_number)
                            == crate::index::schema::number_key(number)
                })
            {
                return Ok(recovered);
            }
            found.good_match = false;
        }
        // "4 Rue Des Cafeiers, 237, Cameroon": the country's calling code where the postcode goes,
        // which some of the data's houses carry as their postcode too ("+237"). It is no postcode.
        if let Some((uncoded, code)) = without_calling_code(query) {
            let coded = |f: &FullAddress| {
                f.address_details
                    .as_option()
                    .is_some_and(|d| !d.house_number.is_empty() && calling_code(&d.postcode, &code))
            };
            if !found.good_match {
                // the house the data gives that code ("+233") before a namesake elsewhere
                let tagged = self
                    .search(query)
                    .1
                    .iter()
                    .filter_map(|v| format_response(v).ok())
                    .find(|f| coded(f));
                if let Some(mut tagged) = tagged {
                    tagged.good_match = true;
                    found = tagged;
                } else {
                    let parsed_number = PARSED_NUMBER.get();
                    let retried = self.resolve_once(&uncoded)?;
                    PARSED_NUMBER.set(parsed_number);
                    // an answer as written gives way to a house only
                    if retried.good_match
                        && (found.display_name.is_empty()
                            || retried
                                .address_details
                                .as_option()
                                .is_some_and(|d| !d.house_number.is_empty()))
                    {
                        found = retried;
                    }
                }
            }
            if coded(&found) {
                without_postcode(&mut found);
            }
        }
        if !found.display_name.is_empty() {
            return Ok(found);
        }
        // The query as written is the last try, unless a country code was dropped from it:
        // "IE, Ireland" parses without a country and lets a same-named place abroad answer.
        let dropped_code = collapsed.as_deref().is_some_and(|c| {
            written
                .split(',')
                .map(str::trim)
                .any(|p| p.chars().count() <= 3 && !c.split(',').any(|k| k.trim() == p))
        });
        if collapsed.is_some() && !dropped_code {
            let retried = self.resolve_once(written)?;
            if !retried.display_name.is_empty() {
                return Ok(retried);
            }
        }
        // the parser's slips that the query itself can mend: no comma before the country, and
        // parenthesised remarks
        for retry in [
            comma_before_country(query),
            place_before_region_code(query),
            without_parentheses(query, true),
            without_parentheses(query, false),
        ]
        .into_iter()
        .flatten()
        {
            let found = self.resolve_once(&retry)?;
            if !found.display_name.is_empty() {
                return Ok(found);
            }
        }
        // a guess about what the parser made of the province: only a confident answer
        if let Some(retry) = tw::without_taiwan_province(query) {
            let retried = self.resolve_once(&retry)?;
            if retried.good_match {
                return Ok(retried);
            }
        }
        // A postcode the data lacks garbles the parse of its town; one it holds is never dropped,
        // so a town it contradicts stays ruled out. Without the postcode, only a place in a town
        // whose own postcode doesn't contradict it answers, and never confidently.
        let country = expected_country(&parsed_country(query.rsplit(',').next().unwrap_or("")));
        if let Some((postcode, retry)) = self
            .index
            .unknown_postcode(query, Some(country.as_str()).filter(|c| !c.is_empty()))
        {
            let mut retried = self.resolve_once(&retry)?;
            let details = retried.address_details.as_option();
            if locality(details) && postcode_matches(&postcode, details) {
                retried.good_match = false;
                return Ok(retried);
            }
        }
        Ok(found)
    }

    fn resolve_once(&self, query: &str) -> Result<FullAddress> {
        if query.is_empty() || query.chars().count() > 256 {
            return Ok(FullAddress::default());
        }
        // Tabs and ASCII record separators between fields read as spaces to both parsers.
        let query = &query.replace(|ch: char| whitespace(ch) && ch != ' ', " ");
        if PLACEHOLDERS.contains(&query.trim_matches(whitespace).to_lowercase().as_str()) {
            return Ok(FullAddress::default());
        }
        let started = std::time::Instant::now();
        if let Some(hit) = self.index.korean_region(query) {
            let mut found = format_response(&self.index.render(&hit, true))?;
            found.good_match = true;
            METRICS
                .search
                .with_label_values(&["korean"])
                .observe(started.elapsed().as_secs_f64());
            METRICS.tiers.with_label_values(&["korean"]).inc();
            return Ok(found);
        }
        if let Some(hits) = jp::houses(&self.index, query) {
            METRICS
                .search
                .with_label_values(&["japanese"])
                .observe(started.elapsed().as_secs_f64());
            if let [hit] = hits.as_slice() {
                let mut found = format_response(&self.index.render(hit, true))?;
                found.good_match = true;
                METRICS.tiers.with_label_values(&["japanese"]).inc();
                return Ok(found);
            }
            if hits.len() > 1 {
                return Ok(FullAddress::default());
            }
        }
        if let Some(hits) = self.index.korean_search(query) {
            METRICS
                .search
                .with_label_values(&["korean"])
                .observe(started.elapsed().as_secs_f64());
            if let [hit] = hits.as_slice() {
                let mut found = format_response(&self.index.render(hit, true))?;
                found.good_match = true;
                METRICS.tiers.with_label_values(&["korean"]).inc();
                return Ok(found);
            }
            if hits.len() > 1 {
                return Ok(FullAddress::default());
            }
        }
        if let Some((hit, exact)) = self.index.gb_postcode_in_area(query) {
            let found = format_response(&self.index.render(&hit, true))?;
            return Ok(accept(
                found,
                &ParsedAddress::new(),
                "",
                "gb_postcode",
                exact,
            ));
        }
        let turned = self.parser.reordered(query)?;
        let query = turned.as_deref().unwrap_or(query);
        let Some(mut address) = self.parse(query)? else {
            tracing::debug!(target: "opengeocoding::address_log", "Address unresolved");
            METRICS.tiers.with_label_values(&["unresolved"]).inc();
            return Ok(FullAddress::default());
        };
        let mended = self.mend_split_country(query, &mut address)?;
        let query = mended.as_deref().unwrap_or(query);
        // A complete address with a town that is also a country ("Cuba, NY 14727") would
        // otherwise be rotated into one whose country is that town.
        let complete = ["house_number", "road", "postcode"]
            .iter()
            .all(|key| !part(&address, key).is_empty());
        if part(&address, "country").is_empty() && !complete {
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
                if let Some(rotated) = self.parse(&rotated_query)?
                    && !part(&rotated, "country").is_empty()
                {
                    address = rotated;
                }
            }
        }
        PARSED_NUMBER.set(!part(&address, "house_number").is_empty());
        let stray = part(&address, "road").to_owned();
        tidy(&mut address);
        self.state_not_country(&mut address);
        recover_postcode(query, &mut address);
        tracing::debug!(target: "opengeocoding::address_log", parsed = ?address, "Address normalized");
        let unstrayed;
        let query = if !stray.is_empty() && part(&address, "road").is_empty() {
            unstrayed = without_word(query, &stray);
            unstrayed.as_str()
        } else {
            query
        };
        // "Kalverstraat 9999, 1012 NX Centrum": libpostal reads a postcode of digits and letters
        // as the road, or its letters alone ("1234", road "nx")
        let joined = format!("{} {}", part(&address, "postcode"), part(&address, "road"));
        let written = joined.trim();
        let compact = normalize_postcode(written);
        if !part(&address, "road").is_empty()
            && compact.len() >= 5
            && compact.chars().any(|c| c.is_ascii_digit())
            && compact.chars().any(char::is_alphabetic)
            && query.to_lowercase().contains(&written.to_lowercase())
            && self.index.knows_postcode(
                written,
                Some(expected_country(&address).as_str()).filter(|c| !c.is_empty()),
            )
        {
            address.shift_remove("road");
            address.insert("postcode".into(), written.to_owned());
        }
        // Repeated fields ("Karnataka, India, India") reach the parser as one "india india".
        if let Some(stated) = address.get_mut("country") {
            let words: Vec<_> = stated.split(' ').collect();
            let half = words.len() / 2;
            if half > 0 && words.len() % 2 == 0 && words[..half] == words[half..] {
                *stated = words[..half].join(" ");
            }
        }
        // A country written in its own language ("România") that the reference data lists only
        // in others: the index knows it by its names in every language.
        if let Some(stated) = address.get_mut("country")
            && country_named(stated).is_empty()
            && let Some(name) = self.index.country(stated).and_then(|cc| country_name(&cc))
        {
            *stated = name.into();
        }
        let coded = self.glued_code(query, &mut address);
        let country = expected_country(&address);
        let postcode = if part(&address, "postcode").is_empty() {
            ie::eircode(query)
        } else {
            part(&address, "postcode").into()
        };
        // A country the parser missed ("Kingdom of Bahrain" as a house) keeps a wrong place in
        // another country from answering, but it doesn't license a country-level fallback.
        let tailed = part(&address, "country").is_empty().then(|| {
            unlabelled_country(query, &address).or(coded).map(|stated| {
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
        let district = part(stated_address, "state_district");
        let subdivision = if !district.is_empty()
            && !part(stated_address, "state").is_empty()
            && stated_address.keys().all(|key| {
                matches!(
                    key.as_str(),
                    "state" | "state_district" | "country_region" | "country"
                )
            }) {
            let mut parent = stated_address.clone();
            parent.shift_remove("state_district");
            self.first_acceptable(
                district,
                stated_address,
                (&stated_country, &postcode),
                "subdivision",
                true,
                None,
            )?
            .filter(|chosen| {
                let details = chosen.found.address_details.as_option();
                matches!(chosen.kind.as_str(), "county" | "state_district")
                    && chosen.named_by(district)
                    && state_consistent(&parent, details)
                    && country_matches(&stated_country, details)
            })
        } else {
            None
        };
        let raw = match subdivision {
            Some(chosen) => Some(chosen),
            None => self.first_acceptable(
                query,
                stated_address,
                (&stated_country, &postcode),
                "raw",
                true,
                None,
            )?,
        };
        // Without a house of its own, the raw answer is the stated town or nothing.
        if raw.as_ref().is_none_or(|chosen| {
            chosen
                .found
                .address_details
                .as_option()
                .is_none_or(|d| d.house_number.is_empty())
        }) && let Some(found) =
            self.house_despite_town(stated_address, &stated_country, &postcode)?
        {
            return Ok(found);
        }
        if raw.as_ref().is_none_or(|chosen| {
            chosen
                .found
                .address_details
                .as_option()
                .is_none_or(|d| d.house_number.is_empty())
        }) && let Some(found) = self.house_despite_postcode(stated_address, &postcode)?
        {
            return Ok(found);
        }
        if raw.as_ref().is_none_or(|chosen| {
            chosen
                .found
                .address_details
                .as_option()
                .is_none_or(|d| d.house_number.is_empty())
        }) && postcode.is_empty()
            && let Some(found) = self.house_in_split_town(query, stated_address, &stated_country)?
        {
            return Ok(found);
        }
        // "715 Meehan St, Santa Maria, CA 93454, Canada": no house in the stated country, but the
        // query without the country finds one whose number, road and postcode are the stated ones.
        if raw.as_ref().is_none_or(|chosen| {
            chosen
                .found
                .address_details
                .as_option()
                .is_none_or(|d| d.house_number.is_empty())
        }) && !stated_country.is_empty()
            && !part(stated_address, "house_number").is_empty()
        {
            let stripped = without_country(query, stated_address);
            let mut without = stated_address.clone();
            without.shift_remove("country");
            if !stripped.is_empty()
                && let Some(better) = self.first_acceptable(
                    &stripped,
                    &without,
                    ("", &postcode),
                    "without_stated_country",
                    false,
                    None,
                )?
                && let Some(d) = better.found.address_details.as_option()
                && !d.house_number.is_empty()
                && house_road_postcode(stated_address, &postcode, Some(d))
            {
                return Ok(accept(
                    better.found,
                    &address,
                    &postcode,
                    "house_without_stated_country",
                    true,
                ));
            }
        }
        if let Some(chosen) = raw {
            let (explained, total) = chosen.explained(query, stated_address);
            // A place that names a few words of a long query is a guess: "Twin City Shopping
            // Centre, Burgersfort" is not Thuin. A street or house answer was checked against its
            // road.
            let details = chosen.found.address_details.as_option();
            let named = ["city", "city_district", "suburb"]
                .iter()
                .any(|key| chosen.named_by(part(stated_address, key)));
            let thin = details.is_none_or(|d| d.road.is_empty())
                && !postcode_agrees(&postcode, details)
                && total >= 3
                && explained * 2 < total;
            if thin && !(named && chosen.countries <= 1) {
                weak = Some(chosen);
            } else {
                // a place the query names, beside more it doesn't explain: a part naming a
                // place that explains more is the place meant ("Ungasan, Badung, Bali" is not
                // the village Badung in Gianyar)
                if thin
                    && chosen.kind != "postcode"
                    && let Some(parted) = self.search_parts(
                        query,
                        stated_address,
                        &stated_country,
                        Some(&chosen),
                        true,
                    )?
                    && parted.kind != "postcode"
                    && !same_place_name(&parted, &chosen)
                {
                    return Ok(part_answer(parted, query, stated_address, &postcode));
                }
                let road = part(stated_address, "road");
                let road_found =
                    road.is_empty() || details.is_some_and(|d| !d.road.is_empty()) || {
                        let (explained, total) = chosen.explained(road, stated_address);
                        explained == total
                    };
                return Ok(accept(chosen.found, &address, &postcode, "raw", road_found));
            }
        }
        // "2725-377 Mem Martins": the postcode's place has another name than the town, which
        // only a stated country lets stand for it. The postcode and the town can state it.
        if let Some(name) = self
            .postcode_country(query, stated_address, &postcode)
            .and_then(|cc| self.index.country_name(&cc))
            .filter(|name| !query.ends_with(name))
        {
            let found = self.resolve_once(&format!("{query}, {name}"))?;
            if !found.display_name.is_empty() {
                return Ok(found);
            }
        }
        if weak.is_none()
            && let Some(found) = self.fallbacks(query, &address, &country, &postcode)?
            // the country the parser missed still rules out a namesake abroad
            && (tailed.is_none()
                || country_matches(&stated_country, found.address_details.as_option()))
        {
            return Ok(found);
        }
        if let Some(chosen) =
            self.search_parts(query, stated_address, &stated_country, weak.as_ref(), false)?
        {
            return Ok(part_answer(chosen, query, stated_address, &postcode));
        }
        if let Some(chosen) = weak {
            return Ok(accept(chosen.found, &address, &postcode, "weak", false));
        }
        if let Some(found) = self.settlement(query, stated_address, &stated_country, &postcode)? {
            return Ok(found);
        }
        tracing::debug!(target: "opengeocoding::address_log", "Address unresolved");
        METRICS.tiers.with_label_values(&["unresolved"]).inc();
        Ok(FullAddress::default())
    }

    /// A ZIP that contradicts a street with no town ("7413 Janes Ranch Road, TX 78747"): the
    /// house is looked up by its state alone, which the index answers only when the state holds
    /// that number on one street of that name. Never a confident answer.
    fn house_despite_postcode(
        &self,
        address: &ParsedAddress,
        postcode: &str,
    ) -> Result<Option<FullAddress>> {
        let (number, road, state) = (
            part(address, "house_number"),
            part(address, "road"),
            part(address, "state"),
        );
        if [number, road, state].iter().any(|p| p.is_empty())
            || postcode.is_empty()
            || !part(address, "city").is_empty()
        {
            return Ok(None);
        }
        for candidate in self.search(&format!("{number} {road}, {state}")).1 {
            let found = format_response(&candidate)?;
            let details = found.address_details.as_option();
            if road_agrees(address, details) && state_consistent(address, details) {
                return Ok(Some(accept(
                    found,
                    address,
                    postcode,
                    "house_without_postcode",
                    false,
                )));
            }
        }
        Ok(None)
    }

    /// "Cabo Verde" after a street parses as a town "cabo" in a country "verde". The country's
    /// English name ("Cape Verde") may parse whole, and is then the query; else the last part is
    /// the country, and the other fields lose the words of its name.
    fn mend_split_country(
        &self,
        query: &str,
        address: &mut ParsedAddress,
    ) -> Result<Option<String>> {
        let Some((head, last)) = query.rsplit_once(',') else {
            return Ok(None);
        };
        let cc = named_country(last);
        if cc.is_empty()
            || part(address, "country").is_empty()
            || !expected_country(address).is_empty()
        {
            return Ok(None);
        }
        if let Some(name) =
            country_name(&cc).filter(|name| normalize_place(name) != normalize_place(last))
        {
            let english = format!("{head}, {name}");
            if let Some(parsed) = self.parse(&english)?
                && expected_country(&parsed) == cc
            {
                *address = parsed;
                return Ok(Some(english));
            }
        }
        let words = |text: &str| -> Vec<String> {
            text.split(whitespace)
                .filter(|w| !w.is_empty())
                .map(normalize_place)
                .collect()
        };
        let (country, words) = (words(part(address, "country")), words(last));
        // a field that ends in the name's first words ("cabo", "skopje north") loses them
        if country.len() >= words.len() || !words.ends_with(&country) {
            return Ok(None);
        }
        for value in address.values_mut() {
            let own: Vec<&str> = value.split(whitespace).filter(|w| !w.is_empty()).collect();
            let shared = (1..=own.len().min(words.len()))
                .rev()
                .find(|&k| {
                    own[own.len() - k..]
                        .iter()
                        .map(|w| normalize_place(w))
                        .eq(words[..k].iter().cloned())
                })
                .unwrap_or(0);
            *value = own[..own.len() - shared].join(" ");
        }
        address.retain(|_, value| !value.is_empty());
        address.insert("country".into(), last.trim_matches(whitespace).into());
        Ok(None)
    }

    /// "Dover, DE 19901": a US state code that libpostal took for a country (Delaware for
    /// Germany), before a ZIP the index knows in the US and not in that country.
    fn state_not_country(&self, address: &mut ParsedAddress) {
        const US_STATES: [&str; 51] = [
            "al", "ak", "az", "ar", "ca", "co", "ct", "de", "dc", "fl", "ga", "hi", "id", "il",
            "in", "ia", "ks", "ky", "la", "me", "md", "ma", "mi", "mn", "ms", "mo", "mt", "ne",
            "nv", "nh", "nj", "nm", "ny", "nc", "nd", "oh", "ok", "or", "pa", "ri", "sc", "sd",
            "tn", "tx", "ut", "vt", "va", "wa", "wv", "wi", "wy",
        ];
        let code = part(address, "country").to_lowercase();
        let zip = part(address, "postcode");
        if part(address, "state").is_empty()
            && US_STATES.contains(&code.as_str())
            && zip.len() == 5
            && zip.bytes().all(|b| b.is_ascii_digit())
            && self.index.knows_postcode(zip, Some("us"))
            && !self.index.knows_postcode(zip, Some(&code))
        {
            address.shift_remove("country");
            address.insert("state".into(), code);
        }
    }

    /// A part of two capitals that the parser glued into a house or road ("Buchhaldenstrasse 1,
    /// CH, Frau" gives road "ch frau") is a code. A country's is returned, to rule out places
    /// abroad; a region's ("NB" for New Brunswick, or Noord-Brabant) becomes the stated state.
    fn glued_code(&self, query: &str, address: &mut ParsedAddress) -> Option<String> {
        let code = glued_code(query, address)?;
        if !expected_country(&parsed_country(&code)).is_empty() {
            return Some(code);
        }
        let stated = expected_country(address);
        let regions = self.index.region_countries(&code);
        if part(address, "state").is_empty()
            && regions
                .iter()
                .any(|cc| stated.is_empty() || filed_country(&stated) == cc)
        {
            for key in ["house", "road"] {
                if let Some(value) = address.get_mut(key) {
                    *value = value
                        .split(whitespace)
                        .filter(|w| !w.is_empty() && !w.eq_ignore_ascii_case(&code))
                        .collect::<Vec<_>>()
                        .join(" ");
                }
            }
            address.retain(|_, value| !value.is_empty());
            address.insert("state".into(), code.to_lowercase());
        }
        None
    }

    /// A neighbourhood the parser split into a suburb and a town ("Vila da Penha" read as suburb
    /// "vila da" and town "penha"), with a house and a state but no postcode: the house of that
    /// number and road in the stated state, when a place of the whole name lies within 5 km of it.
    /// Never a confident answer.
    fn house_in_split_town(
        &self,
        query: &str,
        address: &ParsedAddress,
        country: &str,
    ) -> Result<Option<FullAddress>> {
        let [number, road, suburb, city, state] =
            ["house_number", "road", "suburb", "city", "state"].map(|key| part(address, key));
        let name = format!("{suburb} {city}");
        if [number, road, suburb, city, state]
            .iter()
            .any(|p| p.is_empty())
            || !normalize_place(query).contains(&normalize_place(&name))
        {
            return Ok(None);
        }
        let place = create_query(address, &["state", "country"]);
        // the parser reads a number first ("909 Av. Vicente de Carvalho") into the road's words
        let written = [
            format!("{road} {number}, {place}"),
            format!("{number} {road}, {place}"),
        ];
        for candidate in written.iter().flat_map(|query| self.search(query).1) {
            let found = format_response(&candidate)?;
            let details = found.address_details.as_option();
            if road_agrees(address, details)
                && country_matches(country, details)
                && state_consistent(address, details)
                && let Some(d) = details
                && let Some(g) = found.geo_coordinates.as_option()
                && self.index.place_near(
                    &name,
                    Some(filed_country(&d.country_code)),
                    g.lat,
                    g.lon,
                    5.0,
                )
            {
                return Ok(Some(accept(
                    found,
                    address,
                    "",
                    "house_in_split_town",
                    false,
                )));
            }
        }
        Ok(None)
    }

    /// A town that contradicts the rest ("8 Ayr Court, Yonkers, NY 10901" for a house in the next
    /// town over): the house is looked up without the town, and answered only when its number,
    /// road and postcode are the stated ones, which two places rarely share. A confident answer
    /// only when the stated town is within 10 km of the house, as a postal town is.
    fn house_despite_town(
        &self,
        address: &ParsedAddress,
        country: &str,
        postcode: &str,
    ) -> Result<Option<FullAddress>> {
        if ["house_number", "road", "city"]
            .iter()
            .any(|key| part(address, key).is_empty())
            || postcode.is_empty()
        {
            return Ok(None);
        }
        let mut without = address.clone();
        for key in ["city", "suburb", "city_district", "state_district"] {
            without.shift_remove(key);
        }
        // The street lookup is scoped by a town or by the postcode's own streets, and a common
        // street name misses without either: the postcode's places stand in for the stated town.
        let cc = Some(country).filter(|c| !c.is_empty());
        let mut towns = vec![String::new()];
        for hit in self.index.postcode_hits(postcode, cc).iter().take(5) {
            if let Ok(place) = format_response(&self.index.render(hit, true))
                && let Some(d) = place.address_details.as_option()
                && let Some(town) = [&d.city, &d.town, &d.village, &d.municipality]
                    .into_iter()
                    .find(|t| !t.is_empty())
                && !towns.contains(town)
            {
                towns.push(town.clone());
            }
        }
        for town in towns {
            if !town.is_empty() {
                without.insert("city".into(), town);
            }
            let query = format!(
                "{} {}, {}",
                part(address, "house_number"),
                part(address, "road"),
                create_query(&without, &["city", "state", "postcode", "country"])
            );
            for candidate in self.search(&query).1 {
                let found = format_response(&candidate)?;
                let details = found.address_details.as_option();
                if house_road_postcode(address, postcode, details)
                    && country_matches(country, details)
                    && state_consistent(address, details)
                {
                    // With no country stated, the town and postcode together may name a place in
                    // another country than this house ("2560 Nidau" is Swiss, "82 Hauptstrasse,
                    // 2560" a street in Austria): the pair outweighs a coincidence of house and road.
                    if country.is_empty()
                        && self
                            .search(&format!("{postcode}, {}", part(address, "city")))
                            .1
                            .iter()
                            .filter_map(|v| format_response(v).ok())
                            .any(|f| {
                                let other = f.address_details.as_option();
                                city_agrees(address, other)
                                    && postcode_agrees(postcode, other)
                                    && other.zip(details).is_some_and(|(o, d)| {
                                        filed_country(&o.country_code)
                                            != filed_country(&d.country_code)
                                    })
                            })
                    {
                        continue;
                    }
                    // A postal town names its post office, often the next place over (Salcha for
                    // a house the data files under Aurora Lodge): near the house it agrees.
                    // A city-state is written as the city while the house is filed under a
                    // borough ("Berlin" for Marzahn-Hellersdorf); that city is the state, as is a
                    // province libpostal takes for the town ("Bueng Kan" of Bueng Kan Province).
                    let stated = normalize_place(part(address, "city"));
                    let postal = found.geo_coordinates.as_option().is_some_and(|g| {
                        self.index
                            .place_near(part(address, "city"), cc, g.lat, g.lon, 10.0)
                    }) || found.address_details.as_option().is_some_and(|d| {
                        !stated.is_empty()
                            && (normalize_place(&d.state) == stated
                                || admin_name_agrees(&stated, &d.state))
                    });
                    return Ok(Some(accept(
                        found,
                        address,
                        postcode,
                        "house_postcode",
                        postal,
                    )));
                }
            }
        }
        Ok(None)
    }

    /// Last resort: the stated town (or lone region) on its own, when something else in the query
    /// (a postcode the data files elsewhere or not at all, a street it lacks) kept every candidate
    /// out. The town is a better answer than none, and never a confident one.
    fn settlement(
        &self,
        query: &str,
        address: &ParsedAddress,
        country: &str,
        postcode: &str,
    ) -> Result<Option<FullAddress>> {
        let in_country = Some(country).filter(|c| !c.is_empty());
        let mut city = part(address, "city").to_owned();
        if city.is_empty()
            && ["state", "state_district", "country_region"]
                .iter()
                .all(|k| part(address, k).is_empty())
        {
            // "Mississauaga, Canada": the parser files a lone name and its country as one house
            let stated = normalize_place(part(address, "country"));
            let rest: Vec<&str> = query
                .split(',')
                .map(|p| p.trim_matches(whitespace))
                .filter(|p| !p.is_empty() && normalize_place(p) != stated)
                .collect();
            if let [only] = rest[..]
                && only.split(whitespace).count() <= 3
                && !only.chars().any(|c| c.is_numeric())
            {
                city = only.to_owned();
            }
        }
        // "Beyoğlu/İstanbul": either side may be the town; a name one edit off may be a typo
        let mut towns = vec![city.clone()];
        if city.contains('/') {
            towns.extend(
                city.rsplit('/')
                    .map(|t| t.trim().to_owned())
                    .filter(|t| !t.is_empty()),
            );
        }
        if !city.is_empty()
            && !self.index.names_place(&city, in_country)
            && let Some(similar) = self.index.similar_place(&city, in_country)
        {
            towns.push(similar);
        }
        for town in towns {
            // "West Shady Lane" is a street the parser took for a town
            let street = town.split(whitespace).next_back().map(normalize_place);
            if town.contains(' ')
                && street
                    .as_deref()
                    .is_some_and(|w| w.len() > 2 && canon(w).is_some())
            {
                continue;
            }
            let mut place = ParsedAddress::new();
            for key in ["state", "state_district", "country_region", "country"] {
                if let Some(value) = address.get(key) {
                    place.insert(key.into(), value.clone());
                }
            }
            if !town.is_empty() {
                place.insert("city".into(), town.clone());
            }
            let structured = create_query(&place, &["city", "state", "country"]);
            if place.keys().all(|key| key == "country") || !useful_query(&structured, "", country) {
                continue;
            }
            let Some(chosen) = self.first_acceptable(
                &structured,
                &place,
                (country, ""),
                "settlement",
                true,
                None,
            )?
            else {
                continue;
            };
            // only a settlement named as stated: a region, province or country of that name is
            // another place, and a postcode is not a town
            let county = chosen.kind == "county"
                && chosen
                    .found
                    .address_details
                    .as_option()
                    .is_some_and(|d| gr::municipality_is_town(&d.country_code))
                && self.postcode_near(postcode, &chosen.found, false) == Some(true);
            if !(chosen.anchor() || county) || chosen.kind == "postcode" || !chosen.named_by(&town)
            {
                continue;
            }
            // with no country stated, the region and the postcode have to fit the one the town
            // is in ("Novallas, Zaragoza" is not in the Philippines), and a postcode the data
            // does place has to be near the town
            let details = chosen.found.address_details.as_option();
            let fits = (!country.is_empty()
                || (state_consistent(&place, details)
                    && details.is_none_or(|d| {
                        postcode.is_empty()
                            || postcode_shape(filed_country(&d.country_code), postcode)
                                != Some(false)
                    })))
                && !self.postcode_elsewhere(
                    postcode,
                    &chosen.found,
                    part(address, "road").is_empty() && part(address, "house_number").is_empty(),
                );
            if fits && !self.ambiguous(&structured, &place, country, &chosen.found, &chosen.kind) {
                return Ok(Some(accept(chosen.found, address, "", "settlement", false)));
            }
        }
        Ok(None)
    }

    /// Another place the query names equally well lies far from `found` ("Austrheim" in two
    /// counties): with nothing to choose between them, the first would be a guess.
    fn ambiguous(
        &self,
        structured: &str,
        place: &ParsedAddress,
        country: &str,
        found: &FullAddress,
        own_kind: &str,
    ) -> bool {
        let Some(at) = found.geo_coordinates.as_option() else {
            return false;
        };
        // A city outranks a village of its name: "Madrid" is not "Madrid" in Almería.
        let size = |kind: &str| match kind {
            "city" => 0,
            "town" | "municipality" => 1,
            "village" => 2,
            _ => 3,
        };
        let found_size = size(own_kind);
        self.search(structured)
            .1
            .iter()
            .filter(|v| size(string(&v["addresstype"])) <= found_size)
            .filter_map(|v| format_response(v).ok())
            .filter(|f| {
                let details = f.address_details.as_option();
                city_agrees(place, details)
                    && country_matches(country, details)
                    && state_consistent(place, details)
            })
            .filter_map(|f| f.geo_coordinates.into_option())
            .any(|g| km_between(at.lat, at.lon, g.lat, g.lon) > 50.0)
    }

    /// The data places `postcode` (in the country of `found`), but nowhere near `found`: the
    /// postcode and the town contradict each other, and neither is the better guess.
    /// `town_only` is a query with no street: a code tagged with this country but sited in
    /// another ("28007" of Spain, placed in Niger) is not a contradiction. A house still is.
    fn postcode_elsewhere(&self, postcode: &str, found: &FullAddress, town_only: bool) -> bool {
        self.postcode_near(postcode, found, town_only) == Some(false)
    }

    /// Whether the data places `postcode` within 30 km of `found`; None when it doesn't place it
    /// (see `postcode_elsewhere`).
    fn postcode_near(&self, postcode: &str, found: &FullAddress, town_only: bool) -> Option<bool> {
        let (Some(at), Some(details)) = (
            found.geo_coordinates.as_option(),
            found.address_details.as_option(),
        ) else {
            return None;
        };
        if postcode.is_empty() {
            return None;
        }
        let cc = Some(filed_country(&details.country_code)).filter(|c| !c.is_empty());
        let hits = self.index.postcode_hits(postcode, cc);
        // A code stored under this country but sited in another ("28007" of Spain, placed in
        // Niger) is not evidence the postcode and the town disagree.
        let placed = hits.iter().take(8).filter_map(|hit| {
            let found = format_response(&self.index.render(hit, true)).ok()?;
            let there = found.address_details.as_option();
            if town_only && cc.is_some_and(|cc| !country_matches(cc, there)) {
                return None;
            }
            Some(
                found
                    .geo_coordinates
                    .into_option()
                    .is_none_or(|g| km_between(at.lat, at.lon, g.lat, g.lon) < 30.0),
            )
        });
        let mut any = false;
        let mut near = false;
        for hit in placed {
            any = true;
            near |= hit;
        }
        any.then_some(near)
    }

    /// The one country a query that states none is in by its postcode: Portugal for a postcode
    /// written as only Portugal writes them, else the one country where the postcode's own
    /// places, or the query's candidates that hold it, have the stated town within 10 km. A code
    /// several countries use ("10115") says nothing alone.
    fn postcode_country(
        &self,
        query: &str,
        address: &ParsedAddress,
        postcode: &str,
    ) -> Option<String> {
        if postcode.is_empty() || !part(address, "country").is_empty() {
            return None;
        }
        if pt::portuguese_postcode(postcode) {
            return Some("pt".into());
        }
        let town = ["city", "city_district", "suburb"]
            .iter()
            .map(|key| part(address, key))
            .find(|t| !t.is_empty())?;
        // ponytail: only the postcode's 5 most important places are looked at, so a country
        // whose places of it rank lower is missed; a per-country postcode lookup would lift that.
        let countries: std::collections::HashSet<String> = self
            .index
            .postcode_hits(postcode, None)
            .into_iter()
            .map(|hit| self.index.render(&hit, true))
            .chain(self.search(query).1)
            .filter_map(|value| {
                let found = format_response(&value).ok()?;
                let at = found.geo_coordinates.as_option()?;
                let details = found.address_details.as_option()?;
                let cc = filed_country(&details.country_code).to_lowercase();
                (!cc.is_empty()
                    && normalize_postcode(&details.postcode) == normalize_postcode(postcode)
                    && self.index.place_near(town, Some(&cc), at.lat, at.lon, 10.0))
                .then_some(cc)
            })
            .collect();
        let mut countries = countries.into_iter();
        countries.next().filter(|_| countries.next().is_none())
    }

    /// Structured re-queries of the parsed fields, loosest last.
    fn fallbacks(
        &self,
        query: &str,
        address: &ParsedAddress,
        country: &str,
        postcode: &str,
    ) -> Result<Option<FullAddress>> {
        // a structured re-query can't place a street in a stated state that no town or postcode
        // does ("100 Main Street, OH"), and a stated state ("MA") is not the country of the same
        // code (Morocco)
        let unplaced = part(address, "city").is_empty()
            && postcode.is_empty()
            && !part(address, "state").is_empty();
        let state_only = part(address, "country").is_empty() && !part(address, "state").is_empty();
        for (tier, order) in ORDERS {
            let structured = create_query(address, order);
            if !useful_query(&structured, postcode, country) {
                continue;
            }
            if let Some(chosen) =
                self.first_acceptable(&structured, address, (country, postcode), tier, true, None)?
                // a query with no road in it finds a street only by a state's or town's name
                // ("Calle Baja California Sur" for "Baja California Sur, La Paz")
                && !(unplaced
                    && chosen
                        .found
                        .address_details
                        .as_option()
                        .is_some_and(|d| !d.road.is_empty()))
                && !(!order.contains(&"road")
                    && road_named_for_place(address, chosen.found.address_details.as_option()))
                && !(state_only && chosen.kind == "country")
            {
                let details = chosen.found.address_details.as_option();
                // "3475 Las Vegas Blvd S NV": a street named for its town, which the state alone
                // doesn't find, is in that town
                if details.is_none_or(|d| d.road.is_empty() && !locality(Some(d)))
                    && let Some(found) = self.town_in_road(address, country, postcode)?
                {
                    return Ok(Some(found));
                }
                // A town named in a query its other parts don't explain ("Białka Tatrzańska, ul.
                // Środkowa 181 Termy Bania" is not Bania in Bulgaria): a part naming a place
                // that explains more is the place meant.
                let (explained, total) = chosen.explained(query, address);
                if details.is_none_or(|d| d.road.is_empty())
                    && chosen.kind != "postcode"
                    && total >= 3
                    && explained * 2 < total
                    && let Some(parted) =
                        self.search_parts(query, address, country, Some(&chosen), true)?
                    && parted.kind != "postcode"
                    && !same_place_name(&parted, &chosen)
                {
                    return Ok(Some(part_answer(parted, query, address, postcode)));
                }
                return Ok(Some(accept(chosen.found, address, postcode, tier, false)));
            }
        }
        if let Some(found) = self.town_in_road(address, country, postcode)? {
            return Ok(Some(found));
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
                    // A recognized country is not overridden by a bare name: Malta's "St
                    // Julians" is not the one in Wales. A street, number or postcode in the
                    // query has to place it elsewhere.
                    let details = chosen.found.address_details.as_option();
                    let stated = normalize_place(part(address, "country"));
                    // A road of a common name ("شارع 43", Street 43) is in many countries: a
                    // suburb stated has to be the answer's too.
                    let corroborated = country.is_empty()
                        || (road_agrees(address, details)
                            && suburb_agrees(address, details))
                        || postcode_agrees(postcode, details)
                        // a postcode in the shape of the found country's and not of the stated
                        // one's ("5757 BD" is Dutch, not Belgian)
                        || details
                            .is_some_and(|d| {
                                postcode_shaped_for(query, country, filed_country(&d.country_code))
                            })
                        || details.is_some_and(|d| {
                            !d.country.is_empty()
                                && normalize_place(query).contains(&normalize_place(&d.country))
                        })
                        || postcode_agrees(postcode, details)
                        || details.is_some_and(|d| {
                            [
                                &d.state,
                                &d.county,
                                &d.state_district,
                                &d.province,
                                &d.region,
                            ]
                            .iter()
                            .any(|name| !name.is_empty() && normalize_place(name) == stated)
                        });
                    if !corroborated {
                        continue;
                    }
                    return Ok(Some(accept(chosen.found, address, postcode, &tier, true)));
                }
            }
        }
        Ok(None)
    }

    /// A street named for a town ("3475 Las Vegas Blvd S NV") with a state but no town stated: the
    /// street in the town of its name, in the stated state, when the data has it there. Never a
    /// confident answer: the town is read from the street's name.
    fn town_in_road(
        &self,
        address: &ParsedAddress,
        country: &str,
        postcode: &str,
    ) -> Result<Option<FullAddress>> {
        let road = part(address, "road");
        if road.is_empty() || !part(address, "city").is_empty() || part(address, "state").is_empty()
        {
            return Ok(None);
        }
        let road = normalize_place(road);
        let name: Vec<&str> = road
            .split(' ')
            .filter(|w| w.chars().count() > 1 && canon(w).is_none() && !is_canonical(w))
            .collect();
        if name.concat().chars().count() < 4 || name.len() == road.split(' ').count() {
            return Ok(None);
        }
        let mut with = address.clone();
        with.insert("city".into(), name.join(" "));
        let written = format!(
            "{} {}, {}",
            part(address, "house_number"),
            part(address, "road"),
            create_query(&with, &["city", "state", "postcode", "country"])
        );
        let Some(chosen) = self.first_acceptable(
            written.trim(),
            &with,
            (country, postcode),
            "town_in_road",
            false,
            None,
        )?
        else {
            return Ok(None);
        };
        let details = chosen.found.address_details.as_option();
        let on_road = details.is_some_and(|d| {
            let found = normalize_place(&d.road);
            name.iter().all(|w| found.split(' ').any(|f| f == *w))
        });
        Ok(
            (on_road && city_agrees(&with, details) && state_consistent(address, details))
                .then(|| accept(chosen.found, address, postcode, "town_in_road", false)),
        )
    }

    /// Last resort for a query the parser garbled or that carries words no place has: each
    /// comma-separated part searched on its own. Only a settlement or postcode that accounts for
    /// at least two of the query's words answers, the one explaining most (a later part
    /// breaking ties: addresses end with the larger place), and only when it beats `weak`. With
    /// `named_only`, only a place its own part names.
    fn search_parts(
        &self,
        query: &str,
        address: &ParsedAddress,
        country: &str,
        weak: Option<&Chosen>,
        named_only: bool,
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
            let Some(parsed) = self.parse(text)? else {
                continue;
            };
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
        let mut best: Option<((bool, usize, usize, usize), usize)> = None;
        for (at, (position, named, explained, chosen)) in found.iter().enumerate() {
            // "Nr. 2" is not the postcode NR2: with no country to place one, a postcode needs
            // more of the query behind it
            let unplaced_postcode =
                chosen.kind == "postcode" && country.is_empty() && *explained < 3;
            if !chosen.anchor()
                || unplaced_postcode
                || (named_only && !*named)
                || (*explained < floor && !(guess && *named && *explained >= 1))
            {
                continue;
            }
            let key = (*named, support(at), *explained, *position);
            if best.is_none_or(|(k, _)| key > k) {
                best = Some((key, at));
            }
        }
        Ok(best.map(|(_, at)| found.swap_remove(at).3))
    }

    fn reverse_region(&self, lat: f32, lon: f32) -> Result<Option<FullAddress>> {
        let value = self.index.reverse_region(lat as f64, lon as f64);
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
            let admin_level = integer(&details["admin_level"])?;
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
                    admin_level,
                    local_name: string(&details["localname"]).into(),
                    en_name: en_name.into(),
                    admin_code: admin_code.into(),
                    parent_admin_code: self
                        .index
                        .parent_region_code(place)
                        .filter(|_| admin_level == 6)
                        .unwrap_or_default()
                        .into(),
                    ..Default::default()
                },
            ));
        }
        Ok(regions)
    }
}

/// The country a part of a query names by its name: a code ("CA", "PA") is as often a state's.
fn named_country(part: &str) -> String {
    if part.chars().filter(|c| c.is_alphabetic()).count() < 4 {
        return String::new();
    }
    country_named(part)
}

/// A postcode that is the calling code `code` ("+233", "00977").
fn calling_code(postcode: &str, code: &str) -> bool {
    let digits: String = postcode.chars().filter(char::is_ascii_digit).collect();
    !postcode.chars().any(char::is_alphabetic)
        && digits.strip_prefix("00").unwrap_or(&digits) == code
}

/// `found` without its postcode, which the data gave it from a calling code.
fn without_postcode(found: &mut FullAddress) {
    if let Some(d) = found.address_details.as_option_mut() {
        let postcode = std::mem::take(&mut d.postcode);
        found.display_name = found
            .display_name
            .split(", ")
            .filter(|p| *p != postcode)
            .collect::<Vec<_>>()
            .join(", ");
    }
}

/// Only the answer's own country's ISO 3166-2 codes: a territory answered with its ISO 3166-1
/// code (Puerto Rico `pr`, Guam `gu`, Hong Kong `hk`) is not also its sovereign's region (US-PR).
/// The overseas departments answer as France, so FR-974 stays.
fn own_regions(found: &mut FullAddress) {
    if let Some(d) = found.address_details.as_option_mut()
        && !d.country_code.is_empty()
    {
        let prefix = format!("{}-", d.country_code.to_uppercase());
        d.region_codes
            .retain(|r| r.code.to_uppercase().starts_with(&prefix));
    }
}

fn same_house_number(found: &FullAddress, number: &str) -> bool {
    found.address_details.as_option().is_some_and(|d| {
        use crate::index::schema::number_key;
        crate::index::same_number(&number_key(number), &number_key(&d.house_number))
    })
}

/// Two places of one name ("Choloma" in Cortés and in Santa Bárbara, "Managua" and "La
/// Managua"): a part naming the place a lookup already chose finds a namesake, not another place
/// of the query.
fn same_place_name(a: &Chosen, b: &Chosen) -> bool {
    let own = |c: &Chosen| normalize_place(c.found.display_name.split(',').next().unwrap_or(""));
    let (a, b) = (own(a), own(b));
    let (long, short) = if a.len() >= b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    !short.is_empty() && (long == short || long.ends_with(&format!(" {short}")))
}

/// A place `search_parts` found; a part that accounts for every word of the query is no
/// fallback.
fn part_answer(
    chosen: Chosen,
    query: &str,
    address: &ParsedAddress,
    postcode: &str,
) -> FullAddress {
    let (explained, total) = chosen.explained(query, address);
    let whole = total > 0 && explained == total;
    accept(chosen.found, address, postcode, "part", whole)
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
    let agrees = agrees_with(city);
    names
        .filter_map(|(_, v)| v.as_str())
        .flat_map(|v| v.split(';'))
        .chain(string(&candidate["display_name"]).split(',').next())
        .find(|name| agrees(name))
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
        let formatted = format_postcode(filed_country(&details.country_code), &details.postcode);
        if formatted != details.postcode {
            found.display_name = found.display_name.replace(&details.postcode, &formatted);
            details.postcode = formatted;
        }
    }
    // a P.O. box is a counter, not a place: the town is the best that can be said
    let located = !is_po_box(part(address, "po_box"));
    found.good_match = good && house_found && postcode_found && located;
    tracing::debug!(target: "opengeocoding::address_log", tier, good_match = found.good_match, "Address candidate selected");
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
