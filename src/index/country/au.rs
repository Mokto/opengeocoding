use crate::index::suggest::*;
use crate::index::*;
use crate::matching::*;
use regex::Regex;
use std::sync::LazyLock;

pub const SUBURBS_FILE: &str = "au-suburbs.fst";

pub fn suburb_level(country: [u8; 2]) -> Option<&'static str> {
    (country == *b"AU").then_some("macrohood")
}

pub fn suburb_key(sid: u32, number: &str, x: f64, y: f64) -> Vec<u8> {
    [
        &sid.to_be_bytes()[..],
        &x.to_bits().to_be_bytes()[..],
        &y.to_bits().to_be_bytes()[..],
        schema::number_key(number).as_bytes(),
    ]
    .concat()
}

pub(in crate::index) fn house_suburb(
    index: &Index,
    sid: u32,
    number: &str,
    point: (f64, f64),
    parent: u32,
    en: bool,
) -> Option<String> {
    if !index
        .places
        .get(parent as usize)?
        .cc
        .eq_ignore_ascii_case("au")
    {
        return None;
    }
    let key = suburb_key(sid, number, point.0, point.1);
    let place = u32::try_from(index.suburbs.as_ref()?.get(&key)?).ok()?;
    let suburb = index.places.get(place as usize)?;
    (suburb.cc.eq_ignore_ascii_case("au")
        && suburb.subtype.as_ref() == "macrohood"
        && index.chain(place).contains(&parent))
    .then(|| suburb.name(en).to_string())
}

// A city and its metropolitan locality can share a name without sharing a municipal boundary.
pub(crate) fn municipal_conflict(
    index: &Index,
    candidate: &serde_json::Value,
    parsed: &ParsedAddress,
    postcode: &str,
) -> bool {
    let city = part(parsed, "city");
    let address = &candidate["address"];
    if address["country_code"] != "au"
        || city.is_empty()
        || address["road"].as_str().is_none_or(str::is_empty)
        || (!postcode.is_empty()
            && address["postcode"]
                .as_str()
                .is_some_and(|own| pc_norm(own) == pc_norm(postcode)))
        || address["postal_town"].as_str().is_some_and(|town| {
            ["city", "suburb", "city_district"].iter().any(|key| {
                let stated = part(parsed, key);
                !stated.is_empty() && names_agree(stated, town)
            })
        })
    {
        return false;
    }
    let at = |key: &str| candidate[key].as_str().and_then(|v| v.parse::<f64>().ok());
    let Some((x, y)) = at("lon").zip(at("lat")) else {
        return false;
    };
    let Some(containing) = index.area_at(y, x).0 else {
        return false;
    };
    let county = &index.places[containing as usize];
    if county.subtype.as_ref() != "county" {
        return false;
    }
    let city = normalize_place(city);
    let municipalities = index.places(&city, Some("au"), None, &|p| {
        p.subtype.as_ref() == "county"
            && p.region == county.region
            && p.all_names()
                .any(|name| normalize_place(name).strip_prefix("city of ") == Some(city.as_str()))
    });
    let mut municipalities = municipalities.into_iter().filter_map(|h| match h {
        Hit::Place(p) => Some(p),
        _ => None,
    });
    let Some(municipality) = municipalities.next() else {
        return false;
    };
    municipalities.next().is_none()
        && !index
            .areas
            .iter()
            .flat_map(|areas| areas.containing(y, x))
            .any(|(_, p)| p == municipality)
}

/// An Australian unit before its building's number: "1/44 Macleay St", "Unit 1 44 Macleay St"
/// or "Unit 1, 44 Macleay St". The unit as typed, whether a slash joined it to the number, and
/// the text without it.
fn australian_unit(typed: &Typed) -> Option<(String, bool, Typed)> {
    const KIND: &str = r"(?:unit|u|apt|apartment|flat|shop|suite|villa|townhouse|office|level|lvl|l|floor|fl|site)";
    static SLASH: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^([A-Za-z]?\d{1,5}[A-Za-z]?)/(\d.*\s.*)$").unwrap());
    static NAMED: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(&format!(
            r"(?i)^({KIND}\s*[a-z]?\d{{1,5}}[a-z]?)(?:\s+(\d.*\s.*))?$"
        ))
        .unwrap()
    });
    let head = typed.parts.first()?;
    let mut parts = typed.parts.clone();
    let (unit, slash) = if let Some(c) = SLASH.captures(head) {
        parts[0] = c[2].to_string();
        (c[1].to_string(), true)
    } else {
        let c = NAMED.captures(head)?;
        match c.get(2) {
            Some(rest) => parts[0] = rest.as_str().to_string(),
            None if parts.len() > 1 => {
                parts.remove(0);
            }
            None => return None,
        }
        (c[1].to_string(), false)
    };
    Some((
        unit,
        slash,
        Typed {
            parts,
            open: typed.open,
        },
    ))
}

/// `query` without the unit of an Australian "unit/number" house ("71/206 Phillip Street,
/// Mount Pleasant QLD 4740"), which libpostal keeps as one house number the index may not
/// hold. The building's house answers then, as for "Unit 71, 206 Phillip Street". Australia
/// must be stated, or a part must end in a state code and postcode ("VIC 3065"); Czech,
/// Polish and other slashed numbers mean something else. None without such a number.
pub fn without_australian_unit(query: &str) -> Option<String> {
    static UNIT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^[A-Za-z]?\d{1,5}[A-Za-z]?/(\d{1,6}[A-Za-z]?(?:-\d{1,6}[A-Za-z]?)?\s+\S.*)$")
            .unwrap()
    });
    static STATE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\b(?:act|nsw|nt|qld|sa|tas|vic|wa)\s+\d{4}$").unwrap());
    let parts: Vec<&str> = query
        .split(',')
        .map(|p| p.trim_matches(whitespace))
        .collect();
    let (at, unit) = parts
        .iter()
        .enumerate()
        .find_map(|(i, p)| Some((i, UNIT.captures(p)?)))?;
    let countries: Vec<String> = parts
        .iter()
        .map(|p| expected_country(&parsed_country(p)))
        .filter(|c| !c.is_empty())
        .collect();
    let australian = countries.iter().any(|c| c == "au") || parts.iter().any(|p| STATE.is_match(p));
    if !australian || countries.iter().any(|c| c != "au") {
        return None;
    }
    let mut kept = parts.clone();
    kept[at] = unit.get(1)?.as_str();
    Some(kept.join(", "))
}

impl Index {
    /// Is the text Australian: Australia filtered for or named, or a part that is a state's
    /// code or ends with one and a postcode ("Crace ACT 2911")?
    fn australian(&self, typed: &Typed, cc: Option<&str>) -> bool {
        if let Some(cc) = cc {
            return cc.eq_ignore_ascii_case("au");
        }
        let state = |w: &str| {
            self.regions
                .contains_key(&format!("AU-{}", w.to_uppercase()))
        };
        typed.parts[1..].iter().any(|p| {
            let words: Vec<&str> = p.split(' ').collect();
            self.country(p).as_deref() == Some("au")
                || matches!(words.as_slice(), [code] if state(code))
                || matches!(words.as_slice(), [.., code, pc]
                    if state(code) && pc.len() == 4 && pc.bytes().all(|b| b.is_ascii_digit()))
        })
    }

    /// Houses of an Australian text that finds none as typed: with Australia's state codes and
    /// postcodes understood when it is not stated, then with a unit before the building's number
    /// set aside. The unit stays in the display name, unverified.
    pub(in crate::index) fn suggest_australian(
        &self,
        typed: &Typed,
        cc: Option<&str>,
        focus: Option<&Focus>,
        want: usize,
    ) -> Vec<Suggestion> {
        if !self.australian(typed, cc) {
            return vec![];
        }
        let houses = |typed: &Typed| -> Vec<Suggestion> {
            self.suggest_streets(typed, Some("au"), focus, want)
                .into_iter()
                .filter(|s| s.kind == "address" && s.country_code == "au")
                .collect()
        };
        if cc.is_none() {
            let found = houses(typed);
            if !found.is_empty() {
                return found;
            }
        }
        let Some((unit, slash, building)) = australian_unit(typed) else {
            return vec![];
        };
        houses(&building)
            .into_iter()
            .map(|mut s| {
                s.display_name = if slash {
                    format!("{unit}/{}", s.display_name)
                } else {
                    format!("{unit}, {}", s.display_name)
                };
                s.unverified_suffix = unit.clone();
                s
            })
            .collect()
    }
}
