use super::{Entry, Point, Registry};
use anyhow::{Context, Result, ensure};
use opengeocoding::eval::sha256_hex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::File,
    io::BufReader,
    path::Path,
};

pub const URL: &str = "https://www.bev.gv.at/Services/Downloads/Produktbezogene-Downloads/Unentgeltliche-Produkte/Adressregister.html";

const ADRESSE: [&str; 21] = [
    "ADRCD",
    "GKZ",
    "OKZ",
    "PLZ",
    "SKZ",
    "ZAEHLSPRENGEL",
    "HAUSNRTEXT",
    "HAUSNRZAHL1",
    "HAUSNRBUCHSTABE1",
    "HAUSNRVERBINDUNG1",
    "HAUSNRZAHL2",
    "HAUSNRBUCHSTABE2",
    "HAUSNRBEREICH",
    "HNR_ADR_ZUSAMMEN",
    "GNRADRESSE",
    "HOFNAME",
    "RW",
    "HW",
    "EPSG",
    "QUELLADRESSE",
    "BESTIMMUNGSART",
];
const STRASSE: [&str; 7] = [
    "SKZ",
    "STRASSENNAME",
    "STRASSENNAMENZUSATZ",
    "SZUSADRBEST",
    "GKZ",
    "ZUSTELLORT",
    "ZUSTELLORT_ID",
];
const ORTSCHAFT: [&str; 3] = ["GKZ", "OKZ", "ORTSNAME"];
const GEMEINDE: [&str; 2] = ["GKZ", "GEMEINDENAME"];

/// The federal states by the first digit of the municipality code (GKZ).
const STATES: [&str; 9] = [
    "Burgenland",
    "Kärnten",
    "Niederösterreich",
    "Oberösterreich",
    "Salzburg",
    "Steiermark",
    "Tirol",
    "Vorarlberg",
    "Wien",
];

/// Locations per federal state and class, each from a different locality (Ortschaft).
const TAKE: [(&str, usize); 3] = [("street", 4), ("letter", 2), ("village", 3)];

/// MGI / Austria GK West, Central or East (EPSG:31254, 31255, 31256) to WGS84 latitude and
/// longitude: inverse transverse Mercator on Bessel 1841 (central meridian 10°20′, 13°20′ or
/// 16°20′ E, scale 1, false northing −5000 km), then EPSG:1618 "MGI to WGS 84 (3)", the
/// position-vector Helmert transformation (1.5 m) PROJ applies across Austria.
pub fn mgi_gk(epsg: u32, [rw, hw]: [f64; 2]) -> Option<[f64; 2]> {
    let lon0 = match epsg {
        31254 => 10.,
        31255 => 13.,
        31256 => 16.,
        _ => return None,
    } + 1. / 3.;
    let (a, f) = (6_377_397.155, 1. / 299.152_812_8);
    let [lat, lon] = super::gurs::inverse_tm(a, f, 1., [rw, hw + 5_000_000.]);
    let [x, y, z] = cartesian(a, f, [lat, lon + lon0]);
    let arcsec = |s: f64| (s / 3600f64).to_radians();
    let (rx, ry, rz, m) = (arcsec(5.137), arcsec(1.474), arcsec(5.297), 1. + 2.4232e-6);
    Some(geodetic(
        6_378_137.,
        1. / 298.257_223_563,
        [
            577.326 + m * (x - rz * y + ry * z),
            90.129 + m * (rz * x + y - rx * z),
            463.919 + m * (-ry * x + rx * y + z),
        ],
    ))
}

fn cartesian(a: f64, f: f64, [lat, lon]: [f64; 2]) -> [f64; 3] {
    let e2 = f * (2. - f);
    let (lat, lon) = (lat.to_radians(), lon.to_radians());
    let n = a / (1. - e2 * lat.sin().powi(2)).sqrt();
    [
        n * lat.cos() * lon.cos(),
        n * lat.cos() * lon.sin(),
        n * (1. - e2) * lat.sin(),
    ]
}

fn geodetic(a: f64, f: f64, [x, y, z]: [f64; 3]) -> [f64; 2] {
    let e2 = f * (2. - f);
    let p = x.hypot(y);
    let mut lat = z.atan2(p * (1. - e2));
    for _ in 0..8 {
        let n = a / (1. - e2 * lat.sin().powi(2)).sqrt();
        let h = p / lat.cos() - n;
        lat = z.atan2(p * (1. - e2 * n / (n + h)));
    }
    [lat.to_degrees(), y.atan2(x).to_degrees()]
}

#[derive(Clone, Debug)]
struct Address {
    /// ADRCD, the register's address code (OpenAddresses' and Overture's record id too).
    id: String,
    state: &'static str,
    municipality: String,
    /// ORTSNAME; in Wien, Graz and Klagenfurt "Wien,Donaustadt" or "Graz,10.Bez.:Ries".
    locality: String,
    street: String,
    number: String,
    postcode: String,
    /// ZUSTELLORT, the place a postal address names after the postcode ("Krems/Donau").
    post: String,
    point: [f64; 2],
    record: Value,
}

impl Address {
    /// A house numbered by its locality, whose "street" is the locality's name ("Reinsbach 21").
    fn village(&self) -> bool {
        self.street == self.locality
    }

    fn class(&self) -> &'static str {
        if self.number.ends_with(|c: char| c.is_alphabetic()) {
            "letter"
        } else if self.village() {
            "village"
        } else {
            "street"
        }
    }

    /// The city district a statutory city's locality names ("Donaustadt", "Ries").
    fn district(&self) -> Option<&str> {
        let (_, district) = self.locality.split_once(',')?;
        district.rsplit(':').next()
    }

    fn house(&self) -> String {
        format!("{} {}", self.street, self.number)
    }

    /// The register's postal form: house, postcode and delivery place ("Krems/Donau").
    fn postal(&self) -> String {
        format!("{}, {} {}, Austria", self.house(), self.postcode, self.post)
    }

    /// No postcode: the locality (unless the house or a city district already carries it) and
    /// the municipality, each named once.
    fn municipal(&self) -> String {
        let mut parts = vec![self.house()];
        if !self.village() && self.locality != self.municipality && self.district().is_none() {
            parts.push(self.locality.clone());
        }
        if !self.village() || self.locality != self.municipality {
            parts.push(self.municipality.clone());
        }
        parts.push("Austria".into());
        parts.join(", ")
    }

    fn places(&self) -> Vec<String> {
        super::places([
            self.district().unwrap_or(&self.locality),
            &self.municipality,
        ])
    }
}

/// A sibling table of `ADRESSE.csv`, as rows of its columns.
fn table(path: &Path, name: &str, columns: &[&str]) -> Result<Vec<Vec<String>>> {
    let path = path.with_file_name(name);
    let mut reader = BufReader::new(File::open(&path).with_context(|| path.display().to_string())?);
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty {name}"
    );
    ensure!(
        crate::registry::nad::split(line.trim_start_matches('\u{feff}').trim_end(), ';')
            .iter()
            .map(|c| c.as_ref())
            .eq(columns.iter().copied()),
        "{} is not the BEV table {name}",
        path.display()
    );
    let mut rows = vec![];
    while crate::registry::ban::record(&mut reader, &mut line)? {
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ';');
        ensure!(fields.len() == columns.len(), "{name}: column count");
        rows.push(fields.iter().map(|f| f.trim().to_string()).collect());
    }
    Ok(rows)
}

/// The reference date (Stichtag) `Aktualitaetsstand.txt` states, as BEV's archive names the
/// release: "Adresse_Relationale_Tabellen_Stichtagsdaten_20261001".
fn release(path: &Path) -> Result<String> {
    let text = std::fs::read(path.with_file_name("Aktualitaetsstand.txt"))
        .context("BEV Aktualitaetsstand.txt next to ADRESSE.csv")?;
    let text = String::from_utf8_lossy(&text);
    let date = regex::Regex::new(r"Daten: (\d\d)\.(\d\d)\.(\d{4})")?
        .captures(&text)
        .context("Stichtag in Aktualitaetsstand.txt")?;
    Ok(format!(
        "Adresse_Relationale_Tabellen_Stichtagsdaten_{}{}{}",
        &date[3], &date[2], &date[1]
    ))
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let streets: HashMap<String, Vec<String>> = table(path, "STRASSE.csv", &STRASSE)?
        .into_iter()
        .map(|r| (r[0].clone(), r))
        .collect();
    let localities: HashMap<(String, String), String> = table(path, "ORTSCHAFT.csv", &ORTSCHAFT)?
        .into_iter()
        .map(|mut r| {
            (
                (std::mem::take(&mut r[0]), std::mem::take(&mut r[1])),
                r.remove(2),
            )
        })
        .collect();
    let municipalities: HashMap<String, String> = table(path, "GEMEINDE.csv", &GEMEINDE)?
        .into_iter()
        .map(|mut r| (std::mem::take(&mut r[0]), r.remove(1)))
        .collect();
    let mut reader = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty BEV ADRESSE.csv"
    );
    ensure!(
        crate::registry::nad::split(line.trim_start_matches('\u{feff}').trim_end(), ';')
            .iter()
            .map(|c| c.as_ref())
            .eq(ADRESSE),
        "{} is not the BEV address register's ADRESSE.csv (Adresse Relationale Tabellen)",
        path.display()
    );
    let (mut n, mut eligible) = (0usize, 0usize);
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ';');
        ensure!(
            fields.len() == ADRESSE.len(),
            "BEV ADRESSE.csv row {n}: column count"
        );
        let field = |name: &str| fields[ADRESSE.iter().position(|c| *c == name).unwrap()].trim();
        let coordinate = |name: &str| field(name).parse::<f64>().context(format!("BEV {name}"));
        let (rw, hw) = (coordinate("RW")?, coordinate("HW")?);
        let point = field("EPSG")
            .parse()
            .ok()
            .filter(|_| rw.abs() < 150_000. && (130_000. ..450_000.).contains(&hw))
            .and_then(|epsg| mgi_gk(epsg, [rw, hw]))
            .with_context(|| {
                format!("BEV ADRESSE.csv row {n}: needs MGI / Austria GK RW, HW and EPSG")
            })?;
        let (gkz, number, letter, postcode) = (
            field("GKZ"),
            field("HAUSNRZAHL1"),
            field("HAUSNRBUCHSTABE1"),
            field("PLZ"),
        );
        let street = streets
            .get(field("SKZ"))
            .with_context(|| format!("BEV ADRESSE.csv row {n}: unknown SKZ"))?;
        let locality = localities
            .get(&(gkz.to_string(), field("OKZ").to_string()))
            .with_context(|| format!("BEV ADRESSE.csv row {n}: unknown OKZ"))?;
        let municipality = municipalities
            .get(gkz)
            .with_context(|| format!("BEV ADRESSE.csv row {n}: unknown GKZ"))?;
        let state = gkz
            .bytes()
            .next()
            .filter(|b| (b'1'..=b'9').contains(b))
            .map(|b| STATES[(b - b'1') as usize])
            .with_context(|| format!("BEV ADRESSE.csv row {n}: GKZ"))?;
        // Plain numbers with at most one letter; parcel-number addresses, ranges, "Stg." or
        // "Parz." parts, text before the number and streets with a name suffix are left out.
        if field("GNRADRESSE") != "0"
            || [
                "HAUSNRTEXT",
                "HAUSNRVERBINDUNG1",
                "HAUSNRZAHL2",
                "HAUSNRBUCHSTABE2",
            ]
            .iter()
            .any(|c| !field(c).is_empty())
            || !number.starts_with(|c: char| ('1'..='9').contains(&c))
            || !number.bytes().all(|b| b.is_ascii_digit())
            || letter.chars().count() > 1
            || !letter.chars().all(char::is_alphabetic)
            || field("HNR_ADR_ZUSAMMEN") != format!("{number}{letter}")
            || postcode.len() != 4
            || !postcode.bytes().all(|b| b.is_ascii_digit())
            || !street[2].is_empty()
            || [&street[1], &street[5], locality, municipality]
                .iter()
                .any(|s| s.is_empty())
        {
            continue;
        }
        eligible += 1;
        visit(Address {
            id: field("ADRCD").into(),
            state,
            municipality: municipality.clone(),
            locality: locality.clone(),
            street: street[1].clone(),
            number: format!("{number}{letter}"),
            postcode: postcode.into(),
            post: street[5].clone(),
            point,
            record: json!({
                "ADRCD": field("ADRCD"),
                "GKZ": gkz,
                "GEMEINDENAME": municipality,
                "OKZ": field("OKZ"),
                "ORTSNAME": locality,
                "SKZ": field("SKZ"),
                "STRASSENNAME": street[1],
                "ZUSTELLORT": street[5],
                "PLZ": postcode,
                "HAUSNRZAHL1": number,
                "HAUSNRBUCHSTABE1": letter,
                "RW": field("RW"),
                "HW": field("HW"),
                "EPSG": field("EPSG"),
                "QUELLADRESSE": field("QUELLADRESSE"),
                "BESTIMMUNGSART": field("BESTIMMUNGSART"),
            }),
        })?;
    }
    eprintln!("BEV Adressregister: {n} addresses, {eligible} eligible");
    Ok(())
}

fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(!found.contains_key(&a.id), "duplicate ADRCD {}", a.id);
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "BEV ADRESSE.csv lacks requested ADRCD (or they are no longer eligible)"
    );
    Ok(found)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = release(path)?;
    let mut pools: BTreeMap<(&'static str, &'static str), BTreeMap<String, Address>> =
        BTreeMap::new();
    for_each(path, |a| {
        let pool = pools.entry((a.state, a.class())).or_default();
        let hash = sha256_hex(format!("{seed}\0{}", a.id).as_bytes());
        // ponytail: locality diversity draws on the 200 smallest hashes per state and class; enlarge the pool if it cannot supply TAKE localities.
        if pool.len() < 200 || pool.last_key_value().is_some_and(|(k, _)| hash < *k) {
            pool.insert(hash, a);
            if pool.len() > 200 {
                pool.pop_last();
            }
        }
        Ok(())
    })?;
    let mut lines = vec![];
    for ((state, class), pool) in pools {
        let take = TAKE.iter().find(|(c, _)| *c == class).unwrap().1;
        let mut localities = BTreeSet::new();
        let selected: Vec<_> = pool
            .values()
            .filter(|a| localities.insert((&a.municipality, &a.locality)))
            .take(take)
            .collect();
        eprintln!("BEV {state} {class}: {} locations", selected.len());
        for a in selected {
            let query = a.postal();
            let base = json!({"kind":"address", "source":"bev", "ref":a.id, "cohort":format!("at_{class}"), "queries":[query], "label":query, "tags":["registry-sample", "at", "bev", release, format!("state:{state}"), format!("municipality:{}", a.municipality), class]});
            let mut municipal = base.clone();
            municipal["cohort"] = json!("at_municipal");
            municipal["queries"] = json!([a.municipal()]);
            municipal["tags"]
                .as_array_mut()
                .unwrap()
                .push(json!("municipal_form"));
            lines.extend([base, municipal]);
        }
    }
    ensure!(!lines.is_empty(), "no eligible BEV addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "bev",
    command: "sample-at",
    country: "at",
    input: "bev",
    license: "CC-BY-4.0",
    location: "bev",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let release = release(path)?;
    let mut metadata = super::provenance(path)?;
    metadata.insert("release".into(), json!(release));
    metadata.insert("file".into(), json!("ADRESSE.csv"));
    let mut tables = serde_json::Map::new();
    for name in ["STRASSE.csv", "ORTSCHAFT.csv", "GEMEINDE.csv"] {
        tables.insert(
            name.into(),
            json!(crate::run::sha256_file(&path.with_file_name(name))?),
        );
    }
    metadata.insert("tables_sha256".into(), tables.into());
    metadata.insert("dataset".into(), json!(URL));
    metadata.insert(
        "coordinates".into(),
        json!("RW, HW: MGI / Austria GK (EPSG in the record) access coordinate, converted to WGS84 by the evaluator with EPSG:1618"),
    );
    metadata.insert(
        "attribution".into(),
        json!(format!(
            "© Österreichisches Adressregister, Stichtagsdaten vom {}.{}.{}",
            &release[release.len() - 2..],
            &release[release.len() - 4..release.len() - 2],
            &release[release.len() - 8..release.len() - 4]
        )),
    );
    lookup(path, refs)?
        .into_iter()
        .map(|(id, a)| {
            let mut provenance = metadata.clone();
            provenance.insert("record".into(), a.record.clone());
            let mut components = indexmap::IndexMap::from([
                ("house_number", a.number.clone()),
                ("postcode", a.postcode.clone()),
            ]);
            if !a.village() {
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
    fn mgi_gk_matches_proj() {
        // PROJ's EPSG:1618 pipeline from each zone (cct, Innsbruck, Linz, Eisenstadt, Allentsteig)
        for (epsg, rw_hw, want) in [
            (31254, [81294.48, 233601.03], [47.2362711372, 11.4066390693]),
            (31255, [72544.26, 348450.58], [48.2701859384, 14.3097676845]),
            (31256, [14347.76, 300941.93], [47.8469865833, 16.5238589996]),
            (
                31256,
                [-74902.00, 397423.70],
                [48.7102800835, 15.3143362687],
            ),
        ] {
            let got = mgi_gk(epsg, rw_hw).unwrap();
            assert!(
                (got[0] - want[0]).abs() < 1e-8 && (got[1] - want[1]).abs() < 1e-8,
                "{got:?} {want:?}"
            );
        }
        assert!(mgi_gk(4326, [0., 0.]).is_none());
    }

    #[test]
    fn eligible_rows_query_forms_and_order_independent_sampling() {
        let dir = std::env::temp_dir().join(format!("bev-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ADRESSE.csv");
        let write = |name: &str, header: &[&str], rows: &[&str]| {
            let text: String = rows.iter().map(|r| format!("{r}\r\n")).collect();
            std::fs::write(
                dir.join(name),
                format!("\u{feff}{}\r\n{text}", header.join(";")),
            )
            .unwrap();
        };
        std::fs::write(
            dir.join("Aktualitaetsstand.txt"),
            b"Aktualit\xe4tsstand (Stichtag) der enthaltenen Daten: 01.10.2026\r\n",
        )
        .unwrap();
        write(
            "STRASSE.csv",
            &STRASSE,
            &[
                r#""1";"Muldenweg";"";0;"41007";"Hörsching";"1""#,
                r#""2";"Reinsbach";"";0;"32501";"Allentsteig";"2""#,
                r#""3";"Landstraße";"";0;"30101";"Krems/Donau";"3""#,
                r#""4";"Wagramer Straße";"";0;"90001";"Wien";"4""#,
                r#""5";"Baumgartenberg";"";0;"41103";"Baumgartenberg";"5""#,
                r#""6";"Uferstraße";"FS I";1;"10101";"Eisenstadt";"6""#,
            ],
        );
        write(
            "ORTSCHAFT.csv",
            &ORTSCHAFT,
            &[
                r#""41007";"09843";"Holzleiten""#,
                r#""32501";"06740";"Reinsbach""#,
                r#""30101";"03001";"Krems an der Donau""#,
                r#""90001";"17224";"Wien,Donaustadt""#,
                r#""41103";"10029";"Baumgartenberg""#,
                r#""10101";"00001";"Eisenstadt""#,
            ],
        );
        write(
            "GEMEINDE.csv",
            &GEMEINDE,
            &[
                r#""41007";"Hörsching""#,
                r#""32501";"Allentsteig""#,
                r#""30101";"Krems an der Donau""#,
                r#""90001";"Wien""#,
                r#""41103";"Baumgartenberg""#,
                r#""10101";"Eisenstadt""#,
            ],
        );
        // ADRCD, GKZ, OKZ, PLZ, SKZ, then the number fields, then RW;HW;EPSG
        let row = |id: &str, place: &str, skz: &str, number: &str, rest: &str| {
            format!(
                r#""{id}";{place};"{skz}";"";"";{number};"keine Angabe";{rest};"";{}"#,
                r#"12761.00;300090.18;31256;"G";"Q""#
            )
        };
        let plain = |n: &str, l: &str| format!(r#"{n};"{l}";"";;"""#);
        let rows = [
            row(
                "1",
                r#""41007";"09843";"4063""#,
                "1",
                &plain("22", ""),
                r#""22";0"#,
            ),
            row(
                "2",
                r#""32501";"06740";"3804""#,
                "2",
                &plain("21", "a"),
                r#""21a";0"#,
            ),
            row(
                "3",
                r#""30101";"03001";"3500""#,
                "3",
                &plain("5", ""),
                r#""5";0"#,
            ),
            row(
                "4",
                r#""90001";"17224";"1220""#,
                "4",
                &plain("101", ""),
                r#""101";0"#,
            ),
            row(
                "5",
                r#""41103";"10029";"4342""#,
                "5",
                &plain("71", ""),
                r#""71";0"#,
            ),
            // left out: a parcel-number address, a range, a street with a name suffix
            row(
                "6",
                r#""41007";"09843";"4063""#,
                "1",
                &plain("23", ""),
                r#""23";1"#,
            ),
            row(
                "7",
                r#""41007";"09843";"4063""#,
                "1",
                r#"24;"";"-";26;"""#,
                r#""24-26";0"#,
            ),
            row(
                "8",
                r#""10101";"00001";"7000""#,
                "6",
                &plain("3", ""),
                r#""3";0"#,
            ),
        ];
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        write("ADRESSE.csv", &ADRESSE, &rows);
        let first = sample(&path, "test").unwrap();
        let queries: BTreeSet<_> = first
            .iter()
            .map(|s| s["queries"][0].as_str().unwrap())
            .collect();
        assert_eq!(
            queries,
            BTreeSet::from([
                "Muldenweg 22, 4063 Hörsching, Austria",
                "Muldenweg 22, Holzleiten, Hörsching, Austria",
                "Reinsbach 21a, 3804 Allentsteig, Austria",
                "Reinsbach 21a, Allentsteig, Austria",
                "Landstraße 5, 3500 Krems/Donau, Austria",
                "Landstraße 5, Krems an der Donau, Austria",
                "Wagramer Straße 101, 1220 Wien, Austria",
                "Wagramer Straße 101, Wien, Austria",
                "Baumgartenberg 71, 4342 Baumgartenberg, Austria",
                "Baumgartenberg 71, Austria",
            ])
        );
        assert_eq!(
            first[0]["tags"][3],
            "Adresse_Relationale_Tabellen_Stichtagsdaten_20261001"
        );
        let mut reversed = rows.clone();
        reversed.reverse();
        write("ADRESSE.csv", &ADRESSE, &reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let found = lookup(&path, &BTreeSet::from(["2".into(), "4".into()])).unwrap();
        assert_eq!(found["2"].class(), "letter");
        assert_eq!(found["4"].places(), ["Donaustadt", "Wien"]);
        for id in ["6", "7", "8"] {
            assert!(lookup(&path, &BTreeSet::from([id.into()])).is_err(), "{id}");
        }
        let entries = prepare(&path, &BTreeSet::from(["2".into(), "4".into()])).unwrap();
        assert!(!entries["2"].components.contains_key("road"));
        assert_eq!(entries["4"].components["road"], "Wagramer Straße");
        assert_eq!(
            entries["4"].provenance["attribution"],
            "© Österreichisches Adressregister, Stichtagsdaten vom 01.10.2026"
        );
        write(
            "ADRESSE.csv",
            &ADRESSE,
            &[&rows[0].replace("12761.00;300090.18", "300090.18;12761.00")],
        );
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("MGI / Austria GK")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
