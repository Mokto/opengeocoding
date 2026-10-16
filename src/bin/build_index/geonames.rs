//! GeoNames populated places (`cities500` and `admin1CodesASCII`) added to the places Overture's
//! divisions give, only where no place of that name lies within `NEAR_KM`.
use crate::{NAME_SEP, Place, Str, cell, country, geo::Assigner};
use opengeocoding::index::schema::fold;
use rayon::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    path::Path,
};

const NEAR_KM: f64 = 10.0;

/// Feature codes of places that no longer exist, are historical, or are religious sites.
const SKIPPED: [&str; 5] = ["PPLH", "PPLQ", "PPLW", "PPLCH", "PPLR"];

struct Row {
    /// The name and its ASCII form: what the place answers to.
    names: Vec<String>,
    /// Every other name, used only to see whether Overture has the place. GeoNames' alternates
    /// include labels such as "Huy (Tihange)" and the names of the place's district, which
    /// would answer queries for other places.
    others: Vec<String>,
    cc: [u8; 2],
    admin1: String,
    population: u64,
    subtype: &'static str,
    x: f64,
    y: f64,
}

fn parse(line: &str) -> Option<Row> {
    let f: Vec<&str> = line.split('\t').collect();
    if f.len() < 15 || SKIPPED.contains(&f[7]) || f[1].is_empty() {
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
        population: f[14].parse().unwrap_or(0),
        subtype: if f[7] == "PPLX" {
            "neighborhood"
        } else {
            "locality"
        },
        y: f[4].parse().ok()?,
        x: f[5].parse().ok()?,
    })
}

fn km(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0) * a.1.to_radians().cos()).hypot(a.1 - b.1) * 111.195
}

/// Appends the places GeoNames has and `places` lacks, to `places` and to `places.tsv` in `out`.
/// Returns how many were added.
pub fn add_places(
    places: &mut Vec<Place>,
    assigner: &Assigner,
    cities: &Path,
    admin1: Option<&Path>,
    out: &Path,
) -> usize {
    let text = std::fs::read_to_string(cities).expect("read GeoNames cities file");
    let rows: Vec<Row> = text.lines().filter_map(parse).collect();
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
    let wanted: HashSet<Key> = rows
        .iter()
        .flat_map(|r| r.names.iter().chain(&r.others).map(|n| (r.cc, fold(n))))
        .collect();
    let named: Vec<(Key, (f64, f64))> = places
        .par_iter()
        .filter_map(|p| p.xy.map(|xy| (p, xy)))
        .flat_map_iter(|(p, xy)| {
            p.names
                .split(NAME_SEP)
                .map(|n| (p.country, fold(n)))
                .filter(|k| wanted.contains(k))
                .map(move |k| (k, xy))
        })
        .collect();
    let mut near: HashMap<Key, Vec<(f64, f64)>> = HashMap::new();
    for (k, xy) in named {
        near.entry(k).or_default().push(xy);
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
    for r in rows {
        if !assigner.within(r.x, r.y) {
            continue;
        }
        let known = r.names.iter().chain(&r.others).any(|n| {
            near.get(&(r.cc, fold(n)))
                .is_some_and(|v| v.iter().any(|&xy| km(xy, (r.x, r.y)) < NEAR_KM))
        });
        if known {
            continue;
        }
        let parent = assigner.place(r.x, r.y).or_else(|| {
            let name = admin1.get(&r.admin1)?;
            regions.get(&(r.cc, fold(name))).copied()
        });
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
        writeln!(
            tsv,
            "{}\t\t{}\t{}\t0\t{}\t{}\t{}\tNaN\tNaN\tNaN\tNaN\t{}\t{}\t{}\t",
            r.subtype,
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
            id: Str::default(),
            subtype: r.subtype,
            country: r.cc,
            region,
            names: r.names.join(&NAME_SEP.to_string()).into(),
            population: r.population,
            xy: Some((r.x, r.y)),
            parents: vec![],
            anc,
            parent: None,
            row: Str::default(),
        });
    }
    tsv.flush().unwrap();
    places.len() - before
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
        assert!(parse(&line("PPLH", "")).is_none());
        assert!(parse("garbage").is_none());
    }

    #[test]
    fn nearness_is_in_kilometres() {
        assert!((km((0.0, 0.0), (0.0, 1.0)) - 111.2).abs() < 0.1);
        assert!(km((74.9, 18.1), (74.95, 18.1)) < NEAR_KM);
    }
}
