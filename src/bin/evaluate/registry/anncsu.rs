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

pub const URL: &str = "https://www.anncsu.gov.it/it/consultazione-dellarchivio/open-data/";

const COLUMNS: [&str; 20] = [
    "id",
    "street_id",
    "municipality_code",
    "municipality",
    "municipality_names",
    "province",
    "region",
    "street",
    "street_lang1",
    "street_lang2",
    "locality",
    "number",
    "exponent",
    "specificity",
    "metric",
    "snc",
    "method",
    "copies",
    "lat",
    "lon",
];

#[derive(Clone, Debug)]
struct Address {
    id: String,
    street_id: String,
    municipality_code: String,
    municipality: String,
    /// The municipality's name in the area's other official language, if Istat lists one.
    local_name: String,
    province: String,
    region: String,
    street: String,
    street_lang1: String,
    street_lang2: String,
    locality: String,
    number: String,
    exponent: String,
    specificity: String,
    method: String,
    point: [f64; 2],
}

impl Address {
    fn number(&self) -> String {
        if self.exponent.is_empty() {
            self.number.clone()
        } else {
            format!("{}/{}", self.number, self.exponent)
        }
    }

    /// "12/A" is also written "12A"; "12/1" is not "121".
    fn number_aliases(&self) -> Option<BTreeSet<String>> {
        (!self.exponent.is_empty() && !self.exponent.bytes().all(|b| b.is_ascii_digit()))
            .then(|| BTreeSet::from([self.number(), format!("{}{}", self.number, self.exponent)]))
    }

    /// The register names localities in Italian only, so the other-language forms leave them out.
    fn query(&self, street: &str, town: &str, locality: bool) -> String {
        let mut parts = vec![format!("{street} {}", self.number())];
        if locality
            && !self.locality.is_empty()
            && self.locality.to_uppercase() != town.to_uppercase()
        {
            parts.push(self.locality.clone());
        }
        parts.push(format!("{town} ({})", self.province));
        parts.push("Italy".into());
        parts.join(", ")
    }

    fn queries(&self) -> Vec<(&'static str, String)> {
        let mut queries = vec![("native", self.query(&self.street, &self.municipality, true))];
        if !self.street_lang1.is_empty() {
            let town = if self.local_name.is_empty() {
                &self.municipality
            } else {
                &self.local_name
            };
            queries.push(("language1", self.query(&self.street_lang1, town, false)));
        }
        if !self.street_lang2.is_empty() {
            queries.push((
                "language2",
                self.query(&self.street_lang2, &self.municipality, false),
            ));
        }
        let mut seen = BTreeSet::new();
        queries.retain(|(_, query)| seen.insert(query.clone()));
        queries
    }

    fn category(&self) -> &'static str {
        if !self.street_lang1.is_empty() {
            "bilingual"
        } else if !self.exponent.is_empty() {
            "exponent"
        } else if !self.locality.is_empty() {
            "locality"
        } else {
            "plain"
        }
    }
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty ANNCSU CSV"
    );
    let header = crate::registry::nad::split(line.trim_end(), ',');
    ensure!(
        header.iter().map(|s| s.as_ref()).eq(COLUMNS),
        "{} is not a CSV prepared by tests/registry/anncsu-csv.sql",
        path.display()
    );
    let (mut n, mut eligible) = (0usize, 0usize);
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        if n.is_multiple_of(5_000_000) {
            eprintln!("ANNCSU: {n} rows read");
        }
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(
            fields.len() == COLUMNS.len(),
            "ANNCSU CSV row {n}: column count"
        );
        let field = |i: usize| fields[i].trim();
        let lat: f64 = field(18).parse().context("ANNCSU latitude")?;
        let lon: f64 = field(19).parse().context("ANNCSU longitude")?;
        ensure!(
            (35. ..=48.).contains(&lat) && (6. ..=19.).contains(&lon),
            "ANNCSU CSV row {n}: needs Italian latitude/longitude"
        );
        let digits = |s: &str, max: usize| {
            (1..=max).contains(&s.len())
                && !s.starts_with('0')
                && s.bytes().all(|b| b.is_ascii_digit())
        };
        let (number, exponent, specificity) = (field(11), field(12), field(13));
        let exponent_ok = exponent.is_empty()
            || (exponent.len() == 1 && exponent.bytes().all(|b| b.is_ascii_uppercase()))
            || digits(exponent, 2)
            || ["BIS", "TER", "QUATER"].contains(&exponent);
        // Genoa's and Florence's red (commercial) numbers repeat the black ones on the same
        // street; a plain "12" there means the black number.
        let specificity_ok = ["", "N", "NERO"].contains(&specificity);
        if field(17) != "1"
            || !digits(number, 5)
            || !exponent_ok
            || !specificity_ok
            || !field(14).is_empty()
            || !field(15).is_empty()
            || field(7).chars().filter(|c| c.is_alphabetic()).count() < 2
            || field(7).contains(',')
            || [0, 1, 2, 3, 5, 6].iter().any(|&i| field(i).is_empty())
        {
            continue;
        }
        let local_name = field(4)
            .strip_prefix(field(3))
            .unwrap_or("")
            .trim_start_matches(['/', '-'])
            .split('/')
            .next()
            .unwrap_or("")
            .trim();
        eligible += 1;
        visit(Address {
            id: field(0).into(),
            street_id: field(1).into(),
            municipality_code: field(2).into(),
            municipality: field(3).into(),
            local_name: local_name.into(),
            province: field(5).into(),
            region: field(6).into(),
            street: field(7).into(),
            street_lang1: field(8).into(),
            street_lang2: field(9).into(),
            locality: field(10).into(),
            number: number.into(),
            exponent: exponent.into(),
            specificity: specificity.into(),
            method: field(16).into(),
            point: [lat, lon],
        })?;
    }
    eprintln!("ANNCSU: {n} rows, {eligible} eligible");
    Ok(())
}

fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(!found.contains_key(&a.id), "duplicate ANNCSU id {}", a.id);
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "ANNCSU CSV lacks requested access ids (or they are no longer eligible)"
    );
    Ok(found)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("ANNCSU file name")?;
    let mut pools: BTreeMap<(String, &'static str), BTreeMap<String, Address>> = BTreeMap::new();
    for_each(path, |a| {
        let pool = pools.entry((a.region.clone(), a.category())).or_default();
        let hash = sha256_hex(format!("{seed}\0{}", a.id).as_bytes());
        // ponytail: geographic diversity is limited to 200 candidates per region and class; enlarge the pool if it cannot supply five municipalities.
        if pool.len() < 200 || pool.last_key_value().is_some_and(|(k, _)| hash < *k) {
            pool.insert(hash, a);
            if pool.len() > 200 {
                pool.pop_last();
            }
        }
        Ok(())
    })?;
    let mut lines = vec![];
    for ((region, category), pool) in pools {
        let mut places = BTreeSet::new();
        let selected: Vec<_> = pool
            .values()
            .filter(|a| places.insert(&a.municipality_code))
            .take(5)
            .collect();
        eprintln!("ANNCSU {region} {category}: {} locations", selected.len());
        for a in selected {
            let label = a.query(&a.street, &a.municipality, true);
            for (form, query) in a.queries() {
                lines.push(json!({"kind":"address", "source":"anncsu", "ref":a.id, "cohort":format!("it_{category}"), "queries":[query], "label":label, "tags":["registry-sample", "it", "anncsu", release, format!("region:{region}"), format!("municipality:{}", a.municipality_code), category, form, format!("method:{}", if a.method.is_empty() { "none" } else { &a.method })]}));
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible ANNCSU addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "anncsu",
    command: "sample-it",
    country: "it",
    input: "anncsu",
    license: "CC-BY-4.0",
    location: "anncsu",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert("dataset".into(), json!("https://www.dati.gov.it/view-dataset/dataset?id=stradari-ed-indirizzari-regionali-e-nazionali-da-anncsu"));
    metadata.insert(
        "municipality_names".into(),
        json!("Istat SITUAS, municipalities on 2026-09-15, joined on the cadastral code"),
    );
    metadata.insert("coordinates".into(), json!("COORD_Y_COMUNE/COORD_X_COMUNE, ETRF2000 (epoch 2008.0) latitude/longitude, used as WGS84 without transformation"));
    lookup(path, refs)?.into_iter().map(|(id, a)| {
        let mut provenance = metadata.clone();
        provenance.insert("record".into(), json!({"progressivo_accesso":a.id, "progressivo_nazionale":a.street_id, "codice_comune":a.municipality_code, "odonimo":a.street, "dizione_lingua1":a.street_lang1, "dizione_lingua2":a.street_lang2, "localita":a.locality, "civico":a.number, "esponente":a.exponent, "specificita":a.specificity, "metodo":a.method}));
        Ok((id, Entry { point: Point::Wgs84(a.point), components: indexmap::IndexMap::from([("house_number", a.number())]), radius_floor_km: 0., provenance, number_aliases: a.number_aliases(), places: super::places([a.locality.as_str(), &a.municipality, &a.local_name]) }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eligible_numbers_language_forms_and_order_independent_sampling() {
        let path = std::env::temp_dir().join(format!("anncsu-test-{}.csv", std::process::id()));
        let header = format!("{}\n", COLUMNS.join(","));
        let rows = [
            "1,10,A952,Bolzano,Bolzano/Bozen,BZ,Trentino-Alto Adige/Südtirol,VIA RIENZA,RIENZSTRASSE,,GRIES,12,A,,,,3,1,46.5,11.35\n",
            "2,11,D969,Genova,Genova,GE,Liguria,VIA ROMA,,,,5,,N,,,1,1,44.4,8.9\n",
            "3,11,D969,Genova,Genova,GE,Liguria,VIA ROMA,,,,5,,R,,,1,1,44.4,8.9\n",
            "4,12,A944,Bologna,Bologna,BO,Emilia-Romagna,VIA EMILIA,,,BORGO PANIGALE,7,1,,,,4,1,44.5,11.3\n",
            "8,12,A944,Bologna,Bologna,BO,Emilia-Romagna,\"VIA GANDHI, MONDHAS K\",,,,7,,,,,4,1,44.5,11.3\n",
            "5,12,A944,Bologna,Bologna,BO,Emilia-Romagna,VIA EMILIA,,,,8,,,,,4,2,44.5,11.3\n",
            "6,12,A944,Bologna,Bologna,BO,Emilia-Romagna,VIA EMILIA,,,,9,,,120,,4,1,44.5,11.3\n",
            "7,13,D312,Doberdò del Lago,Doberdò del Lago-Doberdob,GO,Friuli-Venezia Giulia,VIA TRIESTE,TRŽAŠKA ULICA,,,3,BIS,,,,5,1,45.8,13.5\n",
        ];
        let write =
            |rows: &[&str]| std::fs::write(&path, format!("{header}{}", rows.concat())).unwrap();
        write(&rows);
        let first = sample(&path, "test").unwrap();
        let queries: Vec<_> = first
            .iter()
            .map(|s| s["queries"][0].as_str().unwrap())
            .collect();
        assert_eq!(
            queries,
            [
                "VIA EMILIA 7/1, BORGO PANIGALE, Bologna (BO), Italy",
                "VIA TRIESTE 3/BIS, Doberdò del Lago (GO), Italy",
                "TRŽAŠKA ULICA 3/BIS, Doberdob (GO), Italy",
                "VIA ROMA 5, Genova (GE), Italy",
                "VIA RIENZA 12/A, GRIES, Bolzano (BZ), Italy",
                "RIENZSTRASSE 12/A, Bozen (BZ), Italy",
            ]
        );
        let mut reversed = rows;
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let found = lookup(&path, &BTreeSet::from(["1".into(), "4".into()])).unwrap();
        assert_eq!(
            found["1"].number_aliases(),
            Some(BTreeSet::from(["12/A".into(), "12A".into()]))
        );
        assert_eq!(found["4"].number_aliases(), None);
        let expect = crate::registry::with_number_aliases(
            json!({"country":["it"], "level":"house", "max_level":"house", "point":found["1"].point, "radius_km":0.15, "good_match":true, "components":{"house_number":"12/A"}}),
            &found["1"].number_aliases(),
        );
        let expect: crate::score::Expect = serde_json::from_value(expect).unwrap();
        for number in ["12/A", "12a", "12", "12/B"] {
            let response = json!({"goodMatch":true, "geoCoordinates":{"lat":46.5,"lon":11.35}, "addressDetails":{"countryCode":"it","road":"Via Rienza","houseNumber":number}});
            let class = crate::score::score(Some(&expect), &response, true).class;
            assert_eq!(
                class == "pass",
                ["12/A", "12a"].contains(&number),
                "{number}: {class}"
            );
        }
        assert!(lookup(&path, &BTreeSet::from(["3".into()])).is_err());
        write(&[rows[1], rows[1]]);
        assert!(
            lookup(&path, &BTreeSet::from(["2".into()]))
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        write(&["1,10,A952,Bolzano,Bolzano,BZ,X,VIA RIENZA,,,,12,,,,,3,1,5170000,680000\n"]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("Italian")
        );
        std::fs::remove_file(path).unwrap();
    }
}
