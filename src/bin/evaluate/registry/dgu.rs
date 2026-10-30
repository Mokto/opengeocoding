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

pub const URL: &str =
    "https://catalogodatos.gub.uy/dataset/ide-direcciones-geograficas-del-uruguay";

/// The columns `tests/registry/dgu-csv.sql` writes.
const COLUMNS: [&str; 15] = [
    "latitud",
    "longitud",
    "codigo_postal",
    "codigo_via",
    "nombre_via",
    "num_puerta",
    "letra_puerta",
    "km",
    "manzana",
    "solar",
    "nombre_inmueble",
    "localidad",
    "codigo_localidad",
    "departamento",
    "copies",
];

/// Each department as the register names it, its ISO 3166-2 code and its capital's locality.
const DEPARTMENTS: [(&str, &str, &str); 19] = [
    ("ARTIGAS", "UY-AR", "ARTIGAS"),
    ("CANELONES", "UY-CA", "CANELONES"),
    ("CERRO LARGO", "UY-CL", "MELO"),
    ("COLONIA", "UY-CO", "COLONIA DEL SACRAMENTO"),
    ("DURAZNO", "UY-DU", "DURAZNO"),
    ("FLORES", "UY-FS", "TRINIDAD"),
    ("FLORIDA", "UY-FD", "FLORIDA"),
    ("LAVALLEJA", "UY-LA", "MINAS"),
    ("MALDONADO", "UY-MA", "MALDONADO"),
    ("MONTEVIDEO", "UY-MO", "MONTEVIDEO"),
    ("PAYSANDU", "UY-PA", "PAYSANDU"),
    ("RIO NEGRO", "UY-RN", "FRAY BENTOS"),
    ("RIVERA", "UY-RV", "RIVERA"),
    ("ROCHA", "UY-RO", "ROCHA"),
    ("SALTO", "UY-SA", "SALTO"),
    ("SAN JOSE", "UY-SJ", "SAN JOSE DE MAYO"),
    ("SORIANO", "UY-SO", "MERCEDES"),
    ("TACUAREMBO", "UY-TA", "TACUAREMBO"),
    ("TREINTA Y TRES", "UY-TT", "TREINTA Y TRES"),
];

#[derive(Clone, Debug)]
struct Address {
    id: String,
    department: String,
    iso: &'static str,
    capital: bool,
    locality: String,
    street: String,
    /// The door number with its letter: "807", "807A", "807 BIS".
    number: String,
    postcode: String,
    point: [f64; 2],
    record: Value,
}

impl Address {
    fn class(&self) -> &'static str {
        if self.number.ends_with(" BIS") {
            "bis"
        } else if self.number.ends_with(|c: char| c.is_ascii_uppercase()) {
            "letter"
        } else {
            "plain"
        }
    }

    fn setting(&self) -> &'static str {
        if self.capital { "capital" } else { "interior" }
    }

    /// Plain numbers by department and setting, two each; lettered ones by department, one each.
    fn stratum(&self) -> ((String, &'static str, &'static str), usize) {
        match self.class() {
            "plain" => ((self.department.clone(), self.setting(), "plain"), 2),
            class => ((self.department.clone(), "", class), 1),
        }
    }

    /// "SARANDI 673, 85000 TRINIDAD, FLORES, Uruguay"; a locality named as its department
    /// ("SALTO", "MONTEVIDEO") is not repeated.
    fn query(&self, postcode: bool) -> String {
        let town = if postcode {
            format!("{} {}", self.postcode, self.locality)
        } else {
            self.locality.clone()
        };
        let mut parts = vec![format!("{} {}", self.street, self.number), town];
        if self.department != self.locality {
            parts.push(self.department.clone());
        }
        parts.push("Uruguay".into());
        parts.join(", ")
    }
}

/// A name of capitals, digits, spaces and the register's punctuation, with two letters in a row,
/// so not a placeholder such as "UYCALDS_SN_003" or a name with quotes or brackets.
fn is_name(name: &str) -> bool {
    name.trim() == name
        && name
            .chars()
            .all(|c| c.is_uppercase() || c.is_ascii_digit() || " .'-".contains(c))
        && name
            .chars()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|w| w.iter().all(|c| c.is_alphabetic()))
}

/// The door number with an eligible letter ("807", "807A", "807 BIS"), else None.
fn number(number: &str, letter: &str) -> Option<String> {
    let ok = number.starts_with(|c: char| ('1'..='9').contains(&c))
        && number.len() <= 5
        && number.bytes().all(|b| b.is_ascii_digit());
    match letter {
        _ if !ok => None,
        "N/A" => Some(number.into()),
        "BIS" => Some(format!("{number} BIS")),
        l if l.len() == 1 && l.bytes().all(|b| b.is_ascii_uppercase()) => {
            Some(format!("{number}{l}"))
        }
        _ => None,
    }
}

fn rows(path: &Path, mut visit: impl FnMut(usize, &[&str]) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty DGU address CSV"
    );
    ensure!(
        line.trim_end().split(',').eq(COLUMNS),
        "{} is not the DGU address file tests/registry/dgu-csv.sql writes",
        path.display()
    );
    let mut n = 0;
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(
            fields.len() == COLUMNS.len(),
            "DGU address CSV row {n}: column count"
        );
        let fields: Vec<&str> = fields.iter().map(|f| f.as_ref()).collect();
        visit(n, &fields)?;
    }
    Ok(())
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let column = |name: &str| COLUMNS.iter().position(|c| *c == name).unwrap();
    // Localities the register files under more than one department ("LA PAZ" in Canelones and
    // Montevideo, the "... RURAL" areas) are border spill-overs, not where an address is written.
    let mut departments: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    rows(path, |_, f| {
        departments
            .entry(f[column("codigo_localidad")].into())
            .or_default()
            .insert(f[column("departamento")].into());
        Ok(())
    })?;
    let mut eligible = 0usize;
    let mut total = 0usize;
    rows(path, |n, f| {
        total = n;
        let field = |name: &str| f[column(name)];
        let coordinate = |name: &str| {
            field(name)
                .parse::<f64>()
                .context(format!("DGU address {name}"))
        };
        let (lat, lon) = (coordinate("latitud")?, coordinate("longitud")?);
        ensure!(
            (-35.1..-30.0).contains(&lat) && (-58.5..-53.0).contains(&lon),
            "DGU address CSV row {n}: needs WGS84 latitud, longitud in Uruguay"
        );
        let department = field("departamento");
        let (_, iso, capital) = *DEPARTMENTS
            .iter()
            .find(|(d, ..)| *d == department)
            .with_context(|| format!("DGU address CSV row {n}: department {department:?}"))?;
        let (locality, street, postcode) = (
            field("localidad"),
            field("nombre_via"),
            field("codigo_postal"),
        );
        let Some(number) = number(field("num_puerta"), field("letra_puerta")) else {
            return Ok(());
        };
        if field("copies") != "1"
            || field("km") != "N/A"
            || !is_name(street)
            || !is_name(locality)
            || locality.ends_with(" RURAL")
            || departments[field("codigo_localidad")].len() != 1
            || postcode.len() != 5
            || !postcode.bytes().all(|b| b.is_ascii_digit())
        {
            return Ok(());
        }
        eligible += 1;
        let suffix = match field("letra_puerta") {
            "N/A" => String::new(),
            letter => format!("-{letter}"),
        };
        visit(Address {
            id: format!(
                "{}-{}-{}{suffix}",
                field("codigo_localidad"),
                field("codigo_via"),
                field("num_puerta")
            ),
            department: department.into(),
            iso,
            capital: locality == capital,
            locality: locality.into(),
            street: street.into(),
            number,
            postcode: postcode.into(),
            point: [lat, lon],
            record: json!(
                COLUMNS
                    .iter()
                    .filter(|c| **c != "copies")
                    .map(|c| (c.to_string(), json!(field(c))))
                    .collect::<serde_json::Map<_, _>>()
            ),
        })
    })?;
    eprintln!("DGU addresses: {total} rows, {eligible} eligible");
    Ok(())
}

fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(!found.contains_key(&a.id), "duplicate DGU address {}", a.id);
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "DGU address CSV lacks requested addresses (or they are no longer eligible)"
    );
    Ok(found)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("DGU address file name")?;
    let mut picks: BTreeMap<_, Vec<(String, Address)>> = BTreeMap::new();
    for_each(path, |a| {
        let hash = sha256_hex(format!("{seed}\0{}", a.id).as_bytes());
        let (key, n) = a.stratum();
        let kept = picks.entry(key).or_default();
        let at = kept.partition_point(|(h, _)| *h < hash);
        if at < n {
            kept.insert(at, (hash, a));
            kept.truncate(n);
        }
        Ok(())
    })?;
    let mut lines = vec![];
    for a in picks.into_values().flatten().map(|(_, a)| a) {
        let (query, class) = (a.query(true), a.class());
        let base = json!({"kind":"address", "source":"dgu", "ref":a.id, "cohort":format!("uy_{class}"), "queries":[query], "label":query, "tags":["registry-sample", "uy", "dgu", release, format!("department:{}", a.department), a.setting(), class], "extra_regions":[a.iso]});
        let mut bare = base.clone();
        bare["cohort"] = json!("uy_no_postcode");
        bare["queries"] = json!([a.query(false)]);
        bare["tags"]
            .as_array_mut()
            .unwrap()
            .push(json!("no_postcode"));
        lines.extend([base, bare]);
    }
    eprintln!("DGU addresses: {} locations", lines.len() / 2);
    ensure!(!lines.is_empty(), "no eligible DGU addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "dgu",
    command: "sample-uy",
    country: "uy",
    input: "dgu",
    license: "Licencia-de-Datos-Abiertos-Uruguay-0.1",
    location: "dgu",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert("dataset".into(), json!(URL));
    metadata.insert(
        "coordinates".into(),
        json!("latitud, longitud: WGS84 (EPSG:4326) access point of the address as the register publishes it"),
    );
    metadata.insert(
        "attribution".into(),
        json!("Infraestructura de Datos Espaciales (IDE Uruguay), on information of Correo Uruguayo, Direcciones Geográficas del Uruguay; Licencia de Datos Abiertos – Uruguay; data date in source.release"),
    );
    lookup(path, refs)?
        .into_iter()
        .map(|(id, a)| {
            let mut provenance = metadata.clone();
            provenance.insert("record".into(), a.record.clone());
            Ok((
                id,
                Entry {
                    point: Point::Wgs84(a.point.map(|c| (c * 1e6).round() / 1e6)),
                    components: indexmap::IndexMap::from([
                        ("house_number", a.number.clone()),
                        ("postcode", a.postcode.clone()),
                        ("road", a.street.clone()),
                    ]),
                    radius_floor_km: 0.,
                    provenance,
                    number_aliases: None,
                    places: super::places([a.locality.as_str(), &a.department]),
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eligible_rows_query_forms_and_order_independent_sampling() {
        let path = std::env::temp_dir().join(format!("dgu-test-{}.csv", std::process::id()));
        let header = format!("{}\n", COLUMNS.join(","));
        let rows = [
            "-33.5181285914745,-56.9014376909592,85000,16173,SARANDI,673,N/A,N/A,107,0,N/A,TRINIDAD,844,FLORES,1\n",
            "-33.52,-56.90,85000,16173,SARANDI,675,BIS,N/A,N/A,N/A,N/A,TRINIDAD,844,FLORES,1\n",
            "-33.62,-57.10,85100,17000,18 DE JULIO,12,N/A,N/A,N/A,N/A,\"Escuela 5, rural\",ISMAEL CORTINAS,850,FLORES,1\n",
            "-34.8283477186247,-56.2192463520391,12900,8042,BULEVAR JOSE BATLLE Y ORDOÑEZ,6413,A,N/A,N/A,N/A,N/A,MONTEVIDEO,3180,MONTEVIDEO,1\n",
            "-34.82,-56.21,12900,8042,BULEVAR JOSE BATLLE Y ORDOÑEZ,6415,N/A,N/A,N/A,N/A,N/A,MONTEVIDEO,3180,MONTEVIDEO,2\n",
            "-34.82,-56.21,12900,9000,UYMO_SN_003,10,N/A,N/A,N/A,N/A,N/A,MONTEVIDEO,3180,MONTEVIDEO,1\n",
            "-34.82,-56.21,12900,8042,BULEVAR JOSE BATLLE Y ORDOÑEZ,6417,DU,N/A,N/A,N/A,N/A,MONTEVIDEO,3180,MONTEVIDEO,1\n",
            "-34.75,-55.76,15900,7000,ARTIGAS,20,N/A,N/A,N/A,N/A,N/A,LA PAZ,392,MONTEVIDEO,1\n",
            "-34.75,-55.76,15900,7000,ARTIGAS,22,N/A,N/A,N/A,N/A,N/A,LA PAZ,392,CANELONES,1\n",
            "-33.52,-56.90,85000,16173,SARANDI,0,N/A,N/A,N/A,N/A,N/A,TRINIDAD,844,FLORES,1\n",
            "-33.52,-56.90,85000,16173,SARANDI,677,N/A,N/A,N/A,N/A,N/A,TRINIDAD,844,FLORES,1\n",
            "-33.52,-56.90,85000,16173,SARANDI,679,N/A,N/A,N/A,N/A,N/A,TRINIDAD,844,FLORES,1\n",
        ];
        let write =
            |rows: &[&str]| std::fs::write(&path, format!("{header}{}", rows.concat())).unwrap();
        write(&rows);
        let first = sample(&path, "test").unwrap();
        let (trinidad, queries): (BTreeSet<_>, BTreeSet<_>) = first
            .iter()
            .map(|s| s["queries"][0].as_str().unwrap())
            .partition(|q| q.starts_with("SARANDI 67") && !q.contains("BIS"));
        assert_eq!(trinidad.len(), 4, "two plain numbers in the capital");
        assert_eq!(
            queries,
            BTreeSet::from([
                "SARANDI 675 BIS, 85000 TRINIDAD, FLORES, Uruguay",
                "SARANDI 675 BIS, TRINIDAD, FLORES, Uruguay",
                "18 DE JULIO 12, 85100 ISMAEL CORTINAS, FLORES, Uruguay",
                "18 DE JULIO 12, ISMAEL CORTINAS, FLORES, Uruguay",
                "BULEVAR JOSE BATLLE Y ORDOÑEZ 6413A, 12900 MONTEVIDEO, Uruguay",
                "BULEVAR JOSE BATLLE Y ORDOÑEZ 6413A, MONTEVIDEO, Uruguay",
            ])
        );
        assert!(
            first
                .iter()
                .all(|s| s["extra_regions"][0].as_str().unwrap().starts_with("UY-"))
        );
        let mut reversed = rows.to_vec();
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let found = lookup(
            &path,
            &BTreeSet::from(["844-16173-675-BIS".into(), "3180-8042-6413-A".into()]),
        )
        .unwrap();
        assert_eq!(found["844-16173-675-BIS"].class(), "bis");
        assert_eq!(found["3180-8042-6413-A"].class(), "letter");
        assert_eq!(found["3180-8042-6413-A"].number, "6413A");
        for id in [
            "3180-8042-6415",
            "3180-9000-10",
            "3180-8042-6417-DU",
            "392-7000-20",
            "844-16173-0",
        ] {
            assert!(lookup(&path, &BTreeSet::from([id.into()])).is_err(), "{id}");
        }
        write(&[&rows[0].replace("-33.5181285914745,-56.9014376909592", "-56.90,-33.51")]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("WGS84")
        );
        std::fs::remove_file(path).unwrap();
    }
}
