use super::{Entry, Point, Registry};
use anyhow::{Context, Result, ensure};
use flate2::read::MultiGzDecoder;
use opengeocoding::eval::sha256_hex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::Path,
};

pub const URL: &str = "https://www.inegi.org.mx/app/descarga/?ti=6";

const COLUMNS: [&str; 23] = [
    "id",
    "cve_ent",
    "entidad",
    "cve_mun",
    "municipio",
    "cve_loc",
    "localidad",
    "tipo_asent",
    "nomb_asent",
    "tipo_vial",
    "nom_vial",
    "numero_ext",
    "letra_ext",
    "cod_postal",
    "per_ocu",
    "tipo_uni_eco",
    "centro_comercial",
    "fecha_alta",
    "establecimientos",
    "codigos_postales",
    "dispersion_m",
    "latitud",
    "longitud",
];

/// Locations per state and class, each from a different municipality.
const TAKE: [(&str, usize); 2] = [("seat", 3), ("locality", 2)];

/// Kilometre posts, not street numbers, or no street at all.
const NOT_STREETS: [&str; 3] = ["OTRO(ESPECIFIQUE)", "CARRETERA", "AUTOPISTA"];

/// One establishment's address: only the address fields `tests/registry/denue-csv.sql` keeps.
#[derive(Clone, Debug)]
struct Address {
    id: String,
    state_code: String,
    state: String,
    municipality_code: String,
    municipality: String,
    locality_code: String,
    locality: String,
    settlement_type: String,
    settlement: String,
    street_type: String,
    street: String,
    number: String,
    letter: String,
    postcode: String,
    point: [f64; 2],
}

/// "Ciudad Morelos (Cuervos)" and "La Noria [Parque Industrial]" are written without the
/// bracketed alternative.
fn primary(name: &str) -> &str {
    let end = [" (", " ["].iter().filter_map(|p| name.find(p)).min();
    &name[..end.unwrap_or(name.len())]
}

/// "CALLE UNION", but "CALLE DEL OLMO" for a street the register already names "CALLE DEL OLMO",
/// and no type that says nothing ("OTRO").
fn typed(kind: &str, name: &str) -> String {
    let name = primary(name);
    if kind.is_empty() || kind == "OTRO" || name.starts_with(&format!("{kind} ")) {
        name.into()
    } else {
        format!("{kind} {name}")
    }
}

impl Address {
    fn locality_name(&self) -> &str {
        primary(&self.locality)
    }

    fn number(&self) -> String {
        if self.letter.is_empty() {
            self.number.clone()
        } else {
            format!("{}-{}", self.number, self.letter)
        }
    }

    fn town(&self) -> String {
        let locality = self.locality_name();
        if locality == self.municipality {
            format!("{locality}, {}", self.state)
        } else {
            format!("{locality}, {}, {}", self.municipality, self.state)
        }
    }

    /// `full` in the order of INEGI's address standard (Norma Técnica sobre Domicilios
    /// Geográficos), "CALLE UNION 127, COLONIA ZONA CENTRO, 20000 Aguascalientes,
    /// Aguascalientes, Mexico"; `short` with the locality and state only. "CALLE 67 #10" on a
    /// numbered street; a settlement named after the locality is left out.
    fn queries(&self) -> [(&'static str, String); 2] {
        let street = typed(&self.street_type, &self.street);
        let hash = if street.ends_with(|c: char| c.is_ascii_digit()) {
            "#"
        } else {
            ""
        };
        let street = format!("{street} {hash}{}", self.number());
        let fold = |s: &str| deunicode::deunicode(s).to_uppercase();
        let settlement = primary(&self.settlement);
        let settlement = if settlement.is_empty() || fold(settlement) == fold(self.locality_name())
        {
            String::new()
        } else {
            format!("{}, ", typed(&self.settlement_type, settlement))
        };
        [
            (
                "full",
                format!(
                    "{street}, {settlement}{} {}, Mexico",
                    self.postcode,
                    self.town()
                ),
            ),
            (
                "short",
                format!("{street}, {}, {}, Mexico", self.locality_name(), self.state),
            ),
        ]
    }

    fn category(&self) -> &'static str {
        if self.locality_code == "0001" {
            "seat"
        } else {
            "locality"
        }
    }

    /// Mexico City's boroughs are its municipalities, and the city is its state.
    fn places(&self) -> Vec<String> {
        super::places(
            [self.locality_name(), &self.locality, &self.municipality]
                .into_iter()
                .chain((self.state_code == "09").then_some(self.state.as_str())),
        )
    }
}

fn clean(s: &str) -> bool {
    !s.contains([',', 'Ã', 'Â']) && !s.chars().any(|c| ('\u{80}'..='\u{9f}').contains(&c))
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::with_capacity(1 << 20, MultiGzDecoder::new(File::open(path)?));
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty DENUE CSV"
    );
    let header = crate::registry::nad::split(line.trim_end(), ',');
    ensure!(
        header.iter().map(|s| s.as_ref()).eq(COLUMNS),
        "{} is not a CSV prepared by tests/registry/denue-csv.sql",
        path.display()
    );
    let (mut n, mut eligible) = (0usize, 0usize);
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        if n.is_multiple_of(1_000_000) {
            eprintln!("DENUE: {n} rows read");
        }
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(
            fields.len() == COLUMNS.len(),
            "DENUE CSV row {n}: column count"
        );
        let field = |i: usize| fields[i].trim();
        let lat: f64 = field(21).parse().context("DENUE latitude")?;
        let lon: f64 = field(22).parse().context("DENUE longitude")?;
        ensure!(
            (14.3..=32.8).contains(&lat) && (-118.5..=-86.6).contains(&lon),
            "DENUE CSV row {n}: needs Mexican latitude/longitude"
        );
        let (number, letter, postcode) = (field(11), field(12), field(13));
        let (street_type, street) = (field(9), field(10));
        // A small business is often its owner's home, and an older establishment is in the
        // 2015 DENUE copies OpenAddresses publishes; a number on a highway is a kilometre post;
        // an address several establishments share with different postcodes or points far
        // apart is not one place.
        if field(15) != "Fijo"
            || field(14).is_empty()
            || field(14) == "0 a 5 personas"
            || field(17) < "2016"
            || field(16) != "false"
            || !((1..=5).contains(&number.len())
                && !number.starts_with('0')
                && number.bytes().all(|b| b.is_ascii_digit()))
            || !(letter.is_empty()
                || letter.len() == 1 && letter.bytes().all(|b| b.is_ascii_uppercase()))
            || postcode.len() != 5
            || postcode.starts_with("00")
            || !postcode.bytes().all(|b| b.is_ascii_digit())
            || street_type.is_empty()
            || NOT_STREETS.contains(&street_type)
            || street == "NINGUNO"
            || !street.chars().any(char::is_alphanumeric)
            || field(19) != "1"
            || field(20).parse::<f64>().context("DENUE dispersion")? > 30.
            || [1, 2, 3, 4, 5, 6].iter().any(|&i| field(i).is_empty())
            || [2, 4, 6, 7, 8, 9, 10].iter().any(|&i| !clean(field(i)))
        {
            continue;
        }
        eligible += 1;
        visit(Address {
            id: field(0).into(),
            state_code: field(1).into(),
            state: field(2).into(),
            municipality_code: field(3).into(),
            municipality: field(4).into(),
            locality_code: field(5).into(),
            locality: field(6).into(),
            settlement_type: field(7).into(),
            settlement: field(8).into(),
            street_type: street_type.into(),
            street: street.into(),
            number: number.into(),
            letter: letter.into(),
            postcode: postcode.into(),
            point: [lat, lon],
        })?;
    }
    eprintln!("DENUE: {n} rows, {eligible} eligible");
    Ok(())
}

fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(!found.contains_key(&a.id), "duplicate DENUE id {}", a.id);
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "DENUE CSV lacks requested establishment ids (or they are no longer eligible)"
    );
    Ok(found)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("DENUE file name")?;
    let mut pools: BTreeMap<(String, &'static str), BTreeMap<String, Address>> = BTreeMap::new();
    for_each(path, |a| {
        let pool = pools
            .entry((a.state_code.clone(), a.category()))
            .or_default();
        let hash = sha256_hex(format!("{seed}\0{}", a.id).as_bytes());
        // ponytail: municipality diversity draws on the 200 smallest hashes per state and class; enlarge the pool if it cannot supply enough municipalities.
        if pool.len() < 200 || pool.last_key_value().is_some_and(|(k, _)| hash < *k) {
            pool.insert(hash, a);
            if pool.len() > 200 {
                pool.pop_last();
            }
        }
        Ok(())
    })?;
    let mut lines = vec![];
    for ((state, category), pool) in pools {
        let take = TAKE.iter().find(|(c, _)| *c == category).unwrap().1;
        let mut places = BTreeSet::new();
        let selected: Vec<_> = pool
            .values()
            .filter(|a| places.insert(&a.municipality_code))
            .take(take)
            .collect();
        eprintln!("DENUE {state} {category}: {} locations", selected.len());
        for a in selected {
            let [(_, label), _] = a.queries();
            for (form, query) in a.queries() {
                let mut tags = vec![
                    json!("registry-sample"),
                    json!("mx"),
                    json!("denue"),
                    json!(release),
                    json!(format!("state:{state}")),
                    json!(format!("municipality:{state}{}", a.municipality_code)),
                    json!(category),
                    json!(form),
                ];
                if !a.letter.is_empty() {
                    tags.push(json!("letter"));
                }
                lines.push(json!({"kind":"address", "source":"denue", "ref":a.id, "cohort":format!("mx_{category}"), "queries":[query], "label":label, "tags":tags}));
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible DENUE addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "denue",
    command: "sample-mx",
    country: "mx",
    input: "denue",
    license: "INEGI-Terminos-de-Libre-Uso",
    location: "denue",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert(
        "dataset".into(),
        json!(
            "Fuente: INEGI, Directorio Estadístico Nacional de Unidades Económicas (DENUE) 05_2026"
        ),
    );
    metadata.insert(
        "coordinates".into(),
        json!("latitud, longitud of the establishment, WGS84"),
    );
    metadata.insert(
        "transformation".into(),
        json!("address fields only, prepared by tests/registry/denue-csv.sql; queries constructed from them, not by INEGI"),
    );
    lookup(path, refs)?
        .into_iter()
        .map(|(id, a)| {
            let mut provenance = metadata.clone();
            provenance.insert("record".into(), json!({"id":id, "cve_ent":a.state_code, "entidad":a.state, "cve_mun":a.municipality_code, "municipio":a.municipality, "cve_loc":a.locality_code, "localidad":a.locality, "tipo_asent":a.settlement_type, "nomb_asent":a.settlement, "tipo_vial":a.street_type, "nom_vial":a.street, "numero_ext":a.number, "letra_ext":a.letter, "cod_postal":a.postcode}));
            Ok((
                id,
                Entry {
                    point: Point::Wgs84(a.point),
                    components: indexmap::IndexMap::from([
                        ("house_number", a.number()),
                        ("postcode", a.postcode.clone()),
                    ]),
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
    use std::io::Write;

    #[test]
    fn eligible_addresses_query_forms_and_order_independent_sampling() {
        let path = std::env::temp_dir().join(format!("denue-test-{}.csv.gz", std::process::id()));
        let header = format!("{}\n", COLUMNS.join(","));
        let row = |id: &str, edit: &dyn Fn(&mut Vec<String>)| {
            let mut f: Vec<String> = "01,Aguascalientes,001,Aguascalientes,0001,Aguascalientes,COLONIA,ZONA CENTRO,CALLE,UNION,127,\"\",20000,31 a 50 personas,Fijo,false,2019-11,1,1,0.0,21.88427837,-102.2969435"
                .split(',')
                .map(String::from)
                .collect();
            edit(&mut f);
            format!("{id},{}\n", f.join(","))
        };
        let rows = [
            row("1", &|_| {}),
            row("2", &|f| {
                [f[2], f[3], f[4], f[5]] = [
                    "002".into(),
                    "Asientos".into(),
                    "0010".into(),
                    "Villa Juárez (El Llano)".into(),
                ];
                [f[6], f[7]] = ["OTRO".into(), "VILLA JUAREZ".into()];
                [f[10], f[11]] = ["45".into(), "B".into()];
            }),
            row("3", &|f| f[13] = "0 a 5 personas".into()),
            row("4", &|f| f[16] = "2014-12".into()),
            row("5", &|f| f[8] = "CARRETERA".into()),
            row("6", &|f| f[10] = "SN".into()),
            row("7", &|f| f[12] = "2000".into()),
            row("8", &|f| f[18] = "2".into()),
            row("9", &|f| f[19] = "45.0".into()),
            row("10", &|f| f[15] = "true".into()),
            row("11", &|f| f[9] = "\"MÃ\u{81}RQUEZ\"".into()),
            row("12", &|f| f[11] = "AB".into()),
            row("13", &|f| f[9] = "NINGUNO".into()),
            row("14", &|f| {
                [f[2], f[3], f[5]] = ["003".into(), "Calvillo".into(), "Calvillo".into()];
                [f[6], f[7], f[9]] = ["OTRO".into(), "PLAYA NORTE".into(), "67".into()];
            }),
            row("15", &|f| {
                [f[2], f[3], f[5]] = ["005".into(), "Jesús María".into(), "Jesús María".into()];
                [f[6], f[7]] = ["PUERTO".into(), "PUERTO CHICO".into()];
                f[9] = "UNION (DEL DISTRITO)".into();
            }),
            row("16", &|f| {
                [f[2], f[3], f[4], f[5]] = [
                    "007".into(),
                    "Rincón de Romos".into(),
                    "0020".into(),
                    "La Noria [Parque Industrial]".into(),
                ];
                f[9] = "CALLE DEL OLMO".into();
            }),
        ];
        let write = |rows: &[String]| {
            let mut gz = flate2::write::GzEncoder::new(
                File::create(&path).unwrap(),
                flate2::Compression::default(),
            );
            gz.write_all(format!("{header}{}", rows.concat()).as_bytes())
                .unwrap();
            gz.finish().unwrap();
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
                "CALLE UNION 45-B, 20000 Villa Juárez, Asientos, Aguascalientes, Mexico",
                "CALLE UNION 45-B, Villa Juárez, Aguascalientes, Mexico",
                "CALLE UNION 127, COLONIA ZONA CENTRO, 20000 Aguascalientes, Aguascalientes, Mexico",
                "CALLE UNION 127, Aguascalientes, Aguascalientes, Mexico",
                "CALLE 67 #127, PLAYA NORTE, 20000 Calvillo, Aguascalientes, Mexico",
                "CALLE 67 #127, Calvillo, Aguascalientes, Mexico",
                "CALLE UNION 127, PUERTO CHICO, 20000 Jesús María, Aguascalientes, Mexico",
                "CALLE UNION 127, Jesús María, Aguascalientes, Mexico",
                "CALLE DEL OLMO 127, COLONIA ZONA CENTRO, 20000 La Noria, Rincón de Romos, Aguascalientes, Mexico",
                "CALLE DEL OLMO 127, La Noria, Aguascalientes, Mexico",
            ])
        );
        let mut reversed = rows.clone();
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let entries = prepare(&path, &BTreeSet::from(["2".into()])).unwrap();
        assert_eq!(entries["2"].components["house_number"], "45-B");
        assert_eq!(entries["2"].components["postcode"], "20000");
        assert_eq!(
            entries["2"].places,
            ["Villa Juárez", "Villa Juárez (El Llano)", "Asientos"]
        );
        let record = &entries["2"].provenance["record"];
        assert_eq!(record.as_object().unwrap().len(), 14);
        for id in 3..=13 {
            assert!(
                lookup(&path, &BTreeSet::from([id.to_string()])).is_err(),
                "{id}"
            );
        }
        write(&[rows[0].clone(), rows[0].clone()]);
        assert!(
            lookup(&path, &BTreeSet::from(["1".into()]))
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        write(&[rows[0].replace("21.88427837,-102.2969435", "-102.2969435,21.88427837")]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("Mexican")
        );
        std::fs::remove_file(path).unwrap();
    }
}
