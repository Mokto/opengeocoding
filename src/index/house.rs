use super::*;

impl Index {
    /// The houses whose number is one of `numbers` (as keyed) on the streets named `road` that
    /// hold `postcode` or (`near`) lie within 5 km of a point, or with no road on the streets
    /// holding `postcode`; on streets `named` accepts, nearest the point first, at most 20.
    #[expect(clippy::too_many_arguments)]
    pub fn houses_numbered(
        &self,
        numbers: &[String],
        postcode: &str,
        road: &str,
        near: bool,
        cc: Option<&str>,
        (lat, lon): (f64, f64),
        named: impl Fn(&str) -> bool,
    ) -> Vec<Value> {
        let cos = lat.to_radians().cos().max(0.01);
        let meters = |x: f64, y: f64| ((x - lon) * cos).hypot(y - lat) * 111_195.0;
        let pc = pc_norm(postcode);
        let with_postcode = |mut cl: Vec<(Occur, Box<dyn TQuery>)>, n| {
            cl.push(must(term(self.f.pcode, &pc)));
            self.top_of(Kind::Street, cc, cl, n)
        };
        let mut seen = std::collections::HashSet::new();
        let (mut key, mut found) = (vec![], vec![]);
        let mut look = |docs: Vec<DocAddress>, found: &mut Vec<_>| {
            for a in docs {
                if !seen.insert(a) {
                    continue;
                }
                let sid = self.fast(a, "id") as u32;
                for n in numbers {
                    key.clear();
                    key.extend_from_slice(&sid.to_be_bytes());
                    key.extend_from_slice(n.as_bytes());
                    if let Some(v) = self.numbers[sid as usize % self.numbers.len()].get(&key) {
                        let (x, y, own) = schema::unpack(v);
                        found.push((meters(x, y), a, n, x, y, own));
                    }
                }
            }
        };
        if !pc.is_empty() {
            let n = if road.is_empty() { 1000 } else { 200 };
            look(with_postcode(self.musts(self.f.names, road), n), &mut found);
        }
        // the street's namesakes around the answer when its postcode holds none of the numbers
        if near && !road.is_empty() && found.is_empty() {
            let near = self
                .top_of(Kind::Street, cc, self.musts(self.f.names, road), 200)
                .into_iter()
                .filter(|&a| meters(self.fast_f64(a, "x"), self.fast_f64(a, "y")) < 5_000.0)
                .collect();
            look(near, &mut found);
        }
        found.sort_by(|p, q| p.0.total_cmp(&q.0));
        found
            .into_iter()
            .filter_map(|(_, a, n, x, y, own)| {
                let hit = self.house_hit(a, n.clone(), x, y, own)?;
                matches!(&hit, Hit::Street { name, .. } if name.split('\x1f').any(&named))
                    .then(|| self.render(&hit, true))
            })
            .take(20)
            .collect()
    }

    /// House `number` of street document `a`, at its position and with its postcode slot.
    pub(super) fn house_hit(
        &self,
        a: DocAddress,
        number: String,
        x: f64,
        y: f64,
        own: u8,
    ) -> Option<Hit> {
        use tantivy::schema::Value as _;
        let doc: tantivy::TantivyDocument = self.searcher.doc(a).ok()?;
        let postcode = (own > 0)
            .then(|| {
                doc.get_all(self.f.pcode)
                    .nth(own as usize - 1)?
                    .as_str()
                    .map(pc_norm)
            })
            .flatten();
        Some(Hit::Street {
            doc: a,
            sid: self.fast(a, "id") as u32,
            place: self.fast(a, "place") as u32,
            name: stored(&doc, self.f.name),
            x,
            y,
            postal_town: postal_town(&doc, self.f.postal_town, &number, postcode.as_deref()),
            number: Some(number),
            postcode,
            city: None,
            cc: String::new(),
        })
    }

    /// The house a house suggestion of this index names, rendered as a search answer.
    pub fn suggested_house(&self, s: &Suggestion) -> Option<Value> {
        let (_, key) = s.retrieval_id.split_once('/')?;
        let mut parts = key.strip_prefix('h')?.split(':');
        let (segment, doc) = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
        let number = parts.next()?;
        let a = DocAddress::new(segment, doc);
        let sid = self.fast(a, "id") as u32;
        let mut key = sid.to_be_bytes().to_vec();
        key.extend_from_slice(number.as_bytes());
        let value = self.numbers[sid as usize % self.numbers.len()].get(&key)?;
        let (x, y, own) = schema::unpack(value);
        Some(self.render(&self.house_hit(a, number.into(), x, y, own)?, true))
    }

    /// Calls `each` with every house of street `sid`: its number, position and postcode slot.
    pub(super) fn each_house(&self, sid: u32, mut each: impl FnMut(&[u8], f64, f64, u8)) {
        use tantivy_fst::{IntoStreamer, Streamer};
        let mut range = self.numbers[sid as usize % self.numbers.len()]
            .range()
            .ge(sid.to_be_bytes());
        if let Some(next) = sid.checked_add(1) {
            range = range.lt(next.to_be_bytes());
        }
        let mut houses = range.into_stream();
        while let Some((key, value)) = houses.next() {
            let (x, y, own) = schema::unpack(value);
            each(&key[4..], x, y, own);
        }
    }
}
