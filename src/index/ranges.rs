//! US street address ranges (Census TIGER/Line address features), for a house the index lacks:
//! the point is interpolated along the street segment whose range holds the number. A
//! range-interpolated point is approximate (hundreds of metres at worst), never a surveyed house.
//!
//! Two memory-mapped files: `us-ranges.fst` maps `zip5 NUL folded street name` to an offset into
//! `us-ranges.bin`, which holds per key a varint count and that many segments. A segment is a
//! flags byte (bit 0/1: left/right side present; bits 2-3 / 4-5: parity of the left / right side,
//! 1 odd, 2 even, 0 any), each present side's first and last number as varints, the vertex count,
//! then the vertices as 1e-6 degree integers: the first in full, the rest as zigzag varint
//! deltas. The sides' numbers run in the direction of the vertices; left is left of that
//! direction.
use super::{Index, schema::fold};
use tantivy::directory::OwnedBytes;
use tantivy_fst::Map;

pub const FST_FILE: &str = "us-ranges.fst";
pub const BLOB_FILE: &str = "us-ranges.bin";
const FIXED: f64 = 1e6;
/// How far from the street's centreline the point is put, towards the side holding the number.
const SETBACK_M: f64 = 12.0;
const M_PER_DEG: f64 = 111_320.0;

/// A side's numbers, first to last, and its parity (1 odd, 2 even, 0 any).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Side {
    pub from: u32,
    pub to: u32,
    pub parity: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub sides: [Option<Side>; 2],
    pub points: Vec<[i32; 2]>,
}

pub fn key(zip: &str, street: &str) -> String {
    format!("{zip}\0{}", fold(street))
}

pub fn fixed(degrees: f64) -> i32 {
    (degrees * FIXED).round() as i32
}

fn put(out: &mut Vec<u8>, mut v: u32) {
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn get(b: &[u8], at: &mut usize) -> Option<u32> {
    let (mut v, mut shift) = (0u32, 0);
    loop {
        let byte = *b.get(*at)?;
        *at += 1;
        v |= u32::from(byte & 0x7f).checked_shl(shift)?;
        if byte < 0x80 {
            return Some(v);
        }
        shift += 7;
        if shift > 28 {
            return None;
        }
    }
}

fn zigzag(v: i32) -> u32 {
    ((v << 1) ^ (v >> 31)) as u32
}

fn unzigzag(v: u32) -> i32 {
    (v >> 1) as i32 ^ -((v & 1) as i32)
}

pub fn encode(sides: &[Option<Side>; 2], points: &[[i32; 2]], out: &mut Vec<u8>) {
    let mut flags = 0u8;
    for (i, side) in sides.iter().enumerate() {
        if let Some(side) = side {
            flags |= 1 << i | side.parity << (2 + 2 * i);
        }
    }
    out.push(flags);
    for side in sides.iter().flatten() {
        put(out, side.from);
        put(out, side.to);
    }
    put(out, points.len() as u32);
    let mut last = [0i32; 2];
    for p in points {
        for axis in 0..2 {
            put(out, zigzag(p[axis].wrapping_sub(last[axis])));
        }
        last = *p;
    }
}

fn decode(b: &[u8], at: &mut usize) -> Option<Segment> {
    let flags = *b.get(*at)?;
    *at += 1;
    let mut sides = [None; 2];
    for (i, side) in sides.iter_mut().enumerate() {
        if flags >> i & 1 == 1 {
            *side = Some(Side {
                from: get(b, at)?,
                to: get(b, at)?,
                parity: flags >> (2 + 2 * i) & 3,
            });
        }
    }
    let n = get(b, at)? as usize;
    let mut points = Vec::with_capacity(n.min(4096));
    let mut last = [0i32; 2];
    for _ in 0..n {
        for v in &mut last {
            *v = v.wrapping_add(unzigzag(get(b, at)?));
        }
        points.push(last);
    }
    Some(Segment { sides, points })
}

pub struct Ranges {
    map: Map<OwnedBytes>,
    blob: OwnedBytes,
}

impl Ranges {
    pub fn new(fst: OwnedBytes, blob: OwnedBytes) -> anyhow::Result<Self> {
        Ok(Self {
            map: Map::from(tantivy_fst::raw::Fst::new(fst)?),
            blob,
        })
    }

    /// Where house `number` stands on the street named `street` (any spelling the index folds
    /// alike) in ZIP code `zip`: along the narrowest range that holds it, on its side of the
    /// street. `None` when no segment's range holds the number.
    pub fn locate(&self, zip: &str, street: &str, number: u32) -> Option<(f64, f64)> {
        let b = self.blob.as_slice();
        let mut at = self.map.get(key(zip, street))? as usize;
        let mut best: Option<(u32, Segment, usize)> = None;
        for _ in 0..get(b, &mut at)? {
            let segment = decode(b, &mut at)?;
            for (i, side) in segment.sides.iter().enumerate() {
                let Some(side) = side else { continue };
                let (lo, hi) = (side.from.min(side.to), side.from.max(side.to));
                let parity_ok = match side.parity {
                    1 => number % 2 == 1,
                    2 => number.is_multiple_of(2),
                    _ => true,
                };
                if parity_ok
                    && (lo..=hi).contains(&number)
                    && best.as_ref().is_none_or(|(span, ..)| hi - lo < *span)
                {
                    best = Some((hi - lo, segment.clone(), i));
                }
            }
        }
        let (_, segment, i) = best?;
        let side = segment.sides[i]?;
        let fraction = if side.from == side.to {
            0.5
        } else {
            (i64::from(number) - i64::from(side.from)) as f64
                / (i64::from(side.to) - i64::from(side.from)) as f64
        };
        along(&segment.points, fraction.clamp(0.0, 1.0), i == 0)
    }
}

/// The point `fraction` of the way along the line, `SETBACK_M` to its left or right.
fn along(points: &[[i32; 2]], fraction: f64, left: bool) -> Option<(f64, f64)> {
    let first = points.first()?;
    let lat0 = f64::from(first[1]) / FIXED;
    let scale = lat0.to_radians().cos().max(0.01);
    // metres east, north of the first vertex
    let metres = |p: &[i32; 2]| {
        (
            f64::from(p[0] - first[0]) / FIXED * M_PER_DEG * scale,
            f64::from(p[1] - first[1]) / FIXED * M_PER_DEG,
        )
    };
    let lengths: Vec<f64> = points
        .windows(2)
        .map(|w| {
            let (a, b) = (metres(&w[0]), metres(&w[1]));
            (b.0 - a.0).hypot(b.1 - a.1)
        })
        .collect();
    let total: f64 = lengths.iter().sum();
    let (mut remaining, mut i) = (fraction * total, 0);
    while i + 1 < lengths.len() && remaining > lengths[i] {
        remaining -= lengths[i];
        i += 1;
    }
    let Some(&length) = lengths.get(i).filter(|l| **l > 0.0) else {
        return Some((lat0, f64::from(first[0]) / FIXED));
    };
    let (a, b) = (metres(&points[i]), metres(&points[i + 1]));
    let u = remaining / length;
    let (dx, dy) = ((b.0 - a.0) / length, (b.1 - a.1) / length);
    let side = if left { SETBACK_M } else { -SETBACK_M };
    let (x, y) = (
        a.0 + u * (b.0 - a.0) - dy * side,
        a.1 + u * (b.1 - a.1) + dx * side,
    );
    Some((
        lat0 + y / M_PER_DEG,
        f64::from(first[0]) / FIXED + x / (M_PER_DEG * scale),
    ))
}

impl Index {
    /// Where US house `number` of `street` in ZIP code `zip` stands by its street's address range;
    /// `None` without ranges in the index, or when no segment's range holds the number.
    pub fn us_range_point(&self, zip: &str, street: &str, number: u32) -> Option<(f64, f64)> {
        self.us_ranges.as_ref()?.locate(zip, street, number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn built(entries: &[(&str, &str, Vec<Segment>)]) -> Ranges {
        let mut sorted: Vec<(String, &Vec<Segment>)> = entries
            .iter()
            .map(|(zip, street, s)| (key(zip, street), s))
            .collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        let (mut blob, mut fst) = (vec![], tantivy_fst::MapBuilder::memory());
        for (k, segments) in sorted {
            fst.insert(k, blob.len() as u64).unwrap();
            put(&mut blob, segments.len() as u32);
            for s in segments {
                encode(&s.sides, &s.points, &mut blob);
            }
        }
        Ranges::new(
            OwnedBytes::new(fst.into_inner().unwrap()),
            OwnedBytes::new(blob),
        )
        .unwrap()
    }

    /// A block going east along 40 degrees north, 0.001 degree (85 m) long.
    fn east(left: Option<Side>, right: Option<Side>) -> Segment {
        Segment {
            sides: [left, right],
            points: vec![[fixed(-74.0), fixed(40.0)], [fixed(-73.999), fixed(40.0)]],
        }
    }

    #[test]
    fn segments_survive_the_encoding() {
        let s = Segment {
            sides: [
                Some(Side {
                    from: 1,
                    to: 99,
                    parity: 1,
                }),
                Some(Side {
                    from: 300,
                    to: 2,
                    parity: 2,
                }),
            ],
            points: vec![
                [-74_000_000, 40_000_000],
                [-74_000_100, 39_999_900],
                [-73_999_000, 40_000_007],
            ],
        };
        let (mut out, mut at) = (vec![], 0);
        encode(&s.sides, &s.points, &mut out);
        assert_eq!(decode(&out, &mut at), Some(s));
        assert_eq!(at, out.len());
        assert_eq!(decode(&out[..out.len() - 1], &mut 0), None);
    }

    #[test]
    fn a_number_is_placed_along_its_block_on_its_side() {
        let odd = Side {
            from: 1,
            to: 99,
            parity: 1,
        };
        let even = Side {
            from: 2,
            to: 100,
            parity: 2,
        };
        let r = built(&[("08540", "N Main Street", vec![east(Some(even), Some(odd))])]);
        // any spelling of the street the index folds alike
        let (lat, lon) = r.locate("08540", "North Main St", 51).unwrap();
        // the middle of the block, odd numbers on the right: south of an eastbound line
        assert!((lon - -73.9995).abs() < 2e-5, "{lon}");
        assert!(lat < 40.0 && 40.0 - lat < 2e-4, "{lat}");
        let (lat, _) = r.locate("08540", "North Main St", 50).unwrap();
        assert!(lat > 40.0, "even numbers are on the left");
        let (_, lon) = r.locate("08540", "North Main St", 1).unwrap();
        assert!((lon - -74.0).abs() < 2e-5, "{lon}");
    }

    #[test]
    fn a_number_outside_every_range_or_of_the_wrong_zip_is_not_placed() {
        let odd = Side {
            from: 1,
            to: 99,
            parity: 1,
        };
        let r = built(&[("08540", "Main St", vec![east(None, Some(odd))])]);
        assert!(r.locate("08540", "Main St", 101).is_none());
        assert!(
            r.locate("08540", "Main St", 50).is_none(),
            "even number on an odd side"
        );
        assert!(r.locate("08541", "Main St", 51).is_none());
        assert!(r.locate("08540", "Elm St", 51).is_none());
    }

    #[test]
    fn the_narrowest_range_wins_and_a_reversed_range_runs_backwards() {
        let wide = Side {
            from: 1,
            to: 999,
            parity: 0,
        };
        let narrow = Side {
            from: 200,
            to: 100,
            parity: 0,
        };
        let r = built(&[(
            "1",
            "A St",
            vec![east(Some(wide), None), east(None, Some(narrow))],
        )]);
        let (_, lon) = r.locate("1", "A St", 200).unwrap();
        assert!(
            (lon - -74.0).abs() < 2e-5,
            "200 is the first end of the narrow range: {lon}"
        );
    }
}
