use crate::{index::*, proto::opengeocoding::AddressDetails};

pub const MLIT_DIR: &str = "jp-mlit";

const MLIT: &[(&str, &str, &str)] = &[
    ("jp/hokkaido", "jp-mlit:JP-01", "911152"),
    ("jp/aomori", "jp-mlit:JP-02", "911161"),
    ("jp/iwate", "jp-mlit:JP-03", "911148"),
    ("jp/miyagi", "jp-mlit:JP-04", "911140"),
    ("jp/akita", "jp-mlit:JP-05", "911162"),
    ("jp/yamagata", "jp-mlit:JP-06", "911119"),
    ("jp/fukushima", "jp-mlit:JP-07", "911156"),
    ("jp/ibaraki", "jp-mlit:JP-08", "906200"),
    ("jp/tochigi", "jp-mlit:JP-09", "911125"),
    ("jp/gunma", "jp-mlit:JP-10", "911154"),
    ("jp/saitama", "jp-mlit:JP-11", "911129"),
    ("jp/chiba", "jp-mlit:JP-12", "911160"),
    ("jp/tokyo", "jp-mlit:JP-13", "911123"),
    ("jp/kanagawa", "jp-mlit:JP-14", "911145"),
    ("jp/niigata", "jp-mlit:JP-15", "911135"),
    ("jp/toyama", "jp-mlit:JP-16", "911121"),
    ("jp/ishikawa", "jp-mlit:JP-17", "911149"),
    ("jp/fukui", "jp-mlit:JP-18", "911158"),
    ("jp/yamanashi", "jp-mlit:JP-19", "911117"),
    ("jp/nagano", "jp-mlit:JP-20", "911138"),
    ("jp/gifu", "jp-mlit:JP-21", "911155"),
    ("jp/shizuoka", "jp-mlit:JP-22", "911126"),
    ("jp/aichi", "jp-mlit:JP-23", "911163"),
    ("jp/mie", "jp-mlit:JP-24", "911141"),
    ("jp/shiga", "jp-mlit:JP-25", "911128"),
    ("jp/kyoto", "jp-mlit:JP-26", "911142"),
    ("jp/osaka", "jp-mlit:JP-27", "906181"),
    ("jp/hyogo", "jp-mlit:JP-28", "911151"),
    ("jp/nara", "jp-mlit:JP-29", "911136"),
    ("jp/wakayama", "jp-mlit:JP-30", "911120"),
    ("jp/tottori", "jp-mlit:JP-31", "911122"),
    ("jp/shimane", "jp-mlit:JP-32", "911127"),
    ("jp/okayama", "jp-mlit:JP-33", "911133"),
    ("jp/hiroshima", "jp-mlit:JP-34", "911153"),
    ("jp/yamaguchi", "jp-mlit:JP-35", "911118"),
    ("jp/tokushima", "jp-mlit:JP-36", "911124"),
    ("jp/kagawa", "jp-mlit:JP-37", "911147"),
    ("jp/ehime", "jp-mlit:JP-38", "911159"),
    ("jp/kochi", "jp-mlit:JP-39", "911144"),
    ("jp/fukuoka", "jp-mlit:JP-40", "911157"),
    ("jp/saga", "jp-mlit:JP-41", "911130"),
    ("jp/nagasaki", "jp-mlit:JP-42", "911137"),
    ("jp/kumamoto", "jp-mlit:JP-43", "911143"),
    ("jp/oita", "jp-mlit:JP-44", "911134"),
    ("jp/miyazaki", "jp-mlit:JP-45", "911139"),
    ("jp/kagoshima", "jp-mlit:JP-46", "911146"),
    ("jp/okinawa", "jp-mlit:JP-47", "911132"),
];

pub fn mlit_source(dataset: &str, version: &str) -> Option<(&'static str, bool)> {
    MLIT.iter()
        .find(|&&(name, _, _)| name == dataset)
        .map(|&(_, tag, old)| (tag, version == old))
}

pub fn mlit_number(number: &str, legacy: bool) -> Option<&str> {
    if !legacy {
        return (!number.trim().is_empty()).then_some(number.trim());
    }
    // OA 304d22e fixed this join; the pinned pre-fix jobs appended the coordinate zone, not a residence number.
    let (number, zone) = number.rsplit_once('-')?;
    (!number.is_empty() && (1..=19).contains(&zone.parse::<u8>().ok()?)).then_some(number)
}

static POSTCODE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^(?:日本国?)?〒?([0-9０-９]{3}[-－−‐―‑ー]?[0-9０-９]{4})").unwrap()
});

/// Reads 一 to 九十九.
fn kanji_number(t: &str) -> Option<u32> {
    let digit = |c: char| {
        "〇一二三四五六七八九"
            .chars()
            .position(|d| d == c)
            .map(|d| d as u32)
    };
    let chars: Vec<char> = t.chars().collect();
    match chars.as_slice() {
        ['十'] => Some(10),
        [c] => digit(*c),
        ['十', u] => Some(10 + digit(*u)?),
        [t, '十'] => Some(digit(*t)? * 10),
        [t, '十', u] => Some(digit(*t)? * 10 + digit(*u)?),
        _ => None,
    }
}

/// Writes 1 to 99 as 一 to 九十九.
fn kanji_numeral(n: u32) -> Option<String> {
    let digit = |d: u32| "〇一二三四五六七八九".chars().nth(d as usize);
    let (tens, units) = (n / 10, n % 10);
    let mut out = String::new();
    match tens {
        0 => {}
        1 => out.push('十'),
        2..=9 => out.extend([digit(tens)?, '十']),
        _ => return None,
    }
    if units > 0 {
        out.push(digit(units)?);
    }
    (n > 0).then_some(out)
}

/// Candidate (municipality, street, number) readings of a Japanese block address, most likely
/// first: "東京都港区芝公園4-2-8" is chōme 4 of 芝公園 in 港区, block 2, house 8, stored as street
/// "芝公園四丁目" and number "2-8"; towns without chōme store "block-house" under the town.
pub(crate) fn japanese(q: &str) -> Vec<(String, String, String)> {
    readings(q, false)
}

fn readings(q: &str, block: bool) -> Vec<(String, String, String)> {
    static NUMBER: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"[0-9]+(?:(?:[-－−‐―‑ーの]|番地?)[0-9]+){0,3}(?:号|番地?)?$").unwrap()
    });
    let mut parts = q.split(',').map(str::trim);
    let head = parts.next().unwrap_or("");
    if parts.any(|p| {
        let country = crate::matching::country_named(p);
        !country.is_empty() && country != "jp"
    }) {
        return vec![];
    }
    static CHOME: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new("([〇一二三四五六七八九十]+)丁目").unwrap());
    let t: String = head
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap(),
            _ => c,
        })
        .collect();
    let t = t
        .strip_prefix("日本国")
        .or_else(|| t.strip_prefix("日本"))
        .unwrap_or(&t);
    let t = POSTCODE.replace(t, "");
    let t = CHOME.replace_all(&t, |c: &regex::Captures| {
        kanji_number(&c[1]).map_or(c[0].to_string(), |n| format!("{n}丁目"))
    });
    let Some(tail) = NUMBER.find(&t) else {
        return vec![];
    };
    let (prefix, rest) = t.split_at(tail.start());
    let explicit = prefix.ends_with("丁目");
    if prefix.is_empty()
        || !prefix
            .chars()
            .all(|c| c as u32 >= 0x3040 || c == '々' || explicit && c.is_ascii_digit())
    {
        return vec![];
    }
    let number = rest
        .trim_end_matches("番地")
        .trim_end_matches('番')
        .replace("番地", "-")
        .replace(['番', 'の', '－', '−', '‐', 'ー', '―', '‑'], "-")
        .replace('号', "");
    let numbers: Vec<&str> = number.split('-').collect();
    if !explicit && numbers.len() < 2 && !block || explicit && numbers.len() > 3 {
        return vec![];
    }
    let prefecture = town_queries(q).0;
    let first_digit = prefix
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(prefix.len());
    let mut out = vec![];
    for (i, c) in prefix.char_indices().rev() {
        let end = i + c.len_utf8();
        if !"都道府県市区郡町村".contains(c) || end == prefix.len() || end == first_digit {
            continue;
        }
        let (city, town) = prefix.split_at(end);
        let city = city.strip_prefix(&prefecture).unwrap_or(city);
        if city.is_empty() {
            continue;
        }
        if !explicit && numbers.len() >= 3 {
            out.push((
                city.to_string(),
                format!("{town}{}丁目", numbers[0]),
                numbers[1..].join("-"),
            ));
        }
        if numbers.len() <= 3 {
            out.push((city.to_string(), town.to_string(), number.clone()));
        }
        if numbers.len() == 1
            && let Some(stem) = town.strip_suffix("丁目")
        {
            let base = stem.trim_end_matches(|c: char| c.is_ascii_digit());
            let chome = &stem[base.len()..];
            if !chome.is_empty() {
                out.push((
                    city.to_string(),
                    base.to_string(),
                    format!("{chome}-{number}"),
                ));
            }
        }
    }
    out
}

fn scope(index: &Index, city: &str, prefecture: &str, code: &str, qualifiers: &[u64]) -> Vec<u32> {
    let names = std::iter::once(city).chain(city.char_indices().filter_map(|(i, c)| {
        let end = i + c.len_utf8();
        ("市区郡町村".contains(c) && end < city.len()).then_some(&city[end..])
    }));
    let mut scope: Vec<_> = names
        .flat_map(|named| {
            index.places(named, Some("jp"), Some(prefecture), &|p| {
                p.named(name_key(named))
                    && (matches!(p.subtype.as_ref(), "locality" | "localadmin" | "county")
                        || city.ends_with('区')
                            && matches!(p.subtype.as_ref(), "macrohood" | "neighborhood"))
            })
        })
        .filter_map(|h| {
            let Hit::Place(id) = h else {
                return None;
            };
            let chain = index.chain(id);
            if !chain
                .iter()
                .any(|&p| index.places[p as usize].region.eq_ignore_ascii_case(code))
                || qualifiers
                    .iter()
                    .any(|&key| !chain.iter().any(|&p| index.places[p as usize].named(key)))
            {
                return None;
            }
            let mut rest = city;
            for &p in chain.iter().rev() {
                if let Some(name) = index.places[p as usize]
                    .all_names()
                    .filter(|n| !n.is_empty() && rest.starts_with(n))
                    .max_by_key(|n| n.len())
                {
                    rest = &rest[name.len()..];
                }
            }
            rest.is_empty().then_some(id)
        })
        .collect();
    scope.sort_unstable();
    scope.dedup();
    scope
}

pub(crate) fn ward(index: &Index, q: &str) -> Option<Hit> {
    let (city, _, _) = japanese(q)
        .into_iter()
        .find(|(city, _, _)| city.ends_with('区'))?;
    let prefecture = town_queries(q).0;
    let code = index.region_named(&prefecture, "jp")?;
    let ids = scope(index, &city, &prefecture, &code, &[]);
    match ids.as_slice() {
        [id] => Some(Hit::Place(*id)),
        _ => None,
    }
}

#[derive(Default)]
pub(crate) struct Blocks {
    numbers: Vec<tantivy_fst::Map<tantivy::directory::OwnedBytes>>,
    far: Vec<Option<tantivy_fst::Map<tantivy::directory::OwnedBytes>>>,
}

impl Blocks {
    pub(crate) fn open(dir: &std::path::Path) -> anyhow::Result<Self> {
        use tantivy::directory::Directory as _;
        let dir = dir.join(MLIT_DIR);
        if !dir.exists() {
            return Ok(Self::default());
        }
        let shards: usize = std::fs::read_to_string(dir.join("numbers.shards"))?
            .trim()
            .parse()?;
        anyhow::ensure!((1..=256).contains(&shards), "invalid MLIT shard count");
        let mmap = tantivy::directory::MmapDirectory::open(&dir)?;
        let load = |name: &str| -> anyhow::Result<_> {
            Ok(tantivy_fst::Map::from(tantivy_fst::raw::Fst::new(
                mmap.open_read(std::path::Path::new(name))?.read_bytes()?,
            )?))
        };
        let numbers = (0..shards)
            .map(|s| load(&format!("numbers-{s:02}.fst")))
            .collect::<anyhow::Result<_>>()?;
        let far = (0..shards)
            .map(|s| {
                let name = schema::far_file(s);
                dir.join(&name).exists().then(|| load(&name)).transpose()
            })
            .collect::<anyhow::Result<_>>()?;
        Ok(Self { numbers, far })
    }

    fn get(&self, sid: u32, number: &str) -> Option<(f64, f64)> {
        if self.numbers.is_empty() {
            return None;
        }
        let shard = sid as usize % self.numbers.len();
        let mut key = sid.to_be_bytes().to_vec();
        key.extend_from_slice(number.as_bytes());
        if self.far[shard]
            .as_ref()
            .is_some_and(|m| m.get(&key).is_some())
        {
            return None;
        }
        let (x, y, _) = schema::unpack(self.numbers[shard].get(&key)?);
        Some((x, y))
    }
}

pub(crate) fn block_hit(index: &Index, doc: DocAddress, number: &str) -> Option<Hit> {
    let sid = index.fast(doc, "id") as u32;
    let (x, y) = index.jp_blocks.get(sid, number)?;
    let data = index.document(doc).ok()?;
    Some(Hit::Street {
        doc,
        sid,
        place: index.fast(doc, "place") as u32,
        name: format!("{}{number}番", stored(&data, index.f.name)),
        x,
        y,
        number: None,
        postcode: None,
        city: None,
        postal_town: None,
        cc: "jp".into(),
    })
}

pub(crate) fn block_suggestion(index: &Index, doc: DocAddress, number: &str) -> Option<Suggestion> {
    let Hit::Street {
        sid,
        place,
        name,
        x,
        y,
        ..
    } = block_hit(index, doc, number)?
    else {
        return None;
    };
    let mut suggestion = index.street_suggestion(doc, sid, place, &name, None, (x, y));
    suggestion.id = format!("b{sid}:{number}");
    suggestion.retrieval_id = format!(
        "{}/b{}:{}:{number}",
        index.suggestion_scope, doc.segment_ord, doc.doc_id
    );
    Some(suggestion)
}

pub(crate) fn areas(index: &Index, q: &str) -> Option<Vec<(Hit, String)>> {
    use tantivy::schema::Value as _;
    if index.jp_blocks.numbers.is_empty() || !q.split(',').next()?.trim_end().ends_with('番') {
        return None;
    }
    let readings = readings(q, true);
    if readings.is_empty() {
        return None;
    }
    let prefecture = town_queries(q).0;
    let code = index.region_named(&prefecture, "jp")?;
    let region = *index.regions.get(code.as_ref())?;
    let tag = format!("jp-mlit:{code}");
    if q.split(',')
        .skip(1)
        .any(|p| index.country(p.trim()).as_deref() != Some("jp"))
    {
        return Some(vec![]);
    }
    let compact: String = q.chars().filter(|c| !c.is_whitespace()).collect();
    if POSTCODE.is_match(&compact) {
        return Some(vec![]);
    }
    let mut hits = vec![];
    for (city, road, number) in readings {
        if !number.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let mut clauses = index.musts(index.f.names, &road);
        clauses.extend(index.musts(index.f.city, &city));
        clauses.push(must(term(index.f.ids, &region.to_string())));
        let docs = index.top_of(Kind::Street, Some("jp"), clauses, 1000);
        if docs.len() == 1000 {
            return Some(vec![]);
        }
        for doc in docs {
            let stored = index.document(doc).ok()?;
            if !stored
                .get_all(index.f.name)
                .filter_map(|v| v.as_str())
                .any(|n| fold(n) == fold(&road))
            {
                continue;
            }
            let mut found = false;
            let mut valid = true;
            for value in stored.get_all(index.f.postal_town) {
                let (_, n, name, source): (&str, &str, &str, &str) =
                    serde_json::from_str(value.as_str()?).ok()?;
                if n != number || !source.starts_with("jp-mlit:") {
                    continue;
                }
                found = true;
                valid &= source == tag && crate::matching::names_agree(&city, name);
            }
            if found
                && valid
                && let Some(hit) = block_hit(index, doc, &number)
                && !hits.iter().any(|(h, n)| {
                    matches!(h, Hit::Street { doc: d, .. } if *d == doc) && n == &number
                })
            {
                hits.push((hit, number.clone()));
            }
        }
    }
    Some(hits)
}

pub(crate) fn houses(index: &Index, q: &str) -> Option<Vec<Hit>> {
    use tantivy::schema::Value as _;
    if q.is_ascii()
        || q.split_whitespace()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|p| p[0].ends_with(char::is_numeric) && p[1].starts_with(char::is_numeric))
    {
        return None;
    }
    let readings = japanese(q);
    if !readings.first().is_some_and(|(_, _, n)| n.contains('-')) {
        return None;
    }
    let prefecture = town_queries(q).0;
    let code = index.region_named(&prefecture, "jp")?;
    let compact: String = q.chars().filter(|c| !c.is_whitespace()).collect();
    let mut postcode = POSTCODE.captures(&compact).map(|c| {
        schema::number_key(&c[1])
            .chars()
            .filter(char::is_ascii_digit)
            .collect::<String>()
    });
    let mut qualifiers = vec![];
    for part in q.split(',').skip(1).map(str::trim) {
        if let Some(country) = index.country(part) {
            if country != "jp" {
                return None;
            }
            continue;
        }
        let place = if let Some((pc, place)) = index.find_postcode(part, Some("jp")) {
            if pc_norm(&pc).len() != 7 || !postcode_not_contradicted(postcode.as_deref(), Some(&pc))
            {
                return None;
            }
            postcode = Some(pc);
            place
        } else {
            part.to_string()
        };
        if !place.is_empty() {
            qualifiers.push(name_key(&place));
        }
    }
    let mut hits = vec![];
    for (city, road, number) in readings {
        let scope = scope(index, &city, &prefecture, &code, &qualifiers);
        if scope.is_empty() {
            continue;
        }
        let mut parents = vec![];
        if city.ends_with('区') {
            for &id in &scope {
                if let Some(parent) = index.chain(id).into_iter().skip(1).find(|&p| {
                    matches!(
                        index.places[p as usize].subtype.as_ref(),
                        "locality" | "localadmin"
                    )
                }) && !parents.contains(&parent)
                {
                    parents.push(parent);
                }
            }
        }
        for lookup in [&scope, &parents] {
            if lookup.is_empty() {
                continue;
            }
            let before = hits.len();
            let mut clauses = index.musts(index.f.names, &road);
            clauses.push(must(Box::new(BooleanQuery::new(
                lookup
                    .iter()
                    .map(|p| (Occur::Should, term(index.f.ids, &p.to_string())))
                    .collect(),
            ))));
            let docs = index.top_of(Kind::Street, Some("jp"), clauses, 1000);
            if docs.len() == 1000 {
                return Some(vec![]);
            }
            for doc in docs {
                let stored = index.document(doc).ok()?;
                if !stored
                    .get_all(index.f.name)
                    .filter_map(|v| v.as_str())
                    .any(|n| fold(n) == fold(&road))
                {
                    continue;
                }
                let sid = index.fast(doc, "id") as u32;
                let Some((number, value)) = index.house_record(sid, &schema::number_key(&number))
                else {
                    continue;
                };
                let (x, y, own) = schema::unpack(value);
                let hit = index.house_hit(doc, number, x, y, own)?;
                if let Hit::Street {
                    place,
                    postal_town,
                    number: Some(number),
                    ..
                } = &hit
                {
                    let chain = index.chain(*place);
                    if !scope.iter().any(|id| chain.contains(id)) {
                        let mut key = sid.to_be_bytes().to_vec();
                        key.extend_from_slice(number.as_bytes());
                        if !postal_town.as_deref().is_some_and(|town| {
                            crate::matching::names_agree(
                                &city,
                                town.strip_prefix(&prefecture).unwrap_or(town),
                            )
                        }) || chain.iter().any(|&p| {
                            index.places[p as usize]
                                .all_names()
                                .any(|n| n.ends_with('区'))
                        }) || index.far[sid as usize % index.far.len()]
                            .as_ref()
                            .is_some_and(|m| m.get(&key).is_some())
                        {
                            continue;
                        }
                    }
                }
                if matches!(&hit, Hit::Street { postcode: own, .. }
                if postcode.as_deref().is_none_or(|pc| own.as_deref().is_some_and(|p| pc_norm(p) == pc_norm(pc))))
                {
                    hits.push(hit);
                }
            }
            if hits.len() > before {
                break;
            }
        }
    }
    hits.sort_by_key(|h| match h {
        Hit::Street { sid, .. } => *sid,
        _ => 0,
    });
    if let Some(Hit::Street { name, number, x, y, .. }) = hits.first()
        && hits.iter().all(|h| matches!(h, Hit::Street { name: n, number: hn, x: hx, y: hy, .. }
            if fold(n) == fold(name) && hn == number && ((hx - x) * y.to_radians().cos()).hypot(hy - y) < 0.0009))
    {
        // ponytail: co-located copies of one road/number collapse within roughly 100 m; building-level disambiguation needs source identities in the number index.
        hits.truncate(1);
    }
    Some(hits)
}

pub(crate) fn ward_house_matches(
    index: &Index,
    q: &str,
    number: &str,
    (lon, lat): (f32, f32),
) -> bool {
    if number.is_empty()
        || !q.contains('区')
        || !japanese(q).iter().any(|(city, _, _)| city.ends_with('区'))
    {
        return true;
    }
    houses(index, q).is_none_or(|hits| {
        matches!(hits.as_slice(), [Hit::Street { number: Some(n), x, y, .. }]
            if schema::number_key(n) == schema::number_key(number) && *x as f32 == lon && *y as f32 == lat)
    })
}

pub(crate) fn suggest(index: &Index, q: &str, want: usize) -> Option<Vec<Suggestion>> {
    if let Some(hits) = areas(index, q) {
        return Some(
            hits.into_iter()
                .take(want)
                .filter_map(|(h, n)| {
                    let Hit::Street { doc, .. } = h else {
                        return None;
                    };
                    block_suggestion(index, doc, &n)
                })
                .collect(),
        );
    }
    Some(
        houses(index, q)?
            .into_iter()
            .filter_map(|h| match h {
                Hit::Street {
                    doc,
                    sid,
                    place,
                    name,
                    number,
                    x,
                    y,
                    ..
                } => {
                    Some(index.street_suggestion(doc, sid, place, &name, number.as_deref(), (x, y)))
                }
                _ => None,
            })
            .take(want)
            .collect(),
    )
}

/// "〒812-0053 福岡県福岡市東区箱崎２丁目１０−３５": the ward, city and prefecture written before
/// the rest, as place lists the index answers ("東区, 福岡市", "福岡市, 福岡県"), most specific
/// first, when the text starts with the prefecture; or a city written in romaji with its "-shi"
/// ("Kitakyushu-shi Fukuoka"); and the prefecture as written ("" if not). None for text that
/// names neither.
pub(crate) fn town_queries(q: &str) -> (String, Vec<String>) {
    static ROMAJI: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?i)\b([a-z]{3,})-shi\b(?:[\s,]+([a-z]{3,})(?:-ken)?\b)?").unwrap()
    });
    if let Some(c) = ROMAJI.captures(q) {
        let pref = c.get(2).map_or("", |p| p.as_str());
        let within = if pref.is_empty() {
            String::new()
        } else {
            format!("{pref}, ")
        };
        return (pref.into(), vec![format!("{}, {within}Japan", &c[1])]);
    }
    let q = q.trim_start_matches(['日', '本', '国']);
    let t: String = q
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap(),
            _ => c,
        })
        .collect();
    let t = POSTCODE.replace(&t, "");
    let kanji = |s: &str| !s.is_empty() && s.chars().all(|c| c as u32 >= 0x3040 || c == '々');
    // the first `mark` among the first `within` characters, and what follows it
    let cut = |s: &str, marks: &[char], within: usize| -> Option<(String, String)> {
        let at = s
            .chars()
            .take(within)
            .enumerate()
            .find_map(|(i, c)| (i > 0 && marks.contains(&c)).then_some(i))?;
        let end = at
            + 1
            + s.chars()
                .skip(at + 1)
                .take_while(|c| marks.contains(c))
                .count();
        let head: String = s.chars().take(end).collect();
        Some((head.clone(), s[head.len()..].to_string()))
            .filter(|(h, _)| h.chars().count() >= 2 && kanji(h))
    };
    // "四日市 1465-16, 大分県宇佐市": without the prefecture first, a name ending in 市 is not
    // always a city
    let Some((pref, rest)) = ["東京都", "北海道", "京都府", "大阪府"]
        .iter()
        .find_map(|p| t.strip_prefix(p).map(|r| (p.to_string(), r.to_string())))
        .or_else(|| cut(&t, &['県'], 4))
    else {
        return (String::new(), vec![]);
    };
    let (city, rest) = cut(&rest, &['市'], 6).map_or((None, rest.clone()), |(c, r)| (Some(c), r));
    let (ward, rest) = cut(&rest, &['区'], 5).map_or((None, rest), |(w, r)| (Some(w), r));
    let mut out = vec![];
    let within = [&ward, &city]
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let within = if within.is_empty() {
        pref.clone()
    } else {
        within
    };
    // "浅草2-3-1": the chōme, 浅草2丁目
    let at = rest
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(rest.len());
    let (town, numbers) = rest.split_at(at);
    let chome: String = numbers.chars().take_while(char::is_ascii_digit).collect();
    // stored as 浅草二丁目: "2丁目" finds other chōme of a like name
    if kanji(town)
        && let Some(n) = chome.parse().ok().and_then(kanji_numeral)
    {
        out.push(format!("{town}{n}丁目, {within}"));
    }
    if let Some(w) = &ward {
        out.push(format!("{w}, {}", city.as_ref().unwrap_or(&pref)));
    }
    if let Some(c) = &city {
        out.push(format!("{c}, {pref}"));
    }
    (pref, out)
}

pub(crate) fn conflicting_prefecture(index: &Index, q: &str, address: &AddressDetails) -> bool {
    if address.country_code != "jp" || address.region_codes.is_empty() {
        return false;
    }
    let (prefecture, _) = town_queries(q);
    !prefecture.is_empty()
        && index.region_named(&prefecture, "jp").is_some_and(|code| {
            address
                .region_codes
                .iter()
                .all(|r| !r.code.eq_ignore_ascii_case(&code))
        })
}

pub(crate) fn administrative_aligned(
    q: &str,
    whole: &[(String, String)],
) -> Option<Vec<(String, String)>> {
    if !q.contains(['都', '道', '府', '県'])
        || !whole
            .iter()
            .any(|(k, v)| k == "house_number" && !v.is_empty())
        || !whole
            .iter()
            .any(|(k, v)| k == "country" && crate::matching::country_named(v) == "jp")
        || whole
            .iter()
            .any(|(k, v)| k == "country" && crate::matching::country_named(v) != "jp")
    {
        return None;
    }
    let (prefecture, queries) = town_queries(q);
    let (city, within) = queries.last()?.split_once(", ")?;
    // ponytail: only full chōme/block/house forms; block-only support needs a block lookup,
    // not a chōme-block house guess on another chōme.
    if within != prefecture
        || !prefecture.ends_with(['都', '道', '府', '県'])
        || !city.ends_with(['市', '区'])
        || !japanese(q)
            .first()
            .is_some_and(|(_, _, number)| number.contains('-'))
    {
        return None;
    }
    let ward = queries
        .iter()
        .rev()
        .nth(1)
        .and_then(|p| p.strip_suffix(&format!(", {city}")))
        .filter(|p| p.ends_with('区'));
    // libpostal can split 中津市 into state 大分県中 and city 津市, a different city.
    let mut aligned: Vec<_> = whole
        .iter()
        .filter(|(k, _)| k != "state" && k != "city" && !(k == "city_district" && ward.is_some()))
        .cloned()
        .collect();
    aligned.extend([("state".into(), prefecture), ("city".into(), city.into())]);
    if let Some(ward) = ward {
        aligned.push(("city_district".into(), ward.into()));
    }
    Some(aligned)
}

/// "朝日町伊勢宮 472-2", "472-2 朝日町伊勢宮" or "Koyodai 258-2": a Japanese town, district or
/// road and its number, which libpostal cuts into unrelated fields when the name is unspaced kanji
/// or romaji.
pub(crate) fn japanese_head(head: &str) -> Option<Vec<(String, String)>> {
    const NUMBER: &str = "[0-9０-９]+(?:[-－‐−][0-9０-９]+)*";
    static HEAD: LazyLock<[regex::Regex; 2]> = LazyLock::new(|| {
        [
            format!(r"^\s*(?<name>\S.*?)\s+(?<number>{NUMBER})\s*$"),
            format!(r"^\s*(?<number>{NUMBER})\s+(?<name>\S.*?)\s*$"),
        ]
        .map(|re| regex::Regex::new(&re).unwrap())
    });
    let c = HEAD.iter().find_map(|re| re.captures(head))?;
    if !c["name"].chars().any(char::is_alphabetic) {
        return None;
    }
    Some(vec![
        ("road".into(), c["name"].to_lowercase()),
        ("house_number".into(), c["number"].to_lowercase()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kanji_numerals() {
        for n in 1..100 {
            assert_eq!(kanji_number(&kanji_numeral(n).unwrap()), Some(n));
        }
        assert_eq!(kanji_numeral(0), None);
        assert_eq!(kanji_numeral(100), None);
    }

    #[test]
    fn mlit_provenance_and_legacy_coordinate_zones() {
        assert_eq!(MLIT.len(), 47);
        assert_eq!(
            MLIT.iter()
                .map(|(_, tag, _)| tag)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            47
        );
        assert_eq!(
            mlit_source("jp/oita", "911134"),
            Some(("jp-mlit:JP-44", true))
        );
        assert_eq!(
            mlit_source("jp/oita", "future"),
            Some(("jp-mlit:JP-44", false))
        );
        assert!(mlit_source("jp/countrywide", "911134").is_none());
        assert_eq!(mlit_number("5-12", true), Some("5"));
        assert_eq!(mlit_number("1-2-9", true), Some("1-2"));
        assert_eq!(mlit_number("5", false), Some("5"));
        assert_eq!(mlit_number("5-12", false), Some("5-12"));
        for invalid in ["5", "5-0", "5-20", "5-x", "-12"] {
            assert!(mlit_number(invalid, true).is_none(), "{invalid}");
        }
    }

    #[test]
    fn japanese_towns() {
        assert_eq!(
            readings("日本石川県野々市市菅原町11番", true)[0],
            ("野々市市".into(), "菅原町".into(), "11".into())
        );
        assert!(
            town_queries("石川県野々市市菅原町11番")
                .1
                .contains(&"野々市市, 石川県".into())
        );
        assert_eq!(
            town_queries("〒812-0053 福岡県福岡市東区箱崎２丁目１０−３５"),
            (
                "福岡県".into(),
                vec![
                    "箱崎二丁目, 東区, 福岡市".into(),
                    "東区, 福岡市".into(),
                    "福岡市, 福岡県".into()
                ]
            )
        );
        assert_eq!(
            town_queries("福岡県福岡市東区").1,
            ["東区, 福岡市", "福岡市, 福岡県"]
        );
        assert_eq!(
            town_queries("東京都台東区浅草2-3-1").1,
            ["浅草二丁目, 台東区", "台東区, 東京都"]
        );
        // "四日市" is a town of 宇佐市, not Yokkaichi
        assert!(
            town_queries("四日市 1465-16, 大分県宇佐市, 879-0471, Japan")
                .1
                .is_empty()
        );
        assert!(town_queries("Kitakyushu, Fukuoka").1.is_empty());
        assert_eq!(
            town_queries("2-4-26 Gamou Kokuraminami-ku Kitakyushu-shi Fukuoka, 802-0978"),
            ("Fukuoka".into(), vec!["Kitakyushu, Fukuoka, Japan".into()])
        );
        assert_eq!(town_queries("Sapporo-shi, 060-0001").1, ["Sapporo, Japan"]);
    }

    #[test]
    fn japanese_block_addresses() {
        let got = japanese("〒105-0011 東京都港区芝公園４丁目２番８号");
        assert_eq!(got[0], ("港区".into(), "芝公園4丁目".into(), "2-8".into()));
        assert_eq!(got.len(), 1);
        assert_eq!(
            japanese("東京都千代田区大手町1-1-1")[0],
            ("千代田区".into(), "大手町1丁目".into(), "1-1".into())
        );
        assert_eq!(japanese("大阪府大阪市北区梅田三丁目1-3")[0].1, "梅田3丁目");
        assert_eq!(kanji_number("二十一"), Some(21));
        assert!(japanese("1600 Pennsylvania Avenue").is_empty());
        assert!(japanese("東京都").is_empty());
        for query in [
            "大分県中津市沖代町一丁目5番",
            "大分県中津市沖代町一丁目5番地",
        ] {
            assert_eq!(
                japanese(query),
                [
                    ("中津市".into(), "沖代町1丁目".into(), "5".into()),
                    ("中津市".into(), "沖代町".into(), "1-5".into()),
                ]
            );
        }
        assert!(japanese("大分県中津市沖代町一丁目5-9, China").is_empty());
        for (query, road, number) in [
            ("日本青森県十和田市東四番町5-6-3", "東四番町", "5-6-3"),
            (
                "北海道帯広市西１９条南２丁目31-4-8, Japan",
                "西19条南2丁目",
                "31-4-8",
            ),
            ("大分県中津市海風一丁目2-3-4, Japan", "海風1丁目", "2-3-4"),
            ("大分県中津市海風1-2-3-4, Japan", "海風1丁目", "2-3-4"),
        ] {
            let found = japanese(query);
            assert!(
                found.iter().any(|(_, r, n)| r == road && n == number),
                "{query}: {found:?}"
            );
            if query.contains("丁目") {
                assert!(
                    found.iter().all(|(_, _, n)| n == number),
                    "{query}: {found:?}"
                );
            }
        }
    }

    #[test]
    fn written_administration_survives_a_split_city() {
        let whole = [
            ("country", "日本"),
            ("state", "大分県中"),
            ("city", "津市"),
            ("suburb", "沖代町一丁目"),
            ("house_number", "5番9号"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()));
        for q in [
            "日本大分県中津市沖代町一丁目5番9号",
            "大分県中津市沖代町一丁目5番9号, Japan",
        ] {
            let read = administrative_aligned(q, &whole).unwrap();
            let get = |key| read.iter().find(|(k, _)| k == key).unwrap().1.as_str();
            assert_eq!(get("state"), "大分県");
            assert_eq!(get("city"), "中津市");
            assert_eq!(get("suburb"), "沖代町一丁目");
            assert_eq!(get("house_number"), "5番9号");
        }
        let read =
            administrative_aligned("福岡県福岡市東区箱崎二丁目10番35号, Japan", &whole).unwrap();
        assert!(read.contains(&("city".into(), "福岡市".into())));
        assert!(read.contains(&("city_district".into(), "東区".into())));
        for (prefecture, city) in [
            ("三重県", "四日市市"),
            ("千葉県", "市川市"),
            ("千葉県", "市原市"),
        ] {
            let read = administrative_aligned(
                &format!("{prefecture}{city}風見町一丁目2番3号, Japan"),
                &whole,
            )
            .unwrap();
            assert!(read.contains(&("city".into(), city.into())));
        }
        assert!(administrative_aligned("日本大分県中津市沖代町一丁目5番", &whole).is_none());
        let without_number = whole
            .iter()
            .filter(|(k, _)| k != "house_number")
            .cloned()
            .collect::<Vec<_>>();
        assert!(
            administrative_aligned("日本大分県中津市沖代町一丁目5番9号", &without_number).is_none()
        );
        let foreign = [("country".into(), "China".into())];
        assert!(administrative_aligned("大分県中津市沖代町一丁目5番, China", &foreign).is_none());
        let mut conflicting = whole.to_vec();
        conflicting.push(("country".into(), "China".into()));
        assert!(
            administrative_aligned("大分県中津市沖代町一丁目5番9号, China", &conflicting).is_none()
        );
        assert!(administrative_aligned("沖代町一丁目5番, Japan", &whole).is_none());
        assert!(administrative_aligned("Kitakyushu-shi Fukuoka, Japan", &whole).is_none());
    }

    #[test]
    fn japanese_town_and_number_heads() {
        let house = |road: &str, number: &str| {
            Some(vec![
                ("road".to_string(), road.to_string()),
                ("house_number".to_string(), number.to_string()),
            ])
        };
        assert_eq!(
            japanese_head("朝日町伊勢宮 472-2"),
            house("朝日町伊勢宮", "472-2")
        );
        assert_eq!(japanese_head("828 今津町福岡"), house("今津町福岡", "828"));
        assert_eq!(japanese_head("Koyodai 258-2"), house("koyodai", "258-2"));
        assert_eq!(
            japanese_head("国道１５３号 3380-1"),
            house("国道１５３号", "3380-1")
        );
        assert_eq!(
            japanese_head("433-1 畦町433-1"),
            house("畦町433-1", "433-1")
        );
        // a number with letters or kanji in it is the parser's to read
        assert_eq!(japanese_head("中宮 オ9"), None);
        assert_eq!(japanese_head("浜北袋井線 山科字前田3399"), None);
        assert_eq!(japanese_head("7-1"), None);
        assert_eq!(japanese_head("12 34"), None);
    }
}
