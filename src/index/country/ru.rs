//! How Russian addresses are written: a house mark before the number ("д. 10"), "наб." for
//! "набережная", and a building or letter after it ("стр. 1", "лит.А") that the index keys as
//! "10 с1" and "3 лита".

fn cyrillic(text: &str) -> bool {
    text.chars().any(|c| ('\u{400}'..='\u{4ff}').contains(&c))
}

/// `text` as the index writes a Russian address, with the house number joined to its road: none
/// when nothing is to change.
pub fn written(text: &str) -> Option<String> {
    if !cyrillic(text) {
        return None;
    }
    let mut words: Vec<String> = vec![];
    let mut tokens = text.replace('–', "-").replace(',', " , ");
    tokens = tokens.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut it = tokens.split(' ').peekable();
    while let Some(word) = it.next() {
        let lower = word.to_lowercase();
        let digit_next = it
            .peek()
            .is_some_and(|n| n.starts_with(|c: char| c.is_ascii_digit()));
        let mark = |prefix: &str| {
            lower
                .strip_prefix(prefix)
                .filter(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
                .map(str::to_owned)
        };
        match lower.as_str() {
            "д." | "д" | "дом" if digit_next => {}
            "стр." | "стр" if digit_next => {
                words.push(format!("с{}", it.next().unwrap_or_default()));
            }
            "корп." | "корп" | "к." if digit_next => {
                words.push(format!("к{}", it.next().unwrap_or_default()));
            }
            "лит." | "лит" if it.peek().is_some_and(|l| l.chars().count() == 1) => {
                words.push(format!(
                    "лит{}",
                    it.next().unwrap_or_default().to_lowercase()
                ));
            }
            "наб." => words.push("набережная".into()),
            "пр-т" | "пр-кт" | "пр-т." => words.push("проспект".into()),
            _ => {
                if let Some(n) = mark("д.").or_else(|| mark("д")) {
                    words.push(n);
                } else if let Some(n) = lower
                    .strip_prefix("лит.")
                    .filter(|l| l.chars().count() == 1)
                {
                    words.push(format!("лит{n}"));
                } else if let Some(n) = mark("стр.") {
                    words.push(format!("с{n}"));
                } else {
                    words.push(word.to_owned());
                }
            }
        }
    }
    let mut joined = words.join(" ").replace(" , ", ", ");
    // "Варварка, 10": the number goes with its road
    // not "Ленина 49, 303140, Russia": its number is in the road and the next is a postcode
    if let Some((road, rest)) = joined.split_once(", ")
        && !road.contains(|c: char| c.is_ascii_digit())
        && rest.starts_with(|c: char| c.is_ascii_digit())
        && rest.split([',', ' ']).next().is_some_and(|n| n.len() <= 4)
    {
        let (number, tail) = match rest.split_once(", ") {
            Some((n, t)) => (n.to_owned(), Some(t.to_owned())),
            None => (rest.to_owned(), None),
        };
        joined = match tail {
            Some(t) => format!("{road} {number}, {t}"),
            None => format!("{road} {number}"),
        };
    }
    (joined != text).then_some(joined)
}

/// "3 лита", "10 с2", "7-9 к5": a number and the mark and letter or number that follow it.
pub fn house_with_mark(number: &str) -> bool {
    let Some((n, mark)) = number.split_once(' ') else {
        return false;
    };
    n.starts_with(|c: char| c.is_ascii_digit())
        && n.bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'/' | b'-'))
        && ["лит", "с", "к"].iter().any(|m| {
            mark.strip_prefix(m).is_some_and(|rest| {
                !rest.is_empty()
                    && (m == &"лит" && rest.chars().count() == 1
                        || m != &"лит" && rest.bytes().all(|b| b.is_ascii_digit()))
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn house_marks_and_abbreviations_are_written_as_the_index_has_them() {
        for (typed, written_as) in [
            ("ул. Варварка, д. 10", "ул. Варварка 10"),
            (
                "Университетская наб., д. 7–9",
                "Университетская набережная 7-9",
            ),
            (
                "Петропавловская крепость, д.3 лит.А",
                "Петропавловская крепость 3 лита",
            ),
            (
                "Большой Левшинский переулок, 8/1 стр. 1",
                "Большой Левшинский переулок 8/1 с1",
            ),
            ("проспект Гагарина, 23", "проспект Гагарина 23"),
        ] {
            assert_eq!(written(typed).as_deref(), Some(written_as), "{typed}");
        }
        assert_eq!(written("Красная площадь"), None);
        assert_eq!(written("улица Ленина 49, 303140, Russia"), None);
        assert_eq!(written("Львівська вулиця 18, 80316, Ukraine"), None);
        assert_eq!(written("Красная площадь, 303140, Russia"), None);
        assert!(
            house_with_mark("3 лита") && house_with_mark("8/1 с1") && house_with_mark("7-9 к5")
        );
        assert!(
            !house_with_mark("3 литры") && !house_with_mark("3 Street") && !house_with_mark("3")
        );
    }
}
