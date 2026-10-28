use super::*;

const HOUSE_REGION_M: f64 = 100.0;

const STREET_LOCALITY_M: f64 = 1_000.0;

const FAR_REGION_M: f64 = 150_000.0;

pub(super) const NEAR_LOCALITY_M: f64 = 50_000.0;

/// Calls `cell` with the offsets of the cells `r` cells from the centre, around a square ring.
pub(super) fn ring(r: i64, mut cell: impl FnMut(i64, i64)) {
    for dy in -r..=r {
        let step = if dy.abs() == r { 1 } else { 2 * r };
        for dx in (-r..=r).step_by(step as usize) {
            cell(dx, dy);
        }
    }
}

impl Index {
    /// The houses within `radius_m` of a point, nearest first, each with its distance in metres.
    /// Streets are looked up by the grid cells their houses occupy, then their houses by distance.
    // ponytail: rings of cells grow in every direction by the smaller cell side, so a high-latitude
    // search reads more cells than it needs, and cells past the antimeridian or poles aren't
    // searched. Upgrade: per-axis ring counts and wrapped columns.
    fn reverse_houses(&self, lat: f64, lon: f64, radius_m: f64, size: usize) -> Vec<(Value, f64)> {
        let Some(rev) = &self.reverse else {
            return vec![];
        };
        let (cx, cy) = reverse::cell_at(lon, lat);
        let cos = lat.to_radians().cos().max(0.01);
        let cell_m = reverse::CELL_LAT_M.min(reverse::CELL_LON_M * cos);
        let meters = |x: f64, y: f64| ((x - lon) * cos).hypot(y - lat) * 111_195.0;
        // (distance, street id, street document, number, x, y, postcode slot)
        let mut found: Vec<(f64, u32, DocAddress, String, f64, f64, u8)> = vec![];
        let (mut seen, mut docs) = (std::collections::HashSet::new(), vec![]);
        for r in 0..=(radius_m / cell_m).ceil() as i64 {
            docs.clear();
            ring(r, |dx, dy| rev.docs(cx + dx, cy + dy, &mut docs));
            for &packed in &docs {
                if !seen.insert(packed) {
                    continue;
                }
                let (segment, doc) = reverse::unpack_doc(packed);
                let a = DocAddress::new(segment, doc);
                let sid = self.fast(a, "id") as u32;
                self.each_house(sid, |number, x, y, own| {
                    let m = meters(x, y);
                    if m <= radius_m {
                        let number = String::from_utf8_lossy(number).into_owned();
                        found.push((m, sid, a, number, x, y, own));
                    }
                });
            }
            // Ties go by street id, not document address, which differs between two builds.
            found.sort_by(|p, q| {
                p.0.total_cmp(&q.0)
                    .then_with(|| (p.1, &p.3).cmp(&(q.1, &q.3)))
            });
            found.truncate(size);
            // everything within `r` cells' width of the point has been read
            if found.len() == size && found[size - 1].0 <= r as f64 * cell_m {
                break;
            }
        }
        found
            .into_iter()
            .filter_map(|(m, _, a, number, x, y, own)| {
                Some((self.render(&self.house_hit(a, number, x, y, own)?, true), m))
            })
            .collect()
    }

    /// The streets within `radius_m` of a point, nearest first, each at its point nearest it,
    /// with the distance in metres and the postcode of the street's house nearest that point.
    // ponytail: rings grow by the smaller side of a street cell, as in `reverse_houses`.
    fn reverse_streets(&self, lat: f64, lon: f64, radius_m: f64, size: usize) -> Vec<(Value, f64)> {
        use tantivy::schema::Value as _;
        let Some(streets) = &self.streets else {
            return vec![];
        };
        let (cx, cy) = reverse::street_cell_at(lon, lat);
        let cos = lat.to_radians().cos().max(0.01);
        let cell_m = reverse::STREET_CELL_LAT_M.min(reverse::STREET_CELL_LON_M * cos);
        // in latitude steps, from the point
        let (p, sx) = (reverse::steps(lon, lat), 2.0 * cos);
        let from_p = |(x, y): (f64, f64)| ((x - p.0) * sx, y - p.1);
        let (region, country) = self.area_at(lat, lon);
        let code = |p| self.region_code(&self.chain(p));
        let region = region.and_then(code);
        let mut allowed = HashMap::new();
        let mut best: HashMap<u32, (f64, (f64, f64))> = HashMap::new();
        let mut edges = vec![];
        let read_m = |r: i64| (r as f64 - reverse::STREET_SLACK) * cell_m;
        for r in 0.. {
            edges.clear();
            ring(r, |dx, dy| streets.edges(cx + dx, cy + dy, &mut edges));
            for &(a, b, doc) in &edges {
                if !*allowed.entry(doc).or_insert_with(|| {
                    let (segment, id) = reverse::unpack_doc(doc);
                    let place = self.fast(DocAddress::new(segment, id), "place") as u32;
                    country.is_none_or(|c| {
                        place != NONE
                            && self.places[place as usize].cc == self.places[c as usize].cc
                    }) && region.is_none_or(|r| code(place).is_none_or(|c| c == r))
                }) {
                    continue;
                }
                let (d, q) = reverse::nearest((0.0, 0.0), from_p(a), from_p(b));
                let m = d * reverse::STEP_LAT_M;
                if m <= radius_m && best.get(&doc).is_none_or(|e| m < e.0) {
                    best.insert(doc, (m, q));
                }
            }
            if read_m(r) >= radius_m {
                break;
            }
            if best.len() >= size {
                let mut ms: Vec<f64> = best.values().map(|e| e.0).collect();
                if *ms.select_nth_unstable_by(size - 1, f64::total_cmp).1 <= read_m(r) {
                    break;
                }
            }
        }
        let mut found: Vec<_> = best.into_iter().collect();
        found.sort_by(|a, b| a.1.0.total_cmp(&b.1.0).then(a.0.cmp(&b.0)));
        found.truncate(size);
        found
            .into_iter()
            .filter_map(|(packed, (m, q))| {
                let (x, y) = reverse::degrees(p.0 + q.0 / sx, p.1 + q.1);
                let (segment, doc) = reverse::unpack_doc(packed);
                let a = DocAddress::new(segment, doc);
                let sid = self.fast(a, "id") as u32;
                let mut own = (f64::MAX, 0);
                self.each_house(sid, |_, hx, hy, slot| {
                    let d = ((hx - x) * cos).hypot(hy - y);
                    if d < own.0 {
                        own = (d, slot);
                    }
                });
                let doc: tantivy::TantivyDocument = self.searcher.doc(a).ok()?;
                let postcode = (own.1 > 0)
                    .then(|| {
                        doc.get_all(self.f.pcode)
                            .nth(own.1 as usize - 1)?
                            .as_str()
                            .map(pc_norm)
                    })
                    .flatten();
                let (place, city) = self.street_place(self.fast(a, "place") as u32, lat, lon);
                let hit = Hit::Street {
                    doc: a,
                    sid,
                    place,
                    name: stored(&doc, self.f.name),
                    x,
                    y,
                    number: None,
                    postcode,
                    city,
                    postal_town: None,
                    cc: String::new(),
                };
                Some((self.render(&hit, true), m))
            })
            .collect()
    }

    /// A segment's centre can fall in a township's polygon even when the query is near a town
    /// point with no polygon. A descendant keeps the street's ancestry; a sibling contributes
    /// only its name, never its region. A country alone does not corroborate a sibling.
    pub(super) fn street_place(&self, place: u32, lat: f64, lon: f64) -> (u32, Option<String>) {
        let pl = |p: u32| &self.places[p as usize];
        let size = |p: u32| match pl(p).key() {
            "city" => 3,
            "town" => 2,
            "village" => 1,
            _ => 0,
        };
        let Some((near, _)) = self.nearest_locality_within(lat, lon, STREET_LOCALITY_M, |p| {
            &*self.places[p as usize].class != "hamlet"
        }) else {
            return (place, None);
        };
        if place == NONE {
            return (place, None);
        }
        let ours = self.chain(place);
        let parent = pl(near).parent;
        if ours.contains(&near) {
            (place, None)
        } else if self.chain(near).contains(&place) {
            (near, None)
        } else if ours.contains(&parent)
            && !matches!(&*pl(parent).subtype, "country" | "dependency")
            && size(near) >= size(place)
        {
            (place, Some(pl(near).name(true).to_string()))
        } else {
            (place, None)
        }
    }

    /// Up to `size` results for a point: the houses within `radius_m`, nearest first, as
    /// ("address", ...); with none, the streets within it ("street", ...); with none either, the
    /// nearest locality ("locality", ...) wherever it is within about 50 km. Each result comes
    /// with its distance in metres.
    pub fn reverse_search(
        &self,
        lat: f64,
        lon: f64,
        radius_m: f64,
        size: usize,
    ) -> Vec<(&'static str, Value, f64)> {
        let houses = self.reverse_houses(lat, lon, radius_m, size);
        if !houses.is_empty() {
            return houses.into_iter().map(|(v, m)| ("address", v, m)).collect();
        }
        let streets = self.reverse_streets(lat, lon, radius_m, size);
        if !streets.is_empty() {
            return streets.into_iter().map(|(v, m)| ("street", v, m)).collect();
        }
        let place = self.reverse(lat, lon);
        let coordinate = |key: &str| place[key].as_str().and_then(|s| s.parse::<f64>().ok());
        let (Some(y), Some(x)) = (coordinate("lat"), coordinate("lon")) else {
            return vec![];
        };
        let m = ((x - lon) * lat.to_radians().cos()).hypot(y - lat) * 111_195.0;
        vec![("locality", place, m)]
    }

    /// Nearest locality point within about 50 km. One in the region whose polygon holds the point
    /// (or, with none, in the country whose polygon does) comes first, so a nearer town across a
    /// border does not name the point; with no such town in range, the nearest of any.
    pub fn reverse(&self, lat: f64, lon: f64) -> Value {
        let code = |p: u32| self.region_code(&self.chain(p));
        let (region, country) = self.area_at(lat, lon);
        let region = region.and_then(code);
        let same = |p: u32| {
            country.is_none_or(|c| self.places[p as usize].cc == self.places[c as usize].cc)
                && region.is_none_or(|r| code(p).is_none_or(|c| c == r))
        };
        match self
            .nearest_locality_within(lat, lon, NEAR_LOCALITY_M, same)
            .or_else(|| self.nearest_locality_within(lat, lon, NEAR_LOCALITY_M, |_| true))
        {
            Some((p, _)) => self.render(&Hit::Place(p), true),
            None => json!({"error": "Unable to geocode"}),
        }
    }

    /// The polygons that hold a point: the most specific one whose place has a region code, and
    /// the country's.
    pub(super) fn area_at(&self, lat: f64, lon: f64) -> (Option<u32>, Option<u32>) {
        let containing: Vec<_> = self
            .areas
            .iter()
            .flat_map(|a| a.containing(lat, lon))
            .collect();
        // County polygons can spill across a region boundary; the containing region wins.
        let parent = containing.iter().find_map(|&(kind, p)| {
            (kind == 1 && self.region_code(&self.chain(p)).is_some()).then_some(p)
        });
        let (mut region, mut country) = (None, None);
        for (kind, p) in containing {
            let chain = self.chain(p);
            let county_region = (kind == 0)
                .then(|| {
                    chain
                        .iter()
                        .copied()
                        .find(|&p| self.places[p as usize].subtype.as_ref() == "region")
                })
                .flatten();
            if parent.zip(county_region).is_some_and(|(a, b)| a != b) {
                continue;
            }
            if region.is_none() && self.region_code(&chain).is_some() {
                region = Some(p);
            }
            if kind == area::COUNTRY && country.is_none() {
                country = Some(p);
            }
        }
        (region, country)
    }

    /// The address that names a point's region, for `ResolveRegion`: the most specific county,
    /// region, dependency or country polygon that holds the point and has a region code, with
    /// its subdivision below the region when it is a county that has one.
    ///
    /// A point no such polygon holds (offshore past the territorial sea, a gap in the data, a
    /// country whose regions the index lacks) falls back to the nearest points. There a house
    /// within 100 m comes first: its place was assigned by polygon, where the nearest locality
    /// point can lie across a border. Failing that, the nearest locality within 50 km, when it
    /// has a region code. Otherwise (the point is past 50 km, up to 150, or the nearest
    /// locality's parents stop short of a region) a coded locality answers only if it is in the
    /// nearest one's country and no locality of another region is within twice its distance, or
    /// four times when the nearest has no code: that gap suggests the data lacks the point's own
    /// region, which a neighbour's code would misname. When a country polygon holds the point,
    /// only its own localities are read and the house is skipped.
    pub fn reverse_region(&self, lat: f64, lon: f64) -> Value {
        let (region, country) = self.area_at(lat, lon);
        if let Some(p) = region {
            return self.render(&Hit::Place(p), true);
        }
        if country.is_none()
            && let Some((house, _)) = self
                .reverse_houses(lat, lon, HOUSE_REGION_M, 1)
                .into_iter()
                .next()
            && house["address"].get("ISO3166-2-lvl4").is_some()
        {
            return house;
        }
        let code = |p: u32| self.region_code(&self.chain(p));
        let cc = |p: u32| &self.places[p as usize].cc;
        let inside = |p: u32| country.is_none_or(|c| cc(p) == cc(c));
        let found = self
            .nearest_locality_within(lat, lon, FAR_REGION_M, inside)
            .and_then(|(nearest, m)| {
                let coded = code(nearest).is_some();
                if coded && m <= NEAR_LOCALITY_M {
                    return Some(nearest);
                }
                let (p, m) = self.nearest_locality_within(lat, lon, FAR_REGION_M, |p| {
                    inside(p) && code(p).is_some()
                })?;
                let margin = if coded { 2.0 } else { 4.0 } * m;
                let alone = || {
                    self.nearest_locality_within(lat, lon, margin, |q| {
                        inside(q) && code(q).is_some_and(|c| Some(c) != code(p))
                    })
                    .is_none()
                };
                (cc(p) == cc(nearest) && alone()).then_some(p)
            });
        match found {
            Some(p) => self.render(&Hit::Place(p), true),
            None => json!({"error": "Unable to geocode"}),
        }
    }

    fn nearest_locality_within(
        &self,
        lat: f64,
        lon: f64,
        max_m: f64,
        accept: impl Fn(u32) -> bool,
    ) -> Option<(u32, f64)> {
        self.localities.nearest(
            lat,
            lon,
            max_m,
            |p| {
                let pl = &self.places[p as usize];
                (pl.x as f64, pl.y as f64)
            },
            accept,
        )
    }
}
