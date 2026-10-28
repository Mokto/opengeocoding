use super::*;
use crate::{Case, named};

/// A crown dependency with no people in the data, and a town of its name in Wyoming (`us.rs`).
pub const PLACES: &[Place] = &[place!(
    "gg",
    "dependency",
    "",
    "GG",
    "",
    0,
    (-2.58, 49.45),
    "",
    "Guernsey",
    &[("en", "Guernsey")],
)];

pub fn cases() -> Vec<Case> {
    vec![named("Guernsey", "gg", "", &[], true)]
}
