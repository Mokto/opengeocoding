use super::{Entry, Point, Registry};
use anyhow::{Context, Result, ensure};
use opengeocoding::eval::sha256_hex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::Path,
};

pub const URL: &str = "https://geoportaal.maaruum.ee/est/ruumiandmed/aadressiandmed/aadressid-ja-posti-sihtnumbrid-p582.html";

const COLUMNS: [&str; 6] = [
    "ADR_ID",
    "SIHTNUMBER",
    "LAHIAADRESS",
    "TAISAADRESS",
    "VIITEPUNKT_X",
    "VIITEPUNKT_Y",
];

/// Locations per county and class, each from a different settlement.
const TAKE: [(&str, usize); 3] = [("street", 3), ("letter", 2), ("name", 3)];

/// The last word of a street name as ADS writes it.
const STREET_TYPES: [&str; 9] = [
    "tn", "tänav", "tee", "mnt", "maantee", "pst", "puiestee", "põik", "allee",
];

/// L-EST97 (EPSG:3301) easting and northing to WGS84 latitude and longitude: Lambert conformal
/// conic on GRS80 with standard parallels 59°20' and 58° N, origin 57°31'03.19415" N 24° E, false
/// easting 500 km and northing 6375 km. ETRS89 and WGS84 differ by well under a metre here.
pub fn lest97([easting, northing]: [f64; 2]) -> [f64; 2] {
    let (a, f) = (6_378_137f64, 1. / 298.257_222_101f64);
    let e = (f * (2. - f)).sqrt();
    let m = |p: f64| p.cos() / (1. - (e * p.sin()).powi(2)).sqrt();
    let t = |p: f64| {
        (std::f64::consts::FRAC_PI_4 - p / 2.).tan()
            / ((1. - e * p.sin()) / (1. + e * p.sin())).powf(e / 2.)
    };
    let (p1, p2, p0) = (
        (59. + 1. / 3f64).to_radians(),
        58f64.to_radians(),
        57.517_553_930_555_56f64.to_radians(),
    );
    let n = (m(p1).ln() - m(p2).ln()) / (t(p1).ln() - t(p2).ln());
    let big_f = m(p1) / (n * t(p1).powf(n));
    let r0 = a * big_f * t(p0).powf(n);
    let (de, dn) = (easting - 500_000., r0 - (northing - 6_375_000.));
    let tp = ((de * de + dn * dn).sqrt() / (a * big_f)).powf(1. / n);
    let mut lat = std::f64::consts::FRAC_PI_2 - 2. * tp.atan();
    for _ in 0..10 {
        lat = std::f64::consts::FRAC_PI_2
            - 2. * (tp * ((1. - e * lat.sin()) / (1. + e * lat.sin())).powf(e / 2.)).atan();
    }
    [lat.to_degrees(), 24. + de.atan2(dn).to_degrees() / n]
}

#[derive(Clone, Debug)]
struct Address {
    id: String,
    postcode: String,
    short: String,
    full: String,
    county: String,
    municipality: String,
    /// The settlement unit (küla, alevik, alev, linn) or Tallinn's city district (linnaosa).
    settlement: String,
    /// The street and number of a street address; empty for a named one ("Antsu").
    street: String,
    number: String,
    point: [f64; 2],
}

/// A name without its type word: "Ruhnu küla" is Ruhnu, "Keila linn" is Keila.
fn bare(name: &str) -> &str {
    [
        "küla", "alevik", "alev", "linn", "vald", "linnaosa", "maakond",
    ]
    .iter()
    .find_map(|t| name.strip_suffix(t)?.strip_suffix(' '))
    .unwrap_or(name)
}

fn is_street(name: &str) -> bool {
    !name.bytes().any(|b| b.is_ascii_digit())
        && match name.rsplit_once(' ') {
            Some((_, last)) => STREET_TYPES.contains(&last),
            None => name.ends_with("tee"),
        }
}

impl Address {
    fn class(&self) -> &'static str {
        if self.street.is_empty() {
            "name"
        } else if self.number.ends_with(|c: char| c.is_ascii_lowercase()) {
            "letter"
        } else {
            "street"
        }
    }

    /// The settlement a postal address names: not a city district, nor a town of its own name.
    fn village(&self) -> Option<&str> {
        (!self.settlement.is_empty()
            && !self.settlement.ends_with(" linnaosa")
            && self.settlement != self.municipality)
            .then_some(self.settlement.as_str())
    }

    /// Omniva's form: "Pargi tn 2b, 76607 Keila" in a town, "Antsu, Ruhnu küla, Ruhnu vald,
    /// 93001 Saare maakond" elsewhere, the postcode before the town or county.
    fn postal(&self) -> String {
        match self.village() {
            None => format!(
                "{}, {} {}, Estonia",
                self.short,
                self.postcode,
                bare(&self.municipality)
            ),
            Some(village) => format!(
                "{}, {village}, {}, {} {}, Estonia",
                self.short, self.municipality, self.postcode, self.county
            ),
        }
    }

    fn places(&self) -> Vec<String> {
        super::places([
            self.settlement.as_str(),
            bare(&self.settlement),
            &self.municipality,
            bare(&self.municipality),
        ])
    }
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty ADS CSV"
    );
    ensure!(
        line.trim_start_matches('\u{feff}')
            .trim_end()
            .split(';')
            .eq(COLUMNS),
        "{} is not the ADS address extract with postcodes (sihtnumbrid.csv)",
        path.display()
    );
    let (mut n, mut eligible) = (0usize, 0usize);
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ';');
        ensure!(
            fields.len() == COLUMNS.len(),
            "ADS CSV row {n}: column count"
        );
        let field = |i: usize| fields[i].trim();
        let coordinate = |i: usize| field(i).parse::<f64>();
        let (x, y) = (
            coordinate(4).context("ADS VIITEPUNKT_X")?,
            coordinate(5).context("ADS VIITEPUNKT_Y")?,
        );
        ensure!(
            (300_000. ..800_000.).contains(&x) && (6_300_000. ..6_700_000.).contains(&y),
            "ADS CSV row {n}: needs L-EST97 easting VIITEPUNKT_X, northing VIITEPUNKT_Y"
        );
        let (id, postcode, short, full) = (field(0), field(1), field(2), field(3));
        let parts: Vec<&str> = full
            .strip_suffix(short)
            .and_then(|p| p.strip_suffix(", "))
            .map(|p| p.split(", ").collect())
            .unwrap_or_default();
        let (street, number) = short.rsplit_once(' ').unwrap_or_default();
        let number_ok = number.starts_with(|c: char| ('1'..='9').contains(&c))
            && number
                .trim_end_matches(|c: char| c.is_ascii_lowercase())
                .bytes()
                .all(|b| b.is_ascii_digit())
            && number.len()
                - number
                    .trim_end_matches(|c: char| c.is_ascii_lowercase())
                    .len()
                <= 1;
        let settlement = parts.get(2).copied().unwrap_or_default();
        let named = !short.contains([' ', ',', '/'])
            && !short.bytes().any(|b| b.is_ascii_digit())
            && !short.ends_with("tee")
            && settlement.ends_with("küla");
        let (street, number) = if number_ok && is_street(street) {
            (street, number)
        } else if named {
            ("", "")
        } else {
            continue;
        };
        if id.is_empty()
            || !id.bytes().all(|b| b.is_ascii_digit())
            || postcode.len() != 5
            || !postcode.bytes().all(|b| b.is_ascii_digit())
            || !(2..=3).contains(&parts.len())
            || !parts[0].ends_with(" maakond")
        {
            continue;
        }
        eligible += 1;
        visit(Address {
            id: id.into(),
            postcode: postcode.into(),
            short: short.into(),
            full: full.into(),
            county: parts[0].into(),
            municipality: parts[1].into(),
            settlement: settlement.into(),
            street: street.into(),
            number: number.into(),
            point: lest97([x, y]),
        })?;
    }
    eprintln!("ADS: {n} rows, {eligible} eligible");
    Ok(())
}

fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(!found.contains_key(&a.id), "duplicate ADR_ID {}", a.id);
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "ADS CSV lacks requested ADR_IDs (or they are no longer eligible)"
    );
    Ok(found)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("ADS file name")?;
    let mut pools: BTreeMap<(String, &'static str), BTreeMap<String, Address>> = BTreeMap::new();
    for_each(path, |a| {
        let pool = pools.entry((a.county.clone(), a.class())).or_default();
        let hash = sha256_hex(format!("{seed}\0{}", a.id).as_bytes());
        // ponytail: settlement diversity draws on the 200 smallest hashes per county and class; enlarge the pool if it cannot supply TAKE settlements.
        if pool.len() < 200 || pool.last_key_value().is_some_and(|(k, _)| hash < *k) {
            pool.insert(hash, a);
            if pool.len() > 200 {
                pool.pop_last();
            }
        }
        Ok(())
    })?;
    let mut lines = vec![];
    for ((county, class), pool) in pools {
        let take = TAKE.iter().find(|(c, _)| *c == class).unwrap().1;
        let mut settlements = BTreeSet::new();
        let selected: Vec<_> = pool
            .values()
            .filter(|a| settlements.insert((&a.municipality, &a.settlement)))
            .take(take)
            .collect();
        eprintln!("ADS {county} {class}: {} locations", selected.len());
        for a in selected {
            let query = a.postal();
            let base = json!({"kind":"address", "source":"ads", "ref":a.id, "cohort":format!("ee_{class}"), "queries":[query], "label":query, "tags":["registry-sample", "ee", "ads", release, format!("county:{county}"), format!("municipality:{}", a.municipality), class]});
            let mut register = base.clone();
            register["cohort"] = json!("ee_register");
            register["queries"] = json!([format!("{}, Estonia", a.full)]);
            register["tags"]
                .as_array_mut()
                .unwrap()
                .push(json!("register_form"));
            lines.extend([base, register]);
        }
    }
    ensure!(!lines.is_empty(), "no eligible ADS addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "ads",
    command: "sample-ee",
    country: "ee",
    input: "ads",
    license: "MaRu-Open-Data-Licence-2025",
    location: "ads",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert("dataset".into(), json!(URL));
    metadata.insert(
        "coordinates".into(),
        json!("VIITEPUNKT_X, VIITEPUNKT_Y: L-EST97 (EPSG:3301) easting, northing, converted to WGS84 by the evaluator"),
    );
    lookup(path, refs)?
        .into_iter()
        .map(|(id, a)| {
            let mut provenance = metadata.clone();
            provenance.insert("record".into(), json!({"ADR_ID":id, "SIHTNUMBER":a.postcode, "LAHIAADRESS":a.short, "TAISAADRESS":a.full}));
            let mut components = indexmap::IndexMap::from([
                (
                    "house_number",
                    if a.street.is_empty() {
                        a.short.clone()
                    } else {
                        a.number.clone()
                    },
                ),
                ("postcode", a.postcode.clone()),
            ]);
            if !a.street.is_empty() {
                components.insert("road", a.street.clone());
            }
            Ok((
                id,
                Entry {
                    point: Point::Wgs84(a.point.map(|c| (c * 1e6).round() / 1e6)),
                    components,
                    radius_floor_km: 0.,
                    provenance,
                    number_aliases: None,
                    places: a.places(),
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lest97_matches_proj() {
        // PROJ's EPSG:3301 to EPSG:4326 (In-ADS's own WGS84 points differ by up to 0.4 m)
        for (xy, want) in [
            (
                [523306.03, 6574787.36],
                [59.310613477178855, 24.409192210310835],
            ),
            (
                [454981.08, 6407585.67],
                [57.80783216739377, 23.24275185705559],
            ),
            (
                [539621.34, 6587208.36],
                [59.42089824549117, 24.69789842246438],
            ),
        ] {
            let got = lest97(xy);
            assert!(
                (got[0] - want[0]).abs() < 1e-9 && (got[1] - want[1]).abs() < 1e-9,
                "{got:?} {want:?}"
            );
        }
    }

    #[test]
    fn eligible_rows_query_forms_and_order_independent_sampling() {
        let path = std::env::temp_dir().join(format!("ads-test-{}.csv", std::process::id()));
        let header = format!("{}\r\n", COLUMNS.join(";"));
        let rows = [
            "121731;76607;Pargi tn 2b;Harju maakond, Keila linn, Pargi tn 2b;523306.03;6574787.37\r\n",
            "121562;93001;Antsu;Saare maakond, Ruhnu vald, Ruhnu küla, Antsu;454981.08;6407585.67\r\n",
            "2105921;10621;Mustamäe tee 51;Harju maakond, Tallinn, Kristiine linnaosa, Mustamäe tee 51;539625.35;6587225.42\r\n",
            "1;76901;Sarapuu tn 70;Harju maakond, Harku vald, Tabasalu alevik, Sarapuu tn 70;531998.61;6587188.95\r\n",
            "2;50106;Kesktee 4;Tartu maakond, Tartu linn, Tartu linn, Kesktee 4;659000;6474000\r\n",
            "3;44313;Saue tn 13-5;Lääne-Viru maakond, Rakvere linn, Saue tn 13-5;634577.97;6581554.74\r\n",
            "4;76023;Külati tee;Harju maakond, Lääne-Harju vald, Madise küla, Külati tee;507023.07;6571822.77\r\n",
            "5;76002;Meistripunkti alajaam;Harju maakond, Lääne-Harju vald, Harju-Risti küla, Meistripunkti alajaam;500166.4;6565623.3\r\n",
            "6;90401;Haapsalu metskond 174;Lääne maakond, Haapsalu linn, Kiltsi küla, Haapsalu metskond 174;470000;6530000\r\n",
            "7;10621;Sillutise tn 3/1;Harju maakond, Tallinn, Kristiine linnaosa, Sillutise tn 3/1;539625.35;6587225.42\r\n",
            "8;10621;Kopli;Harju maakond, Tallinn, Põhja-Tallinna linnaosa, Kopli;539625.35;6587225.42\r\n",
            "9;10621;Turu tn 6AB;Harju maakond, Tallinn, Kesklinna linnaosa, Turu tn 6AB;539625.35;6587225.42\r\n",
        ];
        let write = |rows: &[&str]| {
            std::fs::write(&path, format!("\u{feff}{header}{}", rows.concat())).unwrap()
        };
        write(&rows);
        let first = sample(&path, "test").unwrap();
        let queries: BTreeSet<_> = first
            .iter()
            .map(|s| s["queries"][0].as_str().unwrap())
            .collect();
        assert_eq!(
            queries,
            BTreeSet::from([
                "Pargi tn 2b, 76607 Keila, Estonia",
                "Harju maakond, Keila linn, Pargi tn 2b, Estonia",
                "Antsu, Ruhnu küla, Ruhnu vald, 93001 Saare maakond, Estonia",
                "Saare maakond, Ruhnu vald, Ruhnu küla, Antsu, Estonia",
                "Mustamäe tee 51, 10621 Tallinn, Estonia",
                "Harju maakond, Tallinn, Kristiine linnaosa, Mustamäe tee 51, Estonia",
                "Sarapuu tn 70, Tabasalu alevik, Harku vald, 76901 Harju maakond, Estonia",
                "Harju maakond, Harku vald, Tabasalu alevik, Sarapuu tn 70, Estonia",
                "Kesktee 4, 50106 Tartu, Estonia",
                "Tartu maakond, Tartu linn, Tartu linn, Kesktee 4, Estonia",
            ])
        );
        let mut reversed = rows;
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let found = lookup(&path, &BTreeSet::from(["121562".into(), "2105921".into()])).unwrap();
        assert_eq!(
            found["121562"].places(),
            ["Ruhnu küla", "Ruhnu", "Ruhnu vald"]
        );
        assert_eq!(
            found["2105921"].places(),
            ["Kristiine linnaosa", "Kristiine", "Tallinn"]
        );
        for id in ["3", "4", "5", "6", "7", "8", "9"] {
            assert!(lookup(&path, &BTreeSet::from([id.into()])).is_err(), "{id}");
        }
        write(&[&rows[0].replace("523306.03;6574787.37", "6574787.37;523306.03")]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("L-EST97")
        );
        std::fs::remove_file(path).unwrap();
    }
}
