/// "Postaja 19a, Tolmin, Slovenia", "Laze 39a, Vrtojba, Šempeter-Vrtojba, Slovenia": a Slovenian
/// address without a postcode, the street (or a village that numbers its houses itself) and
/// number, then the settlement and municipality, or the municipality alone. libpostal reads the
/// village as a venue ("house": "postaja") and joins two places into one town ("vrtojba
/// šempeter-vrtojba"). The street and number, the first place as the town and a second (the
/// municipality, which the index renders as the state) as the state. `None` for an address with
/// a postcode, which libpostal reads well.
pub(crate) fn aligned(q: &str) -> Option<Vec<(String, String)>> {
    let mut parts: Vec<&str> = q.split(',').map(str::trim).collect();
    let country = parts.pop()?.to_lowercase();
    if !matches!(country.as_str(), "slovenia" | "slovenija") {
        return None;
    }
    let [head, ref places @ ..] = parts[..] else {
        return None;
    };
    let (road, number) = head.rsplit_once(' ')?;
    let digits = number.trim_end_matches(|c: char| c.is_alphabetic());
    let numbered = digits.starts_with(|c: char| ('1'..='9').contains(&c))
        && digits.bytes().all(|b| b.is_ascii_digit())
        && number.chars().count() - digits.len() <= 1;
    let named = |p: &str| !p.is_empty() && !p.contains(|c: char| c.is_ascii_digit());
    if !numbered
        || !road.contains(char::is_alphabetic)
        || !(1..=2).contains(&places.len())
        || !places.iter().all(|p| named(p))
    {
        return None;
    }
    let mut parsed = vec![
        ("road".to_string(), road.to_lowercase()),
        ("house_number".to_string(), number.to_lowercase()),
        ("city".to_string(), places[0].to_lowercase()),
    ];
    if let [_, municipality] = places {
        parsed.push(("state".to_string(), municipality.to_lowercase()));
    }
    parsed.push(("country".to_string(), country));
    Some(parsed)
}

/// The town or municipality stated is a Slovenian house's municipality, which the index renders
/// as its state (ISO 3166-2 SI-xxx): "Postaja 19a, Tolmin" is in the village Bača pri Modreju of
/// the municipality Tolmin, and the index files "Laze 39a, Vrtojba, Šempeter-Vrtojba" under
/// Šempeter pri Gorici. Only for a house, which the road and number pin down; a bare town is not
/// every village of its municipality.
pub(crate) fn municipality_agrees(
    address: &crate::matching::ParsedAddress,
    details: &crate::proto::opengeocoding::AddressDetails,
) -> bool {
    use crate::matching::part;
    details.country_code.eq_ignore_ascii_case("si")
        && !details.house_number.is_empty()
        && !details.state.is_empty()
        && [part(address, "city"), part(address, "state")]
            .iter()
            .filter(|stated| !stated.is_empty())
            .any(|stated| {
                // a bilingual municipality's two names ("Koper / Capodistria", "Lendava / Lendva")
                details
                    .state
                    .split(" / ")
                    .any(|name| crate::matching::names_agree(stated, name))
            })
}

/// The municipality a Slovenian address states ([`aligned`]'s state) is not the house's: "Tabor
/// 10, Gorenja vas, Gorenja vas-Poljane" is not Tabor 10 in the Gorenja vas of Logatec.
pub(crate) fn municipality_conflict(
    address: &crate::matching::ParsedAddress,
    details: Option<&crate::proto::opengeocoding::AddressDetails>,
) -> bool {
    let stated = crate::matching::part(address, "state");
    !stated.is_empty()
        && details.is_some_and(|d| {
            d.country_code.eq_ignore_ascii_case("si")
                && !d.house_number.is_empty()
                && !d.state.is_empty()
                && !municipality_agrees(address, d)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_house_settlement_and_municipality() {
        let read = |q: &str| {
            aligned(q).map(|p| {
                p.iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        };
        assert_eq!(
            read("Postaja 19a, Tolmin, Slovenia").as_deref(),
            Some("road=postaja house_number=19a city=tolmin country=slovenia")
        );
        assert_eq!(
            read("Laze 39a, Vrtojba, Šempeter-Vrtojba, Slovenia").as_deref(),
            Some("road=laze house_number=39a city=vrtojba state=šempeter-vrtojba country=slovenia")
        );
        assert_eq!(
            read("Cesta 1. maja 8, Hrastnik, Slovenia").as_deref(),
            Some("road=cesta 1. maja house_number=8 city=hrastnik country=slovenia")
        );
        for kept in [
            "Postaja 19a, 5216 Most na Soči, Slovenia",
            "Ljubljana, Slovenia",
            "Novo Polje, cesta VII 24, Ljubljana, Slovenia",
            "Postaja 19a, Tolmin",
            "Postaja 019, Tolmin, Slovenia",
            "Postaja 19a, Bača, Tolmin, Goriška, Slovenia",
            "Postaja 19a, Tolmin, Croatia",
        ] {
            assert_eq!(read(kept), None, "{kept}");
        }
    }
}
