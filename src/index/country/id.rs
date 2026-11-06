//! Indonesian addresses name their areas from the smallest up, one per comma: "Jl. Merdeka No. 5,
//! RT 02/RW 03, Desa Pepe, Kec. Tegowanu, Kab. Grobogan, Jawa Tengah 58165, Indonesia". libpostal
//! reads the village and district ("Pepe, Tegowanu") as a house name and keeps only the regency,
//! so the village was lost.

use crate::{
    index::{Hit, Index, NONE, Parsed, Place, name_key, parse::field},
    matching::{ParsedAddress, normalize_place, part, region_words},
    proto::opengeocoding::FullAddress,
};
use serde_json::Value;
use std::borrow::Cow;

#[derive(Clone, Copy, PartialEq, PartialOrd)]
enum Level {
    Village,
    District,
    Regency,
    Province,
}

fn prefixed(part: &str) -> Option<(Level, &str)> {
    let (word, rest) = part.split_once([' ', '.'])?;
    let level = match normalize_place(word).as_str() {
        "desa" | "ds" | "kelurahan" | "kel" | "gampong" | "nagari" => Level::Village,
        "kecamatan" | "kec" | "distrik" => Level::District,
        "kabupaten" | "kab" | "kota" | "kotamadya" | "kodya" => Level::Regency,
        "provinsi" | "prov" | "propinsi" => Level::Province,
        _ => return None,
    };
    let name = rest.trim_start_matches(['.', ' ']).trim();
    (!name.is_empty()).then_some((level, name))
}

/// "RT 02/RW 03", "RT.002 RW.003": neighbourhood units no data names.
fn neighbourhood_unit(part: &str) -> bool {
    let folded = normalize_place(part);
    folded.starts_with("rt ") || folded.starts_with("rw ") || folded == "rt" || folded == "rw"
}

/// A part that is not an area: a street, number, block or estate.
fn street_like(part: &str) -> bool {
    part.chars().any(|c| c.is_ascii_digit())
        || matches!(
            normalize_place(part).split(' ').next(),
            Some(
                "jl" | "jln"
                    | "jalan"
                    | "gg"
                    | "gang"
                    | "no"
                    | "blok"
                    | "komplek"
                    | "kompleks"
                    | "perum"
                    | "perumahan"
                    | "dusun"
                    | "dsn"
                    | "lingkungan"
                    | "ruko"
            )
        )
}

/// Provinces and Jakarta's cities as addresses shorten them, by the index's names.
fn abbreviation(name: &str) -> Option<&'static str> {
    Some(match normalize_place(name).as_str() {
        "nad" | "nanggroe aceh darussalam" => "Aceh",
        "sumut" => "Sumatera Utara",
        "sumbar" => "Sumatera Barat",
        "sumsel" => "Sumatera Selatan",
        "babel" => "Kepulauan Bangka Belitung",
        "kepri" => "Kepulauan Riau",
        "jabar" => "Jawa Barat",
        "jateng" => "Jawa Tengah",
        "jatim" => "Jawa Timur",
        "dki" | "dki jakarta" => "Daerah Khusus Ibukota Jakarta",
        "diy" | "di yogyakarta" | "d i yogyakarta" => "Daerah Istimewa Yogyakarta",
        "kalbar" => "Kalimantan Barat",
        "kalteng" => "Kalimantan Tengah",
        "kalsel" => "Kalimantan Selatan",
        "kaltim" => "Kalimantan Timur",
        "kaltara" => "Kalimantan Utara",
        "sulut" => "Sulawesi Utara",
        "sulteng" => "Sulawesi Tengah",
        "sulsel" => "Sulawesi Selatan",
        "sultra" => "Sulawesi Tenggara",
        "sulbar" => "Sulawesi Barat",
        "ntb" => "Nusa Tenggara Barat",
        "ntt" => "Nusa Tenggara Timur",
        "malut" => "Maluku Utara",
        "pabar" => "Papua Barat",
        "jakpus" => "Jakarta Pusat",
        "jakut" => "Jakarta Utara",
        "jakbar" => "Jakarta Barat",
        "jaksel" => "Jakarta Selatan",
        "jaktim" => "Jakarta Timur",
        _ => return None,
    })
}

/// An area part with its short forms written out: "Jatim", "Kota Adm. Jakarta Utara" ("Kota
/// Jakarta Utara"), "Kota Jkt Utara", "Kab. Kep. Selayar". Its label stays.
fn expand(part: &str) -> Cow<'_, str> {
    let (label, name) = match prefixed(part) {
        Some((_, name)) => (&part[..part.len() - name.len()], name),
        None => ("", part),
    };
    if let Some(full) = abbreviation(name) {
        return format!("{label}{full}").into();
    }
    let mut words: Vec<&str> = name.split(' ').collect();
    let administrative = !label.is_empty()
        && words.len() > 1
        && matches!(normalize_place(words[0]).as_str(), "adm" | "administrasi");
    if administrative {
        words.remove(0);
    }
    let mut changed = administrative;
    for word in &mut words {
        let full = match normalize_place(word).as_str() {
            "jkt" => "Jakarta",
            "kep" => "Kepulauan",
            _ => continue,
        };
        *word = full;
        changed = true;
    }
    if changed {
        format!("{label}{}", words.join(" ")).into()
    } else {
        part.into()
    }
}

/// Whether a part names a province of Indonesia, or one of Jakarta's cities by its short form:
/// an address that ends with one is Indonesian without saying so. Only a whole name counts, not
/// the words of one ("Highlands, NJ" is not Highland Papua).
fn indonesian(index: &Index, part: &str) -> bool {
    let part = expand(part);
    let name = prefixed(&part).map_or(&*part, |(_, name)| name);
    let words = region_words(name);
    abbreviation(part.as_ref()).is_some()
        || [
            "jakarta pusat",
            "jakarta utara",
            "jakarta barat",
            "jakarta selatan",
            "jakarta timur",
        ]
        .contains(&normalize_place(name).as_str())
        || index.region_spelt(name, "id").is_some_and(|p| {
            index.places[p as usize]
                .all_names()
                .any(|n| region_words(n) == words)
        })
}

/// libpostal's reading with the short forms written out and the implied country added, for
/// when [`aligned`] does not apply.
fn spelt_out(whole: &[(String, String)], implied: bool) -> Option<Parsed> {
    let mut parsed = whole.to_vec();
    let mut changed = implied;
    for (_, value) in &mut parsed {
        if let Some(full) = abbreviation(value) {
            *value = full.to_lowercase();
            changed = true;
        }
    }
    if implied {
        parsed.push(("country".into(), "indonesia".into()));
    }
    changed.then(|| parsed.into())
}

fn postal(text: &str) -> bool {
    text.len() == 5 && text.bytes().all(|b| b.is_ascii_digit())
}

pub(crate) fn aligned(
    index: &Index,
    query: &str,
    whole: &[(String, String)],
) -> anyhow::Result<Option<Parsed>> {
    let mut parts: Vec<&str> = query
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let countries: Vec<_> = parts.iter().filter_map(|p| index.country(p)).collect();
    if countries.iter().any(|cc| cc != "id") {
        return Ok(None);
    }
    parts.retain(|p| index.country(p).is_none());
    let mut postcode = None;
    if let Some(last) = parts.last_mut() {
        let (head, tail) = last.rsplit_once(' ').unwrap_or(("", last));
        if postal(tail) {
            postcode = Some(tail);
            *last = head.trim();
            if last.is_empty() {
                parts.pop();
            }
        }
    }

    let implied = countries.is_empty();
    if implied && !parts.last().is_some_and(|last| indonesian(index, last)) {
        return Ok(None);
    }
    let fallback = || Ok(spelt_out(whole, implied));

    let expanded: Vec<Cow<str>> = parts.iter().map(|p| expand(p)).collect();
    let mut levels: Vec<(Level, &str)> = Vec::new();
    let mut labelled = false;
    let mut street = parts.len();
    for (at, part) in expanded.iter().enumerate().rev() {
        let part = part.as_ref();
        let below = levels.last().map(|(l, _)| *l);
        // a village can be called "Kota Baru"; out of order, the word is part of the name
        let label = prefixed(part).filter(|(level, _)| below.is_none_or(|b| *level < b));
        if let Some((level, name)) = label {
            labelled = true;
            levels.push((level, name));
        } else if neighbourhood_unit(part) {
            street = at;
            break;
        } else if street_like(part) {
            break;
        } else {
            let level = match below {
                None if index.region_named(part, "id").is_some() => Level::Province,
                None | Some(Level::Province) => Level::Regency,
                Some(Level::Regency) => Level::District,
                Some(Level::District) => Level::Village,
                Some(Level::Village) => break,
            };
            levels.push((level, part));
        }
        street = at;
    }
    // an unlabelled first area is the village the address is in, whatever sits between
    if let [.., (level @ Level::District, name)] = levels.as_mut_slice()
        && !labelled
        && prefixed(name).is_none()
    {
        *level = Level::Village;
    }
    let named = |wanted| levels.iter().find(|(l, _)| *l == wanted).map(|(_, n)| *n);
    let areas = levels.iter().filter(|(l, _)| *l != Level::Province).count();
    // The regency as the index names it: Overture calls a few "Kabupaten Deli Serdang" or "Kota
    // Medan". One the index does not know ("Kota Jkt Utara") would scope nothing and let a
    // namesake elsewhere through, so then libpostal's reading stays.
    let counties = |p: &Place| p.subtype.as_ref() == "county";
    let Some(scope) = named(Level::Regency).and_then(|name| {
        [
            name.to_owned(),
            format!("Kabupaten {name}"),
            format!("Kota {name}"),
        ]
        .into_iter()
        .find(|text| index.places_exact(text, Some("id"), None, &counties).1)
    }) else {
        return fallback();
    };
    // One area alone ("Kec. Seulimeum, Indonesia", as the resolver's fallbacks ask for each part)
    // is left to libpostal, as before.
    if areas < 2 {
        return fallback();
    }

    let rest: Vec<&str> = parts[..street]
        .iter()
        .copied()
        .filter(|p| !neighbourhood_unit(p))
        .collect();
    let mut parsed = if rest.is_empty() {
        Vec::new()
    } else {
        index.parser.parse(&rest.join(", "))?.to_vec()
    };
    parsed.retain(|(key, _)| {
        matches!(
            key.as_str(),
            "house_number" | "road" | "unit" | "level" | "house" | "po_box"
        )
    });
    // The regency goes in `state`, which ranks the places under it first: a village name repeats
    // across Indonesia (Pepe in Grobogan and in Klaten, both in Central Java), and
    // `state_district` is not read when there is a `state`.
    let scope = scope.as_str();
    let town = named(Level::Village).or(named(Level::District).filter(|d| *d != scope));
    // Village names repeat within a regency too (Mekarsari): the stated district, when the
    // index files a village of that name under it, ranks that one first. The regency then
    // stays as the bound the resolver holds answers to.
    let district = named(Level::District).filter(|district| {
        let key = name_key(district);
        named(Level::Village).is_some_and(|village| {
            index
                .places_exact(village, Some("id"), Some(district), &|_| true)
                .0
                .iter()
                .any(|hit| {
                    matches!(hit, Hit::Place(p) if index.chain(*p).iter().any(|&q| index.places[q as usize].named(key)))
                })
        })
    });
    for (key, name) in [
        ("city", town),
        ("state", Some(district.unwrap_or(scope))),
        ("state_district", district.and(Some(scope))),
        (
            "postcode",
            postcode.or(Some(field(whole, "postcode")).filter(|p| postal(p))),
        ),
    ] {
        if let Some(name) = name {
            parsed.push((key.into(), name.to_lowercase()));
        }
    }
    parsed.push(("country".into(), "indonesia".into()));
    Ok(Some(parsed.into()))
}

/// The regency a parse states as its `state_district`, else its `state` (see [`aligned`]), when
/// that names a place of Indonesia below its provinces: its text and folded words.
pub(crate) struct Scope {
    text: String,
    words: Vec<String>,
}

pub(crate) fn stated_scope(index: &Index, parsed: &ParsedAddress) -> Option<Scope> {
    let text = Some(part(parsed, "state_district"))
        .filter(|d| !d.is_empty())
        .unwrap_or(part(parsed, "state"));
    let below_provinces = |p: &Place| {
        !matches!(
            &*p.subtype,
            "country" | "dependency" | "region" | "macroregion"
        )
    };
    (!text.is_empty()
        && index.country(part(parsed, "country")).as_deref() == Some("id")
        && index.region_named(text, "id").is_none()
        && index
            .places_exact(text, Some("id"), None, &below_provinces)
            .1)
        .then(|| Scope {
            text: text.to_owned(),
            words: normalize_place(text)
                .split(' ')
                .map(str::to_owned)
                .collect(),
        })
}

/// Whether one of a place's names holds every word of the scope: "Kota Probolinggo" and
/// "Probolinggo City" are both in Probolinggo.
fn in_scope(place: &Place, scope: &Scope) -> bool {
    place.all_names().any(|name| {
        let words: Vec<String> = normalize_place(name)
            .split(' ')
            .map(str::to_owned)
            .collect();
        scope.words.iter().all(|w| words.contains(w))
    })
}

/// Whether a candidate in Indonesia is outside the stated scope: no place it is in is named
/// like it. The structured fallbacks drop the scope ("Pepe, Indonesia"), and Pepe in Grobogan is
/// not the one in Klaten.
pub(crate) fn outside(index: &Index, scope: &Scope, hit: &Hit, candidate: &Value) -> bool {
    let place = match hit {
        Hit::Place(place) | Hit::Street { place, .. } | Hit::Postcode { place, .. } => *place,
    };
    candidate["address"]["country_code"] == "id"
        && place != NONE
        && !index
            .chain(place)
            .iter()
            .any(|&p| in_scope(&index.places[p as usize], scope))
}

/// Whether an answer lies outside the stated regency: in another regency's polygon, or, where
/// no regency polygon holds it, in a province no regency of that name is in.
pub(crate) fn elsewhere(index: &Index, scope: &Scope, answer: &FullAddress) -> bool {
    let Some(at) = answer.geo_coordinates.as_option() else {
        return false;
    };
    let (lat, lon) = (at.lat, at.lon);
    let regions: Vec<&str> = answer
        .address_details
        .as_option()
        .map(|d| d.region_codes.iter().map(|r| r.code.as_str()).collect())
        .unwrap_or_default();
    let county = index
        .area_at(lat as f64, lon as f64)
        .0
        .filter(|&p| index.places[p as usize].subtype.as_ref() == "county");
    match county {
        Some(county) => !index
            .chain(county)
            .iter()
            .any(|&p| in_scope(&index.places[p as usize], scope)),
        None => {
            let (named, _) = index.places_exact(&scope.text, Some("id"), None, &|p| {
                p.subtype.as_ref() == "county" && in_scope(p, scope)
            });
            !regions.is_empty()
                && !named.iter().any(|hit| match hit {
                    Hit::Place(p) => regions
                        .iter()
                        .any(|r| r.eq_ignore_ascii_case(&index.places[*p as usize].region)),
                    _ => false,
                })
        }
    }
}

/// The stated regency, for an Indonesian address whose village the index lacks or answered from
/// another regency: what the resolver's fallbacks gave before the regency scoped the search, not
/// a namesake elsewhere ("Lubuk Raja" for Lubuk Raya, Binuang in Banten for one in West Sulawesi).
pub(crate) fn regency(index: &Index, scope: &Scope) -> Option<Hit> {
    index
        .places_exact(&scope.text, Some("id"), None, &|p| {
            p.subtype.as_ref() == "county" && in_scope(p, scope)
        })
        .0
        .into_iter()
        .next()
}

/// Whether an answer above the street is a place the query names: its own name holds every
/// word of one of the query's parts, without its label ("Kec. Nisam" for Nisam, "Muara Sipongi"
/// for its market town Pasar Muara Sipongi). "Oba" is not "Oba Tengah".
pub(crate) fn named_in_query(query: &str, name: &str) -> bool {
    let name = normalize_place(name);
    let name: Vec<&str> = name.split(' ').collect();
    query.split(',').map(str::trim).any(|part| {
        let part = normalize_place(prefixed(part).map_or(part, |(_, rest)| rest));
        !part.is_empty() && part.split(' ').all(|w| name.contains(&w))
    })
}
