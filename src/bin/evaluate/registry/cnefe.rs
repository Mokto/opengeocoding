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

pub const URL: &str = "https://www.ibge.gov.br/estatisticas/sociais/populacao/38734-cadastro-nacional-de-enderecos-para-fins-estatisticos.html";

const COLUMNS: [&str; 22] = [
    "id",
    "uf",
    "municipality_code",
    "municipality",
    "street_type",
    "street_title",
    "street_name",
    "number",
    "modifier",
    "cep",
    "ceps",
    "locality",
    "localities",
    "rows",
    "units",
    "min_level",
    "max_level",
    "rural_rows",
    "species",
    "spread_m",
    "lat",
    "lon",
];

/// Locations per state and class.
const TAKE: [(&str, usize); 2] = [("urban", 4), ("rural", 2)];

/// One street number of the register: every row (household, establishment, unit) that shares
/// the municipality, street, number and modifier, as `tests/registry/cnefe-uf.sql` groups them.
#[derive(Clone, Debug)]
struct Address {
    id: String,
    uf: String,
    municipality_code: String,
    municipality: String,
    street: String,
    number: String,
    cep: String,
    locality: String,
    units: String,
    level: String,
    species: String,
    rural: bool,
    point: [f64; 2],
}

impl Address {
    fn postcode(&self) -> String {
        format!("{}-{}", &self.cep[..5], &self.cep[5..])
    }

    fn queries(&self) -> [(&'static str, String); 2] {
        let town = format!("{} - {}", self.municipality, self.uf);
        let mut full = vec![self.street.clone(), self.number.clone()];
        if self.locality != deunicode::deunicode(&self.municipality).to_uppercase() {
            full.push(self.locality.clone());
        }
        full.extend([town.clone(), self.postcode(), "Brazil".into()]);
        [
            ("full", full.join(", ")),
            (
                "short",
                format!("{}, {}, {town}, Brazil", self.street, self.number),
            ),
        ]
    }

    fn category(&self) -> &'static str {
        if self.rural { "rural" } else { "urban" }
    }
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::with_capacity(1 << 20, MultiGzDecoder::new(File::open(path)?));
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty CNEFE CSV"
    );
    let header = crate::registry::nad::split(line.trim_end(), ',');
    ensure!(
        header.iter().map(|s| s.as_ref()).eq(COLUMNS),
        "{} is not a CSV prepared by tests/registry/cnefe-csv.sql",
        path.display()
    );
    let (mut n, mut eligible) = (0usize, 0usize);
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        if n.is_multiple_of(10_000_000) {
            eprintln!("CNEFE: {n} rows read");
        }
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(
            fields.len() == COLUMNS.len(),
            "CNEFE CSV row {n}: column count"
        );
        let field = |i: usize| fields[i].trim();
        ensure!(
            !field(1).is_empty() && !field(3).is_empty(),
            "CNEFE CSV row {n}: municipality {} is not in the IBGE list",
            field(2)
        );
        let lat: f64 = field(20).parse().context("CNEFE latitude")?;
        let lon: f64 = field(21).parse().context("CNEFE longitude")?;
        ensure!(
            (-34. ..=6.).contains(&lat) && (-74. ..=-28.).contains(&lon),
            "CNEFE CSV row {n}: needs Brazilian latitude/longitude"
        );
        let count = |i: usize| field(i).parse::<u64>().context("CNEFE count");
        let (number, cep, street) = (field(7), field(9), [field(4), field(5), field(6)]);
        let (rows, rural_rows) = (count(13)?, count(17)?);
        // A number written with a modifier ("SN", "KM", a utility's pole or meter label) is not
        // a street number, and a street number found in two bairros or under two CEPs, or whose
        // rows are spread out, is several addresses.
        if !field(8).is_empty()
            || !((1..=5).contains(&number.len())
                && !number.starts_with('0')
                && number.bytes().all(|b| b.is_ascii_digit()))
            || cep.len() != 8
            || !cep.bytes().all(|b| b.is_ascii_digit())
            || count(10)? != 1
            || count(12)? != 1
            || field(11).is_empty()
            || !["1", "2"].contains(&field(16))
            || field(19).parse::<f64>().context("CNEFE spread")? > 30.
            || (rural_rows != 0 && rural_rows != rows)
            || street[0].is_empty()
            || !street[2].chars().any(char::is_alphanumeric)
            || street.iter().any(|s| s.contains(','))
        {
            continue;
        }
        eligible += 1;
        visit(Address {
            id: field(0).into(),
            uf: field(1).into(),
            municipality_code: field(2).into(),
            municipality: field(3).into(),
            street: street
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" "),
            number: number.into(),
            cep: cep.into(),
            locality: field(11).into(),
            units: field(14).into(),
            level: field(16).into(),
            species: field(18).into(),
            rural: rural_rows != 0,
            point: [lat, lon],
        })?;
    }
    eprintln!("CNEFE: {n} rows, {eligible} eligible");
    Ok(())
}

fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(!found.contains_key(&a.id), "duplicate CNEFE id {}", a.id);
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "CNEFE CSV lacks requested address ids (or they are no longer eligible)"
    );
    Ok(found)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("CNEFE file name")?;
    let mut pools: BTreeMap<(String, &'static str), BTreeMap<String, Address>> = BTreeMap::new();
    for_each(path, |a| {
        let pool = pools.entry((a.uf.clone(), a.category())).or_default();
        let hash = sha256_hex(format!("{seed}\0{}", a.id).as_bytes());
        // ponytail: geographic diversity is limited to 200 candidates per state and class; enlarge the pool if it cannot supply enough municipalities.
        if pool.len() < 200 || pool.last_key_value().is_some_and(|(k, _)| hash < *k) {
            pool.insert(hash, a);
            if pool.len() > 200 {
                pool.pop_last();
            }
        }
        Ok(())
    })?;
    let mut lines = vec![];
    for ((uf, category), pool) in pools {
        let take = TAKE.iter().find(|(c, _)| *c == category).unwrap().1;
        let mut places = BTreeSet::new();
        let selected: Vec<_> = pool
            .values()
            .filter(|a| places.insert(&a.municipality_code))
            .take(take)
            .collect();
        eprintln!("CNEFE {uf} {category}: {} locations", selected.len());
        for a in selected {
            let [(_, label), _] = a.queries();
            for (form, query) in a.queries() {
                lines.push(json!({"kind":"address", "source":"cnefe", "ref":a.id, "cohort":format!("br_{category}"), "queries":[query], "label":label, "tags":["registry-sample", "br", "cnefe", release, format!("uf:{uf}"), format!("municipality:{}", a.municipality_code), category, form, format!("level:{}", a.level), format!("units:{}", a.units), format!("species:{}", a.species)]}));
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible CNEFE addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "cnefe",
    command: "sample-br",
    country: "br",
    input: "cnefe",
    license: "BR-Decree-8777-2016",
    location: "cnefe",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert("dataset".into(), json!("https://ftp.ibge.gov.br/Cadastro_Nacional_de_Enderecos_para_Fins_Estatisticos/Censo_Demografico_2022/Arquivos_CNEFE/CSV/UF/"));
    metadata.insert(
        "municipality_names".into(),
        json!("IBGE localidades API, municipalities on 2026-10-22, joined on the IBGE code"),
    );
    metadata.insert("coordinates".into(), json!("LATITUDE/LONGITUDE of the street number's lowest COD_UNICO_ENDERECO, SIRGAS 2000 latitude/longitude, used as WGS84 without transformation"));
    lookup(path, refs)?.into_iter().map(|(id, a)| {
        let mut provenance = metadata.clone();
        provenance.insert("record".into(), json!({"cod_unico_endereco":a.id, "cod_municipio":a.municipality_code, "logradouro":a.street, "num_endereco":a.number, "cep":a.cep, "dsc_localidade":a.locality, "nv_geo_coord":a.level, "unidades":a.units, "cod_especie":a.species, "rural":a.rural}));
        Ok((id, Entry { point: Point::Wgs84(a.point), components: indexmap::IndexMap::from([("house_number", a.number.clone()), ("postcode", a.postcode())]), radius_floor_km: 0., provenance, number_aliases: None, places: super::places([a.locality.as_str(), &a.municipality]) }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn eligible_numbers_query_forms_and_order_independent_sampling() {
        let path = std::env::temp_dir().join(format!("cnefe-test-{}.csv.gz", std::process::id()));
        let header = format!("{}\n", COLUMNS.join(","));
        let rows = [
            "10,AC,1200401,Rio Branco,RUA,MAJOR,GUALTER,225,,69912130,1,SOBRAL,1,1,1,1,1,0,1,0.0,-10.002909,-67.836986\n",
            "11,AC,1200401,Rio Branco,RUA,,GOIAS,444,,69900126,1,RIO BRANCO,1,3,2,1,2,0,1 6,12.0,-9.972736,-67.819202\n",
            "12,AC,1200401,Rio Branco,RUA,,GOIAS,0,SN,69900126,1,IVETE VARGAS,1,1,1,1,1,0,6,0.0,-9.97,-67.81\n",
            "13,AC,1200401,Rio Branco,RUA,,ACRE,5,,69900126,2,IVETE VARGAS,1,2,2,1,1,0,1,0.0,-9.97,-67.81\n",
            "14,AC,1200401,Rio Branco,RUA,,PARA,7,,69900126,1,IVETE VARGAS,1,2,2,1,1,0,1,80.0,-9.97,-67.81\n",
            "15,AC,1200401,Rio Branco,RUA,,BAHIA,9,,69900126,1,IVETE VARGAS,1,1,1,3,3,0,1,0.0,-9.97,-67.81\n",
            "16,AC,1200302,Feijó,RAMAL,,DO PAULO,30760,,69960000,1,COLONIA SAO JOSE,1,1,1,1,1,1,3,0.0,-8.16,-70.35\n",
            "17,AC,1200302,Feijó,RUA,,\"BRASIL, A\",3,,69960000,1,CENTRO,1,1,1,1,1,0,1,0.0,-8.16,-70.35\n",
        ];
        let write = |rows: &[&str]| {
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
        let queries: Vec<_> = first
            .iter()
            .map(|s| s["queries"][0].as_str().unwrap())
            .collect();
        assert_eq!(
            queries,
            [
                "RAMAL DO PAULO, 30760, COLONIA SAO JOSE, Feijó - AC, 69960-000, Brazil",
                "RAMAL DO PAULO, 30760, Feijó - AC, Brazil",
                "RUA MAJOR GUALTER, 225, SOBRAL, Rio Branco - AC, 69912-130, Brazil",
                "RUA MAJOR GUALTER, 225, Rio Branco - AC, Brazil",
            ]
        );
        let mut reversed = rows;
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let found = lookup(&path, &BTreeSet::from(["11".into(), "16".into()])).unwrap();
        // the bairro is left out where the register names it after the municipality
        assert_eq!(
            found["11"].queries()[0].1,
            "RUA GOIAS, 444, Rio Branco - AC, 69900-126, Brazil"
        );
        assert!(found["16"].rural);
        let entries = prepare(&path, &BTreeSet::from(["10".into()])).unwrap();
        assert_eq!(entries["10"].components["house_number"], "225");
        assert_eq!(entries["10"].components["postcode"], "69912-130");
        for id in ["12", "13", "14", "15", "17"] {
            assert!(lookup(&path, &BTreeSet::from([id.into()])).is_err(), "{id}");
        }
        write(&[rows[0], rows[0]]);
        assert!(
            lookup(&path, &BTreeSet::from(["10".into()]))
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        write(&["10,,9999999,,RUA,,X,1,,69900126,1,X,1,1,1,1,1,0,1,0.0,-10.0,-67.8\n"]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("IBGE list")
        );
        write(&["10,AC,1200401,Rio Branco,RUA,,X,1,,69900126,1,X,1,1,1,1,1,0,1,0.0,45.0,9.0\n"]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("Brazilian")
        );
        std::fs::remove_file(path).unwrap();
    }
}
