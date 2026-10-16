//! Administrative polygons by position, for `ResolveRegion`. `areas.bin` holds the territorial
//! (coastline plus territorial sea) polygons of the index's counties, regions, dependencies and
//! countries, simplified, as 1e-7 degree integers. Each polygon's edges are bucketed into
//! horizontal slabs, so a containment test reads only the edges crossing the point's slab. The
//! file is memory-mapped; only its table of polygons is read at startup.
//!
//! Layout, little-endian: `MAGIC`, polygon count, vertex count, then one record per polygon
//! (place, kind, bbox x0 y0 x1 y1, slab count k, first vertex, first pool word), the vertices as
//! (x, y) i32 pairs, then a pool of u32 words: per polygon k + 1 slab offsets followed by its
//! slabs' edges, an edge being the index (from the polygon's first vertex) of its first vertex.
use anyhow::{Result, ensure};
use rstar::{RTree, primitives::GeomWithData, primitives::Rectangle};
use std::{fs, io::Write, path::Path};

pub const FILE: &str = "areas.bin";
/// Most specific first: a point's answer is the first kind that holds it.
pub const KINDS: [&str; 4] = ["county", "region", "dependency", "country"];
pub const COUNTRY: u8 = 3;
/// Douglas-Peucker tolerance, in degrees: a point this far from a border is placed as the
/// full-resolution polygon would place it.
pub const TOLERANCE: f64 = 0.0005;
const MAGIC: &[u8; 8] = b"OGAREA01";
const FIXED: f64 = 1e7;
const HEADER: usize = 16;
const RECORD: usize = 36;

/// One polygon to write: its place, its index in `KINDS` and its rings, closed.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct Area {
    pub place: u32,
    pub kind: u8,
    pub rings: Vec<Vec<[i32; 2]>>,
}

/// A closed ring simplified by Douglas-Peucker to within `tolerance` degrees, as fixed-point
/// vertices; `None` when less than a triangle is left.
pub fn simplify(ring: &[[f64; 2]], tolerance: f64) -> Option<Vec<[i32; 2]>> {
    let n = ring.len();
    if n < 4 {
        return None;
    }
    let mut keep = vec![false; n];
    (keep[0], keep[n - 1]) = (true, true);
    let mut todo = vec![(0, n - 1)];
    while let Some((a, b)) = todo.pop() {
        let ([ax, ay], [bx, by]) = (ring[a], ring[b]);
        let (dx, dy) = (bx - ax, by - ay);
        let len2 = dx * dx + dy * dy;
        let (mut far, mut at) = (tolerance * tolerance, None);
        for (i, &[px, py]) in ring.iter().enumerate().take(b).skip(a + 1) {
            let t = if len2 > 0.0 {
                (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (ex, ey) = (ax + t * dx - px, ay + t * dy - py);
            let d = ex * ex + ey * ey;
            if d > far {
                (far, at) = (d, Some(i));
            }
        }
        if let Some(i) = at {
            keep[i] = true;
            todo.extend([(a, i), (i, b)]);
        }
    }
    let mut out: Vec<[i32; 2]> = ring
        .iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(p, _)| p.map(|v| (v * FIXED).round() as i32))
        .collect();
    out.dedup();
    (out.len() >= 4).then_some(out)
}

fn slab(y: i32, y0: i32, y1: i32, k: u32) -> usize {
    ((y as i64 - y0 as i64) * k as i64 / (y1 as i64 - y0 as i64 + 1)).clamp(0, k as i64 - 1)
        as usize
}

/// Writes `areas.bin`, in an order that depends only on the areas. Returns the bytes written.
pub fn write(path: &Path, mut areas: Vec<Area>) -> Result<u64> {
    areas.sort_unstable();
    let (mut records, mut vertices, mut pool) = (vec![], vec![], vec![]);
    for a in &areas {
        let start = vertices.len();
        let mut edges = vec![];
        for r in &a.rings {
            let first = vertices.len() - start;
            vertices.extend_from_slice(r);
            edges.extend(first as u32..(first + r.len() - 1) as u32);
        }
        let v = &vertices[start..];
        let (x0, x1) = (v.iter().map(|p| p[0]).min(), v.iter().map(|p| p[0]).max());
        let (y0, y1) = (v.iter().map(|p| p[1]).min(), v.iter().map(|p| p[1]).max());
        let (Some(x0), Some(x1), Some(y0), Some(y1)) = (x0, x1, y0, y1) else {
            continue;
        };
        if edges.len() < 3 {
            vertices.truncate(start);
            continue;
        }
        let k = (edges.len() / 16).clamp(1, 1 << 16) as u32;
        let span = |e: u32| {
            let (a, b) = (v[e as usize][1], v[e as usize + 1][1]);
            slab(a.min(b), y0, y1, k)..=slab(a.max(b), y0, y1, k)
        };
        let mut offsets = vec![0u32; k as usize + 1];
        for &e in &edges {
            for s in span(e) {
                offsets[s + 1] += 1;
            }
        }
        for s in 0..k as usize {
            offsets[s + 1] += offsets[s];
        }
        let mut slabs = vec![0u32; offsets[k as usize] as usize];
        let mut fill = offsets.clone();
        for &e in &edges {
            for s in span(e) {
                slabs[fill[s] as usize] = e;
                fill[s] += 1;
            }
        }
        records.push([
            a.place,
            a.kind as u32,
            x0 as u32,
            y0 as u32,
            x1 as u32,
            y1 as u32,
            k,
            u32::try_from(start)?,
            u32::try_from(pool.len())?,
        ]);
        pool.extend(offsets);
        pool.extend(slabs);
    }
    let mut w = std::io::BufWriter::new(fs::File::create(path)?);
    w.write_all(MAGIC)?;
    w.write_all(&u32::try_from(records.len())?.to_le_bytes())?;
    w.write_all(&u32::try_from(vertices.len())?.to_le_bytes())?;
    for word in records.iter().flatten() {
        w.write_all(&word.to_le_bytes())?;
    }
    for [x, y] in &vertices {
        w.write_all(&x.to_le_bytes())?;
        w.write_all(&y.to_le_bytes())?;
    }
    for word in &pool {
        w.write_all(&word.to_le_bytes())?;
    }
    w.flush()?;
    Ok((HEADER + records.len() * RECORD + vertices.len() * 8 + pool.len() * 4) as u64)
}

struct Record {
    place: u32,
    kind: u8,
    bbox: [i32; 4],
    k: u32,
    vertex: usize,
    pool: usize,
}

/// The mapped `areas.bin`.
pub struct Areas {
    bytes: tantivy::directory::OwnedBytes,
    vertices: usize,
    pool: usize,
    records: Vec<Record>,
    tree: RTree<GeomWithData<Rectangle<[f64; 2]>, u32>>,
}

fn word(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

impl Areas {
    pub fn new(bytes: tantivy::directory::OwnedBytes) -> Result<Areas> {
        let b = bytes.as_slice();
        ensure!(
            b.len() >= HEADER && &b[..8] == MAGIC,
            "areas.bin is not an area file"
        );
        let (n, nv) = (word(b, 8) as usize, word(b, 12) as usize);
        let vertices = HEADER + n * RECORD;
        let pool = vertices + nv * 8;
        ensure!(
            b.len() >= pool && (b.len() - pool).is_multiple_of(4),
            "areas.bin is truncated"
        );
        let words = (b.len() - pool) / 4;
        let records: Vec<Record> = (0..n)
            .map(|i| {
                let f = |j: usize| word(b, HEADER + i * RECORD + j * 4);
                Record {
                    place: f(0),
                    kind: f(1) as u8,
                    bbox: [f(2), f(3), f(4), f(5)].map(|v| v as i32),
                    k: f(6),
                    vertex: f(7) as usize,
                    pool: f(8) as usize,
                }
            })
            .collect();
        for r in &records {
            ensure!(
                r.k > 0 && r.vertex < nv && r.pool + (r.k as usize) < words,
                "areas.bin has a polygon out of bounds"
            );
        }
        let tree = RTree::bulk_load(
            records
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let [x0, y0, x1, y1] = r.bbox.map(|v| v as f64);
                    GeomWithData::new(Rectangle::from_corners([x0, y0], [x1, y1]), i as u32)
                })
                .collect(),
        );
        Ok(Areas {
            bytes,
            vertices,
            pool,
            records,
            tree,
        })
    }

    /// The places whose polygon holds (`lat`, `lon`), as (kind, place): the most specific kind
    /// first, then the smallest bounding box, then the place id, so overlaps resolve the same
    /// way whatever the build order.
    pub fn containing(&self, lat: f64, lon: f64) -> Vec<(u8, u32)> {
        let (x, y) = ((lon * FIXED).round(), (lat * FIXED).round());
        let mut found: Vec<_> = self
            .tree
            .locate_all_at_point([x, y])
            .map(|g| &self.records[g.data as usize])
            .filter(|r| self.holds(r, x, y))
            .map(|r| {
                let [x0, y0, x1, y1] = r.bbox.map(|v| v as i64);
                ((r.kind, (x1 - x0) * (y1 - y0), r.place), r)
            })
            .collect();
        found.sort_unstable_by_key(|f| f.0);
        found.iter().map(|(_, r)| (r.kind, r.place)).collect()
    }

    /// Even-odd ray casting over the edges of the point's slab. A word out of the file's bounds
    /// (a damaged file) reads as no edge.
    fn holds(&self, r: &Record, x: f64, y: f64) -> bool {
        let b = self.bytes.as_slice();
        let pool = |i: usize| {
            let at = self.pool + (r.pool + i) * 4;
            (at + 4 <= b.len()).then(|| word(b, at) as usize)
        };
        let vertex = |i: usize| {
            let at = self.vertices + (r.vertex + i) * 8;
            (at + 8 <= self.pool)
                .then(|| [word(b, at) as i32 as f64, word(b, at + 4) as i32 as f64])
        };
        let s = slab(y as i32, r.bbox[1], r.bbox[3], r.k);
        let (Some(from), Some(to)) = (pool(s), pool(s + 1)) else {
            return false;
        };
        let base = r.k as usize + 1;
        let mut inside = false;
        for j in from..to {
            let Some(e) = pool(base + j) else { break };
            let (Some([xa, ya]), Some([xb, yb])) = (vertex(e), vertex(e + 1)) else {
                break;
            };
            if (ya > y) != (yb > y) && x < xa + (y - ya) * (xb - xa) / (yb - ya) {
                inside = !inside;
            }
        }
        inside
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, s: f64) -> Vec<[f64; 2]> {
        vec![[x, y], [x + s, y], [x + s, y + s], [x, y + s], [x, y]]
    }

    fn ring(points: Vec<[f64; 2]>) -> Vec<[i32; 2]> {
        simplify(&points, TOLERANCE).unwrap()
    }

    #[test]
    fn simplify_keeps_corners_within_tolerance_and_drops_specks() {
        // a square with a vertex every 0.001° along its edges and one 0.0001° bump
        let mut points = vec![];
        for i in 0..1000 {
            points.push([i as f64 * 0.001, if i == 500 { 0.0001 } else { 0.0 }]);
        }
        points.extend([[1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]]);
        let simple = simplify(&points, TOLERANCE).unwrap();
        assert_eq!(
            simple,
            [
                [0, 0],
                [10_000_000, 0],
                [10_000_000, 10_000_000],
                [0, 10_000_000],
                [0, 0]
            ]
        );
        // a bump past the tolerance stays
        points[500][1] = 0.001;
        assert!(
            simplify(&points, TOLERANCE)
                .unwrap()
                .contains(&[5_000_000, 10_000])
        );
        assert_eq!(simplify(&square(0.0, 0.0, 0.0001), TOLERANCE), None);
    }

    #[test]
    fn finds_the_containing_polygons_most_specific_first() {
        let path = std::env::temp_dir().join(format!("areas-{}.bin", std::process::id()));
        let mut many = vec![[0.0, 0.0]];
        // a long edge crosses many slabs: 64 vertices zigzag along the bottom
        for i in 1..64 {
            many.push([i as f64 * 0.1, if i % 2 == 1 { -0.5 } else { 0.0 }]);
        }
        many.extend([[6.4, 0.0], [6.4, 8.0], [0.0, 8.0], [0.0, 0.0]]);
        let areas = vec![
            // the country, with a lake
            Area {
                place: 7,
                kind: COUNTRY,
                rings: vec![ring(square(0.0, 0.0, 10.0)), ring(square(8.0, 8.0, 1.0))],
            },
            Area {
                place: 5,
                kind: 1,
                rings: vec![ring(many)],
            },
            // two counties overlap; the smaller wins, and an exclave is a second ring
            Area {
                place: 3,
                kind: 0,
                rings: vec![ring(square(1.0, 1.0, 2.0)), ring(square(9.0, 1.0, 0.5))],
            },
            Area {
                place: 2,
                kind: 0,
                rings: vec![ring(square(1.0, 1.0, 1.0))],
            },
        ];
        let bytes = write(&path, areas).unwrap();
        let file = fs::read(&path).unwrap();
        assert_eq!(bytes as usize, file.len());
        let found = Areas::new(tantivy::directory::OwnedBytes::new(file)).unwrap();
        let at = |lat, lon| found.containing(lat, lon);
        assert_eq!(at(1.5, 1.5), [(0, 2), (0, 3), (1, 5), (COUNTRY, 7)]);
        assert_eq!(at(2.5, 2.5), [(0, 3), (1, 5), (COUNTRY, 7)]);
        assert_eq!(at(1.2, 9.2), [(0, 3), (COUNTRY, 7)]);
        assert_eq!(
            at(-0.2, 0.1),
            [(1, 5)],
            "outside the country, in a tooth of the region"
        );
        assert_eq!(at(-0.2, 0.2), [], "between the region's teeth");
        assert_eq!(at(-0.6, 0.1), [], "below the tooth");
        assert_eq!(at(0.5, 7.0), [(COUNTRY, 7)]);
        assert_eq!(at(8.5, 8.5), [], "in the lake");
        assert_eq!(at(11.0, 5.0), []);
        // a damaged file is refused, not read out of bounds
        let mut cut = fs::read(&path).unwrap();
        cut.truncate(cut.len() - 2);
        assert!(Areas::new(tantivy::directory::OwnedBytes::new(cut)).is_err());
        fs::remove_file(&path).unwrap();
    }
}
