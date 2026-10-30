//! GeoNames populated places (the `P` rows of `allCountries`, and `admin1CodesASCII`) added to
//! the places Overture's divisions give, only where no place of that name lies within `NEAR_KM`.
use crate::{NAME_SEP, Place, Str, cell, country, geo::Assigner};
use opengeocoding::index::schema::{ALTERNATES, SMALL_GEONAMES, fold};
use rayon::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, Write},
    path::Path,
};

const NEAR_KM: f64 = 10.0;

/// Feature codes of places that no longer exist, are historical, or are religious sites.
const SKIPPED: [&str; 5] = ["PPLH", "PPLQ", "PPLW", "PPLCH", "PPLR"];

struct Row {
    /// The source's primary name and its ASCII form: what the place answers to.
    names: Vec<String>,
    /// Every other name, used only to see whether Overture has the place. GeoNames' alternates
    /// include labels such as "Huy (Tihange)" and the names of the place's district, which
    /// would answer queries for other places.
    others: Vec<String>,
    cc: [u8; 2],
    admin1: String,
    population: u64,
    subtype: &'static str,
    /// The seat of a second-order division (`PPLA2`): a district's or county's town.
    district_seat: bool,
    x: f64,
    y: f64,
    /// Meets the bar of GeoNames' `cities500`: more than 500 people, a capital or the seat of a
    /// first- to fourth-order division. A smaller place only fills a gap no namesake shares.
    listed: bool,
}

fn area(p: &Place) -> bool {
    matches!(
        p.subtype,
        "country" | "dependency" | "macroregion" | "region" | "macrocounty" | "county"
    )
}

fn parse(line: &str) -> Option<Row> {
    let f: Vec<&str> = line.split('\t').collect();
    if f.len() < 15 || SKIPPED.contains(&f[7]) || f[1].is_empty() {
        return None;
    }
    let population = f[14].parse().unwrap_or(0);
    let listed = population > 500 || matches!(f[7], "PPLC" | "PPLA" | "PPLA2" | "PPLA3" | "PPLA4");
    // a smaller section of a populated place
    if !listed && f[7] == "PPLX" {
        return None;
    }
    let mut names: Vec<String> = vec![f[1].to_string()];
    if f[2] != f[1] && !f[2].is_empty() {
        names.push(f[2].to_string());
    }
    let others = f[3]
        .split(',')
        .map(str::trim)
        .filter(|n| n.chars().any(|c| !c.is_ascii_digit()) && !names.iter().any(|a| a == n))
        .map(str::to_string)
        .collect();
    Some(Row {
        names,
        others,
        cc: country(Some(f[8])),
        admin1: format!("{}.{}", f[8], f[10]),
        population,
        listed,
        district_seat: f[7] == "PPLA2",
        subtype: if f[7] == "PPLX" {
            "neighborhood"
        } else {
            "locality"
        },
        y: f[4].parse().ok()?,
        x: f[5].parse().ok()?,
    })
}

/// What a row finds: a place outside the box, places it names (the one it can add its names to,
/// with the indices and folded forms of those the place lacks), or none, and its parent.
enum Seen {
    Outside,
    Known(Option<(u32, Vec<(usize, String)>)>),
    New(Option<u32>),
}

fn km(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0) * a.1.to_radians().cos()).hypot(a.1 - b.1) * 111.195
}

/// Appends the places GeoNames has and `places` lacks, to `places` and to `places.tsv` in `out`.
/// A place below `cities500`'s bar is added only when nothing else of its country (no place, no
/// other GeoNames row beyond `NEAR_KM`) has its name, and never names a place `places` has.
/// Returns how many were added, and how many of those were below the bar.
pub fn add_places(
    places: &mut Vec<Place>,
    assigner: &Assigner,
    cities: &Path,
    admin1: Option<&Path>,
    out: &Path,
) -> (usize, usize) {
    let text = std::fs::read_to_string(cities).expect("read GeoNames cities file");
    let rows: Vec<Row> = text.par_lines().filter_map(parse).collect();
    let admin1: HashMap<String, String> = admin1
        .map(|p| std::fs::read_to_string(p).expect("read GeoNames admin1 file"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            Some((f.next()?.to_string(), f.next()?.to_string()))
        })
        .collect();

    type Key = ([u8; 2], String);
    type Located = (u32, (f64, f64));
    type Spot = (usize, (f64, f64));
    let wanted: HashSet<Key> = rows
        .par_iter()
        .flat_map_iter(|r| r.names.iter().chain(&r.others).map(|n| (r.cc, fold(n))))
        .collect();
    let named: Vec<(Key, Located)> = places
        .par_iter()
        .enumerate()
        .filter_map(|(i, p)| p.xy.map(|xy| (i as u32, p, xy)))
        .flat_map_iter(|(id, p, xy)| {
            p.names
                .split(NAME_SEP)
                .map(|n| (p.country, fold(n)))
                .filter(|k| wanted.contains(k))
                .map(move |k| (k, (id, xy)))
        })
        .collect();
    let mut near: HashMap<Key, Vec<Located>> = HashMap::new();
    for (k, place) in named {
        near.entry(k).or_default().push(place);
    }
    let mut spots: HashMap<Key, Vec<Spot>> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        let mut keys: Vec<Key> = r.names.iter().map(|n| (r.cc, fold(n))).collect();
        keys.dedup();
        for k in keys {
            spots.entry(k).or_default().push((i, (r.x, r.y)));
        }
    }
    // Other names a row is known by, in the Latin script, for `alternates`.
    let mut others: HashMap<Key, Vec<Spot>> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        let own: Vec<String> = r.names.iter().map(|n| fold(n)).collect();
        for n in r.others.iter().filter(|n| latin(n)) {
            let k = (r.cc, fold(n));
            if !k.1.is_empty() && !own.contains(&k.1) {
                others.entry(k).or_default().push((i, (r.x, r.y)));
            }
        }
    }
    // an admin1 name finds its Overture region when no polygon holds the place
    let mut regions: HashMap<Key, u32> = HashMap::new();
    for (i, p) in places
        .iter()
        .enumerate()
        .filter(|(_, p)| p.subtype == "region")
    {
        for n in p.names.split(NAME_SEP) {
            regions.entry((p.country, fold(n))).or_insert(i as u32);
        }
    }

    let mut tsv = std::io::BufWriter::new(
        std::fs::OpenOptions::new()
            .append(true)
            .open(out.join("places.tsv"))
            .expect("open places.tsv"),
    );
    let before = places.len();
    let mut aliases: HashMap<u32, Vec<String>> = HashMap::new();
    // What each row finds is worked out in parallel; the names added to a place, which a later
    // row's names are checked against, are kept in order below.
    let seen: Vec<Seen> = rows
        .par_iter()
        .enumerate()
        .map(|(i, r)| {
            if !assigner.within(r.x, r.y) {
                return Seen::Outside;
            }
            let mut known: Vec<u32> = r
                .names
                .iter()
                .chain(&r.others)
                .filter_map(|n| near.get(&(r.cc, fold(n))))
                .flatten()
                .filter(|(_, xy)| km(*xy, (r.x, r.y)) < NEAR_KM)
                // A district named for its seat is not the seat: Overture has Thrissur only as
                // a county.
                .filter(|(id, _)| !(r.district_seat && area(&places[*id as usize])))
                .map(|(id, _)| *id)
                .collect();
            known.sort_unstable();
            known.dedup();
            // A small place of a name its country has elsewhere can't be told from the namesake;
            // of rows of one name within NEAR_KM, the first stands for the others.
            let shared = || {
                r.names.iter().any(|n| {
                    let k = (r.cc, fold(n));
                    near.contains_key(&k)
                        || spots[&k]
                            .iter()
                            .any(|&(j, xy)| j < i || (j != i && km(xy, (r.x, r.y)) >= NEAR_KM))
                })
            };
            if !r.listed && (!known.is_empty() || shared()) {
                return Seen::Known(None);
            }
            if known.is_empty() {
                let parent = assigner.place_in(r.x, r.y, Some(r.cc));
                // the nearest town a parent falls back to may lie across a region's line: the
                // region that holds the place's point rules
                let region = assigner.region(r.x, r.y, r.cc);
                let parent = match (parent, region) {
                    (Some(p), Some(g))
                        if places[p as usize].region != places[g as usize].region =>
                    {
                        Some(g)
                    }
                    (None, g) => g,
                    (p, _) => p,
                }
                .or_else(|| {
                    let name = admin1.get(&r.admin1)?;
                    regions.get(&(r.cc, fold(name))).copied()
                });
                return Seen::New(parent);
            }
            known.retain(|&id| places[id as usize].subtype == r.subtype);
            let [id] = known[..] else {
                return Seen::Known(None);
            };
            let has: HashSet<String> = places[id as usize]
                .names
                .split(NAME_SEP)
                .map(fold)
                .collect();
            let missing = (r.names.iter().enumerate())
                .map(|(i, n)| (i, fold(n)))
                .filter(|(_, f)| !has.contains(f))
                .collect();
            Seen::Known(Some((id, missing)))
        })
        .collect();
    let mut added: HashMap<u32, HashSet<String>> = HashMap::new();
    let mut small = 0;
    let mut target: Vec<Option<u32>> = vec![None; rows.len()];
    for (i, (r, seen)) in rows.into_iter().zip(seen).enumerate() {
        let parent = match seen {
            Seen::Outside | Seen::Known(None) => continue,
            Seen::Known(Some((id, missing))) => {
                target[i] = Some(id);
                let (p, added) = (&mut places[id as usize], added.entry(id).or_default());
                for (i, folded) in missing {
                    if added.insert(folded) {
                        let name = &r.names[i];
                        p.names = format!("{}{NAME_SEP}{name}", p.names).into();
                        aliases.entry(id).or_default().push(name.clone());
                    }
                }
                continue;
            }
            Seen::New(parent) => parent,
        };
        let (region, anc) = parent.map_or((Str::default(), Box::default()), |p| {
            let mut anc: Vec<u32> = places[p as usize].anc.to_vec();
            anc.push(p);
            anc.sort_unstable();
            anc.dedup();
            (places[p as usize].region.clone(), anc.into())
        });
        let pairs: Vec<String> = r.names[1..]
            .iter()
            .map(|n| format!("alt_name={}", cell(n)))
            .collect();
        small += usize::from(!r.listed);
        target[i] = Some(places.len() as u32);
        writeln!(
            tsv,
            "{}\t{}\t{}\t{}\t0\t{}\t{}\t{}\tNaN\tNaN\tNaN\tNaN\t{}\t{}\t{}\t",
            r.subtype,
            if r.listed { "geonames" } else { SMALL_GEONAMES },
            crate::country_str(&r.cc),
            region,
            r.population,
            r.x,
            r.y,
            parent.map_or(String::new(), |p| p.to_string()),
            cell(&r.names[0]),
            pairs.join(&NAME_SEP.to_string()),
        )
        .unwrap();
        places.push(Place {
            // ranked as "other" below every Overture locality (importance is population plus the
            // subtype's weight), so a gap filler never outranks the place Overture has
            subtype: "other",
            country: r.cc,
            region,
            names: r.names.join(&NAME_SEP.to_string()).into(),
            population: r.population.min(9_000),
            xy: Some((r.x, r.y)),
            anc,
        });
    }
    tsv.flush().unwrap();
    drop(tsv);
    if !aliases.is_empty() {
        let path = out.join("places.tsv");
        let updated = out.join("places.tsv.tmp");
        let input = std::io::BufReader::new(std::fs::File::open(&path).unwrap());
        let mut output = std::io::BufWriter::new(std::fs::File::create(&updated).unwrap());
        for (id, line) in input.lines().enumerate() {
            let line = line.unwrap();
            if let Some(names) = aliases.get(&(id as u32)) {
                let (row, item) = line.rsplit_once('\t').expect("places.tsv columns");
                let separator = if row.ends_with('\t') { "" } else { "\u{1f}" };
                let pairs = names
                    .iter()
                    .map(|n| format!("alt_name={}", cell(n)))
                    .collect::<Vec<_>>()
                    .join(&NAME_SEP.to_string());
                writeln!(output, "{row}{separator}{pairs}\t{item}").unwrap();
            } else {
                writeln!(output, "{line}").unwrap();
            }
        }
        output.flush().unwrap();
        std::fs::rename(updated, path).unwrap();
    }
    // An other name answers only where nothing else of the country has it: no place, no
    // GeoNames row as its name, no row beyond NEAR_KM as another name.
    let mut map = std::collections::BTreeMap::new();
    for (k, rows) in &others {
        let (first, xy) = rows[0];
        let apart = |s: &[Spot]| s.iter().any(|&(_, at)| km(at, xy) >= NEAR_KM);
        if near.contains_key(k) || apart(rows) || spots.get(k).is_some_and(|s| apart(s)) {
            continue;
        }
        if let Some(id) = target[first] {
            map.insert([&k.0[..], k.1.as_bytes()].concat(), id as u64);
        }
    }
    let mut fst = tantivy_fst::MapBuilder::memory();
    fst.extend_iter(map).unwrap();
    std::fs::write(out.join(ALTERNATES), fst.into_inner().unwrap()).unwrap();
    (places.len() - before, small)
}

/// A name of letters of the Latin script only, without digits or a parenthesis ("Huy
/// (Tihange)").
fn latin(name: &str) -> bool {
    !name.chars().any(|c| {
        c.is_numeric()
            || c == '('
            || (c.is_alphabetic() && c > '\u{24f}' && !('\u{1e00}'..='\u{1eff}').contains(&c))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(code: &str, alt: &str) -> String {
        format!(
            "1\tKalas\tKalas\t{alt}\t18.1\t74.9\tP\t{code}\tIN\t\t16\t\t\t\t4100\t\t1\tAsia/Kolkata\t2020-01-01"
        )
    }

    #[test]
    fn rows_answer_to_their_name_only_and_skip_dead_places() {
        let r = parse(&line("PPL", "Kalas,काळस,413114,Huy (Kalas)")).unwrap();
        assert_eq!(r.names, ["Kalas"]);
        assert_eq!(r.others, ["काळस", "Huy (Kalas)"]);
        assert_eq!((r.x, r.y, r.population), (74.9, 18.1, 4100));
        assert_eq!(r.admin1, "IN.16");
        assert_eq!(r.subtype, "locality");
        assert_eq!(parse(&line("PPLX", "")).unwrap().subtype, "neighborhood");
        assert!(r.listed);
        let small = |code: &str| parse(&line(code, "").replace("\t4100\t", "\t0\t"));
        assert!(!small("PPL").unwrap().listed);
        assert!(small("PPLA3").unwrap().listed);
        assert!(small("PPLX").is_none());
        assert!(parse(&line("PPLH", "")).is_none());
        assert!(parse("garbage").is_none());
    }

    #[test]
    fn only_an_unambiguous_same_country_settlement_keeps_the_primary_names() {
        let root = std::env::temp_dir().join(format!("geonames-aliases-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let cities = root.join("cities.txt");
        std::fs::write(&cities, line("PPL", "Oldname,Untrusted Alias")).unwrap();
        let polys = crate::geo::PolyFile::new(root.join("polys"));
        let assigner = Assigner::new(vec![], polys, vec![], vec![]);
        for (cc, subtype, positions, enriched) in [
            (*b"IN", "locality", vec![(74.9, 18.1)], true),
            (*b"IN", "locality", vec![(74.9, 18.1), (80., 18.1)], true),
            (*b"IN", "locality", vec![(74.9, 18.1), (74.91, 18.1)], false),
            (*b"IN", "county", vec![(74.9, 18.1)], false),
            (*b"FR", "locality", vec![(74.9, 18.1)], false),
        ] {
            let mut places: Vec<Place> = positions
                .iter()
                .map(|&xy| Place {
                    subtype,
                    country: cc,
                    region: "IN-16".into(),
                    names: "Oldname".into(),
                    population: 4100,
                    xy: Some(xy),
                    anc: Box::default(),
                })
                .collect();
            let original = "locality\tvillage\tIN\tIN-16\t0\t4100\t74.9\t18.1\tNaN\tNaN\tNaN\tNaN\t\tOldname\t\tQ1\n";
            std::fs::write(root.join("places.tsv"), original.repeat(places.len())).unwrap();
            add_places(&mut places, &assigner, &cities, None, &root);
            assert_eq!(places[0].names.contains("Kalas"), enriched);
            assert!(!places[0].names.contains("Untrusted Alias"));
            let tsv = std::fs::read_to_string(root.join("places.tsv")).unwrap();
            let first: Vec<_> = tsv.lines().next().unwrap().split('\t').collect();
            assert_eq!(first[14].contains("alt_name=Kalas"), enriched);
            assert_eq!((first[12], first[13], first[15]), ("", "Oldname", "Q1"));
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_district_seat_beside_only_its_namesake_district_is_added() {
        let root = std::env::temp_dir().join(format!("geonames-district-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let cities = root.join("cities.txt");
        let polys = crate::geo::PolyFile::new(root.join("polys"));
        let assigner = Assigner::new(vec![], polys, vec![], vec![]);
        for (code, subtype, added) in [
            ("PPLA2", "county", 1),
            ("PPLA2", "locality", 0),
            ("PPLA2", "neighborhood", 0),
            ("PPL", "county", 0),
        ] {
            std::fs::write(&cities, line(code, "")).unwrap();
            let mut places = vec![Place {
                subtype,
                country: *b"IN",
                region: "IN-16".into(),
                names: "Kalas".into(),
                population: 0,
                xy: Some((74.9, 18.1)),
                anc: Box::default(),
            }];
            std::fs::write(
                root.join("places.tsv"),
                format!(
                    "{subtype}\t\tIN\tIN-16\t0\t0\t74.9\t18.1\tNaN\tNaN\tNaN\tNaN\t\tKalas\t\t\n"
                ),
            )
            .unwrap();
            assert_eq!(
                add_places(&mut places, &assigner, &cities, None, &root).0,
                added,
                "{code} {subtype}"
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn nearness_is_in_kilometres() {
        assert!((km((0.0, 0.0), (0.0, 1.0)) - 111.2).abs() < 0.1);
        assert!(km((74.9, 18.1), (74.95, 18.1)) < NEAR_KM);
    }
}
