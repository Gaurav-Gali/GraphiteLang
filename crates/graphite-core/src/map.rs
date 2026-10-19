//! Map lookup, insertion, and removal.
//!
//! Absence is not a Graphite value. Checked lookup prints one diagnostic and
//! terminates the process. `get_or_default` and `remove` do not.

use std::collections::HashMap;
use std::hash::Hash;

use crate::convert::Site;

pub fn missing(site: Site<'_>, map_type: &str) -> ! {
    eprintln!(
        "Runtime map error at {}:{}:{}:\nmissing key: lookup on '{map_type}'",
        site.file, site.line, site.column
    );
    std::process::exit(1);
}

pub fn get<K, V>(map: &HashMap<K, V>, key: &K, site: Site<'_>, map_type: &str) -> V
where
    K: Eq + Hash,
    V: Clone,
{
    match map.get(key) {
        Some(value) => value.clone(),
        None => missing(site, map_type),
    }
}

pub fn get_mut<'a, K, V>(
    map: &'a mut HashMap<K, V>,
    key: &K,
    site: Site<'_>,
    map_type: &str,
) -> &'a mut V
where
    K: Eq + Hash,
{
    match map.get_mut(key) {
        Some(value) => value,
        None => missing(site, map_type),
    }
}

pub fn assign<K, V>(map: &mut HashMap<K, V>, key: K, value: V) -> V
where
    K: Eq + Hash,
    V: Clone,
{
    map.insert(key, value.clone());
    value
}

pub fn insert<K, V>(map: &mut HashMap<K, V>, key: K, value: V)
where
    K: Eq + Hash,
{
    map.insert(key, value);
}

pub fn remove<K, V>(map: &mut HashMap<K, V>, key: &K) -> bool
where
    K: Eq + Hash,
{
    map.remove(key).is_some()
}

pub fn contains<K, V>(map: &HashMap<K, V>, key: &K) -> bool
where
    K: Eq + Hash,
{
    map.contains_key(key)
}

pub fn get_or_default<K, V>(map: &HashMap<K, V>, key: &K, default: V) -> V
where
    K: Eq + Hash,
    V: Clone,
{
    match map.get(key) {
        Some(value) => value.clone(),
        None => default,
    }
}
