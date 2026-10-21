use super::*;

impl Index {
    /// A street or house suggestion whose street holds no postcode.
    pub(super) fn without_postcodes(&self, s: &Suggestion) -> bool {
        use tantivy::schema::Value as _;
        let Some((segment, doc)) = s
            .retrieval_id
            .split_once('/')
            .and_then(|(_, key)| key.get(1..)?.split(':').next().zip(key.split(':').nth(1)))
            .and_then(|(segment, doc)| Some((segment.parse().ok()?, doc.parse().ok()?)))
        else {
            return false;
        };
        self.searcher
            .doc::<tantivy::TantivyDocument>(tantivy::DocAddress::new(segment, doc))
            .is_ok_and(|doc| {
                doc.get_first(self.f.pcode)
                    .and_then(|v| v.as_str())
                    .is_none()
            })
    }

    pub(super) fn postcode_suggestion(&self, a: tantivy::DocAddress) -> Option<Suggestion> {
        let doc: tantivy::TantivyDocument = self.searcher.doc(a).ok()?;
        let place = self.fast(a, "place") as u32;
        let mut display = vec![stored(&doc, self.f.name)];
        let mut country_code = String::new();
        if place != NONE {
            display.push(self.label(place).to_string());
            country_code = self.places[place as usize].cc.to_string();
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
        docs.into_iter()
            .filter_map(|a| {
                let place = self.fast(a, "place") as u32;
                if scope.as_ref().is_some_and(|ids| {
                    place == NONE || !self.chain(place).iter().any(|id| ids.contains(id))
                }) {
                    return None;
                }
                self.postcode_suggestion(a)
            })
            .take(limit)
            .collect()
    }
}
