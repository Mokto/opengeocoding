use super::*;

impl Index {
    /// This house carries no postcode, or the one it carries is `pc`. Other houses on the same
    /// street may carry postcodes; those do not contradict this house ("6 Rue de Colmar-Berg,
    /// Mertzig, 9169" sits on a street that also serves other codes).
    pub(super) fn house_accepts_postcode(&self, s: &Suggestion, pc: &str) -> bool {
        use tantivy::schema::Value as _;
        let Some((addr, number)) = s.retrieval_id.split_once('/').and_then(|(_, key)| {
            let rest = key.strip_prefix('h')?;
            let mut parts = rest.split(':');
            let segment: u32 = parts.next()?.parse().ok()?;
            let doc: u32 = parts.next()?.parse().ok()?;
            let number = parts.next()?;
            (!number.is_empty()).then(|| (tantivy::DocAddress::new(segment, doc), number))
        }) else {
            return false;
        };
        let sid = self.fast(addr, "id") as u32;
        let Some(shard) = self.numbers.get(sid as usize % self.numbers.len()) else {
            return false;
        };
        let mut key = sid.to_be_bytes().to_vec();
        key.extend_from_slice(number.as_bytes());
        let Some(v) = shard.get(&key) else {
            return false;
        };
        let (_, _, own) = schema::unpack(v);
        if own == 0 {
            return true;
        }
        let Ok(doc) = self.searcher.doc::<tantivy::TantivyDocument>(addr) else {
            return false;
        };
        let stored = doc
            .get_all(self.f.pcode)
            .nth(own as usize - 1)
            .and_then(|v| v.as_str())
            .map(pc_norm)
            .filter(|v| !v.is_empty());
        let Some(stored) = stored else {
            return false;
        };
        super::super::postcode::postcode_not_contradicted(Some(&stored), Some(pc))
    }

    pub(super) fn postcode_suggestion(&self, a: tantivy::DocAddress) -> Option<Suggestion> {
        let doc: tantivy::TantivyDocument = self.searcher.doc(a).ok()?;
        let place = self.fast(a, "place") as u32;
        let mut display = vec![stored(&doc, self.f.name)];
        let mut country_code = String::new();
        if place != NONE {
            display.push(self.label(place).to_string());
            country_code = ps::country(&self.places[place as usize].cc).to_string();
        }
        Some(Suggestion {
            id: format!("z{}:{}", country_code, pc_norm(&stored(&doc, self.f.pcode))),
            retrieval_id: format!("{}/z{}:{}", self.suggestion_scope, a.segment_ord, a.doc_id),
            display_name: display.join(", "),
            unverified_suffix: String::new(),
            kind: "postcode",
            category: "",
            country_code,
            x: self.fast_f64(a, "x"),
            y: self.fast_f64(a, "y"),
            bbox: None,
        })
    }

    pub(super) fn suggest_postcodes(
        &self,
        text: &str,
        qualifiers: &[String],
        open: bool,
        cc: Option<&str>,
        limit: usize,
    ) -> Vec<Suggestion> {
        let pc = pc_norm(text);
        if pc.len() < MIN_POSTCODE {
            return vec![];
        }
        let scope = (!qualifiers.is_empty()).then(|| self.scope(qualifiers, open, cc, None, true));
        let pool = if scope.is_some() { POOL } else { limit * 2 };
        // the code as typed before the longer ones it starts
        let mut docs = self.top_of(
            Kind::Postcode,
            cc,
            vec![must(term(self.f.pcode, &pc))],
            pool,
        );
        if qualifiers.is_empty()
            && open
            && let Some((cl, _)) = self.typed_words(self.f.pcode, std::slice::from_ref(&pc), true)
        {
            for a in self.top_of(Kind::Postcode, cc, cl, pool) {
                if !docs.contains(&a) {
                    docs.push(a);
                }
            }
        }
        let found: Vec<_> = docs
            .iter()
            .filter_map(|&a| {
                let place = self.fast(a, "place") as u32;
                if scope.as_ref().is_some_and(|ids| {
                    place == NONE || !self.chain(place).iter().any(|id| ids.contains(id))
                }) {
                    return None;
                }
                self.postcode_suggestion(a)
            })
            .take(limit)
            .collect();
        // "SW8 1JQ, Lambeth": a postcode's place is a town near it, which need not be in the
        // district named, so a code written in full stands in the country the names are in.
        // "Newry, Mourne and Down" is one place's name with a comma in it.
        match scope {
            Some(ids) if found.is_empty() => {
                let mut named = ids;
                if let [a, b, rest @ ..] = qualifiers
                    && named.is_empty()
                {
                    let joined: Vec<String> = [format!("{a} {b}")]
                        .into_iter()
                        .chain(rest.iter().cloned())
                        .collect();
                    named = self.scope(&joined, open, cc, None, true);
                }
                let mut countries: Vec<String> = named
                    .iter()
                    .map(|&id| ps::country(&self.places[id as usize].cc).to_string())
                    .collect();
                countries.extend(qualifiers.last().and_then(|p| self.country(p)));
                docs.into_iter()
                    .filter_map(|a| self.postcode_suggestion(a))
                    .filter(|s| countries.contains(&s.country_code))
                    .take(limit)
                    .collect()
            }
            _ => found,
        }
    }
}
