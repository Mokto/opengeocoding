/// The index has no PS country: Overture's West Bank and Gaza Strip are countries of their own
/// (xw, xg), whose places answers name as Palestine.
pub(crate) fn territory(cc: &str) -> bool {
    cc.eq_ignore_ascii_case("xw") || cc.eq_ignore_ascii_case("xg")
}

/// The country code an answer gives a place of `cc`.
pub(crate) fn country(cc: &str) -> &str {
    if territory(cc) { "ps" } else { cc }
}

/// The country place a query naming Palestine itself answers with: the larger territory.
pub(crate) fn country_place(cc: &str) -> &str {
    if cc.eq_ignore_ascii_case("ps") {
        "xw"
    } else {
        cc
    }
}

/// `query` with Palestine written as libpostal knows it: "Palestinian Territory", GeoNames' name,
/// runs the town into a house name ("10 شارع القدس, دورا, Palestinian Territory").
pub fn as_palestine(query: &str) -> Option<String> {
    let (head, last) = query.rsplit_once(',')?;
    matches!(
        last.trim().to_lowercase().as_str(),
        "palestinian territory" | "palestinian territories" | "occupied palestinian territory"
    )
    .then(|| format!("{head}, Palestine"))
}
