use crate::index::*;

impl Index {
    /// A US state's region place, by its postal code ("OH") or its name.
    pub(crate) fn us_state(&self, state: &str) -> Option<u32> {
        let state = state.trim();
        if state.len() == 2 {
            return self
                .regions
                .get(&format!("US-{}", state.to_uppercase()))
                .copied();
        }
        let want = name_key(state);
        self.places(state, Some("us"), None, &|p| {
            &*p.subtype == "region" && p.named(want)
        })
        .into_iter()
        .find_map(|h| match h {
            Hit::Place(id) => Some(id),
            _ => None,
        })
    }
}
