use rstar::{RTree, primitives::GeomWithData};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::Mutex,
};
use tantivy::directory::{Directory, MmapDirectory, OwnedBytes};

/// Vertices are stored as 1e-7 degree integers (about 1 cm), half the size of f64 pairs.
const FIXED: f64 = 1e7;

/// The polygons' vertices and slab edges, written to a scratch file as they are read, then
/// memory-mapped. They are about 5 GB for the world: as file pages, the OS can drop them under
/// memory pressure and read them back, where in the heap they added to what the build needs.
pub struct PolyFile {
    path: PathBuf,
    /// The file and the words written to it.
    w: Mutex<(BufWriter<File>, u64)>,
}

impl PolyFile {
    pub fn new(path: PathBuf) -> PolyFile {
        let f = File::create(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        PolyFile {
            path,
            w: Mutex::new((BufWriter::with_capacity(1 << 20, f), 0)),
        }
    }

    /// Appends `words`; returns the position of the first.
    fn put(&self, words: &[u32]) -> u64 {
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let mut g = self.w.lock().unwrap();
        g.0.write_all(&bytes).unwrap();
        g.1 += words.len() as u64;
        g.1 - words.len() as u64
    }

    /// The words written, mapped; the file is removed, and lives on until they are dropped.
    fn map(self) -> OwnedBytes {
        let (w, _) = self.w.into_inner().unwrap();
        w.into_inner().unwrap();
        let path = self.path.display();
        let dir = MmapDirectory::open(self.path.parent().unwrap())
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        let file = dir
            .open_read(self.path.file_name().unwrap().as_ref())
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        let bytes = file.read_bytes().unwrap_or_else(|e| panic!("{path}: {e}"));
        std::fs::remove_file(&self.path).unwrap();
        bytes
    }
}

fn word(data: &[u8], i: u64) -> u32 {
    let at = i as usize * 4;
    u32::from_le_bytes(data[at..at + 4].try_into().unwrap())
}

/// Polygon (all rings of all parts, even-odd rule) with edges bucketed into horizontal slabs,
/// so a containment test only looks at the few edges crossing the point's slab. Its arrays are
/// in the `PolyFile` from word `at`: the vertices as x, y pairs, the `slabs + 1` offsets of each
/// slab's edges, then the edges, by their first vertex. From word `grid`, a `gx` by `gy` grid
/// over its bounding box holds each cell's side (`OUTSIDE`, `INSIDE` or `EDGE`), 16 to a word,
/// so most points are answered without reading an edge.
pub struct Poly {
    place: u32,
    kind: u8,
    vertices: u32,
    slabs: u32,
    at: u64,
    ymin: f64,
    inv_h: f64,
    bbox: [f64; 4],
    gx: u32,
    gy: u32,
    inv_cw: f64,
    inv_ch: f64,
    grid: u64,
}

/// A grid cell no edge comes within `MARGIN` of lies wholly on one side, by more than rounding
/// can move a point, so it answers for every point in it.
const OUTSIDE: u32 = 0;
const INSIDE: u32 = 1;
const EDGE: u32 = 2;
const MARGIN: f64 = 1e-9;

impl Poly {
    pub fn new(
        place: u32,
        kind: u8,
        xy: Vec<[f64; 2]>,
        rings: &[(u32, u32)],
        file: &PolyFile,
    ) -> Option<Poly> {
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
        // The vertices as `contains` reads them, which file the edges in their slabs: by the
        // unrounded ones, a point level with a vertex could miss an edge crossing its line.
        let mut words: Vec<u32> = xy
            .iter()
            .flat_map(|p| p.map(|v| (v * FIXED).round() as i32 as u32))
            .collect();
        let fixed: Vec<[f64; 2]> = words
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| p.map(|v| v as i32 as f64 / FIXED))
            .collect();
        let k = (n_edges / 4).clamp(1, 1 << 16);
        let inv_h = k as f64 / (y1 - y0);
        let slab = |y: f64| (((y - y0) * inv_h).max(0.0) as usize).min(k - 1);
        let edge_ids = || rings.iter().flat_map(|&(s, e)| s..e.saturating_sub(1));
        let mut offsets = vec![0u32; k + 1];
        for i in edge_ids() {
            let (a, b) = (fixed[i as usize][1], fixed[i as usize + 1][1]);
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
            let (a, b) = (fixed[i as usize][1], fixed[i as usize + 1][1]);
            for s in slab(a.min(b))..=slab(a.max(b)) {
                edges[fill[s] as usize] = i;
                fill[s] += 1;
            }
        }
        words.extend(offsets);
        words.extend(edges);
        let mut poly = Poly {
            place,
            kind,
            vertices: xy.len() as u32,
            slabs: k as u32,
            at: 0,
            ymin: y0,
            inv_h,
            bbox: [x0, y0, x1, y1],
            gx: 0,
            gy: 0,
            inv_cw: 0.0,
            inv_ch: 0.0,
            grid: 0,
        };
        let cells = poly.cells(&fixed, rings, n_edges);
        let grid = words.len() as u64;
        words.extend(cells);
        poly.at = file.put(&words);
        poly.grid = poly.at + grid;
        Some(poly)
    }

    fn cell(&self, x: f64, y: f64) -> usize {
        let c = (((x - self.bbox[0]) * self.inv_cw).max(0.0) as usize).min(self.gx as usize - 1);
        let r = (((y - self.bbox[1]) * self.inv_ch).max(0.0) as usize).min(self.gy as usize - 1);
        r * self.gx as usize + c
    }

    /// Sets the grid's size and returns its cells, packed: about as many as the polygon has
    /// edges. A cell near an edge is `EDGE`; any other takes its centre's side.
    fn cells(&mut self, xy: &[[f64; 2]], rings: &[(u32, u32)], n_edges: usize) -> Vec<u32> {
        let [x0, y0, x1, y1] = self.bbox;
        let (w, h) = (x1 - x0, y1 - y0);
        if w <= 0.0 || !w.is_finite() || !h.is_finite() {
            return vec![];
        }
        let target = n_edges.clamp(16, 1 << 22) as f64;
        let gy = (target * h / w).sqrt().round().clamp(1.0, 4096.0) as usize;
        let gx = (target / gy as f64).round().clamp(1.0, 4096.0) as usize;
        (self.gx, self.gy) = (gx as u32, gy as u32);
        (self.inv_cw, self.inv_ch) = (gx as f64 / w, gy as f64 / h);
        let (cw, ch) = (w / gx as f64, h / gy as f64);
        let col = |x: f64| (((x - x0) * self.inv_cw).max(0.0) as usize).min(gx - 1);
        let row = |y: f64| (((y - y0) * self.inv_ch).max(0.0) as usize).min(gy - 1);
        let mut state = vec![OUTSIDE as u8; gx * gy];
        let mut mark = |c0: usize, c1: usize, r0: usize, r1: usize| {
            for r in r0..=r1 {
                state[r * gx + c0..=r * gx + c1].fill(EDGE as u8);
            }
        };
        let edge_ids = || rings.iter().flat_map(|&(s, e)| s..e.saturating_sub(1));
        // Rows whose centre line an edge may cross, for the sides below.
        let mut by_row: Vec<Vec<u32>> = vec![vec![]; gy];
        for i in edge_ids() {
            let ([xa, ya], [xb, yb]) = (xy[i as usize], xy[i as usize + 1]);
            let (c0, c1) = (col(xa.min(xb) - MARGIN), col(xa.max(xb) + MARGIN));
            let (r0, r1) = (row(ya.min(yb) - MARGIN), row(ya.max(yb) + MARGIN));
            if (c1 - c0 + 1) * (r1 - r0 + 1) <= 64 {
                mark(c0, c1, r0, r1);
            } else {
                // A long slanted edge: the cells around points half a cell apart along it.
                let steps = ((xb - xa).abs() / cw).max((yb - ya).abs() / ch).ceil() as usize * 2;
                for t in 0..=steps {
                    let f = t as f64 / steps as f64;
                    let (c, r) = (col(xa + (xb - xa) * f), row(ya + (yb - ya) * f));
                    mark(
                        c.saturating_sub(1),
                        (c + 1).min(gx - 1),
                        r.saturating_sub(1),
                        (r + 1).min(gy - 1),
                    );
                }
            }
            for list in &mut by_row[r0..=r1] {
                list.push(i);
            }
        }
        let mut xs = vec![];
        for (r, list) in by_row.iter().enumerate() {
            let y = y0 + (r as f64 + 0.5) * ch;
            xs.clear();
            for &i in list {
                let ([xa, ya], [xb, yb]) = (xy[i as usize], xy[i as usize + 1]);
                if (ya > y) != (yb > y) {
                    xs.push(xa + (y - ya) * (xb - xa) / (yb - ya));
                }
            }
            xs.sort_unstable_by(f64::total_cmp);
            for (c, cell) in state[r * gx..(r + 1) * gx].iter_mut().enumerate() {
                if *cell != EDGE as u8 {
                    let x = x0 + (c as f64 + 0.5) * cw;
                    let right = xs.len() - xs.partition_point(|&v| v <= x);
                    *cell = (right % 2) as u8;
                }
            }
        }
        state
            .chunks(16)
            .map(|c| c.iter().rev().fold(0, |w, &s| w << 2 | u32::from(s)))
            .collect()
    }

    pub fn contains(&self, data: &[u8], x: f64, y: f64) -> bool {
        if self.gx > 0 {
            let i = self.cell(x, y);
            match word(data, self.grid + i as u64 / 16) >> (2 * (i % 16)) & 3 {
                OUTSIDE => return false,
                INSIDE => return true,
                _ => {}
            }
        }
        self.edges_contain(data, x, y)
    }

    fn edges_contain(&self, data: &[u8], x: f64, y: f64) -> bool {
        let k = self.slabs as usize;
        let s = (((y - self.ymin) * self.inv_h).max(0.0) as usize).min(k - 1) as u64;
        let offsets = self.at + 2 * u64::from(self.vertices);
        let edges = offsets + k as u64 + 1;
        let vertex = |v: u64| {
            let at = self.at + 2 * v;
            [at, at + 1].map(|i| word(data, i) as i32 as f64 / FIXED)
        };
        let mut inside = false;
        for j in word(data, offsets + s)..word(data, offsets + s + 1) {
            let e = u64::from(word(data, edges + u64::from(j)));
            let ([xa, ya], [xb, yb]) = (vertex(e), vertex(e + 1));
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
        self.vertices as usize
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

/// 2D WKB LineString, into `out`.
pub fn wkb_line(buf: &[u8], out: &mut Vec<[f64; 2]>) -> Option<()> {
    out.clear();
    let mut r = Wkb(buf, 0);
    let (le, kind) = r.header()?;
    (kind == 2).then_some(())?;
    for _ in 0..r.u32(le)? {
        out.push([r.f64(le)?, r.f64(le)?]);
    }
    Some(())
}

pub const KINDS: [&str; 6] = [
    "locality",
    "localadmin",
    "county",
    "region",
    "dependency",
    "country",
];

fn canonical_country(cc: [u8; 2]) -> [u8; 2] {
    if matches!(&cc, b"XW" | b"XG") {
        return *b"PS";
    }
    let lower = cc.map(|c| c.to_ascii_lowercase());
    std::str::from_utf8(&lower)
        .ok()
        .and_then(opengeocoding::index::sovereign)
        .map_or(cc, |s| s.as_bytes().try_into().unwrap())
        .map(|c| c.to_ascii_uppercase())
}
const NEAR_LOCALITY_M: f32 = 5000.0;

/// Polygons by grid cell, each in the finest of `LEVELS` grids whose cells are at least as large
/// as its bounding box, so a point's candidates are its cell's at each level: a few lookups where
/// an R-tree of overlapping boxes walked many nodes.
struct Levels(Vec<Level>);

struct Level {
    size: f64,
    cols: usize,
    rows: usize,
    /// Each cell's polygons are `ids[offsets[cell]..offsets[cell + 1]]`.
    offsets: Vec<u32>,
    ids: Vec<u32>,
}

/// The finest level's cell, in degrees; each next level's is `LEVEL_STEP` times larger, and the
/// last holds the world in one cell.
const BASE_CELL: f64 = 0.2;
const LEVEL_STEP: f64 = 4.0;
const LEVELS: usize = 6;

impl Level {
    fn new(size: f64) -> Level {
        let (cols, rows) = (
            (360.0 / size).ceil() as usize,
            (180.0 / size).ceil() as usize,
        );
        Level {
            size,
            cols,
            rows,
            offsets: vec![],
            ids: vec![],
        }
    }

    fn col(&self, x: f64) -> usize {
        (((x + 180.0) / self.size).floor().max(0.0) as usize).min(self.cols - 1)
    }

    fn row(&self, y: f64) -> usize {
        (((y + 90.0) / self.size).floor().max(0.0) as usize).min(self.rows - 1)
    }

    fn at(&self, x: f64, y: f64) -> &[u32] {
        let cell = self.row(y) * self.cols + self.col(x);
        &self.ids[self.offsets[cell] as usize..self.offsets[cell + 1] as usize]
    }
}

impl Levels {
    fn new(polys: &[Poly]) -> Levels {
        let mut levels: Vec<Level> = (0..LEVELS)
            .map(|l| Level::new(BASE_CELL * LEVEL_STEP.powi(l as i32)))
            .collect();
        levels.push(Level::new(360.0));
        // Each polygon's level and the cells its box spans there.
        let spans: Vec<(usize, Vec<usize>)> = polys
            .iter()
            .map(|p| {
                let [x0, y0, x1, y1] = p.bbox;
                let side = (x1 - x0).max(y1 - y0);
                let k = levels.iter().position(|l| l.size >= side).unwrap_or(LEVELS);
                let l = &levels[k];
                let cells = (l.row(y0)..=l.row(y1))
                    .flat_map(|r| (l.col(x0)..=l.col(x1)).map(move |c| r * l.cols + c))
                    .collect();
                (k, cells)
            })
            .collect();
        let mut offsets: Vec<Vec<u32>> = levels
            .iter()
            .map(|l| vec![0; l.cols * l.rows + 1])
            .collect();
        for (k, cells) in &spans {
            for &cell in cells {
                offsets[*k][cell + 1] += 1;
            }
        }
        for o in &mut offsets {
            for i in 1..o.len() {
                o[i] += o[i - 1];
            }
        }
        let mut ids: Vec<Vec<u32>> = offsets
            .iter()
            .map(|o| vec![0; *o.last().unwrap() as usize])
            .collect();
        let mut fill = offsets.clone();
        for (i, (k, cells)) in spans.iter().enumerate() {
            for &cell in cells {
                ids[*k][fill[*k][cell] as usize] = i as u32;
                fill[*k][cell] += 1;
            }
        }
        for ((l, o), ids) in levels.iter_mut().zip(offsets).zip(ids) {
            (l.offsets, l.ids) = (o, ids);
        }
        Levels(levels)
    }

    /// The polygons whose bounding boxes hold the point (edges included).
    fn at<'p>(&'p self, polys: &'p [Poly], x: f64, y: f64) -> impl Iterator<Item = &'p Poly> {
        self.0
            .iter()
            .flat_map(move |l| l.at(x, y))
            .map(|&i| &polys[i as usize])
            .filter(move |p| {
                let [x0, y0, x1, y1] = p.bbox;
                x0 <= x && x <= x1 && y0 <= y && y <= y1
            })
    }
}

pub struct Assigner {
    polys: Vec<Poly>,
    data: OwnedBytes,
    levels: Levels,
    towns: RTree<GeomWithData<[f64; 2], u32>>,
    countries: Vec<[u8; 2]>,
    sovereigns: Vec<[u8; 2]>,
    /// `[min x, min y, max x, max y]`: nothing outside is placed, so nothing outside is indexed.
    pub bbox: Option<[f64; 4]>,
}

impl Assigner {
    pub fn new(
        polys: Vec<Poly>,
        file: PolyFile,
        towns: Vec<([f64; 2], u32)>,
        countries: Vec<([u8; 2], [u8; 2])>,
    ) -> Assigner {
        let levels = Levels::new(&polys);
        let towns = RTree::bulk_load(
            towns
                .into_iter()
                .map(|(p, i)| GeomWithData::new(p, i))
                .collect(),
        );
        let (countries, sovereigns) = countries
            .into_iter()
            .map(|(cc, sovereign)| (canonical_country(cc), canonical_country(sovereign)))
            .unzip();
        Assigner {
            polys,
            data: file.map(),
            levels,
            towns,
            countries,
            sovereigns,
            bbox: None,
        }
    }

    pub fn within(&self, x: f64, y: f64) -> bool {
        self.bbox
            .is_none_or(|b| b[0] <= x && x <= b[2] && b[1] <= y && y <= b[3])
    }

    pub fn place(&self, x: f64, y: f64) -> Option<u32> {
        self.place_in(x, y, None)
    }

    pub fn region(&self, x: f64, y: f64, cc: [u8; 2]) -> Option<u32> {
        let mut regions = self.levels.at(&self.polys, x, y).filter_map(|p| {
            (p.kind == 3
                && self.countries[p.place as usize] == canonical_country(cc)
                && p.contains(&self.data, x, y))
            .then_some(p.place)
        });
        let first = regions.next()?;
        regions.all(|p| p == first).then_some(first)
    }

    /// The most specific administration above a locality whose polygon holds the point.
    pub fn admin(&self, x: f64, y: f64) -> Option<u32> {
        self.levels
            .at(&self.polys, x, y)
            .filter(|p| p.kind != 0 && p.contains(&self.data, x, y))
            .min_by(|a, b| {
                a.kind
                    .cmp(&b.kind)
                    .then(a.area().total_cmp(&b.area()))
                    .then(a.place.cmp(&b.place))
            })
            .map(|p| p.place)
    }

    /// Source country, else the containing administration, constrains every settlement candidate.
    pub fn place_in(&self, x: f64, y: f64, cc: Option<[u8; 2]>) -> Option<u32> {
        if !self.within(x, y) {
            return None;
        }
        let containing = || {
            self.levels
                .at(&self.polys, x, y)
                .filter(|p| p.contains(&self.data, x, y))
        };
        let cc = cc
            .map(canonical_country)
            .or_else(|| self.admin(x, y).map(|p| self.countries[p as usize]));
        let allowed = |p: u32| {
            cc.is_none_or(|c| c == self.countries[p as usize] || c == self.sovereigns[p as usize])
        };
        let mut pick: [Option<(f64, u32)>; 6] = [None; 6];
        for p in containing() {
            let k = p.kind as usize;
            let key = (p.area(), p.place);
            if allowed(p.place) && pick[k].is_none_or(|old| key < old) {
                pick[k] = Some(key);
            }
        }
        if let Some((_, p)) = pick[0] {
            return Some(p);
        }
        // ponytail: degree-ordered neighbours inside a 5 km envelope; scale the tree by cos(lat)
        // if ordering at high latitudes becomes material.
        let dx = NEAR_LOCALITY_M as f64 / 111_195.0 / y.to_radians().cos().abs().max(1e-6);
        let dy = NEAR_LOCALITY_M as f64 / 111_195.0;
        let envelope = rstar::AABB::from_corners([x - dx, y - dy], [x + dx, y + dy]);
        if let Some(t) = self
            .towns
            .locate_in_envelope(envelope)
            .filter(|t| allowed(t.data))
            .filter_map(|t| {
                let [tx, ty] = *t.geom();
                let distance = ((tx - x) * y.to_radians().cos()).hypot(ty - y);
                (distance <= dy).then_some(((tx - x).hypot(ty - y), t.data))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        {
            return Some(t.1);
        }
        pick[1..3].iter().flatten().next().map(|&(_, p)| p)
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

    fn file(name: &str) -> PolyFile {
        let name = format!("polys-{}-{name}", std::process::id());
        PolyFile::new(std::env::temp_dir().join(name))
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
        let f = file("hole");
        let skipped = Poly::new(0, 0, square(0.0, 0.0, 0.0), &[(0, 5)], &f);
        assert!(skipped.is_none(), "no area");
        let p = Poly::new(0, 0, xy, &rings, &f).unwrap();
        let data = f.map();
        let contains = |x, y| p.contains(&data, x, y);
        assert!(contains(1.0, 1.0));
        assert!(contains(9.9, 9.9));
        assert!(!contains(5.0, 5.0), "inside the hole");
        assert!(contains(20.5, 20.5), "second part");
        assert!(!contains(15.0, 15.0), "between parts");
        assert!(!contains(-1.0, 5.0));
    }

    #[test]
    fn grid_cells_answer_as_the_edges_do() {
        // xorshift, so the shapes and points are the same each run
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut rand = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let f = file("grid");
        let mut polys = vec![];
        for n in 0..40 {
            // a jagged ring around a centre, a hole in it, and a far part joined by long edges
            let (cx, cy, r) = (
                rand() * 40.0 - 20.0,
                rand() * 40.0 - 20.0,
                0.5 + rand() * 5.0,
            );
            let k = 8 + (rand() * 400.0) as usize;
            let ring = |r: f64, rand: &mut dyn FnMut() -> f64| -> Vec<[f64; 2]> {
                let mut v: Vec<[f64; 2]> = (0..k)
                    .map(|i| {
                        let a = i as f64 / k as f64 * std::f64::consts::TAU;
                        let d = r * (0.3 + 0.7 * rand());
                        [cx + d * a.cos(), cy + d * a.sin()]
                    })
                    .collect();
                v.push(v[0]);
                v
            };
            let mut xy = ring(r, &mut rand);
            let mut rings = vec![(0, xy.len() as u32)];
            let hole = ring(r * 0.1, &mut rand);
            rings.push((xy.len() as u32, (xy.len() + hole.len()) as u32));
            xy.extend(hole);
            if n % 3 == 0 {
                let s = xy.len() as u32;
                let (fx, fy) = (cx + 30.0, cy - 25.0);
                xy.extend([
                    [fx, fy],
                    [fx + 1.0, fy + 0.3],
                    [cx + 1.0, cy + 2.0],
                    [fx, fy],
                ]);
                rings.push((s, xy.len() as u32));
            }
            polys.extend(Poly::new(n, 0, xy, &rings, &f));
        }
        let data = f.map();
        let (mut inside, mut by_cell) = (0, 0);
        for p in &polys {
            let [x0, y0, x1, y1] = p.bbox;
            for i in 0..20_000 {
                let (mut x, mut y) = (x0 + rand() * (x1 - x0), y0 + rand() * (y1 - y0));
                if i % 4 == 0 {
                    // on a cell's edge or corner
                    x = x0 + (((x - x0) * p.inv_cw).floor()) / p.inv_cw;
                    y = y0 + (((y - y0) * p.inv_ch).round()) / p.inv_ch;
                }
                let want = p.edges_contain(&data, x, y);
                assert_eq!(p.contains(&data, x, y), want, "{x} {y}");
                inside += usize::from(want);
                let i = p.cell(x, y);
                by_cell +=
                    usize::from(word(&data, p.grid + i as u64 / 16) >> (2 * (i % 16)) & 3 != EDGE);
            }
        }
        assert!(inside > 100_000 && by_cell > 200_000, "{inside} {by_cell}");
        let levels = Levels::new(&polys);
        for _ in 0..100_000 {
            let (x, y) = (rand() * 120.0 - 60.0, rand() * 120.0 - 60.0);
            let mut got: Vec<u32> = levels.at(&polys, x, y).map(|p| p.place).collect();
            let mut want: Vec<u32> = polys
                .iter()
                .filter(|p| p.bbox[0] <= x && x <= p.bbox[2] && p.bbox[1] <= y && y <= p.bbox[3])
                .map(|p| p.place)
                .collect();
            got.sort_unstable();
            want.sort_unstable();
            assert_eq!(got, want);
        }
    }

    #[test]
    fn countries_constrain_polygons_and_point_fallbacks_without_splitting_sovereigns() {
        let s = square(0.0, 0.0, 1.0);
        let f = file("countries");
        let a = Assigner::new(
            vec![Poly::new(0, 2, s.clone(), &[(0, s.len() as u32)], &f).unwrap()],
            f,
            vec![([0.501, 0.5], 1), ([0.53, 0.5], 2)],
            vec![(*b"AT", *b"AT"), (*b"LI", *b"LI"), (*b"AT", *b"AT")],
        );
        assert_eq!(a.place(0.5, 0.5), Some(2));
        assert_eq!(a.place_in(0.5, 0.5, Some(*b"AT")), Some(2));
        assert_eq!(a.place_in(0.5, 0.5, Some(*b"LI")), Some(1));
        assert_eq!(a.place(0.9, 0.9), Some(0));
        assert_eq!(canonical_country(*b"GF"), canonical_country(*b"FR"));
        assert_eq!(canonical_country(*b"PR"), canonical_country(*b"US"));
        assert_ne!(canonical_country(*b"GL"), canonical_country(*b"DK"));
        assert_ne!(canonical_country(*b"VG"), canonical_country(*b"GB"));
        assert_eq!(canonical_country(*b"XW"), canonical_country(*b"PS"));
        assert_eq!(canonical_country(*b"XG"), canonical_country(*b"PS"));
        let territory = Assigner::new(
            vec![],
            file("territory"),
            vec![([0.0, 0.0], 0)],
            vec![(*b"AX", *b"FI")],
        );
        assert_eq!(territory.place_in(0.0, 0.0, Some(*b"FI")), Some(0));
        assert_eq!(territory.countries[0], *b"AX");
    }

    #[test]
    fn smallest_locality_then_nearby_town_then_admin() {
        let f = file("nested");
        let poly = |place, kind, s: Vec<[f64; 2]>| {
            Poly::new(place, kind, s.clone(), &[(0, s.len() as u32)], &f).unwrap()
        };
        let a = Assigner::new(
            vec![
                poly(1, 0, square(0.0, 0.0, 1.0)),
                poly(2, 0, square(0.0, 0.0, 0.1)),
                poly(3, 2, square(-5.0, -5.0, 20.0)),
            ],
            f,
            vec![([3.0, 3.0], 4)],
            vec![(*b"AT", *b"AT"); 5],
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
