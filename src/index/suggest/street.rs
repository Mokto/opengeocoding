use super::*;
use crate::index::country::{co, us};

/// A house number written as a part of its own, "12", "12A" or "nº 12", as the number:
/// short enough not to be a postcode.
pub(super) fn comma_number(part: &str) -> Option<&str> {
    let number = super::house_number(part);
    let digits = number.bytes().take_while(u8::is_ascii_digit).count();
    (matches!(digits, 1..=4)
        && number.len() <= digits + 1
        && number.bytes().skip(digits).all(|b| b.is_ascii_alphabetic()))
    .then_some(number)
}

impl Index {
    pub(super) fn suggest_number_comma(
        &self,
        typed: &Typed,
        cc: Option<&str>,
        focus: Option<&Focus>,
        want: usize,
    ) -> Vec<Suggestion> {
        let [number, road, rest @ ..] = typed.parts.as_slice() else {
            return vec![];
        };
        if number.len() > MAX_NUMBER
            || !number.starts_with(|c: char| c.is_ascii_digit())
            || !number
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'/' || c == b'-')
        {
            return vec![];
        }
        let mut parts = vec![format!("{number} {road}")];
        parts.extend_from_slice(rest);
        self.suggest_streets(
            &Typed {
                parts,
                open: typed.open,
            },
            cc,
            focus,
            want,
        )
        .into_iter()
        .filter(|s| s.kind == "address")
        .collect()
    }

    /// Streets, and houses when a number is typed, with the road in the first part and the
    /// town after a comma ("12 main st, spring"). With no comma the town may still end the text.
    /// The houses `suggest` would list for a whole address, without its places, landmarks
    /// and fallbacks: the search's lookup for a house its parse missed.
    pub fn suggest_houses(&self, text: &str, cc: Option<&str>, limit: usize) -> Vec<Suggestion> {
        let text = text.trim_start();
        if text.chars().count() > MAX_TEXT || text.trim().is_empty() || limit == 0 {
            return vec![];
        }
        let mut out = match self.chinese_suggest(text, cc) {
            Some(found) => found,
            None => {
                // the whole address is written: its last word is finished
                let typed = Typed {
                    open: false,
                    ..Typed::new(text)
                };
                if typed.parts.is_empty() {
                    return vec![];
                }
                let found = self.suggest_streets(&typed, cc, None, limit);
                if found.iter().any(|s| s.kind == "address") {
                    found
                } else {
                    self.suggest_number_comma(&typed, cc, None, limit)
                }
            }
        };
        out.retain(|s| {
            s.kind == "address"
                && cc.is_none_or(|c| {
                    s.country_code.is_empty() || s.country_code.eq_ignore_ascii_case(c)
                })
        });
        out.truncate(limit);
        out
    }

    pub(in crate::index) fn suggest_streets(
        &self,
        typed: &Typed,
        cc: Option<&str>,
        focus: Option<&Focus>,
        want: usize,
    ) -> Vec<Suggestion> {
        let one_part = typed.parts.len() == 1;
        let country = (!one_part)
            .then(|| {
                typed
                    .parts
                    .last()
                    .filter(|p| p.chars().count() > 3)
                    .and_then(|p| {
                        let cc = self.country(p)?;
                        // a region named so ("Georgia"), not one with the country's name as
                        // a later word ("South Australia", "Capital Region of Denmark")
                        (!self.name_candidates(&fold(p), false, false).iter().any(
                            |&(id, fit, _)| {
                                fit != INSIDE
                                    && (self.places[id as usize].subtype.as_ref() == "region"
                                        || self.own_codes.contains_key(&id)
                                        || self.subdivision_codes.contains_key(&id))
                            },
                        ))
                        .then_some(cc)
                    })
            })
            .flatten();
        if cc
            .zip(country.as_deref())
            .is_some_and(|(a, b)| !a.eq_ignore_ascii_case(b))
        {
            return vec![];
        }
        let stated = cc.is_some();
        let cc = cc.or(country.as_deref());
        let qualifiers = typed.parts[1..].join(", ");
        // A 4-digit code is read only with a country ("9169" is also a house number). When a
        // region shares the country's name ("Luxembourg" is a Belgian province, as "Georgia" is
        // a state), the country is not set above, so try the name once more for the postcode.
        let extracted = qualifiers
            .chars()
            .any(|c| c.is_ascii_digit())
            .then(|| {
                self.find_postcode(&qualifiers, cc)
                    .or_else(|| self.find_postcode(&us::zip5(&qualifiers), cc))
                    .or_else(|| {
                        if country.is_some() {
                            return None;
                        }
                        let named = self.country(typed.parts.last()?)?;
                        self.find_postcode(&qualifiers, Some(&named))
                    })
            })
            .flatten();
        // a stated France also means the places of its overseas departments, which have codes of
        // their own (their streets are France's)
        static SOVEREIGNS: LazyLock<std::collections::HashSet<&str>> = LazyLock::new(|| {
            let letters = || b'a'..=b'z';
            letters()
                .flat_map(|a| letters().map(move |b| String::from_utf8(vec![a, b]).unwrap()))
                .filter_map(|t| super::sovereign(&t))
                .collect()
        });
        let place_cc = cc.filter(|c| stated || !SOVEREIGNS.contains(c.to_lowercase().as_str()));
        let parts = extracted
            .as_ref()
            .map(|(_, rest)| Typed::new(rest).parts)
            .unwrap_or_else(|| typed.parts[1..].to_vec());
        let postcode = extracted.as_ref().map(|(pc, _)| pc.as_str());
        // "Rua José Relvas nº 13": the mark says only that a number follows, as "č.p." does for a
        // Czech descriptive number; an evidential one is keyed with its mark ("č.ev. 169")
        let mut marked: Vec<String> = vec![];
        for w in typed.parts[0].split(' ') {
            match w.to_lowercase().as_str() {
                "nº" | "n.º" | "n°" | "n.°" | "č.p." | "čp." => {}
                _ if has_digit(w)
                    && marked
                        .last()
                        .is_some_and(|m| matches!(m.to_lowercase().as_str(), "č.ev." | "čev.")) =>
                {
                    *marked.last_mut().unwrap() = format!("č.ev. {w}");
                }
                _ => marked.push(w.to_string()),
            }
        }
        let raw: Vec<&str> = marked.iter().map(String::as_str).collect();
        // the number: the first or last word, when a road remains; both when both could be
        // ("11 Listopada 26" is house 26 on a road named for a date)
        let number_at = (raw.len() > 1)
            .then(|| {
                [0, raw.len() - 1]
                    .into_iter()
                    .filter(|&i| has_digit(raw[i]) && cz_sk::number_len(raw[i]) <= MAX_NUMBER)
            })
            .into_iter()
            .flatten();
        let mut tries: Vec<(Option<String>, bool, Vec<&str>)> = vec![];
        for i in number_at {
            tries.push((
                Some(raw[i].to_owned()),
                i == raw.len() - 1,
                raw.iter()
                    .enumerate()
                    .filter(|&(j, _)| j != i)
                    .map(|(_, w)| *w)
                    .collect(),
            ));
        }
        // "route 66": a digit word that is part of the road
        tries.push((None, false, raw.clone()));
        // "Muurlantie 340 A": OpenStreetMap keys some houses with the space
        if raw.len() > 2 {
            for start in [0, raw.len() - 2] {
                let number = raw[start..start + 2].join(" ");
                if number.len() <= MAX_NUMBER && compact_house_suffix(&number).is_some() {
                    let mut road = raw.clone();
                    road.drain(start..start + 2);
                    tries.push((Some(number), start > 0, road));
                }
            }
        }
        // "AC 43 S # 86B-18": Bogotá's register keys the plate "86B 18". OpenStreetMap keys many
        // as written ("11-23"), so the plate comes last, after every try failed to find a house.
        let plate = cc
            .is_some_and(|c| c.eq_ignore_ascii_case("co"))
            .then(|| co::split_plate(&typed.parts[0]))
            .flatten();
        let plated = plate.is_some();
        tries.extend(plate.map(|(road, plate)| (Some(plate), true, road)));
        let mut out = vec![];
        let mut streets = vec![];
        for (number, at_end, road) in tries {
            let number_word = number.as_deref();
            let number_open = typed.open && one_part && at_end;
            let road_open = typed.open && one_part && !number_open;
            let road_words = words_open(&road.join(" "), road_open);
            if road_words.is_empty() {
                continue;
            }
            if postcode.is_some()
                && number_word.is_some()
                && (parts.len() <= 1 || ((2..=3).contains(&parts.len()) && country.is_some()))
            {
                // "9 pittevej, aidt, 8881 thorsø, denmark": a place between the road and the
                // postcode's town is another name of the house's place, or its postal town
                let candidates: &[usize] = if parts.len() == 3 { &[1, 0] } else { &[0] };
                for &at in candidates {
                    let city = parts
                        .get(at)
                        .filter(|p| country.is_none() || self.country(p) != country)
                        .map(|p| (p.as_str(), typed.open && parts.len() == 1));
                    let found = self.streets_typed(
                        &road_words,
                        false,
                        None,
                        number_word,
                        false,
                        cc,
                        postcode,
                        city,
                        want,
                    );
                    if found.iter().any(|s| s.kind == "address") {
                        return found;
                    }
                }
            }
            let found = if one_part {
                // "main st springfield": the end of the text may name the town, and then the
                // streets there are better than any other "main st"
                let mut found = vec![];
                let mut split = road_words.len();
                while split > 1 && road_words.len() - split < 3 && found.len() < want {
                    split -= 1;
                    let town = [road_words[split..].join(" ")];
                    let ids = self.scope(&town, road_open, cc, focus, false);
                    if ids.is_empty() {
                        continue;
                    }
                    found.extend(self.streets_typed(
                        &road_words[..split],
                        false,
                        Some(&ids),
                        number_word,
                        false,
                        cc,
                        postcode,
                        None,
                        want - found.len(),
                    ));
                }
                found.extend(self.streets_typed(
                    &road_words,
                    road_open,
                    None,
                    number_word,
                    number_open,
                    cc,
                    postcode,
                    None,
                    want,
                ));
                found
            } else {
                let mut ids = self.scope(&parts, typed.open, place_cc, focus, true);
                if ids.is_empty() && country.is_some() {
                    ids = self.scope(&parts[..parts.len() - 1], false, place_cc, focus, true);
                }
                let mut found = if ids.is_empty() {
                    vec![]
                } else {
                    self.streets_typed(
                        &road_words,
                        false,
                        Some(&ids),
                        number_word,
                        false,
                        cc,
                        postcode,
                        None,
                        want,
                    )
                };
                // a suburb the houses' addresses name but their places do not ("2 Clara St,
                // Randwick" is filed under Sydney), with nothing else after it but the country,
                // or the town around it ("22 Brackenburn Drive, Ohauiti, Tauranga, New Zealand").
                // A region code written after it ("Kingsford QLD") holds the houses to that region.
                // "941 University Court, Evansville, IN 47712": the town's region by its code
                // (or "Essex, Irvington Township, NJ 07111", after its county)
                let region_after = match parts.as_slice() {
                    [_, code] | [_, _, code] if country.is_none() => self.regions_coded(code, cc),
                    _ => vec![],
                };
                let town = match parts.as_slice() {
                    [town] => Some(town.clone()),
                    [town, _] if country.is_some() || !region_after.is_empty() => {
                        Some(town.clone())
                    }
                    [_, town, _] if !region_after.is_empty() && !has_digit(town) => {
                        Some(town.clone())
                    }
                    // both named by the houses, not a floor or door ("rådhuspladsen 1, 4.,
                    // københavn, denmark")
                    [suburb, town, _]
                        if country.is_some() && ![suburb, town].iter().any(|p| has_digit(p)) =>
                    {
                        Some(format!("{suburb} {town}"))
                    }
                    _ => None,
                };
                // a street alone, when its places are not under the town ("Roseburn Street,
                // Edinburgh" is Murrayfield's, in the City of Edinburgh)
                let street_only = !raw.iter().any(|w| has_digit(w));
                if (number_word.is_some() && !found.iter().any(|s| s.kind == "address")
                    || street_only && found.is_empty())
                    && let Some(town) = town
                        .as_ref()
                        .filter(|p| country.is_none() || self.country(p) != country)
                {
                    // "Rho (MI)": a province's code holds the houses to it, as a region's does;
                    // a sigla in parentheses that is no code ("Saint-Pierre (AO)") says no more
                    // than the town
                    let bracketed = town
                        .strip_suffix(')')
                        .and_then(|t| t.rsplit_once(" ("))
                        .filter(|(_, code)| code.len() == 2);
                    let coded = if region_after.is_empty() {
                        bracketed
                            .or(town.rsplit_once(' '))
                            .and_then(|(name, code)| {
                                let code = format!("{}-{code}", cc?).to_uppercase();
                                let region = self.regions.get(&code).copied().or_else(|| {
                                    self.subdivision_codes
                                        .iter()
                                        .find(|(_, c)| ***c == *code)
                                        .map(|(&p, _)| p)
                                })?;
                                Some((name, vec![region]))
                            })
                    } else {
                        Some((town.as_str(), region_after))
                    };
                    let (town, region) = match (&coded, bracketed) {
                        (Some((name, region)), _) => (*name, Some(&region[..])),
                        (None, Some((name, _))) => (name, None),
                        (None, None) => (town.as_str(), None),
                    };
                    // with a postcode, the town alone was looked for already. A town that is a
                    // part of the settlement the houses are filed under ("Bahnstraße 20, Wagram
                    // am Wagram, 3483" is Grafenwörth's) has them in its postcode there.
                    let by_town = if postcode.is_none() || region.is_some() {
                        let in_town = |town| {
                            self.streets_typed(
                                &road_words,
                                false,
                                region,
                                number_word,
                                false,
                                cc,
                                postcode,
                                Some((town, typed.open && parts.len() == 1)),
                                want,
                            )
                        };
                        let found = in_town(town);
                        match us::municipality(town).filter(|_| region.is_some()) {
                            Some(name) if !found.iter().any(|s| s.kind == "address") => {
                                in_town(name)
                            }
                            _ => found,
                        }
                    } else {
                        let settlements: Vec<u32> = ids
                            .iter()
                            .flat_map(|&id| {
                                self.chain(id).into_iter().skip(1).take_while(|&p| {
                                    matches!(
                                        self.places[p as usize].subtype.as_ref(),
                                        "locality"
                                            | "localadmin"
                                            | "borough"
                                            | "macrohood"
                                            | "neighborhood"
                                            | "microhood"
                                    )
                                })
                            })
                            .collect();
                        if settlements.is_empty() {
                            vec![]
                        } else {
                            self.streets_typed(
                                &road_words,
                                false,
                                Some(&settlements),
                                number_word,
                                false,
                                cc,
                                postcode,
                                None,
                                want,
                            )
                        }
                    };
                    if by_town.iter().any(|s| s.kind == "address")
                        || street_only && !by_town.is_empty()
                    {
                        found = by_town;
                    }
                }
                // "Calle Real 7, Pinedo, Valdegovía, Álava, Spain": a settlement the houses'
                // places do not name, before the municipality they are filed under. The
                // municipality must be a town, not only a region, and the settlement no place or
                // one within 50 km of it, with the house within 3 km; not a Danish floor, which
                // the floor fallback keeps
                if number_word.is_some()
                    && !found.iter().any(|s| s.kind == "address")
                    && parts.len() >= 3
                    && !has_digit(&parts[0])
                    && !dk::danish_floor(&parts[0])
                {
                    let rest = &parts[1..];
                    let mut ids = self.scope(rest, typed.open, place_cc, focus, true);
                    if ids.is_empty() && country.is_some() {
                        ids = self.scope(&rest[..rest.len() - 1], false, place_cc, focus, true);
                    }
                    let towns: Vec<&Place> = ids
                        .iter()
                        .map(|&id| &self.places[id as usize])
                        .filter(|p| matches!(p.subtype.as_ref(), "locality" | "localadmin"))
                        .collect();
                    let named: Vec<&Place> = self
                        .places(&parts[0], place_cc, None, &|_| true)
                        .into_iter()
                        .filter_map(|h| match h {
                            Hit::Place(id) => Some(&self.places[id as usize]),
                            _ => None,
                        })
                        .collect();
                    let km = |p: &Place, x: f64, y: f64| {
                        ((p.x as f64 - x) * y.to_radians().cos()).hypot(p.y as f64 - y) * 111.0
                    };
                    let near = named.is_empty()
                        || named
                            .iter()
                            .any(|p| towns.iter().any(|t| km(p, t.x as f64, t.y as f64) < 50.0));
                    if !towns.is_empty() && near {
                        let mut by_municipality = self.streets_typed(
                            &road_words,
                            false,
                            Some(&ids),
                            number_word,
                            false,
                            cc,
                            postcode,
                            None,
                            want,
                        );
                        // the settlement named picks among the municipality's same-named streets
                        if !named.is_empty() {
                            by_municipality.retain(|s| {
                                s.kind != "address" || named.iter().any(|p| km(p, s.x, s.y) < 3.0)
                            });
                        }
                        if by_municipality.iter().any(|s| s.kind == "address") {
                            found = by_municipality;
                        }
                    }
                }
                found
            };
            if plated && !found.iter().any(|s| s.kind == "address") {
                if streets.is_empty() {
                    streets = found;
                }
                continue;
            }
            if !found.is_empty() {
                out = found;
                break;
            }
        }
        if out.is_empty() {
            out = streets;
        }
        // "Fulham Road, London, SW6 1HS": no house, and no street in the places named, so the
        // streets the postcode serves
        if out.is_empty() && postcode.is_some() && !raw.iter().any(|w| has_digit(w)) {
            let road = words(&raw.join(" "));
            out = self.streets_typed(&road, false, None, None, false, cc, postcode, None, want);
        }
        // "4 Church Street, Amberley, 7410, New Zealand": the town places a house the street's
        // postcode list does not name (New Zealand's register, or a Luxembourg house whose
        // neighbours on the street carry other codes). With no town, the postcode's own place.
        if !out.iter().any(|s| s.kind == "address")
            && raw.iter().any(|w| has_digit(w))
            && let Some((pc, rest)) = &extracted
        {
            let rest = Typed::new(rest).parts;
            let town = if rest
                .iter()
                .any(|p| country.is_none() || self.country(p) != country)
            {
                Some(None)
            } else {
                self.postcodes(pc, cc)
                    .into_iter()
                    .find_map(|hit| match hit {
                        Hit::Postcode { place, .. } if place != NONE => {
                            Some(Some(self.places[place as usize].primary.to_string()))
                        }
                        _ => None,
                    })
            };
            if let Some(town) = town {
                let parts = [typed.parts[0].clone()]
                    .into_iter()
                    .chain(town)
                    .chain(rest)
                    .collect();
                let found: Vec<_> = self
                    .suggest_streets(
                        &Typed {
                            parts,
                            open: typed.open,
                        },
                        cc,
                        focus,
                        want,
                    )
                    .into_iter()
                    .filter(|s| s.kind == "address" && self.house_accepts_postcode(s, pc))
                    .collect();
                if !found.is_empty() {
                    return found;
                }
            }
        }
        if !out.iter().any(|s| s.kind == "address") && raw.len() > 2 {
            for start in [0, raw.len() - 2] {
                if let Some(number) = compact_house_suffix(&raw[start..start + 2].join(" ")) {
                    let mut parts = typed.parts.clone();
                    parts[0] = raw[..start]
                        .iter()
                        .copied()
                        .chain([number.as_str()])
                        .chain(raw[start + 2..].iter().copied())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let found = self.suggest_streets(
                        &Typed {
                            parts,
                            open: typed.open,
                        },
                        cc,
                        focus,
                        want,
                    );
                    if found.iter().any(|s| s.kind == "address") {
                        return found;
                    }
                }
            }
        }
        // "ul. M. Kopernika 11": the index names it Mikołaja Kopernika, mostly without "ul."
        if !out.iter().any(|s| s.kind == "address")
            && let Some(bare) = pl::bare_road(&typed.parts[0], cc)
        {
            let mut parts = typed.parts.clone();
            parts[0] = bare;
            let found = self.suggest_streets(
                &Typed {
                    parts,
                    open: typed.open,
                },
                cc,
                focus,
                want,
            );
            if out.is_empty() || found.iter().any(|s| s.kind == "address") {
                return found;
            }
        }
        // "شارع الكنيسه 5, مركز منفلوط, اسيوط": an Egyptian address's town parts as the index
        // names them
        if !out.iter().any(|s| s.kind == "address")
            && cc.is_none_or(|c| c.eq_ignore_ascii_case("eg"))
            && let Some(towns) = self.egyptian_towns(&typed.parts[1..], cc.is_some())
        {
            let parts = typed.parts[..1].iter().cloned().chain(towns).collect();
            let found = self.suggest_streets(
                &Typed {
                    parts,
                    open: typed.open,
                },
                cc,
                focus,
                want,
            );
            if out.is_empty() || found.iter().any(|s| s.kind == "address") {
                return found;
            }
        }
        // "Rua dos Farias 12 2955-243 Pinhal Novo": a known postcode after the number starts
        // the town's part
        if !out.iter().any(|s| s.kind == "address")
            && let [whole] = typed.parts.as_slice()
        {
            let words: Vec<&str> = whole.split(' ').collect();
            let at = (2..words.len()).find(|&i| {
                has_digit(words[i - 1])
                    && has_digit(words[i])
                    && (words[i].len() >= 5 || words[i].contains(|c: char| !c.is_ascii_digit()))
                    && self.postcode_exists(words[i], cc)
            });
            if let Some(at) = at {
                let found = self.suggest_streets(
                    &Typed {
                        parts: vec![words[..at].join(" "), words[at..].join(" ")],
                        open: typed.open,
                    },
                    cc,
                    focus,
                    want,
                );
                if found.iter().any(|s| s.kind == "address") {
                    return found;
                }
            }
        }
        // "Rua Pereira Charula, 12, 5340-278 Macedo de Cavaleiros": the number as its own part
        if !out.iter().any(|s| s.kind == "address")
            && let [road, number, rest @ ..] = typed.parts.as_slice()
            && !road.split(' ').any(has_digit)
            && let Some(number) = comma_number(number)
        {
            let mut parts = vec![format!("{road} {number}")];
            parts.extend_from_slice(rest);
            let found = self.suggest_streets(
                &Typed {
                    parts,
                    open: typed.open,
                },
                cc,
                focus,
                want,
            );
            if found.iter().any(|s| s.kind == "address") {
                return found;
            }
        }
        // "Avenida Calle 43 S # 86B-18": Bogotá's register names it "AC 43 S"
        if !out.iter().any(|s| s.kind == "address")
            && cc.is_some_and(|c| c.eq_ignore_ascii_case("co"))
            && let Some(road) = co::bogota_road(&typed.parts[0])
            && road.len() < typed.parts[0].len()
            && co::split_plate(&road).is_some()
        {
            let mut parts = typed.parts.clone();
            parts[0] = road;
            let found = self.suggest_streets(
                &Typed {
                    parts,
                    open: typed.open,
                },
                cc,
                focus,
                want,
            );
            if found.iter().any(|s| s.kind == "address") {
                return found;
            }
        }
        // "CL 22BIS S # 9C-18 E, Bogotá" names no country: a plate only when Colombia has the
        // house, as "Rue Beck 2-4, 1222 Luxembourg" is a range elsewhere
        if !out.iter().any(|s| s.kind == "address")
            && cc.is_none()
            && co::split_plate(&typed.parts[0]).is_some()
        {
            let found = self.suggest_streets(typed, Some("co"), focus, want);
            if found.iter().any(|s| s.kind == "address") {
                return found;
            }
        }
        // "12 Rua dos Farias": Portugal's register names it "R FARIAS". Only Portugal's form,
        // which expands no further: other countries' turn back ("straat" <-> "street").
        if !out.iter().any(|s| s.kind == "address")
            && typed.parts[0].split(' ').any(has_digit)
            && cc.map_or_else(
                || postcode.is_some_and(pt::portuguese_postcode),
                |c| c.eq_ignore_ascii_case("pt"),
            )
        {
            for road in crate::matching::road_expansions(&raw.join(" "), "pt") {
                let mut parts = typed.parts.clone();
                parts[0] = road;
                let found = self.suggest_streets(
                    &Typed {
                        parts,
                        open: typed.open,
                    },
                    cc,
                    focus,
                    want,
                );
                if found.iter().any(|s| s.kind == "address") {
                    return found;
                }
            }
        }
        out.truncate(want);
        out
    }

    /// Ids of the places the parts name, to scope streets to: those places and all inside them.
    pub(in crate::index) fn scope(
        &self,
        parts: &[String],
        open: bool,
        cc: Option<&str>,
        focus: Option<&Focus>,
        codes: bool,
    ) -> Vec<u32> {
        // two letters of a name are too many places to scope streets to. Only after a comma is a
        // town misspelt or named by a later word ("main st, lodnon"): the end of "rue de rivoli"
        // or "united states" is not a town
        let short = open && parts.len() == 1 && parts[0].chars().count() < 3;
        let places = |names| -> Vec<u32> {
            self.suggest_places(parts, open, cc, focus, names, 8)
                .into_iter()
                .map(|(id, _)| id)
                .collect()
        };
        let mut ids = match (short, codes) {
            (true, _) => vec![],
            (false, false) => places(Names::Strict),
            (false, true) => {
                let ids = places(Names::Inside);
                if ids.is_empty() {
                    places(Names::Typo)
                } else {
                    ids
                }
            }
        };
        // an Egyptian town also means its markaz or qism counties, which hold its houses
        if cc.is_none_or(|c| c.eq_ignore_ascii_case("eg")) {
            let counties = self
                .suggest
                .egyptian_counties
                .get_or_init(|| eg::counties(&self.places));
            for at in 0..ids.len() {
                let pl = &self.places[ids[at] as usize];
                if pl.cc.eq_ignore_ascii_case("eg") && &*pl.subtype == "locality" {
                    for c in eg::town_counties(&self.places, counties, &self.chain(ids[at])) {
                        if !ids.contains(&c) {
                            ids.push(c);
                        }
                    }
                }
            }
        }
        // "main st, il" and "10 downing st, uk": a region's code, a country's name or code. Only
        // after a comma: in "main st" the "st" is a word being typed.
        if let ([one], true) = (parts, codes) {
            let q = fold(one);
            ids.extend(self.regions_coded(&q, cc));
            ids.extend(
                self.countries_named(&q, open)
                    .into_iter()
                    .filter(|c| cc.is_none_or(|cc| c.eq_ignore_ascii_case(cc)))
                    .filter_map(|c| self.country_places.get(c).copied()),
            );
        }
        ids.dedup();
        ids
    }

    /// The town parts of an Egyptian address as the index may hold them, if they differ: without
    /// the unit words when a place is named so ("مركز منفلوط" is منفلوط), once when the town
    /// repeats its governorate ("المنيا, المنيا"), and, with the country `stated`, without a
    /// suburb that names no place ("غرب النوباريه, البحيرة, 22773") while a postcode or two
    /// places remain.
    fn egyptian_towns(&self, parts: &[String], stated: bool) -> Option<Vec<String>> {
        let postcode = |p: &str| p.chars().all(|c| c.is_ascii_digit());
        let named = |p: &str| {
            let key = name_key(p);
            postcode(p)
                || self.country(p).is_some()
                || self
                    .suggest_places(&[p.to_owned()], false, Some("eg"), None, Names::Strict, 8)
                    .iter()
                    .any(|&(id, _)| self.places[id as usize].named(key))
        };
        let mut towns: Vec<(String, bool)> = parts
            .iter()
            .map(|p| match eg::bare_town(p) {
                Some(bare) if named(&bare) => (bare, true),
                _ => (p.clone(), named(p)),
            })
            .collect();
        towns.dedup_by(|b, a| fold(&a.0) == fold(&b.0));
        let places = towns
            .iter()
            .filter(|(p, known)| *known && !postcode(p) && self.country(p).is_none())
            .count();
        if stated && (places >= 2 || towns.iter().any(|(p, _)| postcode(p))) {
            towns.retain(|(_, known)| *known);
        }
        let towns: Vec<String> = towns.into_iter().map(|(p, _)| p).collect();
        (towns != parts).then_some(towns)
    }

    /// The regions whose ISO 3166-2 code ends with `code` ("IN" is US-IN), in `cc` if given.
    fn regions_coded(&self, code: &str, cc: Option<&str>) -> Vec<u32> {
        if !(2..=3).contains(&code.len()) {
            return vec![];
        }
        let suffix = format!("-{}", code.to_uppercase());
        self.regions
            .iter()
            .filter_map(|(code, &place)| {
                let pl = &self.places[place as usize];
                (code.ends_with(&suffix) && cc.is_none_or(|c| pl.cc.eq_ignore_ascii_case(c)))
                    .then_some(place)
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::index) fn streets_typed(
        &self,
        road: &[String],
        road_open: bool,
        scope: Option<&[u32]>,
        number: Option<&str>,
        number_open: bool,
        cc: Option<&str>,
        postcode: Option<&str>,
        city: Option<(&str, bool)>,
        want: usize,
    ) -> Vec<Suggestion> {
        if road_open && road.len() == 1 && road[0].chars().count() < 3 {
            return vec![];
        }
        let Some((mut base, rarest)) = self.typed_words(self.f.names, road, road_open) else {
            return vec![];
        };
        let mut in_town = None;
        if let Some((city, open)) = city {
            let Some((clauses, rarest)) =
                self.typed_words(self.f.city, &words_open(city, open), open)
            else {
                return vec![];
            };
            base.extend(clauses);
            in_town = Some(rarest);
        }
        let in_scope = scope
            .map(|ids| {
                ids.iter()
                    .map(|id| self.docs_with(self.f.ids, &id.to_string()))
                    .sum::<u64>()
            })
            .into_iter()
            .chain(postcode.map(|pc| self.docs_with(self.f.pcode, &pc_norm(pc))))
            .chain(in_town)
            .min();
        let limit = if scope.is_some() {
            MAX_WORK_IN_TOWN
        } else {
            MAX_WORK
        };
        if rarest.min(in_scope.unwrap_or(u64::MAX)) > limit {
            return vec![];
        }
        let number = number.map(schema::number_key).filter(|n| !n.is_empty());
        let road_text = road.join(" ");
        let mut streets = vec![];
        // each name's best ranked part: its houses come before those of the name's other parts
        let mut first = HashMap::new();
        let mut segments = std::collections::HashSet::new();
        // the best place of the scope alone first: "5th avenue, new york" is Manhattan's
        // before any other in the state
        let phases: Vec<Option<&[u32]>> = match scope {
            Some(ids) if ids.len() > 1 => vec![Some(&ids[..1]), Some(ids)],
            other => vec![other],
        };
        for ids in phases {
            if first.len() >= want {
                break;
            }
            let mut cl: Vec<_> = base.iter().map(|(o, q)| (*o, q.box_clone())).collect();
            if let Some(pc) = postcode {
                cl.push(must(term(self.f.pcode, &pc_norm(pc))));
            }
            if let Some(ids) = ids {
                cl.push(must(Box::new(BooleanQuery::new(
                    ids.iter()
                        .map(|id| (Occur::Should, term(self.f.ids, &id.to_string())))
                        .collect(),
                ))));
            }
            for a in self.top_of(Kind::Street, cc, cl, want.max(5) * 4) {
                let doc: tantivy::TantivyDocument = self.searcher.doc(a).unwrap();
                let name = best_name(&doc, self.f.name, &road_text);
                let (sid, place) = (self.fast(a, "id") as u32, self.fast(a, "place") as u32);
                first.entry((fold(&name), place)).or_insert(sid);
                if segments.insert(sid) {
                    streets.push((a, sid, place, name));
                }
            }
        }
        let scope_rank = |place: u32| {
            scope.and_then(|ids| {
                self.chain(place)
                    .iter()
                    .filter_map(|c| ids.iter().position(|i| i == c))
                    .min()
            })
        };
        // those named as typed before those that only contain it ("Main Street" over "High
        // Springs Main Street")
        // and the street so named before one that goes on ("Gran Vía" before "Gran Vía del Sur")
        streets.sort_by_cached_key(|s| {
            let name = fold(&s.3);
            (
                scope_rank(s.2),
                !name.starts_with(&road_text),
                name != road_text,
                first.get(&(name, s.2)) != Some(&s.1),
            )
        });
        let mut houses = vec![];
        let mut plain = vec![];
        // Each cell holds different houses; deduplicate names only after checking the number.
        let mut answered = std::collections::HashSet::new();
        for (a, sid, place, name) in streets {
            let key = (fold(&name), place);
            if answered.contains(&key) {
                continue;
            }
            let mut found = false;
            if let Some(n) = &number {
                let shard = &self.numbers[sid as usize % self.numbers.len()];
                let mut key = sid.to_be_bytes().to_vec();
                key.extend_from_slice(n.as_bytes());
                let mut numbers: Vec<(String, u64)> = shard
                    .get(&key)
                    .map(|v| (n.clone(), v))
                    .into_iter()
                    .collect();
                if numbers.is_empty()
                    && let Some((alt, v)) = self.house_record(sid, n)
                {
                    numbers.push((alt, v));
                }
                if numbers.is_empty()
                    && let Some(combined) = self.combined_number(&[a], n)
                {
                    let mut key = sid.to_be_bytes().to_vec();
                    key.extend_from_slice(combined.as_bytes());
                    numbers.extend(shard.get(&key).map(|v| (combined, v)));
                }
                if number_open {
                    let mut after = key.clone();
                    *after.last_mut().unwrap() += 1;
                    let mut stream = shard.range().ge(&key).lt(&after).into_stream();
                    while numbers.len() < 4
                        && let Some((k, v)) = stream.next()
                    {
                        if k != key.as_slice()
                            && let Ok(more) = std::str::from_utf8(&k[4..])
                        {
                            numbers.push((more.to_string(), v));
                        }
                    }
                }
                for (n, v) in numbers {
                    let (x, y, own) = schema::unpack(v);
                    if let Some(pc) = postcode.filter(|_| own > 0) {
                        use tantivy::schema::Value as _;
                        let doc: tantivy::TantivyDocument = self.searcher.doc(a).unwrap();
                        if doc
                            .get_all(self.f.pcode)
                            .nth(own as usize - 1)
                            .and_then(|v| v.as_str())
                            .map(pc_norm)
                            != Some(pc_norm(pc))
                        {
                            continue;
                        }
                    }
                    houses.push(self.street_suggestion(a, sid, place, &name, Some(&n), (x, y)));
                    found = true;
                }
            }
            if found {
                answered.insert(key);
            } else {
                plain.push((sid, place, name, a));
            }
        }
        let mut out = houses;
        for (sid, place, name, a) in plain {
            if answered.insert((fold(&name), place)) {
                let xy = (self.fast_f64(a, "x"), self.fast_f64(a, "y"));
                out.push(self.street_suggestion(a, sid, place, &name, None, xy));
            }
        }
        out.truncate(want);
        out
    }

    pub(in crate::index) fn street_suggestion(
        &self,
        a: tantivy::DocAddress,
        sid: u32,
        place: u32,
        name: &str,
        number: Option<&str>,
        (x, y): (f64, f64),
    ) -> Suggestion {
        let mut display: Vec<String> = number.map(String::from).into_iter().collect();
        display.push(name.to_string());
        let mut country_code = String::new();
        if place != NONE {
            if let Some(chain) = number.and_then(|_| self.house_chain(&self.chain(place), x, y)) {
                self.address(&chain, true, &mut Map::new(), &mut display);
            } else {
                display.push(self.label(place).to_string());
            }
            country_code = self.places[place as usize].cc.to_string();
        }
        Suggestion {
            id: match number {
                Some(n) => format!("h{sid}:{n}"),
                None => format!("s{sid}"),
            },
            retrieval_id: format!(
                "{}/{}{}:{}{}",
                self.suggestion_scope,
                if number.is_some() { "h" } else { "s" },
                a.segment_ord,
                a.doc_id,
                number
                    .map(|n| format!(":{}", schema::number_key(n)))
                    .unwrap_or_default(),
            ),
            display_name: display.join(", "),
            unverified_suffix: String::new(),
            kind: if number.is_some() {
                "address"
            } else {
                "street"
            },
            category: "",
            country_code,
            x,
            y,
            bbox: None,
        }
    }
}
