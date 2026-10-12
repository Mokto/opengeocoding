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
    // French
    ("chemin", "ch"),
    ("impasse", "imp"),
    ("allee", "all"),
    ("faubourg", "fbg"),
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
    ("general", "gen"),
    ("gral", "gen"),
    ("gal", "gen"),
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

/// German and Dutch compounds: "hauptstrasse" -> "hauptstr", like "Hauptstr.".
fn compound(t: &str) -> Option<usize> {
    ["strasse", "straat"].iter().find_map(|s| {
        t.strip_suffix(s)
            .filter(|stem| !stem.is_empty())
            .map(|stem| stem.len())
    })
}

fn apply(text: &mut String) {
    if let Some(c) = canon(text) {
        text.clear();
        text.push_str(c);
    } else if let Some(stem) = compound(text) {
        text.truncate(stem);
        text.push_str("str");
    }
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
        CanonStream(self.0.token_stream(text))
    }
}

pub struct CanonStream<T>(T);

impl<T: TokenStream> TokenStream for CanonStream<T> {
    fn advance(&mut self) -> bool {
        if !self.0.advance() {
            return false;
        }
        apply(&mut self.0.token_mut().text);
        true
    }

    fn token(&self) -> &Token {
        self.0.token()
    }

    fn token_mut(&mut self) -> &mut Token {
        self.0.token_mut()
    }
}
