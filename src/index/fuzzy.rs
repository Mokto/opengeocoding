use super::*;

/// A street word long enough to correct, and a word rather than a number or code ("3526").
pub(super) fn fuzzable(t: &str) -> bool {
    t.chars().count() >= 4 && t.chars().all(char::is_alphabetic)
}

/// Matches terms within one insertion, deletion, substitution or adjacent transposition of a word
/// (optimal string alignment over bytes, so a non-ASCII letter's substitution counts twice) that
/// start with its first byte: typed addresses rarely get the first letter wrong, and requiring it
/// lets the walk skip all but one branch of the term dictionary.
pub(super) struct OneEdit<'a>(pub(super) &'a [u8]);

/// Longest word `OneEdit` takes: the analyzer drops longer tokens.
pub(super) const MAX_WORD: usize = 40;

/// (previous DP row, DP row, last byte); distances saturate at 2. The previous row of the start
/// state is all 2s, so it never allows a transposition.
type EditState = ([u8; MAX_WORD + 1], [u8; MAX_WORD + 1], u8);

impl tantivy_fst::Automaton for OneEdit<'_> {
    type State = EditState;

    fn start(&self) -> EditState {
        let mut row = [2; MAX_WORD + 1];
        for (j, d) in row.iter_mut().enumerate().take(self.0.len() + 1) {
            *d = j.min(2) as u8;
        }
        ([2; MAX_WORD + 1], row, 0)
    }

    fn is_match(&self, (_, row, _): &EditState) -> bool {
        row[self.0.len()] <= 1
    }

    fn can_match(&self, (_, row, _): &EditState) -> bool {
        row[..=self.0.len()].iter().any(|&d| d <= 1)
    }

    fn accept(&self, (prev, row, last): &EditState, c: u8) -> EditState {
        let w = self.0;
        // terms hold no zero bytes, so `last` is 0 only at the start
        if *last == 0 && w.first() != Some(&c) {
            return ([2; MAX_WORD + 1], [2; MAX_WORD + 1], c);
        }
        let mut next = [2; MAX_WORD + 1];
        next[0] = (row[0] + 1).min(2);
        for j in 1..=w.len() {
            let mut d = (row[j] + 1)
                .min(next[j - 1] + 1)
                .min(row[j - 1] + u8::from(w[j - 1] != c));
            if j > 1 && c == w[j - 2] && *last == w[j - 1] {
                d = d.min(prev[j - 2] + 1);
            }
            next[j] = d.min(2);
        }
        (*row, next, c)
    }
}

/// A typed street word that the index would correct to `indexed`: a name word within one edit
/// and with the same first letter ("whte" for "white"), or a misspelt street type or direction
/// ("srteet" for "st").
pub fn misspelt_word(typed: &str, indexed: &str) -> bool {
    (fuzzable(typed) && typed.chars().next() == indexed.chars().next() && one_edit(typed, indexed))
        || misspelt_kind(typed).iter().any(|k| k == indexed)
}

/// The canonical forms of every street type or direction within an edit of `word`:
/// "southewst" is as near "southeast" as "southwest", and only the street tells which.
pub(super) fn misspelt_kind(word: &str) -> Vec<String> {
    if canon::is_canonical(word) {
        return vec![];
    }
    let mut kinds: Vec<String> = canon::VARIANTS
        .iter()
        .filter(|(full, _)| one_edit(word, full))
        .map(|(_, short)| short.to_string())
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    kinds
}

/// Within one insertion, deletion, substitution or transposition of adjacent letters.
pub(super) fn one_edit(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let (a, b) = (&a[head..], &b[head..]);
    match (a.len(), b.len()) {
        (0, 0) => true,
        (m, n) if m == n => {
            a[1..] == b[1..] || (m >= 2 && a[0] == b[1] && a[1] == b[0] && a[2..] == b[2..])
        }
        (m, n) if m == n + 1 => a[1..] == *b,
        (m, n) if m + 1 == n => *a == b[1..],
        _ => false,
    }
}
