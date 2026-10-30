use super::*;

/// Parsed address components, shared between callers and the cache.
pub type Parsed = Arc<[(String, String)]>;

pub(super) fn field<'a>(parsed: &'a [(String, String)], key: &str) -> &'a str {
    parsed
        .iter()
        .find(|(k, _)| k == key)
        .map_or("", |(_, v)| v.as_str())
}

/// "street, town, STATE ZIP" read with its commas as the boundaries. libpostal reads across them:
/// "14 Breezewood Common, East Amherst, NY 14051" puts "East" in the road, and "3 Canterbury
/// Street, Egg Harbor Township, NJ 08234" splits the town. Each outer part parses cleanly alone
/// (`head` as only a house number and road, `tail` as only a state and postcode), so when the
/// parse of the `whole` query names a different town, or none because its road swallowed the
/// middle part ("4724 3rd Place Northwest, Washingotn, DC"), the middle part is the town. A whole
/// parse without a town whose road is its own is left alone (the town check it then skips would be
/// a new, stricter one), and so is one that splits the middle into a suburb and a town ("Avondale
/// Goodyear").
pub fn comma_aligned(
    head: &[(String, String)],
    extras: &[&str],
    middle: &str,
    tail: &[(String, String)],
    whole: &[(String, String)],
) -> Option<Vec<(String, String)>> {
    let only =
        |a: &[(String, String)], keys: &[&str]| a.iter().all(|(k, _)| keys.contains(&k.as_str()));
    if !alignable(middle, extras, whole)
        || extras
            .iter()
            .any(|e| e.trim().is_empty() || e.chars().any(|c| c.is_ascii_digit()))
        || !head_aligns(head)
        || field(tail, "postcode").is_empty()
        || !only(tail, &["state", "postcode"])
    {
        return None;
    }
    let middle = middle.trim().to_lowercase();
    let mut aligned = head.to_vec();
    aligned.extend(
        extras
            .iter()
            .map(|e| ("state_district".to_string(), e.trim().to_lowercase())),
    );
    aligned.push(("city".into(), middle));
    aligned.extend(tail.iter().cloned());
    Some(aligned)
}

/// The checks of [`comma_aligned`] that need no parse of the outer parts, so the two extra
/// libpostal calls are made only for the few queries that can still pass.
fn alignable(middle: &str, extras: &[&str], whole: &[(String, String)]) -> bool {
    let middle = middle.trim().to_lowercase();
    let (city, road, suburb) = (
        field(whole, "city"),
        field(whole, "road"),
        field(whole, "suburb"),
    );
    // a road read as a suburb ("2219 Pacific Coast Highway, Lomita")
    let road_as_suburb = road.is_empty() && !suburb.is_empty();
    // a county part read into the road ("15 Westervelt Place, Essex, Irvington Township")
    let county_in_road = extras.iter().any(|e| {
        let e = e.trim().to_lowercase();
        !e.is_empty() && road.contains(&e)
    });
    !middle.is_empty()
        && !middle.chars().any(|c| c.is_ascii_digit())
        && field(whole, "country").is_empty()
        && field(whole, "state") != middle
        && (!city.is_empty() || road.contains(&middle))
        && (suburb.is_empty() || road_as_suburb)
        && field(whole, "city_district").is_empty()
        && (city != middle || road_as_suburb || county_in_road)
}

/// "701 GREENWOOD Avenue, OH 45229": with no town, libpostal takes the street type for one
/// ("road=greenwood", "city=avenue"), or the state for part of the road ("1516 RUTH Avenue, OH"
/// gives "road=ruth avenue oh"). When the street part alone parses as a house on a road, and the
/// state part as a state, those are the parse.
pub fn street_aligned(
    head: &[(String, String)],
    head_text: &str,
    tail: &[(String, String)],
    whole: &[(String, String)],
) -> Option<Vec<(String, String)>> {
    let city = field(whole, "city");
    // "avenue" or "shady lane" taken for the town, or the street name with no road left
    // ("Clairemont"). A town the whole parse already split from its road ("Temple" after
    // "Kutztown Rd.") is also inside the unsplit street part; dropping it leaves the postcode,
    // which another country may share ("19560" is French).
    let word = |w: &str| canon::canon(w).is_some() || canon::is_canonical(w);
    let last = city.split_whitespace().next_back().unwrap_or("");
    let streetish = !city.is_empty() && (word(city) || word(last));
    let in_head = !city.is_empty() && head_text.to_lowercase().contains(city);
    let misread = field(whole, "state").is_empty()
        || (in_head && (streetish || field(whole, "road").is_empty()));
    let only = tail
        .iter()
        .all(|(k, _)| matches!(k.as_str(), "state" | "postcode"));
    (misread && head_aligns(head) && only && !field(tail, "state").is_empty())
        .then(|| head.iter().chain(tail).cloned().collect())
}

pub(super) fn postal_city_aligned(
    head: &[(String, String)],
    tail: &str,
    country: &(String, String),
    whole: &[(String, String)],
) -> Option<Vec<(String, String)>> {
    let cc = crate::matching::expected_country(&[country.clone()].into_iter().collect());
    let words: Vec<_> = tail.split_whitespace().collect();
    let (postcode, city) = [2, 1]
        .into_iter()
        .filter(|&at| at < words.len())
        .map(|at| (words[..at].join(" "), words[at..].join(" ")))
        .find(|(pc, _)| crate::matching::postcode_shape(&cc, pc) == Some(true))?;
    // A conflicting two-word postcode must stay intact: "9406 KX" is not Belgian "9406".
    if postcode == words[0]
        && words.len() > 2
        && crate::matching::postcode_shape("nl", &words[..2].join(" ")) == Some(true)
    {
        return None;
    }
    if !head_aligns(head)
        || !one_house(head)
        || (postcode_country_suffix(tail, whole)
            && ["house_number", "road"]
                .iter()
                .any(|key| field(head, key) != field(whole, key)))
        || !["state", "state_district", "suburb", "city_district"]
            .iter()
            .all(|k| field(whole, k).is_empty())
        || !city.chars().any(char::is_alphabetic)
        || city.chars().any(|c| c.is_ascii_digit())
    {
        return None;
    }
    let mut parsed = head.to_vec();
    parsed.extend([
        ("postcode".into(), postcode.to_lowercase()),
        ("city".into(), city.trim().to_lowercase()),
    ]);
    Some(parsed)
}

pub(super) fn postcode_country_suffix(tail: &str, whole: &[(String, String)]) -> bool {
    if !one_house(whole)
        || whole.iter().filter(|(k, _)| k == "postcode").count() != 1
        || whole.iter().filter(|(k, _)| k == "country").count() != 2
        || whole.iter().any(|(k, _)| k == "country_region")
    {
        return false;
    }
    let words: Vec<_> = tail.split_whitespace().collect();
    // postcode_shape also accepts outcodes and strips non-ASCII characters.
    words.len() > 2
        && words[0].len() == 4
        && words[0].bytes().all(|b| b.is_ascii_digit())
        && words[1].len() == 2
        && words[1].bytes().all(|b| b.is_ascii_alphabetic())
        && whole.windows(2).any(|pair| {
            pair[0].0 == "postcode"
                && pair[0].1 == words[0]
                && pair[1].0 == "country"
                && pair[1].1.eq_ignore_ascii_case(words[1])
        })
}

pub(super) fn postal_without_commas(
    q: &str,
    whole: &[(String, String)],
    libpostal: impl Fn(&str) -> Result<Vec<(String, String)>>,
) -> Result<Option<Vec<(String, String)>>> {
    if whole.iter().filter(|(k, _)| k == "house_number").count() < 2 {
        return Ok(None);
    }
    let Some(country) = whole.last().filter(|(k, _)| k == "country") else {
        return Ok(None);
    };
    let cc = crate::matching::expected_country(&[country.clone()].into_iter().collect());
    let lower = q.trim().to_lowercase();
    let Some(body) = lower.strip_suffix(&country.1).map(str::trim_end) else {
        return Ok(None);
    };
    for (_, postcode) in whole.iter().rev().filter(|(k, v)| {
        k == "house_number" && crate::matching::postcode_shape(&cc, v) == Some(true)
    }) {
        if let Some((head, rest)) = body.rsplit_once(postcode)
            && head.ends_with(char::is_whitespace)
            && rest.starts_with(char::is_whitespace)
            && let Some(mut aligned) = postal_city_aligned(
                &libpostal(head.trim())?,
                &format!("{postcode}{rest}"),
                country,
                whole,
            )
        {
            aligned.push(country.clone());
            return Ok(Some(aligned));
        }
    }
    Ok(None)
}

/// A US state code, alone or before a ZIP: the second part of "street, STATE ZIP".
fn state_part(part: &str) -> bool {
    static STATE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^[A-Za-z]{2}(\s+\d{5}(-\d{4})?)?$").unwrap());
    STATE.is_match(part.trim())
}

pub(super) fn head_aligns(head: &[(String, String)]) -> bool {
    !field(head, "road").is_empty()
        && head
            .iter()
            .all(|(k, _)| matches!(k.as_str(), "house_number" | "road" | "unit"))
}

/// "Country, STATE ZIP, town, number street": libpostal reads such a reversed address front to
/// back and finds two roads ("road=shawnee", "road=south dixon avenue") or takes the country for
/// a district, or with the state as one road ("road=united states wy"). When the first part is
/// the country, the parts in the usual order, if they parse into one house on one road.
pub(super) fn reversed(
    q: &str,
    whole: &[(String, String)],
    libpostal: impl Fn(&str) -> Result<Vec<(String, String)>>,
) -> Result<Option<String>> {
    let parts: Vec<&str> = q.split(',').map(str::trim).collect();
    if parts.len() < 3 {
        return Ok(None);
    }
    let labelled = whole.first().is_some_and(|(k, v)| {
        matches!(k.as_str(), "country" | "state_district") && *v == parts[0].to_lowercase()
    });
    let repeats = whole
        .iter()
        .enumerate()
        .any(|(i, (k, _))| whole[..i].iter().any(|(seen, _)| seen == k));
    let country_first =
        labelled || (repeats && matches!(&libpostal(parts[0])?[..], [(k, _)] if k == "country"));
    Ok(country_first.then(|| parts.into_iter().rev().collect::<Vec<_>>().join(", ")))
}

/// "Church Street 4, Amberley": a number written after a road that ends in its type, which
/// libpostal can leave in the road ("church street 4") or read beside a second road. The address
/// with the number first, and the number.
pub(crate) fn number_last(q: &str) -> Option<(String, String)> {
    let (head, rest) = q.split_once(',')?;
    let (road, number) = head.trim().rsplit_once(char::is_whitespace)?;
    let road = road.trim();
    let typed = road.rsplit(char::is_whitespace).next().is_some_and(|w| {
        let f = fold(w);
        canon::canon(&f).is_some() || canon::is_canonical(&f)
    });
    (typed
        && road.starts_with(char::is_alphabetic)
        && number.starts_with(|c: char| c.is_ascii_digit())
        && number
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-')))
    .then(|| (format!("{number} {road},{rest}"), number.to_string()))
}

pub(super) fn one_house(parsed: &[(String, String)]) -> bool {
    ["house_number", "road"]
        .iter()
        .all(|key| parsed.iter().filter(|(k, _)| k == key).count() == 1)
}

/// A unit libpostal left at the end of the road ("south fillmore street #b", "201st avenue
/// souhtwest apt 10a"), moved to its own field. The unit has a digit or is one letter, so "Ste
/// Foy" and "Rue Sainte Anne" keep theirs.
pub(super) fn unit_from_road(parsed: &mut Vec<(String, String)>) {
    static UNIT: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r"^(.+?)\s+((?:#\s*|(?:apt|apartment|unit|suite|ste)\.?\s+#?)(?:[a-z]|[a-z]{0,3}\d[a-z0-9-]{0,4}))$",
        )
        .unwrap()
    });
    if parsed.iter().any(|(k, _)| k == "unit") {
        return;
    }
    let Some(at) = parsed.iter().position(|(k, _)| k == "road") else {
        return;
    };
    if let Some(c) = UNIT.captures(&parsed[at].1) {
        let (road, unit) = (c[1].to_string(), c[2].to_string());
        parsed[at].1 = road;
        parsed.insert(at + 1, ("unit".into(), unit));
    }
}

/// "13 Кӯчаи Каримов, Вахш": libpostal labels a bare number before the road a house name. With
/// no house number, and a part of `q` that is that number and the whole road, it is the house
/// number: a road cut short ("strada ion" of "2 Strada Ion Druță") or run on across commas is
/// not the number's.
pub(super) fn number_from_house(q: &str, parsed: &mut [(String, String)]) {
    if parsed.iter().any(|(k, _)| k == "house_number") {
        return;
    }
    let Some(road) = parsed
        .iter()
        .find(|(k, _)| k == "road")
        .map(|(_, v)| v.clone())
    else {
        return;
    };
    let mut houses = parsed.iter_mut().filter(|(k, _)| k == "house");
    if let (Some((k, v)), None) = (houses.next(), houses.next()) {
        let digits = v.trim_end_matches(char::is_alphabetic);
        let beside = q.split(',').any(|part| {
            let words: Vec<String> = part.split_whitespace().map(str::to_lowercase).collect();
            let rest: Vec<&str> = words
                .iter()
                .map(String::as_str)
                .filter(|w| w != v)
                .collect();
            rest.len() + 1 == words.len() && rest.join(" ") == road
        });
        if !digits.is_empty()
            && digits.bytes().all(|b| b.is_ascii_digit())
            && v.chars().count() - digits.len() <= 1
            && beside
        {
            *k = "house_number".into();
        }
    }
}

/// "57 شارع دار المريسة, عين المريسة, Lebanon": libpostal reads the last two parts, a town and the
/// country, as its one house name. With a road and without a town or country of their own,
/// they are those.
pub(super) fn town_country_from_house(q: &str, parsed: &mut Vec<(String, String)>) {
    if !parsed.iter().any(|(k, _)| k == "road")
        || parsed.iter().filter(|(k, _)| k == "house").count() != 1
        || parsed
            .iter()
            .any(|(k, _)| matches!(k.as_str(), "city" | "country"))
    {
        return;
    }
    let parts: Vec<String> = q.split(',').map(|p| p.trim().to_lowercase()).collect();
    let [.., town, country] = parts.as_slice() else {
        return;
    };
    if town.is_empty() || crate::matching::country_named(country).is_empty() {
        return;
    }
    let both = format!("{town} {country}");
    if let Some(at) = parsed.iter().position(|(k, v)| k == "house" && *v == both) {
        parsed.splice(
            at..=at,
            [
                ("city".to_string(), town.clone()),
                ("country".to_string(), country.clone()),
            ],
        );
    }
}

/// "5 شارع الكنيسه, مركز منفلوط, …": libpostal reads a number written before an Arabic street
/// as part of the road (or of a house name) and lets the road run on past the comma, but parses
/// the same address with the number after the street. That order, and the number, when the
/// number heads the first part and at least two words follow it: one word after it is a dated
/// name ("15 مايو", "6 أكتوبر"). The caller keeps it only if libpostal then reads the number
/// as the house number.
fn arabic_number_last(q: &str) -> Option<(String, String)> {
    let (head, rest) = q.split_once(',').unwrap_or((q, ""));
    let (number, street) = head.trim().split_once(' ')?;
    let street = street.trim();
    (!number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit())
        && street.split_whitespace().count() >= 2
        && street
            .chars()
            .any(|c| ('\u{0600}'..='\u{06ff}').contains(&c)))
    .then(|| {
        let tail = if rest.is_empty() {
            String::new()
        } else {
            format!(",{rest}")
        };
        (format!("{street} {number}{tail}"), number.to_string())
    })
}

/// The process's one libpostal parser.
pub struct Parser {
    pub(super) postal: Mutex<rustpostal::LibModules>,
    pub(super) cache: Mutex<HashMap<Box<str>, Parsed>>,
    pub(super) capacity: usize,
}

impl Parser {
    /// Parses kept by default: a resolve parses its query twice (resolver, then index) and
    /// libpostal's single lock is the service's throughput limit.
    pub const DEFAULT_CACHE: usize = 65_536;

    /// Loads the models, caching [`Self::DEFAULT_CACHE`] parses.
    pub fn setup() -> Result<Self> {
        Self::setup_with_cache(Self::DEFAULT_CACHE)
    }

    /// Loads the models; once per process, shared by the resolver and the index. `cache` is the
    /// number of parses kept; 0 parses every call.
    pub fn setup_with_cache(cache: usize) -> Result<Self> {
        let parser = Self::unloaded(cache);
        parser.load()?;
        Ok(parser)
    }

    /// A parser whose models are not loaded: [`Self::load`] it before the first `parse`. Lets
    /// the several seconds of model loading overlap other startup work.
    pub fn unloaded(cache: usize) -> Self {
        Self {
            postal: Mutex::new(rustpostal::LibModules::Address),
            cache: Mutex::default(),
            capacity: cache,
        }
    }

    /// Loads the models; once per process.
    pub fn load(&self) -> Result<()> {
        let postal = self.postal.lock().unwrap_or_else(|e| e.into_inner());
        postal
            .setup()
            .map_err(|_| anyhow::anyhow!("libpostal model setup failed"))
    }

    /// Blocks: call it off the async executor.
    // ponytail: one lock serializes every libpostal call (the parser keeps one scratch context
    // in native state, so parses cannot overlap). The cache is emptied when full, not an LRU.
    pub fn parse(&self, q: &str) -> Result<Parsed> {
        if self.capacity > 0
            && let Some(hit) = self.cache.lock().unwrap_or_else(|e| e.into_inner()).get(q)
        {
            return Ok(hit.clone());
        }
        let parsed: Parsed = {
            let _guard = self.postal.lock().unwrap_or_else(|e| e.into_inner());
            let libpostal = |text: &str| -> Result<Vec<(String, String)>> {
                Ok(rustpostal::address::parse_address(text, None, None)?
                    .into_iter()
                    .collect())
            };
            let kosovo = xk::parsed(q);
            let mut whole = match &kosovo {
                Some(read) => read.clone(),
                None => libpostal(q)?,
            };
            let mut swapped = None;
            if kosovo.is_none()
                && let Some((turned, number)) = arabic_number_last(q)
            {
                let found = libpostal(&turned)?;
                if found
                    .iter()
                    .any(|(k, v)| k == "house_number" && v.rsplit(' ').next() == Some(&number))
                {
                    whole = found;
                    swapped = Some(turned);
                }
            }
            // "Ahtri tn 12, Tallinn": libpostal reads the number into a road with an abbreviated
            // Estonian street type, not into one written out
            let mut estonian = false;
            if swapped.is_none()
                && crate::matching::expected_country(&whole.iter().cloned().collect()) == "ee"
                && let Some(full) = ee::full_road(q)
            {
                whole = libpostal(&full)?;
                swapped = Some(full);
                estonian = true;
            }
            let typed = q;
            let q = swapped.as_deref().unwrap_or(q);
            let mut parts: Vec<&str> = q.split(',').collect();
            // "…, NJ 07111, United States": the country part is set aside while the rest aligns
            let country = match whole.last() {
                Some((k, v))
                    if k == "country"
                        && parts.len() >= 3
                        && parts.last().is_some_and(|p| p.trim().to_lowercase() == *v) =>
                {
                    parts.pop();
                    Some((k.clone(), v.clone()))
                }
                _ => None,
            };
            let inner: Vec<_> = whole
                .iter()
                .filter(|(k, _)| country.is_none() || k != "country")
                .cloned()
                .collect();
            let mut aligned = kosovo.map(|_| inner.clone());
            if let Some((_, c)) = &country
                && matches!(c.as_str(), "japan" | "日本")
                && let [head, ref rest @ ..] = parts[..]
                && let Some(house) = jp::japanese_head(head)
            {
                let mut a = house;
                // a district (郡) only prefixes the names of its towns and villages
                let rest: Vec<&str> = rest
                    .iter()
                    .copied()
                    .filter(|p| !p.trim().ends_with('郡'))
                    .collect();
                a.extend(
                    libpostal(rest.join(",").trim())?
                        .into_iter()
                        .filter(|(k, _)| !matches!(k.as_str(), "road" | "house_number")),
                );
                aligned = Some(a);
            }
            if aligned.is_none()
                && country.as_ref().is_some_and(|(_, c)| mx::mexico(c))
                && let [head, ref rest @ ..] = parts[..]
                && rest.len() >= 2
            {
                aligned = mx::standard_aligned(&libpostal(head.trim())?, rest);
            }
            if aligned.is_none()
                && country.as_ref().is_some_and(|(_, c)| tw::taiwan(c))
                && let [head, ref places @ ..] = parts[..]
            {
                aligned = tw::aligned(head, places);
            }
            match parts[..] {
                _ if aligned.is_some() => {}
                [head, tail] if state_part(tail) => {
                    aligned = street_aligned(
                        &libpostal(head.trim())?,
                        head,
                        &libpostal(tail.trim())?,
                        &inner,
                    );
                    // "Riverside, CT": a lone town and its state code that libpostal fused into
                    // one road ("riverside ct")
                    if aligned.is_none()
                        && tail.trim().len() == 2
                        && matches!(&whole[..], [(k, _)] if k == "road")
                        && !head.chars().any(|c| c.is_ascii_digit())
                    {
                        aligned = Some(vec![
                            ("city".to_string(), head.trim().to_lowercase()),
                            ("state".to_string(), tail.trim().to_lowercase()),
                        ]);
                    }
                }
                // "New York, United States": the comma ends the place, but libpostal splits it
                // into a town "new" and a state "york" when the country follows
                [head, tail]
                    if whole.last().is_some_and(|(k, v)| {
                        k == "country" && *v == tail.trim().to_lowercase()
                    }) =>
                {
                    let split = whole[..whole.len() - 1]
                        .iter()
                        .map(|(_, v)| v.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    // only a name split into places: a postcode or house number is its own field
                    let places = whole[..whole.len() - 1].iter().all(|(k, _)| {
                        matches!(
                            k.as_str(),
                            "city" | "state" | "state_district" | "suburb" | "city_district"
                        )
                    });
                    if whole.len() > 2
                        && places
                        && let [(k, v)] = &libpostal(head.trim())?[..]
                        && *v == split
                    {
                        aligned =
                            Some(vec![(k.clone(), v.clone()), whole[whole.len() - 1].clone()]);
                    }
                }
                // "street, county, town, STATE ZIP": the parts between the street and the town
                // are the county
                [head, ref extras @ .., middle, tail] if alignable(middle, extras, &inner) => {
                    let head = libpostal(head.trim())?;
                    if head_aligns(&head) {
                        aligned =
                            comma_aligned(&head, extras, middle, &libpostal(tail.trim())?, &inner);
                    }
                }
                [head, tail]
                    if country.is_some()
                        && whole
                            .iter()
                            .any(|(k, v)| k == "road" && !head.to_lowercase().contains(v)) =>
                {
                    aligned = postal_city_aligned(
                        &libpostal(head.trim())?,
                        tail,
                        country.as_ref().unwrap(),
                        &inner,
                    );
                }
                // The general alignment may repair the road; this path must preserve it.
                [head, tail] if country.is_some() && postcode_country_suffix(tail, &whole) => {
                    aligned = postal_city_aligned(
                        &libpostal(head.trim())?,
                        tail,
                        country.as_ref().unwrap(),
                        &whole,
                    );
                }
                _ => {}
            }
            if aligned.is_none()
                && let [head, area, town, province] = parts[..]
                && country.as_ref().is_some_and(|(_, c)| es::spain(c))
            {
                aligned = es::area_aligned(&libpostal(head.trim())?, area, town, province, &inner);
            }
            if aligned.is_none()
                && country
                    .as_ref()
                    .is_some_and(|(_, c)| matches!(c.as_str(), "hong kong" | "hongkong"))
                && let [head, ref places @ ..] = parts[..]
                && !places.is_empty()
            {
                let head = match hk_mo::numbered(head) {
                    Some(numbered) => numbered,
                    None => libpostal(head.trim())?,
                };
                if head_aligns(&head) && one_house(&head) {
                    aligned = Some(hk_mo::aligned(&head, places));
                }
            }
            if aligned.is_none() && parts.len() == 1 {
                aligned = postal_without_commas(q, &whole, libpostal)?;
            }
            let estonian_parts = ee::aligned(q, &whole, libpostal)?;
            let mut parsed = match aligned {
                Some(mut a) => {
                    a.extend(country);
                    a
                }
                None => whole,
            };
            if let Some(read) = estonian_parts
                .or_else(|| si::aligned(q))
                .or_else(|| lt::aligned(q))
            {
                parsed = read;
            }
            unit_from_road(&mut parsed);
            number_from_house(q, &mut parsed);
            town_country_from_house(q, &mut parsed);
            if estonian {
                ee::typed_road(&mut parsed, typed);
            }
            pt::number_from_avenida(&mut parsed);
            cn_tw::number_from_cjk_road(&mut parsed);
            cz_sk::czech_number_marks(&mut parsed);
            de::numbered_street(q, &mut parsed);
            parsed.into_iter().collect()
        };
        if self.capacity > 0 {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if cache.len() >= self.capacity {
                cache.clear();
            }
            cache.insert(q.into(), parsed.clone());
        }
        Ok(parsed)
    }

    /// A reversed address ("Country, STATE ZIP, town, number street") in the usual order, or an
    /// Italian one without the frazione the parse runs across ([`it::without_frazione`]), for the
    /// caller to resolve instead. Not done in [`Self::parse`]: the resolver's structured
    /// re-queries are written country first on purpose, and the search reads the query's words.
    pub fn reordered(&self, q: &str) -> Result<Option<String>> {
        let whole = self.parse(q)?;
        if let Some(trimmed) = it::without_frazione(q, &whole) {
            return Ok(Some(trimmed));
        }
        let Some(turned) = reversed(q, &whole, |t| Ok(self.parse(t)?.to_vec()))? else {
            return Ok(None);
        };
        Ok(one_house(&self.parse(&turned)?).then_some(turned))
    }
}

impl Index {
    /// "Rruga Ymer Puka 7, 4401, Albania": libpostal, which has seen few addresses of some
    /// countries, reads the country written last as a road and the postcode part before it as a
    /// house number. The rest parsed alone, that part as the postcode, and the country, when a
    /// street of that name and postcode in the country holds the house. Otherwise the postcode
    /// would scope out the house the unscoped search finds, and a placeholder ("00000") or a
    /// dialling code ("509, Haiti") would answer as a postcode. A country read as a house is
    /// left to the resolver's `country_in_tail`.
    fn country_as_road(&self, q: &str, whole: &[(String, String)]) -> Result<Option<Parsed>> {
        let Some((head, last)) = q.rsplit_once(',') else {
            return Ok(None);
        };
        let postcode = head.rsplit(',').next().unwrap_or("").trim();
        let last = last.trim().to_lowercase();
        let misread = match whole {
            [.., (n, number), (r, road)] => {
                n == "house_number"
                    && (number == postcode || number.ends_with(&format!(" {postcode}")))
                    && r == "road"
                    && *road == last
            }
            _ => false,
        };
        if !misread
            || !postcode.bytes().all(|b| b.is_ascii_digit())
            || postcode.bytes().all(|b| b == b'0')
        {
            return Ok(None);
        }
        let Some(cc) = self.country(&last) else {
            return Ok(None);
        };
        let mut parsed = self.parser.parse(head.trim())?.to_vec();
        // read alone, the part can still be a house number ("1 Rruga Ymer Puka, 4401")
        if let Some((k, _)) = parsed
            .iter_mut()
            .rev()
            .find(|(k, v)| k == "house_number" && v == postcode)
        {
            *k = "postcode".into();
        }
        let road = parsed.iter().rev().find(|(k, _)| k == "road");
        let number = parsed.iter().find(|(k, _)| k == "house_number");
        let (Some((_, road)), Some((_, number))) = (road, number) else {
            return Ok(None);
        };
        let housed = self
            .streets(
                without_number_mark(road),
                Some(house_number(number)),
                None,
                Some(postcode),
                Some(&cc),
                false,
            )
            .iter()
            .any(|h| {
                matches!(
                    h,
                    Hit::Street {
                        number: Some(_),
                        ..
                    }
                )
            });
        if !housed {
            return Ok(None);
        }
        parsed.push(("country".into(), last));
        Ok(Some(parsed.into()))
    }

    pub(crate) fn parse_address(&self, q: &str, corroboration: bool) -> Result<Option<Parsed>> {
        let mut parsed = self.parser.parse(q)?;
        if let Some(repaired) = self.country_as_road(q, &parsed)? {
            parsed = repaired;
        }
        if q.contains('號')
            && let Some((units, road, number)) = cn_tw::chinese(q)
            && let [state, city] = units.as_slice()
            && state.ends_with('市')
            && city.ends_with('區')
        {
            let native = q.split([',', '，']).find(|p| p.contains('號'));
            if let Some(native) = native {
                let cc = self.country(field(&parsed, "country"));
                let city_key = name_key(city);
                let state_key = name_key(state);
                if !self
                    .places(city, cc.as_deref(), Some(state), &|p| p.named(city_key))
                    .iter()
                    .any(|hit| {
                        let Hit::Place(id) = hit else { return false };
                        self.chain(*id)
                            .iter()
                            .any(|&p| self.places[p as usize].named(state_key))
                    })
                {
                    return Ok(None);
                }
                let mut repaired = Vec::new();
                for (key, value) in parsed.iter() {
                    if matches!(key.as_str(), "road" | "house_number") {
                        continue;
                    }
                    if matches!(
                        key.as_str(),
                        "city" | "suburb" | "city_district" | "state" | "state_district"
                    ) {
                        if !native.contains(value.as_str()) {
                            return Ok(None);
                        }
                        continue;
                    }
                    repaired.push((key.clone(), value.clone()));
                }
                repaired.extend([
                    ("state".into(), state.clone()),
                    ("city".into(), city.clone()),
                    ("road".into(), road),
                    ("house_number".into(), number),
                ]);
                parsed = repaired.into();
            }
        }
        if let Some(Hit::Street {
            name,
            number: Some(number),
            ..
        }) = self.hk_chinese(q)
        {
            let mut repaired: Vec<(String, String)> = parsed
                .iter()
                .filter(|(key, _)| {
                    matches!(key.as_str(), "unit" | "level" | "entrance" | "staircase")
                })
                .cloned()
                .collect();
            repaired.extend([
                ("road".into(), name),
                ("house_number".into(), number),
                ("country".into(), "香港".into()),
            ]);
            parsed = repaired.into();
        }
        // Repairing the parse also changes structured fallbacks; require a corroborated house first.
        if let Some((aligned, _)) = self.nz_comma_house(q, &parsed)? {
            return Ok(Some(aligned));
        }
        if !parsed
            .iter()
            .any(|(k, _)| matches!(k.as_str(), "house_number" | "postcode"))
        {
            let parts: Vec<_> = q.split(',').map(str::trim).collect();
            if let [town, region, country] = parts.as_slice()
                && let Some(cc) = self.country(country)
                && self
                    .country(field(&parsed, "country"))
                    .is_none_or(|stated| stated == cc)
            {
                let mut name = *town;
                let (mut found, mut exact) = self.places_named(
                    name,
                    Some(&cc),
                    Some(region),
                    &|p| matches!(p.subtype.as_ref(), "locality" | "localadmin"),
                    false,
                );
                if !exact
                    && let Some((bare, kind)) = town.rsplit_once(char::is_whitespace)
                    && kind.eq_ignore_ascii_case("village")
                {
                    name = bare.trim();
                    (found, exact) = self.places_named(
                        name,
                        Some(&cc),
                        Some(region),
                        &|p| p.key() == "village",
                        false,
                    );
                }
                if exact {
                    let canonical = found.iter().any(|hit| {
                        let Hit::Place(id) = hit else { return false };
                        let p = &self.places[*id as usize];
                        crate::matching::names_agree(name, &p.primary)
                            || crate::matching::names_agree(name, p.name(true))
                    });
                    for hit in found {
                        let Hit::Place(id) = hit else { continue };
                        if !self.places[id as usize].named(name_key(name)) {
                            continue;
                        }
                        if let Some(parent) = self.chain(id).iter().skip(1).find_map(|&id| {
                            let p = &self.places[id as usize];
                            (matches!(
                                p.subtype.as_ref(),
                                "localadmin" | "county" | "macrocounty" | "region" | "macroregion"
                            ) && p.all_names().any(crate::matching::agrees_with(region)))
                            .then_some(p)
                        }) {
                            let key = if matches!(parent.subtype.as_ref(), "region" | "macroregion")
                            {
                                "state"
                            } else {
                                "state_district"
                            };
                            return Ok(Some(
                                vec![
                                    ("city".into(), name.into()),
                                    (key.into(), parent.name(true).into()),
                                    ("country".into(), (*country).into()),
                                ]
                                .into(),
                            ));
                        }
                    }
                    if !canonical {
                        let (_, known) = self.places_exact(region, Some(&cc), None, &|p| {
                            matches!(
                                p.subtype.as_ref(),
                                "localadmin" | "county" | "macrocounty" | "region" | "macroregion"
                            )
                        });
                        if known {
                            // An alias cannot bypass a contradictory qualifier through fragment fallbacks.
                            return Ok(None);
                        }
                    }
                }
            }
        }
        if corroboration && parsed.iter().any(|(k, _)| k == "postcode") {
            return Ok(Some(parsed));
        }
        let first_cc = parsed
            .iter()
            .find(|(l, _)| l == "country")
            .and_then(|(_, c)| self.country(c));
        let stated = parsed
            .iter()
            .find(|(l, _)| l == "postcode")
            .map(|(_, v)| pc_norm(v));
        // A house number can also be a postcode: a US ZIP ("15411 Woodcliffe Trail, IN") or a
        // Taiwanese district's three digits ("511 林森路二段, 虎尾鎮"). With a state, or written in
        // its road's part of the query, it is the house; alone in a part it may be the postcode.
        let beside_road = |number: &str| {
            parsed.iter().filter(|(l, _)| l == "road").any(|(_, road)| {
                q.split(',').any(|part| {
                    let part = part.to_lowercase();
                    part.contains(number) && part.contains(road.as_str())
                })
            })
        };
        let house = parsed
            .iter()
            .find(|(l, _)| l == "house_number")
            .filter(|(_, v)| parsed.iter().any(|(l, _)| l == "state") || beside_road(v))
            .map(|(_, v)| pc_norm(v));
        if let Some((pc, rest)) = self.find_postcode(q, first_cc.as_deref())
            && stated.as_ref().is_none_or(|s| {
                pc_norm(&pc).len() > s.len() && pc_norm(&pc).starts_with(s.as_str())
            })
            && house != Some(pc_norm(&pc))
            // Lookup may try an ambiguous number; corroboration cannot overwrite requested fields.
            && (!corroboration
                || !["house_number", "road", "po_box", "unit", "level"].iter().any(|key| {
                    parsed.iter().find(|(k, _)| k == key).is_some_and(|(_, value)| {
                        pc_norm(value).contains(&pc_norm(&pc))
                    })
                }))
        {
            let mut repaired = self.parser.parse(&rest)?.to_vec();
            if corroboration {
                if ["house_number", "road"].iter().any(|key| {
                    repaired.iter().filter(|(k, _)| k == key).count() > 1
                        || (!field(&parsed, key).is_empty()
                            && field(&parsed, key) != field(&repaired, key))
                }) {
                    return Ok(Some(parsed));
                }
                for (key, value) in parsed.iter().filter(|(k, _)| {
                    matches!(
                        k.as_str(),
                        "country" | "state" | "state_district" | "country_region"
                    )
                }) {
                    repaired.retain(|(k, _)| k != key);
                    repaired.push((key.clone(), value.clone()));
                }
            }
            repaired.push(("postcode".into(), pc));
            parsed = repaired.into();
        }
        Ok(Some(parsed))
    }

    pub(super) fn parse(&self, q: &str) -> Vec<(String, String)> {
        // libpostal returns "İ" decomposed; the tokenizer would split at the combining dot
        let clean = |t: &str| {
            t.chars()
                .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
                .collect()
        };
        self.parse_address(q, false)
            .ok()
            .flatten()
            .map(|v| v.iter().map(|(l, t)| (l.clone(), clean(t))).collect())
            .unwrap_or_default()
    }
}
