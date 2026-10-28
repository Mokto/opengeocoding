use crate::index::parse::{head_aligns, one_house};

/// The country as libpostal labels it in a Mexican address.
pub(crate) fn mexico(country: &str) -> bool {
    matches!(country, "mexico" | "méxico")
}

/// An address in the order of INEGI's address standard, "CALLE UNION 127, COLONIA ZONA CENTRO,
/// 20000 Aguascalientes, Aguascalientes, México": street and number, settlement, postcode and
/// locality, municipality where it differs, state; or with the locality and state only
/// ("CALLE UNION 127, Aguascalientes, Aguascalientes, México"). libpostal reads the settlement
/// as the road and drops the street, the locality as the road and the postcode as its number,
/// or splits a locality named for its settlement ("Colonia González Ortega") into a road and a
/// town. The street part parsed on its own, the locality as the town and the last part as the
/// state; the settlement and the municipality are left out.
pub(crate) fn standard_aligned(
    head: &[(String, String)],
    rest: &[&str],
) -> Option<Vec<(String, String)>> {
    let (postcode, town, state) = match rest.iter().position(|p| postcode_town(p).is_some()) {
        Some(at) => {
            let (postcode, town) = postcode_town(rest[at])?;
            let state = match &rest[at + 1..] {
                [state] | [_, state] => state.trim().to_lowercase(),
                _ => return None,
            };
            (Some(postcode), town, state)
        }
        None => match rest {
            [town, state] if !town.chars().any(|c| c.is_ascii_digit()) => (
                None,
                town.trim().to_lowercase(),
                state.trim().to_lowercase(),
            ),
            _ => return None,
        },
    };
    if !head_aligns(head)
        || town.is_empty()
        || !one_house(head)
        || !state.chars().any(char::is_alphabetic)
        || state.chars().any(|c| c.is_ascii_digit())
    {
        return None;
    }
    let mut aligned = head.to_vec();
    aligned.extend(postcode.map(|p| ("postcode".into(), p)));
    aligned.push(("city".into(), town));
    aligned.push(("state".into(), state));
    Some(aligned)
}

/// "20000 Aguascalientes": a five-digit postcode and the locality.
fn postcode_town(part: &str) -> Option<(String, String)> {
    let (postcode, town) = part.trim().split_once(' ')?;
    let town = town.trim().to_lowercase();
    (postcode.len() == 5
        && postcode.bytes().all(|b| b.is_ascii_digit())
        && town.chars().any(char::is_alphabetic)
        && !town.chars().any(|c| c.is_ascii_digit()))
    .then(|| (postcode.into(), town))
}

/// A Mexican road without its type, as INEGI's establishment directory (DENUE) names it in
/// OpenAddresses: "calle misión de santo domingo" is its "MISIÓN DE SANTO DOMINGO".
pub(crate) fn bare_road(road: &str) -> Option<String> {
    const TYPES: &[&str] = &[
        "calle",
        "avenida",
        "av",
        "av.",
        "boulevard",
        "bulevar",
        "blvd",
        "blvd.",
        "calzada",
        "privada",
        "prolongación",
        "prolongacion",
        "circuito",
        "periférico",
        "periferico",
        "cerrada",
        "andador",
        "callejón",
        "callejon",
        "retorno",
        "circunvalación",
        "circunvalacion",
    ];
    let (kind, name) = road.trim().split_once(' ')?;
    let name = name.trim();
    (TYPES.contains(&kind.to_lowercase().as_str()) && name.chars().any(char::is_alphanumeric))
        .then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(parts: &[(&str, &str)]) -> Vec<(String, String)> {
        parts
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn the_standard_order_is_street_postcode_town_and_state() {
        let head = pairs(&[
            ("road", "boulevard antonio mijares"),
            ("house_number", "16"),
        ]);
        let want = pairs(&[
            ("road", "boulevard antonio mijares"),
            ("house_number", "16"),
            ("postcode", "23400"),
            ("city", "san josé del cabo"),
            ("state", "baja california sur"),
        ]);
        for rest in [
            &[
                " COLONIA CENTRO",
                " 23400 San José del Cabo",
                " Los Cabos",
                " Baja California Sur",
            ][..],
            &[" 23400 San José del Cabo", " Baja California Sur"][..],
        ] {
            assert_eq!(standard_aligned(&head, rest), Some(want.clone()));
        }
        assert_eq!(
            standard_aligned(&head, &[" Colonia González Ortega", " Zacatecas"]),
            Some(pairs(&[
                ("road", "boulevard antonio mijares"),
                ("house_number", "16"),
                ("city", "colonia gonzález ortega"),
                ("state", "zacatecas"),
            ]))
        );
        for rest in [
            &[
                " COLONIA CENTRO",
                " San José del Cabo",
                " Baja California Sur",
            ][..],
            &[" 23400 San José del Cabo"][..],
            &[
                " 23400 San José del Cabo",
                " A",
                " B",
                " Baja California Sur",
            ][..],
            &[" 2340 San José del Cabo", " Baja California Sur"][..],
        ] {
            assert_eq!(standard_aligned(&head, rest), None, "{rest:?}");
        }
        let colonia = pairs(&[
            ("road", "colonia"),
            ("house_number", "centro"),
            ("city", "x"),
        ]);
        assert_eq!(standard_aligned(&colonia, &[" 23400 X", " Y"]), None);
    }

    #[test]
    fn a_road_loses_its_type() {
        assert_eq!(
            bare_road("calle misión de santo domingo").as_deref(),
            Some("misión de santo domingo")
        );
        assert_eq!(bare_road("Av. Juárez").as_deref(), Some("Juárez"));
        assert_eq!(bare_road("calle 13").as_deref(), Some("13"));
        assert_eq!(bare_road("calle"), None);
        assert_eq!(bare_road("paseo de la reforma"), None);
        assert_eq!(bare_road("misión de santo domingo"), None);
    }
}
