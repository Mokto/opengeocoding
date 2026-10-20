//! Street-name token canonicalization, applied after lowercasing and ASCII folding at build and
//! query time alike. Each variant maps to one short form, so "S Vermont Ave" and "South Vermont
//! Avenue", or "Hauptstr." and "Hauptstraße", fold to the same tokens. Some short forms are shared
//! across meanings (st: street/saint, dr: drive/doctor, cl: calle/close, pl: square); that is
//! harmless because both sides of a match are mapped the same way.
use tantivy::tokenizer::{Token, TokenFilter, TokenStream, Tokenizer};

/// (variant, canonical form) of folded tokens.
pub const VARIANTS: &[(&str, &str)] = &[
    // English
    ("street", "st"),
    ("saint", "st"),
    ("sankt", "st"),
    ("sint", "st"),
    ("sainte", "ste"),
    ("road", "rd"),
    ("avenue", "av"),
    ("ave", "av"),
    ("avenida", "av"),
    ("avda", "av"),
    ("boulevard", "blvd"),
    ("boul", "blvd"),
    ("bd", "blvd"),
    ("bvd", "blvd"),
    ("bulevar", "blvd"),
    ("blvr", "blvd"),
    ("drive", "dr"),
    ("drv", "dr"),
    ("doctor", "dr"),
    ("doutor", "dr"),
    ("lane", "ln"),
    ("court", "ct"),
    ("crt", "ct"),
    ("place", "pl"),
    ("plaza", "pl"),
    ("pza", "pl"),
    ("piazza", "pl"),
    ("praca", "pl"),
    ("pca", "pl"),
    ("plac", "pl"),
    ("terrace", "ter"),
    ("tce", "ter"),
    ("terr", "ter"),
    ("circle", "cir"),
    ("crescent", "cres"),
    ("close", "cl"),
    ("calle", "cl"),
    ("cll", "cl"),
    ("c", "cl"),
    ("highway", "hwy"),
    ("parkway", "pkwy"),
    ("square", "sq"),
    ("parade", "pde"),
    ("circuit", "cct"),
    ("trail", "trl"),
    ("mount", "mt"),
    ("fort", "ft"),
    ("heights", "hts"),
    ("centre", "ctr"),
    ("center", "ctr"),
    ("expressway", "expy"),
    ("freeway", "fwy"),
    ("route", "rte"),
    ("north", "n"),
    ("south", "s"),
    ("east", "e"),
    ("west", "w"),
    ("northeast", "ne"),
    ("northwest", "nw"),
    ("southeast", "se"),
    ("southwest", "sw"),
    // English ordinal words; numeric ordinals lose their suffix in `apply` ("5th" -> "5", as
    // New York's own address data writes "5 AVENUE")
    ("first", "1"),
    ("second", "2"),
    ("third", "3"),
    ("fourth", "4"),
    ("fifth", "5"),
    ("sixth", "6"),
    ("seventh", "7"),
    ("eighth", "8"),
    ("ninth", "9"),
    ("tenth", "10"),
    ("eleventh", "11"),
    ("twelfth", "12"),
    // French
    ("chemin", "ch"),
    ("impasse", "imp"),
    ("allee", "all"),
    ("faubourg", "fbg"),
    ("nord", "n"),
    ("sud", "s"),
    ("est", "e"),
    ("ouest", "o"),
    // Spanish, Colombian
    ("carrera", "kr"),
    ("cra", "kr"),
    ("kra", "kr"),
    ("diagonal", "dg"),
    ("transversal", "tv"),
    ("transv", "tv"),
    ("travessa", "tv"),
    ("trav", "tv"),
    ("carretera", "ctra"),
    ("carr", "ctra"),
    ("camino", "cno"),
    ("cmno", "cno"),
    ("paseo", "ps"),
    ("pso", "ps"),
    ("pasaje", "pje"),
    ("psje", "pje"),
    ("privada", "priv"),
    ("cerrada", "cda"),
    ("nuestra", "ntra"),
    ("senora", "sra"),
    ("general", "gen"),
    ("gral", "gen"),
    ("gal", "gen"),
    // Catalan, Basque
    ("carrer", "cl"),
    ("kalea", "cl"),
    ("avinguda", "av"),
    ("placa", "pl"),
    ("passeig", "ps"),
    // Portuguese
    ("rua", "r"),
    ("estrada", "estr"),
    ("rodovia", "rod"),
    ("alameda", "al"),
    ("aleja", "al"),
    ("aleje", "al"),
    ("largo", "lg"),
    ("lgo", "lg"),
    ("professor", "prof"),
    ("coronel", "cel"),
    ("marechal", "mal"),
    ("presidente", "pres"),
    ("pte", "pres"),
    ("governador", "gov"),
    ("engenheiro", "eng"),
    ("capitao", "cap"),
    ("santa", "sta"),
    ("santo", "sto"),
    ("senhora", "sra"),
    ("sao", "s"),
    // German, Dutch, Italian
    ("strasse", "str"),
    ("straat", "str"),
    ("strada", "str"),
    ("burgemeester", "burg"),
    // Norwegian, Swedish
    ("gate", "gt"),
    ("gata", "gt"),
    ("gaten", "gt"),
    ("viale", "vle"),
    // Turkish
    ("caddesi", "cd"),
    ("cadde", "cd"),
    ("cad", "cd"),
    ("sokak", "sk"),
    ("sokagi", "sk"),
    ("sok", "sk"),
    ("bulvari", "blv"),
    ("bulvar", "blv"),
    ("bul", "blv"),
    ("mahallesi", "mh"),
    ("mahalle", "mh"),
    ("mah", "mh"),
    // Hungarian
    ("utca", "u"),
    // Polish
    ("ulica", "ul"),
    ("osiedle", "os"),
    // Russian, Ukrainian
    ("улица", "ул"),
    ("проспект", "пр"),
    ("просп", "пр"),
    ("переулок", "пер"),
    ("шоссе", "ш"),
    ("площадь", "пл"),
    ("вулиця", "вул"),
    ("провулок", "пров"),
];

/// The canonical form of one folded token, if it isn't already canonical.
pub fn canon(t: &str) -> Option<&'static str> {
    thread_local! {
        static MAP: std::collections::HashMap<&'static str, &'static str> = VARIANTS.iter().copied().collect();
    }
    MAP.with(|m| m.get(t).copied())
}

/// Whether `t` is the canonical short form of some variant ("rd", "st").
pub fn is_canonical(t: &str) -> bool {
    thread_local! {
        static SHORT: std::collections::HashSet<&'static str> = VARIANTS.iter().map(|(_, c)| *c).collect();
    }
    SHORT.with(|s| s.contains(t))
}

/// "21st", "2nd", "3rd", "5th": the length of the digits.
fn ordinal(t: &str) -> Option<usize> {
    let digits = t.bytes().take_while(u8::is_ascii_digit).count();
    (digits > 0 && matches!(&t[digits..], "st" | "nd" | "rd" | "th")).then_some(digits)
}

/// Compounds and their written short forms: "hauptstrasse" -> "hauptstr" ("Hauptstr."),
/// "storgata" -> "storgt" ("Storgt."), "drottninggatan" -> "drottningg" ("Drottningg.").
fn compound(t: &str) -> Option<(usize, &'static str)> {
    [
        ("strasse", "str"),
        ("straat", "str"),
        ("gatan", "g"),
        ("gaten", "gt"),
        ("gata", "gt"),
        ("gate", "gt"),
        ("vagen", "v"),
    ]
    .iter()
    .find_map(|(s, short)| {
        t.strip_suffix(s)
            .filter(|stem| stem.len() > 2)
            .map(|stem| (stem.len(), *short))
    })
}

/// "芝公園四丁目", "浅草５丁目" and "梅田1丁目" all write the chōme number, "濱海路三段" the
/// section number: read it as digits.
fn chome(text: &mut String) {
    let Some(at) = text.find("丁目").or_else(|| text.find('段')) else {
        return;
    };
    let (head, tail) = text.split_at(at);
    let digits = |c: char| "〇一二三四五六七八九十".contains(c);
    let start = head.trim_end_matches(digits).len();
    let kanji = &head[start..];
    let value = |c: char| {
        "〇一二三四五六七八九"
            .chars()
            .position(|d| d == c)
            .map(|d| d as u32)
    };
    let chars: Vec<char> = kanji.chars().collect();
    let n = match chars.as_slice() {
        ['十'] => Some(10),
        [c] => value(*c),
        ['十', u] => value(*u).map(|u| 10 + u),
        [t, '十'] => value(*t).map(|t| t * 10),
        [t, '十', u] => value(*t).zip(value(*u)).map(|(t, u)| t * 10 + u),
        _ => None,
    };
    if let Some(n) = n {
        *text = format!("{}{n}{tail}", &head[..start]);
    }
}

fn apply(text: &mut String) {
    // "İ" lowercases to "i" plus a combining dot that ASCII folding keeps
    if text.chars().any(|c| ('\u{300}'..='\u{36f}').contains(&c)) {
        text.retain(|c| !('\u{300}'..='\u{36f}').contains(&c));
    }
    if let Some(c) = canon(text) {
        text.clear();
        text.push_str(c);
    } else if let Some(n) = ordinal(text) {
        text.truncate(n);
    } else if let Some((stem, short)) = compound(text) {
        text.truncate(stem);
        text.push_str(short);
    }
}

/// Han, kana and Hangul: written without spaces, so whole runs never match a shorter query.
fn cjk(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x11ff | 0x3040..=0x30ff | 0x3130..=0x318f | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff | 0x20000..=0x2ffff)
}

/// A token's CJK runs as overlapping bigrams (a lone character stays), other runs whole.
fn split(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < chars.len() {
        let kind = cjk(chars[i]);
        let j = i + chars[i..].iter().take_while(|&&c| cjk(c) == kind).count();
        let run = &chars[i..j];
        if kind && run.len() > 1 {
            out.extend(run.windows(2).map(|w| w.iter().collect::<String>()));
        } else {
            out.push(run.iter().collect());
        }
        i = j;
    }
    out
}

#[derive(Clone)]
pub struct Canon;

#[cfg(test)]
mod tests {
    #[test]
    fn variants_are_unique_and_never_canonical_forms() {
        let mut seen = std::collections::HashSet::new();
        for (v, c) in super::VARIANTS {
            assert!(seen.insert(*v), "{v} listed twice");
            assert!(super::canon(c).is_none(), "{c} is canonical and a variant");
        }
    }
}

impl TokenFilter for Canon {
    type Tokenizer<T: Tokenizer> = CanonFilter<T>;

    fn transform<T: Tokenizer>(self, tokenizer: T) -> CanonFilter<T> {
        CanonFilter(tokenizer)
    }
}

#[derive(Clone)]
pub struct CanonFilter<T>(T);

impl<T: Tokenizer> Tokenizer for CanonFilter<T> {
    type TokenStream<'a> = CanonStream<T::TokenStream<'a>>;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        CanonStream {
            tail: self.0.token_stream(text),
            pending: vec![],
        }
    }
}

pub struct CanonStream<T> {
    tail: T,
    /// Remaining pieces of a split token, last first.
    pending: Vec<String>,
}

impl<T: TokenStream> TokenStream for CanonStream<T> {
    fn advance(&mut self) -> bool {
        if let Some(next) = self.pending.pop() {
            self.tail.token_mut().text = next;
            return true;
        }
        if !self.tail.advance() {
            return false;
        }
        let token = self.tail.token_mut();
        if token.text.chars().any(cjk) {
            if token.text.contains("丁目") || token.text.contains('段') {
                chome(&mut token.text);
            }
            let mut parts = split(&token.text);
            parts.iter_mut().for_each(apply);
            parts.reverse();
            token.text = parts.pop().unwrap_or_default();
            self.pending = parts;
        } else {
            apply(&mut token.text);
        }
        true
    }

    fn token(&self) -> &Token {
        self.tail.token()
    }

    fn token_mut(&mut self) -> &mut Token {
        self.tail.token_mut()
    }
}
