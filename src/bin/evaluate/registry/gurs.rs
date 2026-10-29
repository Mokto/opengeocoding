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

pub const URL: &str = "https://podatki.gov.si/dataset/register-naslovov";

const COLUMNS: [&str; 39] = [
    "FEATUREID",
    "EID_NASLOV",
    "OBCINA_SIFRA",
    "OBCINA_NAZIV",
    "OBCINA_NAZIV_DJ",
    "NASELJE_SIFRA",
    "NASELJE_NAZIV",
    "NASELJE_NAZIV_DJ",
    "ULICA_SIFRA",
    "ULICA_NAZIV",
    "ULICA_NAZIV_DJ",
    "POSTNI_OKOLIS_SIFRA",
    "POSTNI_OKOLIS_NAZIV",
    "POSTNI_OKOLIS_NAZIV_DJ",
    "HS_STEVILKA",
    "HS_DODATEK",
    "ST_STANOVANJA",
    "E",
    "N",
    "EID_OBCINA",
    "EID_NASELJE",
    "EID_ULICA",
    "EID_POSTNI_OKOLIS",
    "EID_HISNA_STEVILKA",
    "EID_STANOVANJE",
    "EID_STAVBA",
    "EID_CETRTNA_SKUPNOST",
    "EID_DZ_VOLISCE",
    "EID_KRAJEVNA_SKUPNOST",
    "EID_LOKALNO_VOLISCE",
    "EID_LOKALNA_VOLILNA_ENOTA",
    "EID_SOLSKI_OKOLIS",
    "EID_STATISTICNA_REGIJA",
    "EID_UPRAVNA_ENOTA",
    "EID_VASKA_SKUPNOST",
    "EID_VOLILNA_ENOTA_DZ",
    "EID_VOLILNI_OKRAJ",
    "EID_KOHEZIJSKA_REGIJA",
    "DATUM_SYS",
];

/// The twelve statistical regions by EID, as the register of spatial units names them
/// (RPE_SLO_PROSTORSKE_ENOTE_statisticne_regije of the same release); the address file has only
/// the EID.
const REGIONS: [(&str, &str); 12] = [
    ("110900000214281329", "Pomurska"),
    ("110900000214281592", "Podravska"),
    ("110900000214281675", "Koroška"),
    ("110900000214281758", "Savinjska"),
    ("110900000214281832", "Zasavska"),
    ("110900000214281915", "Posavska"),
    ("110900000214282053", "Jugovzhodna Slovenija"),
    ("110900000214282137", "Osrednjeslovenska"),
    ("110900000214282210", "Gorenjska"),
    ("110900000214282301", "Primorsko-notranjska"),
    ("110900000214282483", "Goriška"),
    ("110900000214282566", "Obalno-kraška"),
];

/// Locations per statistical region and class, each from a different settlement.
const TAKE: [(&str, usize); 3] = [("street", 3), ("letter", 2), ("settlement", 3)];

/// D96/TM (EPSG:3794) easting and northing to latitude and longitude: transverse Mercator on
/// GRS80, central meridian 15° E, scale 0.9999, false easting 500 km, false northing −5000 km
/// (Snyder's inverse series, millimetres within Slovenia). D96 (ETRS89) and WGS84 are treated as
/// the same.
pub fn d96tm([easting, northing]: [f64; 2]) -> [f64; 2] {
    let [lat, lon] = inverse_tm(
        6_378_137.,
        1. / 298.257_222_101,
        0.9999,
        [easting - 500_000., northing + 5_000_000.],
    );
    [lat, 15. + lon]
}

/// Inverse transverse Mercator (Snyder's series) on the ellipsoid of semi-major axis `a` and
/// flattening `f`: easting from the central meridian and northing from the equator, in metres, to
/// latitude and longitude from the central meridian, in degrees.
pub fn inverse_tm(a: f64, f: f64, k0: f64, [easting, northing]: [f64; 2]) -> [f64; 2] {
    let e2 = f * (2. - f);
    let ep2 = e2 / (1. - e2);
    let e1 = (1. - (1. - e2).sqrt()) / (1. + (1. - e2).sqrt());
    let mu = northing / k0 / (a * (1. - e2 / 4. - 3. * e2 * e2 / 64. - 5. * e2.powi(3) / 256.));
    let p = mu
        + (1.5 * e1 - 27. / 32. * e1.powi(3)) * (2. * mu).sin()
        + (21. / 16. * e1 * e1 - 55. / 32. * e1.powi(4)) * (4. * mu).sin()
        + 151. / 96. * e1.powi(3) * (6. * mu).sin()
        + 1097. / 512. * e1.powi(4) * (8. * mu).sin();
    let (c, t) = (ep2 * p.cos().powi(2), p.tan().powi(2));
    let w = 1. - e2 * p.sin().powi(2);
    let (n, r) = (a / w.sqrt(), a * (1. - e2) / w.powf(1.5));
    let d = easting / (n * k0);
    let lat = p - n * p.tan() / r
        * (d * d / 2. - (5. + 3. * t + 10. * c - 4. * c * c - 9. * ep2) * d.powi(4) / 24.
            + (61. + 90. * t + 298. * c + 45. * t * t - 252. * ep2 - 3. * c * c) * d.powi(6)
                / 720.);
    let lon = (d - (1. + 2. * t + c) * d.powi(3) / 6.
        + (5. - 2. * c + 28. * t - 3. * c * c + 8. * ep2 + 24. * t * t) * d.powi(5) / 120.)
        / p.cos();
    [lat.to_degrees(), lon.to_degrees()]
}

#[derive(Clone, Debug)]
struct Address {
    /// EID_HISNA_STEVILKA, the house number's identifier (OpenAddresses' and Overture's id too).
    id: String,
    region: &'static str,
    municipality: String,
    municipality_dj: String,
    settlement: String,
    settlement_dj: String,
    /// Empty where the settlement has no streets and numbers its houses itself ("Zapuže 19").
    street: String,
    number: String,
    postcode: String,
    post: String,
    point: [f64; 2],
    record: Value,
}

impl Address {
    fn class(&self) -> &'static str {
        if self.number.ends_with(|c: char| c.is_alphabetic()) {
            "letter"
        } else if self.street.is_empty() {
            "settlement"
        } else {
            "street"
        }
    }

    /// "Trzinska cesta 20a", or "Zapuže 19" in a settlement without streets.
    fn house(&self) -> String {
        let name = if self.street.is_empty() {
            &self.settlement
        } else {
            &self.street
        };
        format!("{name} {}", self.number)
    }

    /// Pošta Slovenije's order, house then postcode and post office, with the settlement after a
    /// street when the post office has another name ("Trzinska cesta 20a, Loka pri Mengšu, 1234
    /// Mengeš"), as Pošta writes it where a post office has several streets of one name.
    fn postal(&self) -> String {
        let mut parts = vec![self.house()];
        if !self.street.is_empty()
            && self.post.split(" - ").next() != Some(self.settlement.as_str())
        {
            parts.push(self.settlement.clone());
        }
        parts.push(format!("{} {}", self.postcode, self.post));
        parts.push("Slovenia".into());
        parts.join(", ")
    }

    /// No postcode: the settlement (if a street does not already carry it) and the municipality,
    /// each named once ("Zapuže 19, Radovljica", "Razgledna cesta 32a, Bled").
    fn municipal(&self) -> String {
        let mut parts = vec![self.house()];
        if !self.street.is_empty() && self.settlement != self.municipality {
            parts.push(self.settlement.clone());
        }
        if !self.street.is_empty() || self.settlement != self.municipality {
            parts.push(self.municipality.clone());
        }
        parts.push("Slovenia".into());
        parts.join(", ")
    }

    fn places(&self) -> Vec<String> {
        super::places([
            self.settlement.as_str(),
            &self.settlement_dj,
            &self.municipality,
            &self.municipality_dj,
        ])
    }
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty GURS RN CSV"
    );
    ensure!(
        line.trim_start_matches('\u{feff}')
            .trim_end()
            .split(',')
            .eq(COLUMNS),
        "{} is not the GURS register of addresses for Slovenia (RN_SLO_NASLOVI_register_naslovov_*.csv)",
        path.display()
    );
    let (mut n, mut houses, mut eligible) = (0usize, 0usize, 0usize);
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(
            fields.len() == COLUMNS.len(),
            "GURS RN CSV row {n}: column count"
        );
        let field = |name: &str| fields[COLUMNS.iter().position(|c| *c == name).unwrap()].trim();
        if !field("ST_STANOVANJA").is_empty() {
            continue;
        }
        houses += 1;
        let coordinate = |name: &str| field(name).parse::<f64>().context(format!("GURS {name}"));
        let (e, north) = (coordinate("E")?, coordinate("N")?);
        ensure!(
            (370_000. ..630_000.).contains(&e) && (25_000. ..200_000.).contains(&north),
            "GURS RN CSV row {n}: needs D96/TM easting E, northing N"
        );
        let region = REGIONS
            .iter()
            .find(|(eid, _)| *eid == field("EID_STATISTICNA_REGIJA"))
            .with_context(|| format!("GURS RN CSV row {n}: unknown statistical region"))?
            .1;
        let (number, letter, postcode) = (
            field("HS_STEVILKA"),
            field("HS_DODATEK"),
            field("POSTNI_OKOLIS_SIFRA"),
        );
        if !number.starts_with(|c: char| ('1'..='9').contains(&c))
            || !number.bytes().all(|b| b.is_ascii_digit())
            || letter.chars().count() > 1
            || !letter.chars().all(char::is_lowercase)
            || postcode.len() != 4
            || !postcode.bytes().all(|b| b.is_ascii_digit())
            || [
                "EID_HISNA_STEVILKA",
                "OBCINA_NAZIV",
                "NASELJE_NAZIV",
                "POSTNI_OKOLIS_NAZIV",
            ]
            .iter()
            .any(|c| field(c).is_empty())
        {
            continue;
        }
        eligible += 1;
        visit(Address {
            id: field("EID_HISNA_STEVILKA").into(),
            region,
            municipality: field("OBCINA_NAZIV").into(),
            municipality_dj: field("OBCINA_NAZIV_DJ").into(),
            settlement: field("NASELJE_NAZIV").into(),
            settlement_dj: field("NASELJE_NAZIV_DJ").into(),
            street: field("ULICA_NAZIV").into(),
            number: format!("{number}{letter}"),
            postcode: postcode.into(),
            post: field("POSTNI_OKOLIS_NAZIV").into(),
            point: d96tm([e, north]),
            record: json!({
                "EID_HISNA_STEVILKA": field("EID_HISNA_STEVILKA"),
                "EID_NASLOV": field("EID_NASLOV"),
                "OBCINA_NAZIV": field("OBCINA_NAZIV"),
                "NASELJE_NAZIV": field("NASELJE_NAZIV"),
                "ULICA_NAZIV": field("ULICA_NAZIV"),
                "HS_STEVILKA": number,
                "HS_DODATEK": letter,
                "POSTNI_OKOLIS_SIFRA": postcode,
                "POSTNI_OKOLIS_NAZIV": field("POSTNI_OKOLIS_NAZIV"),
                "E": field("E"),
                "N": field("N"),
                "DATUM_SYS": field("DATUM_SYS"),
            }),
        })?;
    }
    eprintln!("GURS RN: {n} rows, {houses} house numbers, {eligible} eligible");
    Ok(())
}

fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(
                !found.contains_key(&a.id),
                "duplicate EID_HISNA_STEVILKA {}",
                a.id
            );
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "GURS RN CSV lacks requested EID_HISNA_STEVILKA (or they are no longer eligible)"
    );
    Ok(found)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("GURS RN file name")?;
    let mut pools: BTreeMap<(&'static str, &'static str), BTreeMap<String, Address>> =
        BTreeMap::new();
    for_each(path, |a| {
        let pool = pools.entry((a.region, a.class())).or_default();
        let hash = sha256_hex(format!("{seed}\0{}", a.id).as_bytes());
        // ponytail: settlement diversity draws on the 200 smallest hashes per region and class; enlarge the pool if it cannot supply TAKE settlements.
        if pool.len() < 200 || pool.last_key_value().is_some_and(|(k, _)| hash < *k) {
            pool.insert(hash, a);
            if pool.len() > 200 {
                pool.pop_last();
            }
        }
        Ok(())
    })?;
    let mut lines = vec![];
    for ((region, class), pool) in pools {
        let take = TAKE.iter().find(|(c, _)| *c == class).unwrap().1;
        let mut settlements = BTreeSet::new();
        let selected: Vec<_> = pool
            .values()
            .filter(|a| settlements.insert((&a.municipality, &a.settlement)))
            .take(take)
            .collect();
        eprintln!("GURS RN {region} {class}: {} locations", selected.len());
        for a in selected {
            let query = a.postal();
            let base = json!({"kind":"address", "source":"gurs", "ref":a.id, "cohort":format!("si_{class}"), "queries":[query], "label":query, "tags":["registry-sample", "si", "gurs", release, format!("region:{region}"), format!("municipality:{}", a.municipality), class]});
            let mut municipal = base.clone();
            municipal["cohort"] = json!("si_municipal");
            municipal["queries"] = json!([a.municipal()]);
            municipal["tags"]
                .as_array_mut()
                .unwrap()
                .push(json!("municipal_form"));
            lines.extend([base, municipal]);
        }
    }
    ensure!(!lines.is_empty(), "no eligible GURS RN addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "gurs",
    command: "sample-si",
    country: "si",
    input: "gurs",
    license: "CC-BY-4.0",
    location: "gurs",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert("dataset".into(), json!(URL));
    metadata.insert(
        "coordinates".into(),
        json!("E, N: D96/TM (EPSG:3794) easting, northing, converted to WGS84 by the evaluator"),
    );
    metadata.insert(
        "attribution".into(),
        json!("Geodetska uprava Republike Slovenije, Register naslovov; date of the database in source.release"),
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
    fn d96tm_matches_proj() {
        // PROJ's EPSG:3794 to EPSG:4326 (cs2cs), across the country
        for (en, want) in [
            ([497672., 155778.], [46.541643509370, 14.969646513505]),
            ([466101., 111972.], [46.146678342341, 14.561181407096]),
            ([377120., 45000.], [45.534078397802, 13.426691370136]),
            ([617310., 193265.], [46.868556406705, 16.538812450921]),
        ] {
            let got = d96tm(en);
            assert!(
                (got[0] - want[0]).abs() < 1e-8 && (got[1] - want[1]).abs() < 1e-8,
                "{got:?} {want:?}"
            );
        }
    }

    #[test]
    fn eligible_rows_query_forms_and_order_independent_sampling() {
        let path = std::env::temp_dir().join(format!("gurs-test-{}.csv", std::process::id()));
        let header = format!("{}\r\n", COLUMNS.join(","));
        // FEATUREID..DATUM_SYS, with every EID after EID_HISNA_STEVILKA blank but the region's
        let row = |id: &str, place: &str, street: &str, number: &str, apartment: &str, en: &str| {
            format!(
                "REGISTER_NASLOVOV.{id},{id},{place},{street},{number},{apartment},{en},1,2,3,4,h{id},,,,,,,,,110900000214282137,,,,,,2024-02-29T17:40:09\r\n"
            )
        };
        let rows = [
            row(
                "1",
                "72,Mengeš,,2,Loka pri Mengšu,",
                "6,Trzinska cesta,,1234,Mengeš,",
                "20,a",
                "",
                "466101,111972",
            ),
            row(
                "2",
                "102,Radovljica,,80,Zapuže,",
                ",,,4275,Begunje na Gorenjskem,",
                "19,",
                "",
                "436000,140000",
            ),
            row(
                "3",
                "3,Bled,,1,Bled,",
                "5,Razgledna cesta,,4260,Bled,",
                "32,",
                "",
                "430000,136000",
            ),
            row(
                "4",
                "90,Piran,Pirano,8,Piran,Pirano",
                "4,Galilejeva ulica,Via Galileo Galilei,6330,Piran - Pirano,",
                "6,",
                "",
                "389000,43000",
            ),
            row(
                "5",
                "61,Ljubljana,,11,Ljubljana,",
                "710,\"Novo Polje, cesta VII\",,1260,Ljubljana - Polje,",
                "24,",
                "",
                "467484,102579",
            ),
            row(
                "6",
                "61,Ljubljana,,11,Ljubljana,",
                "1,Galjevica,,1000,Ljubljana,",
                "80,",
                "3",
                "462000,101000",
            ),
            row(
                "7",
                "61,Ljubljana,,11,Ljubljana,",
                "1,Galjevica,,1000,Ljubljana,",
                "0,",
                "",
                "462000,101000",
            ),
            row(
                "8",
                "61,Ljubljana,,11,Ljubljana,",
                "1,Galjevica,,1000,Ljubljana,",
                "8,ab",
                "",
                "462000,101000",
            ),
        ];
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        let write =
            |rows: &[&str]| std::fs::write(&path, format!("{header}{}", rows.concat())).unwrap();
        write(&rows);
        let first = sample(&path, "test").unwrap();
        let queries: BTreeSet<_> = first
            .iter()
            .map(|s| s["queries"][0].as_str().unwrap())
            .collect();
        assert_eq!(
            queries,
            BTreeSet::from([
                "Trzinska cesta 20a, Loka pri Mengšu, 1234 Mengeš, Slovenia",
                "Trzinska cesta 20a, Loka pri Mengšu, Mengeš, Slovenia",
                "Zapuže 19, 4275 Begunje na Gorenjskem, Slovenia",
                "Zapuže 19, Radovljica, Slovenia",
                "Razgledna cesta 32, 4260 Bled, Slovenia",
                "Razgledna cesta 32, Bled, Slovenia",
                "Galilejeva ulica 6, 6330 Piran - Pirano, Slovenia",
                "Galilejeva ulica 6, Piran, Slovenia",
                "Novo Polje, cesta VII 24, 1260 Ljubljana - Polje, Slovenia",
                "Novo Polje, cesta VII 24, Ljubljana, Slovenia",
            ])
        );
        let mut reversed = rows.clone();
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let found = lookup(&path, &BTreeSet::from(["h1".into(), "h4".into()])).unwrap();
        assert_eq!(found["h1"].places(), ["Loka pri Mengšu", "Mengeš"]);
        assert_eq!(found["h4"].places(), ["Piran", "Pirano"]);
        assert_eq!(found["h1"].class(), "letter");
        for id in ["h6", "h7", "h8"] {
            assert!(lookup(&path, &BTreeSet::from([id.into()])).is_err(), "{id}");
        }
        write(&[&rows[0].replace("466101,111972", "111972,466101")]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("D96/TM")
        );
        std::fs::remove_file(path).unwrap();
    }
}
