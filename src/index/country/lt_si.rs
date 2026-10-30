impl crate::index::Index {
    /// "Pivola 91, Hoče-Slivnica", "Pagėgių sav., Užbalių k. 1": the house of that number on a
    /// street of exactly that name in the municipality's polygon (a Slovenian region, a
    /// Lithuanian county). The builder files a village's road under each locality it
    /// crosses, more streets of one name in a municipality than the street lookup weighs, and
    /// near a border under a locality of the next municipality (Pivola's under Razvanje, in
    /// Maribor). Several when places of the municipality hold distinct houses of the number,
    /// which only the settlement could tell apart; copies of one house count once.
    pub(in crate::index) fn municipal_houses(
        &self,
        road: &str,
        number: &str,
        municipality: &str,
        cc: &str,
    ) -> Vec<crate::index::Hit> {
        use crate::index::{Hit, Kind, must, schema, term};
        use tantivy::query::{BooleanQuery, Occur};
        use tantivy::schema::Value as _;
        let want = crate::index::name_key(municipality);
        let regions: Vec<u32> = self
            .places(municipality, Some(cc), None, &|p| match cc {
                "lt" => &*p.subtype == "county",
                _ => matches!(&*p.subtype, "region" | "macroregion"),
            })
            .into_iter()
            .filter_map(|h| match h {
                Hit::Place(id) if self.places[id as usize].named(want) => Some(id),
                _ => None,
            })
            .collect();
        let cl = self.musts(self.f.names, road);
        if regions.is_empty() || cl.is_empty() {
            return vec![];
        }
        let mut scoped = self.musts(self.f.names, road);
        scoped.push(must(Box::new(BooleanQuery::new(
            regions
                .iter()
                .map(|id| (Occur::Should, term(self.f.ids, &id.to_string())))
                .collect(),
        ))));
        let (road, number) = (schema::fold(road), schema::number_key(number));
        let mut found: Vec<Hit> = vec![];
        // The streets filed under the municipality, then those of the name in all the country, for
        // one filed under the next municipality. ponytail: 1,000 of each by importance; a name
        // as common as a street type ("Cesta") can miss the second kind.
        let streets = self.top_of(Kind::Street, Some(cc), scoped, 1000);
        for a in streets
            .into_iter()
            .chain(self.top_of(Kind::Street, Some(cc), cl, 1000))
        {
            let Ok(doc) = self.searcher.doc::<tantivy::TantivyDocument>(a) else {
                continue;
            };
            if !doc
                .get_all(self.f.name)
                .filter_map(|v| v.as_str())
                .any(|name| schema::fold(name) == road)
            {
                continue;
            }
            let sid = self.fast(a, "id") as u32;
            let Some((form, v)) = self.house_record(sid, &number) else {
                continue;
            };
            let (x, y, own) = schema::unpack(v);
            if self.area_at(y, x).0.is_some_and(|r| regions.contains(&r)) {
                found.extend(self.house_hit(a, form, x, y, own));
            }
        }
        let point = |h: &Hit| match h {
            Hit::Street { x, y, .. } => (*x, *y),
            _ => (0., 0.),
        };
        let Some(&(x, y)) = found.first().map(point).as_ref() else {
            return found;
        };
        // copies of one house in two streets of one name, not two houses
        if found.iter().all(|h| {
            let (hx, hy) = point(h);
            ((hx - x) * y.to_radians().cos()).hypot(hy - y) * 111_000. < 300.
        }) {
            found.truncate(1);
        }
        found
    }
}
