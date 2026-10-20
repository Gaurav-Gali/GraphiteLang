//! Map lookup, insertion, and removal.
//!
//! Absence is not a Graphite value. Checked lookup prints one diagnostic and
//! terminates the process. `get_or_default` and `remove` do not.

use std::collections::HashMap;
use std::hash::Hash;

use crate::convert::Site;
use crate::diagnostic::{self, RuntimeError};

pub fn get<K, V>(
    map: &HashMap<K, V>,
    key: &K,
    site: Site<'_>,
    map_type: &str,
) -> Result<V, RuntimeError>
where
    K: Eq + Hash,
    V: Clone,
{
    match map.get(key) {
        Some(value) => Ok(value.clone()),
        None => Err(diagnostic::missing_key(site, map_type)),
    }
}

pub fn get_mut<'a, K, V>(
    map: &'a mut HashMap<K, V>,
    key: &K,
    site: Site<'_>,
    map_type: &str,
) -> Result<&'a mut V, RuntimeError>
where
    K: Eq + Hash,
{
    match map.get_mut(key) {
        Some(value) => Ok(value),
        None => Err(diagnostic::missing_key(site, map_type)),
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
