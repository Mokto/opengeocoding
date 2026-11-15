use crate::{
    index::{Hit, Index, fold, name_key},
    matching::country_named,
    proto::opengeocoding::FullAddress,
};

pub(crate) fn habitation(index: &Index, query: &str, found: &FullAddress) -> Option<Hit> {
    let parts: Vec<_> = query.split(',').map(str::trim).collect();
    let (name, state, country) = match parts.as_slice() {
        [name, country] => (*name, None, *country),
        [name, state, country] => (*name, Some(*state), *country),
        _ => return None,
    };
    if country_named(country) != "in" {
        return None;
    }
    if !found.display_name.is_empty()
        && found.address_details.as_option().is_none_or(|d| {
            d.country_code != "in"
                || [
                    &d.house_number,
                    &d.road,
                    &d.city,
                    &d.town,
                    &d.village,
                    &d.hamlet,
                    &d.suburb,
                    &d.neighbourhood,
                    &d.city_district,
                    &d.quarter,
                    &d.municipality,
                    &d.county,
                ]
                .iter()
                .any(|value| !value.is_empty())
        })
    {
        return None;
    }
    let hit = index.locality_named(name, "in", true)?;
    let Hit::Place(id) = hit else {
        return None;
    };
    let place = &index.places[id as usize];
    if place.class.as_ref() != "pmgsy_habitation" || fold(name) != fold(&place.primary) {
        return None;
    }
    if let Some(state) = state
        && !index.chain(id).iter().skip(1).any(|&parent| {
            let parent = &index.places[parent as usize];
            parent.subtype.as_ref() == "region"
                && (parent.region.eq_ignore_ascii_case(state)
                    || parent.named(name_key(&fold(state))))
        })
    {
        return None;
    }
    Some(hit)
}
