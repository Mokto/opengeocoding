//! Locality points by position, so a coordinate finds the nearest one without a scan. The points
//! are sorted by 0.05° grid cell, row by row, so one row of a box is one binary-searched range.
//! Built at startup from the place table; it is not part of the index files.
use rayon::prelude::*;

const CELL_DEGREES: f64 = 0.05;
const COLUMNS: u32 = (360.0 / CELL_DEGREES) as u32 + 1;
const METERS_PER_DEGREE: f64 = 111_195.0;

pub struct Localities {
    /// (cell, place id), sorted; cell = row * COLUMNS + column.
    cells: Vec<(u32, u32)>,
}

fn cell(lon: f64, lat: f64) -> (u32, u32) {
    let at = |v: f64, max: f64| ((v / CELL_DEGREES).floor() as i64).clamp(0, max as i64) as u32;
    (
        at(lon + 180.0, 360.0 / CELL_DEGREES),
        at(lat + 90.0, 180.0 / CELL_DEGREES),
    )
}

impl Localities {
    /// `points` are (place id, longitude, latitude).
    pub fn new(points: impl Iterator<Item = (u32, f64, f64)>) -> Localities {
        let mut cells: Vec<(u32, u32)> = points
            .map(|(id, lon, lat)| {
                let (column, row) = cell(lon, lat);
                (row * COLUMNS + column, id)
            })
            .collect();
        cells.par_sort_unstable();
        Localities { cells }
    }

    /// The place nearest (`lat`, `lon`) within `max_m` metres that `accept`s, by `position`'s
    /// (lon, lat), with its distance in metres. The box doubles until the circle in it holds the
    /// nearest point. Columns past the antimeridian aren't searched.
    pub fn nearest(
        &self,
        lat: f64,
        lon: f64,
        max_m: f64,
        position: impl Fn(u32) -> (f64, f64),
        accept: impl Fn(u32) -> bool,
    ) -> Option<(u32, f64)> {
        self.nearest_by_reach(lat, lon, max_m, 1.0, |id| (position(id), 1.0), accept)
    }

    /// As `nearest`, but by distance divided by each place's reach (1 to `max_reach`), which
    /// `place` gives with its (lon, lat): a place twice another's reach wins at twice its distance.
    pub fn nearest_by_reach(
        &self,
        lat: f64,
        lon: f64,
        max_m: f64,
        max_reach: f64,
        place: impl Fn(u32) -> ((f64, f64), f64),
        accept: impl Fn(u32) -> bool,
    ) -> Option<(u32, f64)> {
        let (column, row) = cell(lon, lat);
        let cos = lat.to_radians().cos().max(0.01);
        let metres = |x: f64, y: f64| ((x - lon) * cos).hypot(y - lat) * METERS_PER_DEGREE;
        let cell_m = CELL_DEGREES * METERS_PER_DEGREE;
        let mut reach = 500.0f64;
        loop {
            let half = reach.min(max_m);
            let rows = (half / cell_m).ceil() as u32;
            let columns = (half / (cell_m * cos)).ceil() as u32;
            let (first, last) = (
                column.saturating_sub(columns),
                (column + columns).min(COLUMNS - 1),
            );
            // (distance / reach, distance, place)
            let mut best: Option<(f64, f64, u32)> = None;
            for r in row.saturating_sub(rows)..=row + rows {
                let (from, to) = (r * COLUMNS + first, r * COLUMNS + last);
                let start = self.cells.partition_point(|&(c, _)| c < from);
                for &(c, id) in &self.cells[start..] {
                    if c > to {
                        break;
                    }
                    let ((x, y), reach) = place(id);
                    let m = metres(x, y);
                    let score = m / reach;
                    if m <= max_m && best.is_none_or(|(b, ..)| score < b) && accept(id) {
                        best = Some((score, m, id));
                    }
                }
            }
            // every place outside the circle is more than `half` away: it scores above half / max_reach
            if let Some((score, m, id)) = best
                && (score * max_reach <= half || half >= max_m)
            {
                return Some((id, m));
            }
            if half >= max_m {
                return None;
            }
            reach *= 2.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_nearest_point_within_range() {
        let points = [
            (0u32, 10.0, 50.0),
            (1, 10.02, 50.0),
            (2, 10.3, 50.0),
            (3, -179.99, 0.0),
        ];
        let found = Localities::new(points.iter().copied());
        let at = |id: u32| (points[id as usize].1, points[id as usize].2);
        let id = |lat, lon, max_m| found.nearest(lat, lon, max_m, at, |_| true).map(|f| f.0);
        assert_eq!(id(50.0, 10.015, 50_000.0), Some(1));
        assert_eq!(id(50.0, 10.001, 50_000.0), Some(0));
        // 0.3° of longitude at 50° is 21 km: only reached once the box has grown
        assert_eq!(id(50.0, 10.25, 50_000.0), Some(2));
        assert_eq!(id(50.0, 10.25, 1_000.0), None);
        assert_eq!(id(0.0, -180.0, 50_000.0), Some(3));
        assert_eq!(id(-60.0, 100.0, 50_000.0), None);
        assert_eq!(id(90.0, 0.0, 50_000.0), None);
        // a place `accept` refuses is skipped, and the distance is the accepted one's
        let (skipped, m) = found
            .nearest(50.0, 10.001, 50_000.0, at, |p| p != 0)
            .unwrap();
        assert_eq!(skipped, 1);
        assert!((m - 0.019 * 0.6428 * 111_195.0).abs() < 50.0, "{m}");
    }

    #[test]
    fn a_place_of_more_reach_wins_further_away() {
        // (village, town of reach 2, a point the village names, one the town names)
        for (village, town, at, nearer) in [
            ((10.0, 50.0), (10.03, 50.0), (50.0, 10.008), (50.0, 10.012)),
            // at 49.97 the village, 7.8 km away, is read before the town, 15 km
            ((10.0, 50.04), (10.0, 49.835), (49.99, 10.0), (49.97, 10.0)),
        ] {
            let points = [(0u32, village.0, village.1), (1, town.0, town.1)];
            let found = Localities::new(points.iter().copied());
            let reach = |id: u32| {
                let p = points[id as usize];
                ((p.1, p.2), if id == 0 { 1.0 } else { 2.0 })
            };
            let id = |(lat, lon)| found.nearest_by_reach(lat, lon, 50_000.0, 2.0, reach, |_| true);
            assert_eq!(id(at).map(|f| f.0), Some(0), "{at:?}");
            let (place, m) = id(nearer).unwrap();
            assert_eq!(place, 1, "{nearer:?}");
            let (x, y) = (town.0 - nearer.1, town.1 - nearer.0);
            assert!((m - (x * 0.6428).hypot(y) * 111_195.0).abs() < 50.0, "{m}");
        }
    }
}
