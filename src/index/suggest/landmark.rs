use super::*;

impl Index {
    pub(super) fn suggest_landmarks(
        &self,
        typed: &Typed,
        cc: Option<&str>,
        want: usize,
    ) -> Vec<Suggestion> {
        // ponytail: cap name positions at two; extend explicit forms rather than permuting arbitrary qualifiers.
        for name_at in 0..typed.parts.len().min(2) {
            let first = &typed.parts[name_at];
            // two letters start too many names to rank
            if first.chars().count() < 3 {
                continue;
            }
            let open = typed.open && name_at + 1 == typed.parts.len();
            let words = words_open(first, open);
            let forms = if open {
                open_forms(&words)
            } else {
                vec![words.join(" ")]
            };
            let quals: Vec<_> = typed
                .parts
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != name_at)
                .map(|(i, p)| (fold(p), typed.open && i + 1 == typed.parts.len()))
                .filter(|(q, open)| !(*open && q.chars().count() < 2))
                .map(|(q, open)| {
                    let ccs = self.countries_named(&q, open);
                    ((q, ccs), open)
                })
                .collect();
            let mut ids: Vec<u32> = vec![];
            for form in &forms {
                for &id in self.landmarks.find(form, open).iter() {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
            ids.sort_unstable_by_key(|&id| (Reverse(self.landmarks.list[id as usize].score), id));
            let found: Vec<_> = ids
                .into_iter()
                .map(|id| (id, &self.landmarks.list[id as usize]))
                .filter(|(_, l)| {
                    cc.is_none_or(|c| self.places[l.place as usize].cc.eq_ignore_ascii_case(c))
                        && quals.iter().all(|(qual, open)| {
                            let q = &qual.0;
                            let names = self.folded_names(l.place);
                            names
                                .iter()
                                .any(|n| n == q || *open && q.len() >= 3 && n.starts_with(q))
                                || self.qualifies(l.place, std::slice::from_ref(qual), *open)
                        })
                })
                .take(want)
                .map(|(id, _)| self.landmark_suggestion(id))
                .collect();
            if !found.is_empty() {
                return found;
            }
        }
        vec![]
    }

    pub(super) fn landmark_suggestion(&self, id: u32) -> Suggestion {
        let l = &self.landmarks.list[id as usize];
        Suggestion {
            id: format!("l{id}"),
            retrieval_id: format!("{}/l{id}", self.suggestion_scope),
            display_name: format!("{}, {}", self.landmarks.name(l), self.label(l.place)),
            unverified_suffix: String::new(),
            kind: "landmark",
            category: l.category,
            country_code: ps::country(&self.places[l.place as usize].cc).to_string(),
            x: l.x as f64,
            y: l.y as f64,
            bbox: None,
        }
    }
}
