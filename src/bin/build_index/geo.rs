use rstar::{RTree, primitives::GeomWithData, primitives::Rectangle};

/// Polygon (all rings of all parts, even-odd rule) with edges bucketed into horizontal slabs,
/// so a containment test only looks at the few edges crossing the point's slab.
/// Vertices are stored as 1e-7 degree integers (about 1 cm), half the size of f64 pairs.
const FIXED: f64 = 1e7;

pub struct Poly {
    place: u32,
    kind: u8,
    xy: Vec<[i32; 2]>,
    ymin: f64,
    inv_h: f64,
    offsets: Vec<u32>,
    edges: Vec<u32>,
    bbox: [f64; 4],
}

impl Poly {
    pub fn new(place: u32, kind: u8, xy: Vec<[f64; 2]>, rings: &[(u32, u32)]) -> Option<Poly> {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in &xy {
            x0 = x0.min(p[0]);
            x1 = x1.max(p[0]);
            y0 = y0.min(p[1]);
            y1 = y1.max(p[1]);
        }
        let n_edges: usize = rings
            .iter()
            .map(|(s, e)| (e - s).saturating_sub(1) as usize)
            .sum();
        if n_edges < 3 || y1 <= y0 {
            return None;
        }
        let k = (n_edges / 4).clamp(1, 1 << 16);
        let inv_h = k as f64 / (y1 - y0);
        let slab = |y: f64| (((y - y0) * inv_h) as usize).min(k - 1);
        let edge_ids = || rings.iter().flat_map(|&(s, e)| s..e.saturating_sub(1));
        let mut offsets = vec![0u32; k + 1];
        for i in edge_ids() {
            let (a, b) = (xy[i as usize][1], xy[i as usize + 1][1]);
            for s in slab(a.min(b))..=slab(a.max(b)) {
                offsets[s + 1] += 1;
            }
        }
        for s in 0..k {
            offsets[s + 1] += offsets[s];
        }
        let mut fill = offsets.clone();
        let mut edges = vec![0u32; offsets[k] as usize];
        for i in edge_ids() {
            let (a, b) = (xy[i as usize][1], xy[i as usize + 1][1]);
            for s in slab(a.min(b))..=slab(a.max(b)) {
                edges[fill[s] as usize] = i;
                fill[s] += 1;
            }
        }
        let xy = xy
            .iter()
            .map(|p| [(p[0] * FIXED).round() as i32, (p[1] * FIXED).round() as i32])
            .collect();
        Some(Poly {
            place,
            kind,
            xy,
            ymin: y0,
            inv_h,
            offsets,
            edges,
            bbox: [x0, y0, x1, y1],
        })
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        let k = self.offsets.len() - 1;
        let s = (((y - self.ymin) * self.inv_h).max(0.0) as usize).min(k - 1);
        let mut inside = false;
        for &e in &self.edges[self.offsets[s] as usize..self.offsets[s + 1] as usize] {
            let [xa, ya] = self.xy[e as usize].map(|v| v as f64 / FIXED);
            let [xb, yb] = self.xy[e as usize + 1].map(|v| v as f64 / FIXED);
            if (ya > y) != (yb > y) && x < xa + (y - ya) * (xb - xa) / (yb - ya) {
                inside = !inside;
            }
        }
        inside
    }

    fn area(&self) -> f64 {
        (self.bbox[2] - self.bbox[0]) * (self.bbox[3] - self.bbox[1])
    }

    pub fn vertices(&self) -> usize {
        self.xy.len()
    }
}

struct Wkb<'a>(&'a [u8], usize);

impl Wkb<'_> {
    fn u32(&mut self, le: bool) -> Option<u32> {
        let b: [u8; 4] = self.0.get(self.1..self.1 + 4)?.try_into().ok()?;
        self.1 += 4;
        Some(if le {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        })
    }
    fn f64(&mut self, le: bool) -> Option<f64> {
        let b: [u8; 8] = self.0.get(self.1..self.1 + 8)?.try_into().ok()?;
        self.1 += 8;
        Some(if le {
            f64::from_le_bytes(b)
        } else {
            f64::from_be_bytes(b)
        })
    }
    fn header(&mut self) -> Option<(bool, u32)> {
        let le = *self.0.get(self.1)? == 1;
        self.1 += 1;
        Some((le, self.u32(le)?))
    }
    fn polygons(&mut self, xy: &mut Vec<[f64; 2]>, rings: &mut Vec<(u32, u32)>) -> Option<()> {
        let (le, kind) = self.header()?;
        match kind {
            3 => {
                for _ in 0..self.u32(le)? {
                    let start = xy.len() as u32;
                    for _ in 0..self.u32(le)? {
                        xy.push([self.f64(le)?, self.f64(le)?]);
                    }
                    rings.push((start, xy.len() as u32));
                }
                Some(())
            }
            6 => {
                for _ in 0..self.u32(le)? {
                    self.polygons(xy, rings)?;
                }
                Some(())
            }
            _ => None,
        }
    }
}

/// Vertices and the ring ranges into them.
pub type Rings = (Vec<[f64; 2]>, Vec<(u32, u32)>);

/// 2D WKB Polygon or MultiPolygon.
pub fn wkb_polygons(buf: &[u8]) -> Option<Rings> {
    let (mut xy, mut rings) = (vec![], vec![]);
    Wkb(buf, 0).polygons(&mut xy, &mut rings)?;
    Some((xy, rings))
}

/// 2D WKB Point.
pub fn wkb_point(buf: &[u8]) -> Option<(f64, f64)> {
    let mut r = Wkb(buf, 0);
    let (le, kind) = r.header()?;
    (kind == 1).then_some(())?;
    Some((r.f64(le)?, r.f64(le)?))
}

pub const KINDS: [&str; 3] = ["locality", "localadmin", "county"];
const NEAR_LOCALITY_M: f32 = 5000.0;

pub struct Assigner {
    polys: Vec<Poly>,
    tree: RTree<GeomWithData<Rectangle<[f64; 2]>, u32>>,
    towns: RTree<GeomWithData<[f64; 2], u32>>,
}

impl Assigner {
    pub fn new(polys: Vec<Poly>, towns: Vec<([f64; 2], u32)>) -> Assigner {
        let tree = RTree::bulk_load(
            polys
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    GeomWithData::new(
                        Rectangle::from_corners([p.bbox[0], p.bbox[1]], [p.bbox[2], p.bbox[3]]),
                        i as u32,
                    )
                })
                .collect(),
        );
        let towns = RTree::bulk_load(
            towns
                .into_iter()
                .map(|(p, i)| GeomWithData::new(p, i))
                .collect(),
        );
        Assigner { polys, tree, towns }
    }

    /// Containing locality polygon, else a locality point within 5 km, else the local admin
    /// area, else the county. Overlapping polygons of one kind resolve to the smallest.
    pub fn place(&self, x: f64, y: f64) -> Option<u32> {
        let mut pick: [Option<(f64, u32)>; 3] = [None; 3];
        for c in self.tree.locate_all_at_point([x, y]) {
            let p = &self.polys[c.data as usize];
            let k = p.kind as usize;
            if pick[k].is_none_or(|(a, _)| p.area() < a) && p.contains(x, y) {
                pick[k] = Some((p.area(), p.place));
            }
        }
        if let Some((_, p)) = pick[0] {
            return Some(p);
        }
        // ponytail: nearest by raw lon/lat degrees, then metres with cos(lat); can pick a slightly
        // farther town at high latitudes. Upgrade: scale x by cos(lat) in the tree.
        if let Some(t) = self.towns.nearest_neighbor([x, y]) {
            let [tx, ty] = *t.geom();
            if (((tx - x) * y.to_radians().cos()).hypot(ty - y) * 111_195.0) as f32
                <= NEAR_LOCALITY_M
            {
                return Some(t.data);
            }
        }
        pick[1].or(pick[2]).map(|(_, p)| p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wkb_polygon(rings: &[&[[f64; 2]]]) -> Vec<u8> {
        let mut b = vec![1u8];
        b.extend(3u32.to_le_bytes());
        b.extend((rings.len() as u32).to_le_bytes());
        for r in rings {
            b.extend((r.len() as u32).to_le_bytes());
            for p in *r {
                b.extend(p[0].to_le_bytes());
                b.extend(p[1].to_le_bytes());
            }
        }
        b
    }

    fn square(x: f64, y: f64, s: f64) -> Vec<[f64; 2]> {
        vec![[x, y], [x + s, y], [x + s, y + s], [x, y + s], [x, y]]
    }

    #[test]
    fn polygon_with_hole_and_parts() {
        let (outer, hole) = (square(0.0, 0.0, 10.0), square(4.0, 4.0, 2.0));
        let mut multi = vec![1u8];
        multi.extend(6u32.to_le_bytes());
        multi.extend(2u32.to_le_bytes());
        multi.extend(wkb_polygon(&[&outer, &hole]));
        multi.extend(wkb_polygon(&[&square(20.0, 20.0, 1.0)]));
        let (xy, rings) = wkb_polygons(&multi).unwrap();
        let p = Poly::new(0, 0, xy, &rings).unwrap();
        assert!(p.contains(1.0, 1.0));
        assert!(p.contains(9.9, 9.9));
        assert!(!p.contains(5.0, 5.0), "inside the hole");
        assert!(p.contains(20.5, 20.5), "second part");
        assert!(!p.contains(15.0, 15.0), "between parts");
        assert!(!p.contains(-1.0, 5.0));
    }

    #[test]
    fn smallest_locality_then_nearby_town_then_admin() {
        let poly = |place, kind, s: Vec<[f64; 2]>| {
            Poly::new(place, kind, s.clone(), &[(0, s.len() as u32)]).unwrap()
        };
        let a = Assigner::new(
            vec![
                poly(1, 0, square(0.0, 0.0, 1.0)),
                poly(2, 0, square(0.0, 0.0, 0.1)),
                poly(3, 2, square(-5.0, -5.0, 20.0)),
            ],
            vec![([3.0, 3.0], 4)],
        );
        assert_eq!(
            a.place(0.05, 0.05),
            Some(2),
            "nested localities: smallest wins"
        );
        assert_eq!(a.place(0.5, 0.5), Some(1));
        assert_eq!(a.place(3.01, 3.0), Some(4), "town point ~1 km away");
        assert_eq!(a.place(8.0, 8.0), Some(3), "only the county contains it");
        assert_eq!(a.place(50.0, 50.0), None);
    }
}
