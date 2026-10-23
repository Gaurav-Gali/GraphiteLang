//! Strings as sequences of Unicode scalar values.
//!
//! Indexing and slicing use the same bounds as arrays. A slice is a new
//! string. Methods that change length or order mutate the receiver; search
//! and case methods return a new value.

use crate::array;
use crate::convert::Site;
use crate::diagnostic::{self, RuntimeError};

pub fn read_signed(value: &str, index: i128, site: Site<'_>) -> Result<char, RuntimeError> {
    let chars = scalars(value);
    array::read_signed(&chars, index, site)
}

pub fn read_unsigned(value: &str, index: u128, site: Site<'_>) -> Result<char, RuntimeError> {
    let chars = scalars(value);
    array::read_unsigned(&chars, index, site)
}

pub fn write_signed(
    value: &mut String,
    index: i128,
    ch: char,
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    let mut chars = scalars(value);
    array::write_signed(&mut chars, index, ch, site)?;
    *value = chars.into_iter().collect();
    Ok(())
}

pub fn write_unsigned(
    value: &mut String,
    index: u128,
    ch: char,
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    let mut chars = scalars(value);
    array::write_unsigned(&mut chars, index, ch, site)?;
    *value = chars.into_iter().collect();
    Ok(())
}

pub fn slice(
    value: &str,
    start: Option<i128>,
    stop: Option<i128>,
    step: Option<i128>,
    site: Site<'_>,
) -> Result<String, RuntimeError> {
    let chars = scalars(value);
    let taken = array::slice_copy(&chars, start, stop, step, site)?;
    Ok(taken.into_iter().collect())
}

pub fn assign_slice(
    value: &mut String,
    start: Option<i128>,
    stop: Option<i128>,
    step: Option<i128>,
    replacement: &str,
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    let mut chars = scalars(value);
    let repl = scalars(replacement);
    array::assign_slice(&mut chars, start, stop, step, &repl, None, site)?;
    *value = chars.into_iter().collect();
    Ok(())
}

pub fn push_char(value: &mut String, ch: char) {
    value.push(ch);
}

pub fn extend(value: &mut String, extra: &str) {
    value.push_str(extra);
}

pub fn insert_signed(
    value: &mut String,
    index: i128,
    ch: char,
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    let mut chars = scalars(value);
    array::insert_signed(&mut chars, index, ch, None, site)?;
    *value = chars.into_iter().collect();
    Ok(())
}

pub fn insert_unsigned(
    value: &mut String,
    index: u128,
    ch: char,
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    let mut chars = scalars(value);
    array::insert_unsigned(&mut chars, index, ch, None, site)?;
    *value = chars.into_iter().collect();
    Ok(())
}

pub fn remove_signed(value: &mut String, index: i128, site: Site<'_>) -> Result<(), RuntimeError> {
    let mut chars = scalars(value);
    array::remove_signed(&mut chars, index, site)?;
    *value = chars.into_iter().collect();
    Ok(())
}

pub fn remove_unsigned(
    value: &mut String,
    index: u128,
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    let mut chars = scalars(value);
    array::remove_unsigned(&mut chars, index, site)?;
    *value = chars.into_iter().collect();
    Ok(())
}

pub fn pop_last(value: &mut String, site: Site<'_>) -> Result<char, RuntimeError> {
    let mut chars = scalars(value);
    let ch = array::pop_last(&mut chars, site)?;
    *value = chars.into_iter().collect();
    Ok(ch)
}

pub fn pop_signed(value: &mut String, index: i128, site: Site<'_>) -> Result<char, RuntimeError> {
    let mut chars = scalars(value);
    let ch = array::pop_signed(&mut chars, index, site)?;
    *value = chars.into_iter().collect();
    Ok(ch)
}

pub fn pop_unsigned(value: &mut String, index: u128, site: Site<'_>) -> Result<char, RuntimeError> {
    let mut chars = scalars(value);
    let ch = array::pop_unsigned(&mut chars, index, site)?;
    *value = chars.into_iter().collect();
    Ok(ch)
}

pub fn clear(value: &mut String) {
    value.clear();
}

pub fn reverse(value: &mut String) {
    let mut chars = scalars(value);
    chars.reverse();
    *value = chars.into_iter().collect();
}

pub fn split(
    value: &str,
    sep: Option<&str>,
    maxsplit: Option<i128>,
    site: Site<'_>,
) -> Result<Vec<String>, RuntimeError> {
    split_at(value, sep, maxsplit, false, site)
}

pub fn rsplit(
    value: &str,
    sep: Option<&str>,
    maxsplit: Option<i128>,
    site: Site<'_>,
) -> Result<Vec<String>, RuntimeError> {
    split_at(value, sep, maxsplit, true, site)
}

pub fn strip(value: &str, chars: Option<&str>) -> String {
    trim(value, chars, true, true)
}

pub fn lstrip(value: &str, chars: Option<&str>) -> String {
    trim(value, chars, true, false)
}

pub fn rstrip(value: &str, chars: Option<&str>) -> String {
    trim(value, chars, false, true)
}

pub fn lower(value: &str) -> String {
    value.to_lowercase()
}

pub fn upper(value: &str) -> String {
    value.to_uppercase()
}

pub fn replace(value: &str, old: &str, new: &str) -> String {
    if old.is_empty() {
        let mut out = String::new();
        out.push_str(new);
        for ch in value.chars() {
            out.push(ch);
            out.push_str(new);
        }
        return out;
    }
    value.replace(old, new)
}

pub fn find(haystack: &str, needle: &str) -> i64 {
    find_from(haystack, needle)
        .map(|index| index as i64)
        .unwrap_or(-1)
}

pub fn index_of(haystack: &str, needle: &str, site: Site<'_>) -> Result<i64, RuntimeError> {
    find_from(haystack, needle)
        .map(|index| index as i64)
        .ok_or_else(|| diagnostic::value_not_found(site))
}

pub fn count(haystack: &str, needle: &str) -> i64 {
    if needle.is_empty() {
        return scalars(haystack).len() as i64 + 1;
    }
    let haystack = scalars(haystack);
    let needle = scalars(needle);
    let mut index = 0;
    let mut total = 0i64;
    while index + needle.len() <= haystack.len() {
        if haystack[index..index + needle.len()] == needle[..] {
            total += 1;
            index += needle.len();
        } else {
            index += 1;
        }
    }
    total
}

pub fn starts_with(value: &str, prefix: &str) -> bool {
    value.starts_with(prefix)
}

pub fn ends_with(value: &str, suffix: &str) -> bool {
    value.ends_with(suffix)
}

pub fn join(sep: &str, parts: &[String]) -> String {
    parts.join(sep)
}

fn scalars(value: &str) -> Vec<char> {
    value.chars().collect()
}

fn find_from(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    let haystack = scalars(haystack);
    let needle = scalars(needle);
    haystack
        .windows(needle.len())
        .position(|window| window == needle.as_slice())
}

fn trim(value: &str, chars: Option<&str>, left: bool, right: bool) -> String {
    let set: Vec<char> = chars.map(scalars).unwrap_or_default();
    let drop = |ch: char| {
        if chars.is_none() {
            ch.is_whitespace()
        } else {
            set.contains(&ch)
        }
    };
    let mut items = scalars(value);
    if left {
        while items.first().is_some_and(|ch| drop(*ch)) {
            items.remove(0);
        }
    }
    if right {
        while items.last().is_some_and(|ch| drop(*ch)) {
            items.pop();
        }
    }
    items.into_iter().collect()
}

fn split_at(
    value: &str,
    sep: Option<&str>,
    maxsplit: Option<i128>,
    from_end: bool,
    _site: Site<'_>,
) -> Result<Vec<String>, RuntimeError> {
    let limit = match maxsplit {
        Some(value) if value < 0 => None,
        Some(value) => Some(value as u64),
        None => None,
    };
    match sep {
        None => Ok(split_whitespace(value, limit)),
        Some("") => Ok(split_chars(value, limit, from_end)),
        Some(sep) => Ok(split_sep(value, sep, limit, from_end)),
    }
}

fn split_chars(value: &str, maxsplit: Option<u64>, from_end: bool) -> Vec<String> {
    let chars: Vec<char> = value.chars().collect();
    if chars.is_empty() || maxsplit == Some(0) {
        return if chars.is_empty() {
            Vec::new()
        } else {
            vec![value.to_string()]
        };
    }
    let Some(limit) = maxsplit else {
        return chars.into_iter().map(|ch| ch.to_string()).collect();
    };
    if limit as usize >= chars.len() {
        return chars.into_iter().map(|ch| ch.to_string()).collect();
    }
    let limit = limit as usize;
    if from_end {
        let keep = chars.len() - limit;
        let mut parts = vec![chars[..keep].iter().collect::<String>()];
        parts.extend(chars[keep..].iter().map(|ch| ch.to_string()));
        parts
    } else {
        let mut parts: Vec<String> = chars[..limit].iter().map(|ch| ch.to_string()).collect();
        parts.push(chars[limit..].iter().collect());
        parts
    }
}

fn split_sep(value: &str, sep: &str, maxsplit: Option<u64>, from_end: bool) -> Vec<String> {
    if maxsplit == Some(0) {
        return vec![value.to_string()];
    }
    if from_end {
        let Some(maxsplit) = maxsplit else {
            let mut parts: Vec<String> = value.rsplit(sep).map(str::to_string).collect();
            parts.reverse();
            return parts;
        };
        let mut parts: Vec<String> = value
            .rsplitn(maxsplit as usize + 1, sep)
            .map(str::to_string)
            .collect();
        parts.reverse();
        return parts;
    }
    let Some(maxsplit) = maxsplit else {
        return value.split(sep).map(str::to_string).collect();
    };
    value
        .splitn(maxsplit as usize + 1, sep)
        .map(str::to_string)
        .collect()
}

fn split_whitespace(value: &str, maxsplit: Option<u64>) -> Vec<String> {
    if value.trim().is_empty() {
        return Vec::new();
    }
    if maxsplit == Some(0) {
        return vec![value.trim_start().to_string()];
    }
    let Some(maxsplit) = maxsplit else {
        return value.split_whitespace().map(str::to_string).collect();
    };
    let mut parts = Vec::new();
    let mut rest = value.trim_start();
    for _ in 0..maxsplit {
        if rest.is_empty() {
            break;
        }
        let end = rest
            .char_indices()
            .find(|(_, ch)| ch.is_whitespace())
            .map(|(index, _)| index)
            .unwrap_or(rest.len());
        parts.push(rest[..end].to_string());
        rest = rest[end..].trim_start();
    }
    if !rest.is_empty() {
        parts.push(rest.to_string());
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::Site;

    fn site() -> Site<'static> {
        Site {
            file: "text.grf",
            line: 1,
            column: 1,
            end_column: 2,
            excerpt: "name",
        }
    }

    #[test]
    fn slices_follow_the_array_rules() {
        let here = site();
        assert_eq!(slice("abcd", None, None, Some(-1), here).unwrap(), "dcba");
        assert_eq!(slice("abcd", Some(1), Some(3), None, here).unwrap(), "bc");
        assert_eq!(slice("abcd", None, Some(2), None, here).unwrap(), "ab");
        assert_eq!(slice("aéb", Some(2), None, None, here).unwrap(), "b");
        assert!(slice("abcd", None, None, Some(0), here).is_err());
    }

    #[test]
    fn assignment_replaces_scalar_ranges() {
        let here = site();
        let mut name = String::from("abcd");
        assign_slice(&mut name, Some(1), Some(3), None, "XY", here).unwrap();
        assert_eq!(name, "aXYd");
        write_signed(&mut name, -1, 'z', here).unwrap();
        assert_eq!(name, "aXYz");
    }

    #[test]
    fn split_and_search_match_python() {
        let here = site();
        assert_eq!(
            split("a b  c", None, None, here).unwrap(),
            vec!["a", "b", "c"]
        );
        assert_eq!(
            split("a,b,c", Some(","), Some(1), here).unwrap(),
            vec!["a", "b,c"]
        );
        assert_eq!(
            rsplit("a,b,c", Some(","), Some(1), here).unwrap(),
            vec!["a,b", "c"]
        );
        assert_eq!(
            split("a,,b", Some(","), None, here).unwrap(),
            vec!["a", "", "b"]
        );
        assert_eq!(
            split("1234abc", Some(""), None, here).unwrap(),
            vec!["1", "2", "3", "4", "a", "b", "c"]
        );
        assert_eq!(
            split("aéb", Some(""), Some(1), here).unwrap(),
            vec!["a", "éb"]
        );
        assert_eq!(
            rsplit("abc", Some(""), Some(1), here).unwrap(),
            vec!["ab", "c"]
        );
        assert!(split("", Some(""), None, here).unwrap().is_empty());
        assert_eq!(strip("xxabxx", Some("x")), "ab");
        assert_eq!(find("aéb", "b"), 2);
        assert_eq!(find("abc", "z"), -1);
        assert_eq!(count("aaa", "aa"), 1);
        assert_eq!(replace("aaa", "a", "b"), "bbb");
        assert!(starts_with("abcd", "ab"));
        assert!(ends_with("abcd", "cd"));
        assert_eq!(join(",", &["a".into(), "b".into()]), "a,b");
    }
}
