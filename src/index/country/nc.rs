pub fn permits_house_postcode(
    source: [u8; 2],
    place: Option<[u8; 2]>,
    postcode: Option<&str>,
) -> bool {
    (source != *b"NC" && place != Some(*b"NC"))
        || postcode
            .map(str::trim)
            .filter(|pc| !pc.is_empty())
            .is_none_or(|pc| {
                pc.len() == 5 && pc.starts_with("988") && pc.bytes().all(|b| b.is_ascii_digit())
            })
}
