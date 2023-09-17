use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub mod address;
pub mod city;
pub mod street;
pub mod street_v2;

mod helper;

pub fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}
