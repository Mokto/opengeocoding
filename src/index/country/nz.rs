use crate::index::*;

pub(crate) fn nz_localities_aligned(
    head: &[(String, String)],
    suburb: Option<&str>,
    city: &str,
    whole: &[(String, String)],
) -> Option<Vec<(String, String)>> {
    let suburb = suburb.map(|s| s.trim().to_lowercase());
    let city = city.trim().to_lowercase();
    if !head_aligns(head)
        || !one_house(head)
        || field(head, "house_number") != field(whole, "house_number")
        || field(head, "unit") != field(whole, "unit")
        || !whole.iter().all(|(k, _)| {
            matches!(
                k.as_str(),
                "house_number" | "road" | "unit" | "city" | "suburb" | "city_district" | "house"
            )
        })
        || suburb
            .iter()
            .chain([&city])
            .any(|s| !s.chars().any(char::is_alphabetic) || s.chars().any(|c| c.is_ascii_digit()))
    {
        return None;
    }
    let mut parsed = head.to_vec();
    parsed.extend(suburb.map(|s| ("suburb".into(), s)));
    parsed.push(("city".into(), city));
    Some(parsed)
}

impl Index {
    /// "number road, [suburb,] town, New Zealand": the house on that road the suburb and town
    /// place, where libpostal reads the road or a town across the commas.
    pub(crate) fn nz_comma_house(
        &self,
        q: &str,
        whole: &[(String, String)],
    ) -> Result<Option<(Parsed, Hit)>> {
        let parts: Vec<_> = q.split(',').map(str::trim).collect();
        let (head, suburb, city, country) = match parts.as_slice() {
            [head, suburb, city, country] => (*head, Some(*suburb), *city, *country),
            [head, city, country] => (*head, None, *city, *country),
            _ => return Ok(None),
        };
        if self.country(country).as_deref() != Some("nz")
            || whole
                .iter()
                .filter(|(k, _)| k == "country")
                .any(|(_, country)| self.country(country).as_deref() != Some("nz"))
        {
            return Ok(None);
        }
        let inner: Vec<_> = whole
            .iter()
            .filter(|(k, _)| k != "country")
            .cloned()
            .collect();
        let Some(mut parsed) =
            nz_localities_aligned(&self.parser.parse(head)?, suburb, city, &inner)
        else {
            return Ok(None);
        };
        let road = field(&parsed, "road");
        let number = field(&parsed, "house_number");
        // a town alone only repairs a road or town read across the comma
        if suburb.is_none()
            && field(&inner, "road") == road
            && field(&inner, "city") == field(&parsed, "city")
        {
            return Ok(None);
        }
        let postal = |hit: &Hit, name: &str| {
            matches!(hit, Hit::Street { postal_town: Some(town), .. }
                if crate::matching::names_agree(name, town))
        };
        let in_town = |hit: &Hit| {
            self.house_locality_named(hit, city)
                || self.house_town_named(hit, city)
                || (suburb.is_none() && postal(hit, city))
        };
        let mut hit = self
            .streets(
                road,
                Some(number),
                Some(suburb.unwrap_or(city)),
                None,
                Some("nz"),
                true,
            )
            .into_iter()
            .find(|hit| {
                matches!(hit, Hit::Street { name, number: Some(n), .. }
                if fold(name) == fold(road)
                    && schema::number_key(n) == schema::number_key(number)
                    && in_town(hit)
                    && suburb.is_none_or(|suburb| {
                        self.house_locality_named(hit, suburb) || postal(hit, suburb)
                    }))
            });
        // the town only the house's source names is its postal city, as streets_by names it
        let postal_city = hit.as_ref().is_some_and(|hit| {
            !self.house_locality_named(hit, city) && self.house_town_named(hit, city)
        });
        if let Some(Hit::Street { city: found, .. }) = &mut hit {
            *found = postal_city.then(|| title(city));
        }
        parsed.push(("country".into(), country.to_lowercase()));
        Ok(hit.map(|hit| (parsed.into(), hit)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nz_comma_alignment_keeps_numbers_units_and_geographic_guards() {
        let pairs = |p: &[(&str, &str)]| -> Vec<(String, String)> {
            p.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        for number in ["5a", "10-12", "2/19", "19b"] {
            let head = pairs(&[("house_number", number), ("road", "orchard road")]);
            let whole = pairs(&[
                ("house_number", number),
                ("road", "orchard road kōwhai bay"),
                ("city", "harbourton"),
            ]);
            let aligned =
                nz_localities_aligned(&head, Some("Kōwhai Bay"), "Harbourton", &whole).unwrap();
            assert_eq!(field(&aligned, "house_number"), number);
            assert_eq!(field(&aligned, "road"), "orchard road");
            assert_eq!(field(&aligned, "suburb"), "kōwhai bay");
            assert_eq!(field(&aligned, "city"), "harbourton");
            assert_eq!(
                nz_localities_aligned(&head, Some("Kōwhai Bay"), "Harbourton", &aligned).unwrap(),
                aligned
            );
            for key in [
                "state",
                "state_district",
                "country",
                "postcode",
                "po_box",
                "level",
            ] {
                let mut guarded = whole.clone();
                guarded.push((key.into(), "other".into()));
                assert!(
                    nz_localities_aligned(&head, Some("Kōwhai Bay"), "Harbourton", &guarded)
                        .is_none()
                );
            }
            for (suburb, city) in [("", "Harbourton"), ("Kōwhai Bay", "Harbourton 1234")] {
                assert!(nz_localities_aligned(&head, Some(suburb), city, &whole).is_none());
            }
            let changed = pairs(&[("house_number", "999"), ("road", "orchard road")]);
            assert!(
                nz_localities_aligned(&changed, Some("Kōwhai Bay"), "Harbourton", &whole).is_none()
            );
        }
        let head = pairs(&[
            ("unit", "2"),
            ("house_number", "19"),
            ("road", "orchard road"),
        ]);
        let mut whole = head.clone();
        whole.push(("city".into(), "kōwhai bay harbourton".into()));
        let aligned =
            nz_localities_aligned(&head, Some("Kōwhai Bay"), "Harbourton", &whole).unwrap();
        assert_eq!(field(&aligned, "unit"), "2");
        whole[0].1 = "3".into();
        assert!(nz_localities_aligned(&head, Some("Kōwhai Bay"), "Harbourton", &whole).is_none());
    }
}
