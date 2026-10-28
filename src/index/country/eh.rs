use crate::proto::opengeocoding::AddressDetails;

/// The data files Western Sahara's places under Morocco, in the provinces Morocco administers it
/// as (Assa-Zag's for Mahbes) and their regions.
pub(crate) fn moroccan(d: &AddressDetails) -> bool {
    const CODES: &[&str] = &[
        "MA-11", "MA-12", "MA-AOU", "MA-ASZ", "MA-BOD", "MA-ESM", "MA-LAA", "MA-OUD",
    ];
    d.country_code.eq_ignore_ascii_case("ma")
        && d.region_codes
            .iter()
            .any(|r| CODES.iter().any(|c| r.code.eq_ignore_ascii_case(c)))
}
