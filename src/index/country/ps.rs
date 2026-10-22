/// The index has no PS country: Overture's West Bank and Gaza Strip are countries of their own
/// (xw, xg), whose places answers name as Palestine.
pub(crate) fn territory(cc: &str) -> bool {
    cc.eq_ignore_ascii_case("xw") || cc.eq_ignore_ascii_case("xg")
}

/// The country code an answer gives a place of `cc`.
pub(crate) fn country(cc: &str) -> &str {
    if territory(cc) { "ps" } else { cc }
}
