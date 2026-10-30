use super::*;

impl Index {
    pub(super) fn streets(
        &self,
        road: &str,
        number: Option<&str>,
        city: Option<&str>,
        postcode: Option<&str>,
        cc: Option<&str>,
        exact: bool,
    ) -> Vec<Hit> {
        self.streets_in(road, number, city, None, postcode, cc, exact)
    }

    /// `streets`, where a `suburb` of the town stated picks between same-named streets that
    /// hold the number: two villages' "Calle Real 7" in one municipality.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn streets_in(
        &self,
        road: &str,
        number: Option<&str>,
        city: Option<&str>,
        suburb: Option<&str>,
        postcode: Option<&str>,
        cc: Option<&str>,
        exact: bool,
    ) -> Vec<Hit> {
        let named: Vec<u32> = city
            .into_iter()
            .flat_map(|city| {
                self.places(city, cc, None, &|p| {
                    !matches!(&*p.subtype, "country" | "dependency")
                })
            })
            .filter_map(|h| match h {
                Hit::Place(id) => Some(id),
                _ => None,
            })
            .collect();
        let near = suburb.and_then(|s| self.suburb_point(s, &named, cc));
        let hits = self.streets_by(road, number, city, &named, postcode, cc, exact, false, near);
        // Bogotá's register keys "# 86B-18" as "86B 18"
        if cc == Some("co")
            && let Some(plate) = number.and_then(co::colombian_plate)
            && !hits.iter().any(|h| {
                matches!(
                    h,
                    Hit::Street {
                        number: Some(_),
                        ..
                    }
                )
            })
        {
            let found = self.streets_in(road, Some(&plate), city, suburb, postcode, cc, exact);
            if found.iter().any(|h| {
                matches!(
                    h,
                    Hit::Street {
                        number: Some(_),
                        ..
                    }
                )
            }) {
                return found;
            }
        }
        let housed = hits.iter().any(|h| {
            matches!(
                h,
                Hit::Street {
                    number: Some(_),
                    ..
                }
            )
        });
        if !housed
            && let Some(compact) =
                number.and_then(|n| compact_house_suffix(n).or_else(|| hyphened_house_suffix(n)))
        {
            let found = self.streets_in(road, Some(&compact), city, suburb, postcode, cc, exact);
            if found.iter().any(|h| {
                matches!(
                    h,
                    Hit::Street {
                        number: Some(_),
                        ..
                    }
                )
            }) {
                return found;
            }
        }
        // Another spelling of the road can hold the house even when the stated one names a
        // street without it: Portugal's register writes "R FARIAS" beside the map's "Rua dos
        // Farias".
        if !housed && number.is_some_and(|n| !n.trim().is_empty()) {
            let mut alternatives = cc
                .or(postcode
                    .filter(|p| pt::portuguese_postcode(p))
                    .map(|_| "pt"))
                .map(|cc| crate::matching::road_expansions(road, cc))
                .unwrap_or_default();
            alternatives.extend(pl::bare_road(road, cc));
            if !alternatives.is_empty()
                && (!named.is_empty() || postcode.is_some_and(|p| self.postcode_exists(p, cc)))
            {
                let mut found = vec![];
                let mut ids = std::collections::BTreeSet::new();
                for name in alternatives {
                    let next: Vec<_> = self
                        .streets_by(&name, number, city, &named, postcode, cc, false, true, near)
                        .into_iter()
                        .filter(|h| {
                            matches!(h, Hit::Street { number: Some(_), postcode: own, name: found, .. }
                            if postcode_not_contradicted(own.as_deref(), postcode)
                                // "tabasco" is not "Privada Tabasco"
                                && (!cc.is_some_and(|c| c.eq_ignore_ascii_case("mx"))
                                    || mx::same_street(road, found)))
                        })
                        .collect();
                    if next.is_empty() {
                        continue;
                    }
                    let next_ids = next
                        .iter()
                        .filter_map(|h| match h {
                            Hit::Street { sid, .. } => Some(*sid),
                            _ => None,
                        })
                        .collect();
                    // "vn." can name two distinct roads, not just two spellings of one.
                    if !found.is_empty() && ids != next_ids {
                        return hits;
                    }
                    ids = next_ids;
                    found = next;
                }
                if !found.is_empty() {
                    return found;
                }
            }
        }
        // A misspelt street ("Tarilwood Dr, Clinton"): only inside a stated town or postcode, and
        // only a house that exists there answers.
        if hits.is_empty()
            && number.is_some_and(|n| !n.trim().is_empty())
            && let Some(name) = self.respelt(road, city, &named, postcode, cc)
        {
            return self
                .streets_by(
                    &name, number, city, &named, postcode, cc, false, false, near,
                )
                .into_iter()
                .filter(|h| match h {
                    Hit::Street {
                        number: Some(_),
                        postcode: own,
                        ..
                    } => postcode_not_contradicted(own.as_deref(), postcode),
                    _ => false,
                })
                .collect();
        }
        hits
    }

    /// The point of a settlement or neighbourhood named `suburb` within 15 km of a town in
    /// `towns`.
    fn suburb_point(&self, suburb: &str, towns: &[u32], cc: Option<&str>) -> Option<(f64, f64)> {
        let local = |p: &Place| {
            matches!(
                &*p.subtype,
                "locality" | "localadmin" | "macrohood" | "neighborhood" | "microhood"
            )
        };
        self.places(suburb, cc, None, &local)
            .into_iter()
            .filter_map(|h| match h {
                Hit::Place(id) if !towns.contains(&id) => Some(&self.places[id as usize]),
                _ => None,
            })
            .find(|p| {
                towns.iter().any(|&t| {
                    let t = &self.places[t as usize];
                    let km = ((p.x - t.x) * t.y.to_radians().cos()).hypot(p.y - t.y) * 111.0;
                    km < 15.0
                })
            })
            .map(|p| (p.x as f64, p.y as f64))
    }

    /// The name of a street in the stated town or postcode that has every word of `road` but
    /// one, and that one within an edit of the word there, or the misspelt street type
    /// ("srteet"). Words missing from the scope are counted first, so a road the index lacks
    /// costs a few lookups rather than an expansion.
    pub(super) fn respelt(
        &self,
        road: &str,
        city: Option<&str>,
        named: &[u32],
        postcode: Option<&str>,
        cc: Option<&str>,
    ) -> Option<String> {
        // Most misses are streets the index lacks; without a real town or postcode to search
        // in, there is nothing to correct against.
        if named.is_empty() && !postcode.is_some_and(|p| self.postcode_exists(p, cc)) {
            return None;
        }
        let mut scope: Vec<(Occur, Box<dyn TQuery>)> = named
            .iter()
            .map(|id| (Occur::Should, term(self.f.ids, &id.to_string())))
            .collect();
        if let Some(city) = city {
            let postal = self.musts(self.f.city, city);
            if !postal.is_empty() {
                scope.push((Occur::Should, Box::new(BooleanQuery::new(postal))));
            }
        }
        if let Some(pc) = postcode.map(pc_norm).filter(|p| !p.is_empty()) {
            scope.push((Occur::Should, term(self.f.pcode, &pc)));
        }
        if scope.is_empty() {
            return None;
        }
        let scoped = |mut cl: Vec<(Occur, Box<dyn TQuery>)>| {
            cl.push(must(Box::new(BooleanQuery::new(
                scope.iter().map(|(o, q)| (*o, q.box_clone())).collect(),
            ))));
            cl
        };
        let words: Vec<String> = fold(road)
            .split(' ')
            .filter(|t| !t.is_empty())
            .map(String::from)
            .collect();
        let mut missing = words.iter().enumerate().filter(|(_, w)| {
            !self.any_of(Kind::Street, cc, scoped(vec![must(term(self.f.names, w))]))
        });
        let (wrong, word) = missing.next()?;
        if missing.next().is_some() || !fuzzable(word) {
            return None;
        }
        // a street type or direction, or a name ("adlee" is near "allee" and "adele"); the
        // words themselves, not tantivy's FuzzyTermQuery: that marks every document holding any
        // of them in a bitset per segment before the scope narrows anything
        let mut alternatives = misspelt_kind(word);
        alternatives.extend(self.terms_near(word).iter().cloned());
        if alternatives.is_empty() {
            return None;
        }
        let mut cl: Vec<(Occur, Box<dyn TQuery>)> = words
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != wrong)
            .map(|(_, w)| must(term(self.f.names, w)))
            .collect();
        cl.push(must(Box::new(BooleanQuery::new(
            alternatives
                .iter()
                .map(|w| (Occur::Should, term(self.f.names, w)))
                .collect(),
        ))));
        let best = self
            .top_of(Kind::Street, cc, scoped(cl), 1)
            .into_iter()
            .next()?;
        let doc: tantivy::TantivyDocument = self.searcher.doc(best).ok()?;
        let mut respelt = words.clone();
        let name = best_name(&doc, self.f.name, road);
        // the street's own spelling of the word, so the exact search finds it
        respelt[wrong] = fold(&name)
            .split(' ')
            .find(|w| alternatives.iter().any(|a| a == w))?
            .to_string();
        Some(respelt.join(" "))
    }

    /// Street-name words within one edit of `word`, with the same first letter.
    pub(super) fn terms_near(&self, word: &str) -> std::sync::Arc<[String]> {
        if word.len() > MAX_WORD {
            return std::sync::Arc::new([]);
        }
        if let Some(hit) = self.near.lock().unwrap().get(word) {
            return hit.clone();
        }
        let mut out = std::collections::BTreeSet::new();
        for segment in self.searcher.segment_readers() {
            let Ok(index) = segment.inverted_index(self.f.names) else {
                continue;
            };
            let Ok(mut terms) = index.terms().search(OneEdit(word.as_bytes())).into_stream() else {
                continue;
            };
            while terms.advance() {
                out.extend(std::str::from_utf8(terms.key()).ok().map(String::from));
            }
        }
        let out: std::sync::Arc<[String]> = out.into_iter().collect();
        let mut cache = self.near.lock().unwrap();
        // ponytail: dropped whole when full, not LRU; ~100k words is a few MB
        if cache.len() >= 100_000 {
            cache.clear();
        }
        cache.insert(word.to_string(), out.clone());
        out
    }

    #[expect(clippy::too_many_arguments)]
    pub(super) fn streets_by(
        &self,
        road: &str,
        number: Option<&str>,
        city: Option<&str>,
        named: &[u32],
        postcode: Option<&str>,
        cc: Option<&str>,
        prefer_exact: bool,
        strict: bool,
        near_suburb: Option<(f64, f64)>,
    ) -> Vec<Hit> {
        let base = || self.musts(self.f.names, road);
        let long = long_words(road);
        let exact = (!road.is_ascii() && road.contains(['路', '街', '道', '段', '巷', '弄']))
            .then(|| fold(road));
        if self.musts(self.f.names, road).is_empty() {
            return vec![];
        }
        let mut scopes: Vec<Option<Box<dyn TQuery>>> = vec![];
        let mut near = None;
        let mut county_scope = None;
        if let Some(city) = city {
            let ids: Vec<u32> = named
                .iter()
                .copied()
                // A neighbourhood's streets belong to the town around it (Seynod is now part of
                // Annecy): scope to that town too.
                .flat_map(|id| {
                    let town = matches!(
                        &*self.places[id as usize].subtype,
                        "macrohood" | "neighborhood" | "microhood"
                    )
                    .then(|| {
                        self.chain(id).into_iter().skip(1).find(|&p| {
                            matches!(&*self.places[p as usize].subtype, "locality" | "localadmin")
                        })
                    })
                    .flatten();
                    std::iter::once(id).chain(town)
                })
                .collect();
            // Towns before same-named counties: Milan's Via Brera, not one elsewhere in the
            // province of Milan that has the number.
            let county = |id: &&u32| {
                matches!(
                    &*self.places[**id as usize].subtype,
                    "county" | "macrocounty"
                )
            };
            // City-provinces (Istanbul, Bangkok, Moscow) hold streets outside the town proper,
            // but only a region named exactly like the city stands for it.
            let want = name_key(city);
            let region = |id: &&u32| {
                matches!(
                    &*self.places[**id as usize].subtype,
                    "region" | "macroregion"
                )
            };
            let named_region = |id: &&u32| region(id) && self.places[**id as usize].named(want);
            let scope = |ids: Vec<&u32>, postal: bool| {
                let mut any: Vec<(Occur, Box<dyn TQuery>)> = ids
                    .into_iter()
                    .map(|id| (Occur::Should, term(self.f.ids, &id.to_string())))
                    .collect();
                let city_terms = self.musts(self.f.city, city);
                if postal && !city_terms.is_empty() {
                    any.push((Occur::Should, Box::new(BooleanQuery::new(city_terms))));
                }
                (!any.is_empty()).then(|| Box::new(BooleanQuery::new(any)) as Box<dyn TQuery>)
            };
            if let Some(s) = scope(
                ids.iter().filter(|id| !county(id) && !region(id)).collect(),
                true,
            ) {
                scopes.push(Some(s));
            }
            if let Some(s) = scope(ids.iter().filter(county).collect(), false) {
                county_scope = Some(scopes.len());
                scopes.push(Some(s));
            }
            if let Some(s) = scope(ids.iter().filter(named_region).collect(), false) {
                scopes.push(Some(s));
            }
        }
        let town_scopes = scopes.len();
        if let Some(pc) = postcode.map(pc_norm).filter(|p| !p.is_empty()) {
            scopes.push(Some(term(self.f.pcode, &pc)));
            // Streets near the postcode, when neither the town nor the street's own postcodes
            // place it ("Köpmangatan 26, 895 30 Anundsjö": Anundsjö is a parish).
            if let Some(Hit::Postcode { x, y, .. }) = self.postcodes(&pc, cc).first() {
                near = Some((*x, *y));
            }
        }
        if city.is_none() && postcode.is_none() {
            scopes.push(None);
        }
        // last: same-named streets within about 5 km of the postcode, filtered by distance
        // rather than a range query, which would scan every document's coordinates.
        // ponytail: only the 1000 most important same-named streets are checked, so a name
        // shared by more ("Main Street") can miss; a geo-cell term field would lift that.
        let near_scope = near.is_some();
        if near_scope {
            scopes.push(None);
        }
        let scope_count = scopes.len();
        // Streets whose addresses name the requested city as their postal city.
        let postal_city: std::collections::HashSet<DocAddress> = match city {
            Some(c) if !self.musts(self.f.city, c).is_empty() => {
                let mut cl = base();
                cl.push(must(Box::new(BooleanQuery::new(
                    self.musts(self.f.city, c),
                ))));
                self.top_of(Kind::Street, cc, cl, 1000)
                    .into_iter()
                    .collect()
            }
            _ => Default::default(),
        };
        // `streets` looks a Colombian plate up again in the register's form, county included
        let county_house = cc == Some("co")
            && number.is_some_and(|n| !n.trim().is_empty() && co::colombian_plate(n).is_none());
        let number = number.map(schema::number_key);
        // Streets that hold the requested postcode report it, as Nominatim reports an address's.
        let with_postcode: std::collections::HashSet<DocAddress> =
            match postcode.map(pc_norm).filter(|p| !p.is_empty()) {
                Some(pc) => {
                    let mut cl = base();
                    cl.push(must(term(self.f.pcode, &pc)));
                    self.top_of(Kind::Street, cc, cl, 1000)
                        .into_iter()
                        .collect()
                }
                None => Default::default(),
            };
        let postcode_places = postcode
            .map(|pc| self.postcodes(pc, cc))
            .unwrap_or_default();
        let near_postcode = match postcode_places.first() {
            Some(Hit::Postcode { x, y, .. }) => Some((*x, *y)),
            _ => None,
        };
        let exact_road = fold(road);
        let lookup = |a: DocAddress, n: &str| {
            let sid = self.fast(a, "id") as u32;
            self.house_record(sid, n).map(|(_, v)| v)
        };
        let mut without_house = None;
        for (i, scope) in scopes.into_iter().enumerate() {
            let mut cl = base();
            if let Some(s) = scope {
                cl.push(must(s));
            }
            let mut top = match near {
                Some((px, py)) if near_scope && i + 1 == scope_count => self
                    .top_of(Kind::Street, cc, cl, 1000)
                    .into_iter()
                    .filter(|&a| {
                        let (x, y) = (self.fast_f64(a, "x"), self.fast_f64(a, "y"));
                        ((x - px) * py.to_radians().cos()).hypot(y - py) < 0.05
                    })
                    .take(STREET_CANDIDATES)
                    .collect(),
                // ponytail: exact native names are filtered below; 1,000 segments cap the scan
                // until an indexed exact-name field can replace it.
                _ => self.top_of(
                    Kind::Street,
                    cc,
                    cl,
                    if exact.is_some() {
                        1000
                    } else {
                        STREET_CANDIDATES
                    },
                ),
            };
            // Only a house answers from the county; most of its streets have none.
            if without_house.is_some()
                && !top
                    .iter()
                    .any(|&a| number.as_deref().is_some_and(|n| lookup(a, n).is_some()))
            {
                return without_house.unwrap_or_default();
            }
            if !long.is_empty() || exact.is_some() {
                top.retain(|&a| {
                    use tantivy::schema::Value as _;
                    let doc: tantivy::TantivyDocument = self.searcher.doc(a).unwrap();
                    names_hold(doc.get_all(self.f.name).filter_map(|v| v.as_str()), &long)
                        && exact.as_ref().is_none_or(|road| {
                            doc.get_all(self.f.name)
                                .filter_map(|v| v.as_str())
                                .any(|name| fold(name) == *road)
                        })
                });
            }
            if prefer_exact {
                let named: Vec<_> = top
                    .iter()
                    .copied()
                    .filter(|&a| {
                        use tantivy::schema::Value as _;
                        let doc: tantivy::TantivyDocument = self.searcher.doc(a).unwrap();
                        doc.get_all(self.f.name)
                            .filter_map(|v| v.as_str())
                            .any(|name| fold(name) == exact_road)
                    })
                    .collect();
                // An exact road without the number must not borrow a house from a longer name.
                if !named.is_empty() {
                    top = named;
                }
            }
            if top.is_empty() {
                continue;
            }
            // Streets holding the postcode first, then those nearest its centre: a municipality
            // can have two Fjällvägen, and the postcode tells them apart.
            let dist = |a: &DocAddress| {
                near_postcode.map_or(0, |(px, py)| {
                    let (x, y) = (self.fast_f64(*a, "x"), self.fast_f64(*a, "y"));
                    (((x - px) * py.to_radians().cos()).hypot(y - py) * 1e4) as u64
                })
            };
            // and those in the place named rather than the town around it (Port Chalmers' Beach
            // Street, not central Dunedin's)
            let in_named = |a: &DocAddress| {
                let place = self.fast(*a, "place") as u32;
                place != NONE && self.chain(place).iter().any(|p| named.contains(p))
            };
            top.sort_by_cached_key(|a| (!with_postcode.contains(a), !in_named(a), dist(a)));
            // `own`: the house's postcode slot (1-based, into the street's stored list)
            let street = |a: DocAddress, number: Option<String>, (x, y): (f64, f64), own: u8| {
                let doc: tantivy::TantivyDocument = self.searcher.doc(a).unwrap();
                let name = best_name(&doc, self.f.name, road);
                let km =
                    |(px, py): (f64, f64)| ((px - x) * py.to_radians().cos()).hypot(py - y) * 111.0;
                let own = (own > 0)
                    .then(|| {
                        use tantivy::schema::Value as _;
                        doc.get_all(self.f.pcode)
                            .nth(own as usize - 1)
                            .and_then(|v| v.as_str())
                            .map(|pc| self.postcode_display(pc, cc))
                    })
                    .flatten();
                let postal_town = number
                    .as_deref()
                    .and_then(|n| postal_town(&doc, self.f.postal_town, n, own.as_deref()));
                // Sources disagree on some houses' postcodes (one OSM object says 112 21, another
                // 111 52); the requested one stands when the street holds it and it is centred
                // within 1 km of the house.
                let own = own.filter(|o| {
                    postcode.is_none_or(|p| {
                        pc_norm(o) == pc_norm(p)
                            || !(with_postcode.contains(&a)
                                && near_postcode.is_some_and(|c| km(c) < 1.0))
                    })
                });
                // Nominatim gives an address without a postcode its nearest postcode's; here, the
                // requested one when the street holds it or its centre is within 3 km.
                let postcode = own.or_else(|| {
                    postcode
                        .filter(|_| {
                            with_postcode.contains(&a) || near_postcode.is_some_and(|p| km(p) < 3.0)
                        })
                        .map(|p| p.trim().to_string())
                });
                let place = self.fast(a, "place") as u32;
                let named = |c: &str| {
                    let want = name_key(c);
                    place != NONE
                        && self
                            .chain(place)
                            .iter()
                            .any(|&p| self.places[p as usize].named(want))
                };
                let city = city
                    .filter(|c| {
                        use tantivy::schema::Value as _;
                        postal_city.contains(&a)
                            && !named(c)
                            && (doc.get_first(self.f.postal_town).is_none()
                                || doc.get_all(self.f.postal_town).any(|value| {
                                    value
                                        .as_str()
                                        .and_then(|v| {
                                            serde_json::from_str::<(&str, &str, &str, &str)>(v).ok()
                                        })
                                        .is_some_and(|(_, _, name, _)| {
                                            let name = fold(name);
                                            fold(c)
                                                .split(' ')
                                                .all(|word| name.split(' ').any(|w| w == word))
                                        })
                                }))
                    })
                    .map(|c| match cc {
                        Some("lt") => crate::index::country::lt::settlement_title(c),
                        _ => title(c),
                    });
                Hit::Street {
                    doc: a,
                    sid: self.fast(a, "id") as u32,
                    place,
                    name,
                    x,
                    y,
                    number,
                    postcode,
                    city,
                    postal_town,
                    cc: cc.unwrap_or_default().to_string(),
                }
            };
            let own_postcode = |a: DocAddress, own: u8| {
                use tantivy::schema::Value as _;
                let doc: tantivy::TantivyDocument = self.searcher.doc(a).ok()?;
                (own > 0)
                    .then(|| {
                        doc.get_all(self.f.pcode)
                            .nth(own as usize - 1)?
                            .as_str()
                            .map(pc_norm)
                    })
                    .flatten()
            };
            if city.is_none() && postcode.is_none() && cc.is_none() {
                // Nothing places the street: answer only when few same-named streets (with the
                // number) exist, all in one country ("oosterschelde 3", not "66 parks").
                let n = number.as_deref().filter(|n| !n.is_empty());
                let fits: Vec<DocAddress> = top
                    .iter()
                    .copied()
                    .filter(|&a| n.is_none_or(|n| lookup(a, n).is_some()))
                    .collect();
                let countries: std::collections::HashSet<&str> = fits
                    .iter()
                    .map(|&a| self.fast(a, "place") as u32)
                    .map(|p| {
                        if p == NONE {
                            ""
                        } else {
                            &*self.places[p as usize].cc
                        }
                    })
                    .collect();
                if fits.is_empty()
                    || fits.len() > 3
                    || countries.len() != 1
                    || countries.contains("")
                {
                    return vec![];
                }
            }
            if let Some(n) = number.as_deref().filter(|n| !n.is_empty()) {
                let combined = (!top.iter().any(|&a| lookup(a, n).is_some()))
                    .then(|| self.combined_number(&top, n))
                    .flatten();
                let n = combined.as_deref().unwrap_or(n);
                let mut key = vec![];
                let mut conflicting = false;
                // Of the streets holding the house, those named exactly as written first (Jaszcz,
                // not Wybudowanie pod Jaszcz nearer the postcode's centre).
                let folded = fold(road);
                let as_written = |a: &DocAddress| {
                    use tantivy::schema::Value as _;
                    self.searcher
                        .doc::<tantivy::TantivyDocument>(*a)
                        .is_ok_and(|doc| {
                            doc.get_all(self.f.name)
                                .filter_map(|v| v.as_str())
                                .any(|name| fold(name) == folded)
                        })
                };
                let mut by_name: Vec<DocAddress> = top
                    .iter()
                    .copied()
                    .filter(|&a| lookup(a, n).is_some())
                    .collect();
                // A numbered branch ("Иван Хаџиниколов 2", a lane off Иван Хаџиниколов) is
                // another street: when the street as written holds the house, the branch's
                // doesn't answer for it.
                let branch = |a: &DocAddress| {
                    use tantivy::schema::Value as _;
                    self.searcher
                        .doc::<tantivy::TantivyDocument>(*a)
                        .is_ok_and(|doc| {
                            doc.get_all(self.f.name)
                                .filter_map(|v| v.as_str())
                                .any(|name| {
                                    fold(name)
                                        .strip_prefix(&folded)
                                        .and_then(|rest| rest.strip_prefix(' '))
                                        .is_some_and(|rest| {
                                            !rest.is_empty()
                                                && rest.bytes().all(|b| b.is_ascii_digit())
                                        })
                                })
                        })
                };
                if by_name.len() > 1 && by_name.iter().any(branch) && by_name.iter().any(as_written)
                {
                    by_name.retain(|a| !branch(a));
                }
                let from_suburb = |v: u64| {
                    near_suburb.map_or(0, |(px, py)| {
                        let (x, y, _) = schema::unpack(v);
                        (((x - px) * py.to_radians().cos()).hypot(y - py) * 1e4) as u64
                    })
                };
                // Each copy of the number: the stored house and a second one far along the
                // same street.
                let copies = |a: DocAddress| -> Vec<u64> {
                    let sid = self.fast(a, "id") as u32;
                    let Some((form, v)) = self.house_record(sid, n) else {
                        return vec![];
                    };
                    let mut key = sid.to_be_bytes().to_vec();
                    key.extend_from_slice(form.as_bytes());
                    let far = self.far[sid as usize % self.numbers.len()]
                        .as_ref()
                        .and_then(|m| m.get(&key));
                    std::iter::once(v).chain(far).collect()
                };
                // Several villages' house of that number: one far from the suburb stated is
                // another village's.
                if near_suburb.is_some()
                    && by_name.iter().map(|&a| copies(a).len()).sum::<usize>() > 1
                {
                    by_name.retain(|&a| copies(a).into_iter().any(|h| from_suburb(h) < 150));
                }
                if by_name.len() > 1 {
                    let wanted = postcode.map(pc_norm).filter(|p| !p.is_empty());
                    by_name.sort_by_cached_key(|&a| {
                        (
                            !with_postcode.contains(&a),
                            !in_named(&a),
                            !as_written(&a),
                            near_suburb.and(lookup(a, n)).map_or(0, from_suburb),
                            wanted.is_none()
                                || lookup(a, n)
                                    .is_none_or(|v| own_postcode(a, schema::unpack(v).2) != wanted),
                        )
                    });
                }
                // A second house of the number far along the street (two villages' "Calle Mayor
                // 3" in one municipality) is the one only it has the requested postcode for.
                // Otherwise the first stands, unless `strict` refuses the guess.
                let house = |a: DocAddress, sid: u32, key: &[u8]| -> Option<u64> {
                    let shard = sid as usize % self.numbers.len();
                    let v = self.numbers[shard].get(key)?;
                    let Some(f) = self.far[shard].as_ref().and_then(|m| m.get(key)) else {
                        return Some(v);
                    };
                    let wanted = postcode.map(pc_norm).filter(|p| !p.is_empty());
                    let holds =
                        |v: u64| wanted.is_some() && own_postcode(a, schema::unpack(v).2) == wanted;
                    match (holds(v), holds(f)) {
                        (true, false) => Some(v),
                        (false, true) => Some(f),
                        _ if near_suburb.is_some() => {
                            Some(std::cmp::min_by_key(v, f, |&h| from_suburb(h)))
                        }
                        _ => (!strict).then_some(v),
                    }
                };
                for &a in &by_name {
                    let sid = self.fast(a, "id") as u32;
                    let Some((form, _)) = self.house_record(sid, n) else {
                        return without_house.unwrap_or_default();
                    };
                    key.clear();
                    key.extend_from_slice(&sid.to_be_bytes());
                    key.extend_from_slice(form.as_bytes());
                    if let Some(v) = house(a, sid, &key) {
                        let (x, y, own) = schema::unpack(v);
                        // The town and the postcode disagree: when the house in the town has
                        // another postcode and the requested postcode holds the same street and
                        // number, the postcode is taken to be right and the town wrong.
                        let wanted = postcode.map(pc_norm).filter(|p| !p.is_empty());
                        if i < town_scopes
                            && let Some(wanted) = &wanted
                            && own_postcode(a, own).is_some_and(|o| o != *wanted)
                            && let Some((b, form, (x, y, own))) = with_postcode
                                .iter()
                                .filter_map(|&b| {
                                    let sid = self.fast(b, "id") as u32;
                                    let (form, v) = self.house_record(sid, n)?;
                                    Some((b, form, schema::unpack(v)))
                                })
                                .filter(|&(b, _, (_, _, own))| {
                                    own_postcode(b, own).as_ref() == Some(wanted)
                                })
                                .min_by_key(|(b, _, _)| (dist(b), self.fast(*b, "id")))
                        {
                            return vec![street(b, Some(form), (x, y), own)];
                        }
                        let found = street(a, Some(form), (x, y), own);
                        // Raw house postcodes can disagree with the nearby-source corroboration above.
                        // A requested postcode centred far away covers too much to contradict the
                        // house's own (Brazil's town-wide "47820-000" for a house in 47821-030).
                        if with_postcode.contains(&a)
                            && near_postcode.is_some_and(|(px, py)| {
                                ((px - x) * py.to_radians().cos()).hypot(py - y) * 111.0 < 3.0
                            })
                            && wanted.as_ref().is_some_and(|wanted| match &found {
                                Hit::Street {
                                    postcode: Some(actual),
                                    ..
                                } => !crate::matching::postcodes_compatible(wanted, actual),
                                _ => false,
                            })
                            && postcode_places.iter().any(|hit| match hit {
                                Hit::Postcode { place, .. } if *place != NONE => {
                                    let street_place = self.fast(a, "place") as u32;
                                    street_place != NONE
                                        && matches!(
                                            self.places[*place as usize].subtype.as_ref(),
                                            "locality" | "localadmin"
                                        )
                                        && (self.chain(street_place).contains(place)
                                            || (matches!(
                                                self.places[street_place as usize].subtype.as_ref(),
                                                "locality" | "localadmin"
                                            ) && self.chain(*place).contains(&street_place)))
                                }
                                _ => false,
                            })
                        {
                            conflicting = true;
                            continue;
                        }
                        // like Nominatim, a found house is the only answer
                        return vec![found];
                    } else {
                        // only `strict` leaves a held number without a house
                        return without_house.unwrap_or_default();
                    }
                }
                if conflicting {
                    return without_house.unwrap_or_default();
                }
            }
            // A street crossing a cell edge is indexed in parts: answer the middle of the parts
            // near the first, not one part's.
            let a = top[0];
            let xy = |b: DocAddress| (self.fast_f64(b, "x"), self.fast_f64(b, "y"));
            let name = |b: DocAddress| {
                let doc: tantivy::TantivyDocument = self.searcher.doc(b).unwrap();
                stored(&doc, self.f.name)
            };
            let ((ax, ay), place, first) = (xy(a), self.fast(a, "place"), name(a));
            let parts: Vec<(f64, f64)> = top
                .iter()
                .copied()
                .filter(|&b| {
                    let (bx, by) = xy(b);
                    let km = ((bx - ax) * ay.to_radians().cos()).hypot(by - ay) * 111.0;
                    km < 3.0 && self.fast(b, "place") == place && name(b) == first
                })
                .map(xy)
                .collect();
            let n = parts.len() as f64;
            let mid = parts
                .iter()
                .fold((0.0, 0.0), |(x, y), p| (x + p.0 / n, y + p.1 / n));
            let found = vec![street(a, None, mid, 0)];
            // Colombia's county is the municipality, not a province of other towns: Bogotá's
            // localities sit partly under it and partly under "Bogotá ciudad", so a same-named
            // street there can hold the house the town's lacks (CL 22BIS S in San Cristóbal and
            // in Kennedy).
            if county_house && county_scope == Some(i + 1) {
                without_house.get_or_insert(found);
                continue;
            }
            return without_house.unwrap_or(found);
        }
        without_house.unwrap_or_default()
    }
}
