use super::{NO_PLACE, country, fold, geo::Assigner, sources::csv_fields, with_cell};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

pub const NOTICE: &str = "Byggir á upplýsingum frá HMS, Staðfangaskrá (https://hms.is/stadfangaskra).\nUsed under Leiðbeiningar varðandi endurnot opinberra upplýsinga, the Icelandic public information reuse terms stated in the dataset metadata.\nChanges: official HEITI_NF/HEITI_TGF aliases and numberless address points geographically joined to indexed streets. HMS has not endorsed this index.\n";

#[derive(Default)]
pub struct Street {
    pub aliases: Vec<String>,
    pub numberless: Vec<(String, f64, f64)>,
}

pub fn read(
    path: &Path,
    sha256: &str,
    assigner: &Assigner,
) -> Result<HashMap<(u32, String), Street>> {
    ensure!(
        sha256.len() == 64
            && sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "--iceland-addresses-sha256 must be a lowercase SHA-256"
    );
    ensure!(
        super::store::sha256(path)? == sha256,
        "Iceland capture SHA-256 mismatch"
    );
    let text = std::fs::read_to_string(path)?;
    let mut lines = text.lines();
    let header = csv_fields(lines.next().context("empty Iceland capture")?)?;
    let at = |name: &str| {
        header
            .iter()
            .position(|h| h == name)
            .with_context(|| format!("Iceland capture lacks {name}"))
    };
    let (id, nf, tgf, number, letter, suffix, pc, lat, lon, review) = (
        at("HNITNUM")?,
        at("HEITI_NF")?,
        at("HEITI_TGF")?,
        at("HUSNR")?,
        at("BOKST")?,
        at("VIDSK")?,
        at("POSTNR")?,
        at("N_HNIT_WGS84")?,
        at("E_HNIT_WGS84")?,
        at("YFIRFARID")?,
    );
    let mut seen = HashSet::new();
    let mut streets = HashMap::<(u32, String), Street>::new();
    for (i, line) in lines.enumerate() {
        let row = csv_fields(line).with_context(|| format!("Iceland row {}", i + 2))?;
        ensure!(
            row.len() == header.len(),
            "Iceland row {} has the wrong width",
            i + 2
        );
        ensure!(
            !row[id].is_empty() && seen.insert(row[id].to_string()),
            "duplicate or empty Iceland HNITNUM"
        );
        let (x, y): (f64, f64) = (row[lon].parse()?, row[lat].parse()?);
        ensure!(
            (-24.7..=-13.3).contains(&x) && (63.0..=66.7).contains(&y),
            "Iceland coordinate outside WGS84 bounds"
        );
        if !assigner.within(x, y) || matches!(&*row[review], "2" | "9") || row[nf].is_empty() {
            continue;
        }
        let place = assigner
            .place_in(x, y, Some(country(Some("IS"))))
            .unwrap_or(NO_PLACE);
        let key = with_cell(&fold(&row[nf]), x, y, &mut String::new()).to_string();
        let street = streets.entry((place, key)).or_default();
        if !row[tgf].is_empty()
            && row[tgf] != row[nf]
            && !street.aliases.iter().any(|a| a == &*row[tgf])
        {
            street.aliases.push(row[tgf].to_string());
        }
        if row[number].is_empty()
            && row[letter].is_empty()
            && row[suffix].is_empty()
            && row[pc].len() == 3
            && row[pc].bytes().all(|b| b.is_ascii_digit())
        {
            street.numberless.push((row[pc].to_string(), x, y));
        }
    }
    ensure!(!seen.is_empty(), "empty Iceland capture");
    for street in streets.values_mut() {
        street.aliases.sort();
        street.numberless.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.1.total_cmp(&b.1))
                .then(a.2.total_cmp(&b.2))
        });
        street.numberless.dedup();
    }
    Ok(streets)
}
