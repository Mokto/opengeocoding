use crate::matching::*;

/// "6 Avenida da Noruega, Ribeira de Pena, Portugal": libpostal reads a number written before a
/// Portuguese avenida as part of the road (it splits "6 Rua …" and "6 Av. …"). Only with the
/// country stated as Portugal: "6 Avenida" is a whole street name in Guatemala.
pub(crate) fn number_from_avenida(parsed: &mut Vec<(String, String)>) {
    if parsed.iter().any(|(k, _)| k == "house_number")
        || crate::matching::expected_country(&parsed.iter().cloned().collect()) != "pt"
    {
        return;
    }
    let Some(at) = parsed.iter().position(|(k, _)| k == "road") else {
        return;
    };
    let Some((number, road)) = parsed[at].1.split_once(' ') else {
        return;
    };
    if !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit())
        && road.starts_with("avenida ")
    {
        let (number, road) = (number.to_string(), road.to_string());
        parsed[at].1 = road;
        parsed.insert(at, ("house_number".into(), number));
    }
}

/// A Portuguese road as the national address register (CTT) writes it, which is how the index
/// stores most Portuguese houses: "Rua dos Farias" is "R FARIAS", "Bairro de Santa Bárbara" is
/// "BR S BARBARA", "Rua Tenente Valadim" is "R TEN VALADIM". The register abbreviates the
/// leading street type, most titles and Santa/Santo, and usually drops the particle after the
/// type. Only the abbreviations it uses for the large majority of its names are applied ("Marquês" is also a surname, "Monsenhor" mostly stays whole).
pub(crate) fn ctt_road(road: &str) -> Vec<String> {
    const TYPES: &[(&str, &str)] = &[
        ("praceta", "pct"),
        ("bairro", "br"),
        ("caminho", "cam"),
        ("urbanizacao", "urb"),
        ("sitio", "sit"),
        ("praca", "pc"),
        ("beco", "bc"),
        ("quinta", "qta"),
        ("calcada", "cc"),
        ("calcadinha", "ccnh"),
        ("monte", "mte"),
        ("casal", "csl"),
        ("vereda", "vr"),
        ("canada", "can"),
        ("quelha", "qlha"),
        ("rotunda", "rot"),
        ("escadinhas", "escnh"),
        ("escadas", "esc"),
        ("parque", "pq"),
        ("azinhaga", "az"),
        ("ladeira", "lad"),
        ("loteamento", "loteam"),
        ("jardim", "jrd"),
        ("conjunto", "conj"),
        ("zona", "zn"),
        ("terreiro", "terr"),
        ("rampa", "ramp"),
        ("aldeamento", "aldt"),
        ("herdade", "hrd"),
        ("viela", "vla"),
        ("patio", "pto"),
    ];
    const TITLES: &[(&str, &str)] = &[
        ("tenente", "ten"),
        ("comendador", "comend"),
        ("comandante", "cmdt"),
        ("almirante", "alm"),
        ("conselheiro", "cons"),
        ("visconde", "visc"),
        ("brigadeiro", "brig"),
        ("sargento", "sarg"),
        ("infante", "inf"),
        ("frei", "fr"),
        ("coronel", "cor"),
        ("arquitecto", "arq"),
        ("arquiteto", "arq"),
        ("duque", "dq"),
        ("embaixador", "emb"),
        ("doutora", "dra"),
        ("dom", "d"),
        ("dona", "d"),
        ("nossa", "nsa"),
        ("santa", "s"),
        ("santo", "s"),
    ];
    const PARTICLES: &[&str] = &["da", "do", "das", "dos", "de"];
    let road = normalize_place(road);
    let short = |table: &[(&str, &'static str)], w: &str| {
        table.iter().find(|(full, _)| *full == w).map(|(_, s)| *s)
    };
    // the street type, after a house number typed first ("12 rua dos farias")
    let at = road
        .split(' ')
        .position(|w| !w.starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or(0);
    let form = road
        .split(' ')
        .enumerate()
        .filter(|&(i, w)| i != at + 1 || !PARTICLES.contains(&w))
        .map(|(i, w)| {
            (i == at)
                .then(|| short(TYPES, w))
                .flatten()
                .or_else(|| short(TITLES, w))
                .unwrap_or(w)
        })
        .collect::<Vec<_>>()
        .join(" ");
    // a bare type ("Rua da") names no street
    if form.contains(' ') && form != road {
        vec![form]
    } else {
        vec![]
    }
}

/// "2955-243": no other country writes its postcodes so, which tells a query that names no country
/// to read its road as Portugal's register does. Only the written shape counts: "50667" could be
/// German or French, and a country-scoped spelling needs the country to be certain.
pub(crate) fn portuguese_postcode(postcode: &str) -> bool {
    let b = postcode.trim().as_bytes();
    b.len() == 8 && b[4] == b'-' && b[..4].iter().chain(&b[5..]).all(u8::is_ascii_digit)
}
