use super::*;

impl Index {
    pub(super) fn suggest_landmarks(
        &self,
        typed: &Typed,
        cc: Option<&str>,
        want: usize,
    ) -> Vec<Suggestion> {
        let first = &typed.parts[0];
        // two letters start too many names to rank
        if first.chars().count() < 3 {
            return vec![];
        }
        let one_part = typed.parts.len() == 1;
        let open = typed.open && one_part;
        let words = words_open(first, open);
        let forms = if open {
            open_forms(&words)
        } else {
            vec![words.join(" ")]
        };
        let quals: Vec<(String, Vec<&str>)> = typed.parts[1..]
            .iter()
            .map(|p| fold(p))
            .filter(|q| !(typed.open && q.chars().count() < 2))
            .map(|q| {
                let ccs = self.countries_named(&q, typed.open);
                (q, ccs)
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
        ids.into_iter()
            .map(|id| (id, &self.landmarks.list[id as usize]))
            .filter(|(_, l)| {
                cc.is_none_or(|c| self.places[l.place as usize].cc.eq_ignore_ascii_case(c))
                    && quals.iter().enumerate().all(|(i, (q, _))| {
                        let open = typed.open && i + 1 == quals.len();
                        let names = self.folded_names(l.place);
                        names
                            .iter()
                            .any(|n| n == q || open && q.len() >= 3 && n.starts_with(q))
                            || self.qualifies(l.place, &quals[i..=i], open)
                    })
            })
            .take(want)
            .map(|(id, _)| self.landmark_suggestion(id))
            .collect()
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
            country_code: self.places[l.place as usize].cc.to_string(),
            x: l.x as f64,
            y: l.y as f64,
            bbox: None,
        }
    }
}
