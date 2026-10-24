//! Array indexing and Python-style slicing.
//!
//! Slice bounds follow `PySlice_GetIndicesEx`: omitted components stay
//! distinct from explicit values, negative bounds count from the end, and
//! out-of-range bounds are clamped. A negative step uses an exclusive stop
//! of `-1` when the stop is omitted. Indexing does not clamp.
//!
//! Unit-step assignment (`step` absent or `1`) replaces the clamped range.
//! When the adjusted stop is below the start, the range is empty and the
//! replacement is inserted at the start. Any other step replaces exactly
//! the selected positions, in traversal order, or leaves the array unchanged.

use crate::convert::Site;
use crate::diagnostic::{self, RuntimeError};

pub fn read_signed<T: Clone>(items: &[T], index: i128, site: Site<'_>) -> Result<T, RuntimeError> {
    let Some(at) = resolve_index_signed(items.len() as u64, index) else {
        return Err(diagnostic::array_bounds(site, index, items.len()));
    };
    Ok(items[at as usize].clone())
}

pub fn read_unsigned<T: Clone>(
    items: &[T],
    index: u128,
    site: Site<'_>,
) -> Result<T, RuntimeError> {
    let Some(at) = resolve_index_unsigned(items.len() as u64, index) else {
        return Err(diagnostic::array_bounds(site, index, items.len()));
    };
    Ok(items[at as usize].clone())
}

pub fn write_signed<T: Clone>(
    items: &mut [T],
    index: i128,
    value: T,
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    let Some(at) = resolve_index_signed(items.len() as u64, index) else {
        return Err(diagnostic::array_bounds(site, index, items.len()));
    };
    items[at as usize] = value;
    Ok(())
}

pub fn write_unsigned<T: Clone>(
    items: &mut [T],
    index: u128,
    value: T,
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    let Some(at) = resolve_index_unsigned(items.len() as u64, index) else {
        return Err(diagnostic::array_bounds(site, index, items.len()));
    };
    items[at as usize] = value;
    Ok(())
}

/// Replace `items[index]`, or append when a nonnegative index equals the
/// current length. A larger index stays out of bounds, so holes are not filled.
pub fn assign_signed<T: Clone>(
    items: &mut Vec<T>,
    index: i128,
    value: T,
    limit: Option<u64>,
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    if index >= 0 && index == i128::from(items.len() as u64) {
        return push(items, value, limit, site);
    }
    write_signed(items, index, value, site)
}

pub fn assign_unsigned<T: Clone>(
    items: &mut Vec<T>,
    index: u128,
    value: T,
    limit: Option<u64>,
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    if index == u128::from(items.len() as u64) {
        return push(items, value, limit, site);
    }
    write_unsigned(items, index, value, site)
}

pub fn extend<T: Clone>(
    items: &mut Vec<T>,
    extra: &[T],
    limit: Option<u64>,
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    let new_len = (items.len() as u64).saturating_add(extra.len() as u64);
    ensure_limit(new_len, limit, site)?;
    items.extend(extra.iter().cloned());
    Ok(())
}

pub fn pop_last<T>(items: &mut Vec<T>, site: Site<'_>) -> Result<T, RuntimeError> {
    items.pop().ok_or_else(|| diagnostic::empty_pop(site))
}

pub fn pop_signed<T>(items: &mut Vec<T>, index: i128, site: Site<'_>) -> Result<T, RuntimeError> {
    let Some(at) = resolve_index_signed(items.len() as u64, index) else {
        return Err(diagnostic::array_bounds(site, index, items.len()));
    };
    Ok(items.remove(at as usize))
}

pub fn pop_unsigned<T>(items: &mut Vec<T>, index: u128, site: Site<'_>) -> Result<T, RuntimeError> {
    let Some(at) = resolve_index_unsigned(items.len() as u64, index) else {
        return Err(diagnostic::array_bounds(site, index, items.len()));
    };
    Ok(items.remove(at as usize))
}

pub fn position<T: PartialEq>(items: &[T], value: &T, site: Site<'_>) -> Result<i64, RuntimeError> {
    items
        .iter()
        .position(|item| item == value)
        .map(|index| index as i64)
        .ok_or_else(|| diagnostic::value_not_found(site))
}

pub fn count_eq<T: PartialEq>(items: &[T], value: &T) -> i64 {
    items.iter().filter(|item| *item == value).count() as i64
}

pub fn push<T>(
    items: &mut Vec<T>,
    value: T,
    limit: Option<u64>,
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    ensure_limit(items.len().saturating_add(1) as u64, limit, site)?;
    items.push(value);
    Ok(())
}

pub fn insert_signed<T: Clone>(
    items: &mut Vec<T>,
    index: i128,
    value: T,
    limit: Option<u64>,
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    let Some(at) = resolve_insert_signed(items.len() as u64, index) else {
        return Err(diagnostic::insertion_position(site, index, items.len()));
    };
    ensure_limit(items.len().saturating_add(1) as u64, limit, site)?;
    items.insert(at as usize, value);
    Ok(())
}

pub fn insert_unsigned<T: Clone>(
    items: &mut Vec<T>,
    index: u128,
    value: T,
    limit: Option<u64>,
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    let Some(at) = resolve_insert_unsigned(items.len() as u64, index) else {
        return Err(diagnostic::insertion_position(site, index, items.len()));
    };
    ensure_limit(items.len().saturating_add(1) as u64, limit, site)?;
    items.insert(at as usize, value);
    Ok(())
}

pub fn ensure_limit(len: u64, limit: Option<u64>, site: Site<'_>) -> Result<(), RuntimeError> {
    let Some(limit) = limit else {
        return Ok(());
    };
    if len > limit {
        return Err(diagnostic::array_limit(site, len, limit));
    }
    Ok(())
}

pub fn remove_signed<T>(
    items: &mut Vec<T>,
    index: i128,
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    let Some(at) = resolve_index_signed(items.len() as u64, index) else {
        return Err(diagnostic::array_bounds(site, index, items.len()));
    };
    items.remove(at as usize);
    Ok(())
}

pub fn remove_unsigned<T>(
    items: &mut Vec<T>,
    index: u128,
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    let Some(at) = resolve_index_unsigned(items.len() as u64, index) else {
        return Err(diagnostic::array_bounds(site, index, items.len()));
    };
    items.remove(at as usize);
    Ok(())
}

pub fn slice_copy<T: Clone>(
    items: &[T],
    start: Option<i128>,
    stop: Option<i128>,
    step: Option<i128>,
    site: Site<'_>,
) -> Result<Vec<T>, RuntimeError> {
    let indices = match slice_indices(items.len() as u64, start, stop, step) {
        Ok(indices) => indices,
        Err(SliceFault::ZeroStep) => return Err(diagnostic::zero_step(site)),
    };
    Ok(indices
        .into_iter()
        .map(|index| items[index as usize].clone())
        .collect())
}

pub fn assign_unit<T: Clone>(
    items: &mut Vec<T>,
    start: Option<i128>,
    stop: Option<i128>,
    replacement: &[T],
) {
    let (start, stop) = unit_bounds(items.len() as u64, start, stop);
    let _ = items.splice(start as usize..stop as usize, replacement.iter().cloned());
}

pub fn assign_step<T: Clone>(
    items: &mut Vec<T>,
    start: Option<i128>,
    stop: Option<i128>,
    step: i128,
    replacement: &[T],
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    let indices = match slice_indices(items.len() as u64, start, stop, Some(step)) {
        Ok(indices) => indices,
        Err(SliceFault::ZeroStep) => return Err(diagnostic::zero_step(site)),
    };
    if indices.len() != replacement.len() {
        return Err(diagnostic::slice_length(
            site,
            indices.len(),
            replacement.len(),
        ));
    }
    for (index, value) in indices.into_iter().zip(replacement.iter()) {
        items[index as usize] = value.clone();
    }
    Ok(())
}

pub fn index_signed(len: u64, index: i128, site: Site<'_>) -> Result<u64, RuntimeError> {
    resolve_index_signed(len, index)
        .ok_or_else(|| diagnostic::array_bounds(site, index, len as usize))
}

pub fn index_unsigned(len: u64, index: u128, site: Site<'_>) -> Result<u64, RuntimeError> {
    resolve_index_unsigned(len, index)
        .ok_or_else(|| diagnostic::array_bounds(site, index, len as usize))
}

pub fn assign_slice<T: Clone>(
    items: &mut Vec<T>,
    start: Option<i128>,
    stop: Option<i128>,
    step: Option<i128>,
    replacement: &[T],
    limit: Option<u64>,
    site: Site<'_>,
) -> Result<(), RuntimeError> {
    match step {
        None | Some(1) => {
            let (start_at, stop_at) = unit_bounds(items.len() as u64, start, stop);
            let new_len = (items.len() as u64)
                .saturating_sub(stop_at.saturating_sub(start_at))
                .saturating_add(replacement.len() as u64);
            ensure_limit(new_len, limit, site)?;
            assign_unit(items, start, stop, replacement);
            Ok(())
        }
        Some(step) => assign_step(items, start, stop, step, replacement, site),
    }
}

pub fn expect_len(len: usize, expected: usize, site: Site<'_>) -> Result<(), RuntimeError> {
    if len != expected {
        return Err(diagnostic::destructure_length(site, expected, len));
    }
    Ok(())
}

pub fn resolve_index_signed(len: u64, index: i128) -> Option<u64> {
    if len == 0 {
        return None;
    }
    let resolved = if index < 0 {
        let resolved = (len as i128).checked_add(index)?;
        if resolved < 0 {
            return None;
        }
        resolved
    } else {
        index
    };
    if resolved < 0 || (resolved as u64) >= len {
        None
    } else {
        Some(resolved as u64)
    }
}

pub fn resolve_index_unsigned(len: u64, index: u128) -> Option<u64> {
    if index >= u128::from(len) {
        None
    } else {
        Some(index as u64)
    }
}

pub fn resolve_insert_signed(len: u64, index: i128) -> Option<u64> {
    let resolved = if index < 0 {
        let resolved = (len as i128).checked_add(index)?;
        if resolved < 0 {
            return None;
        }
        resolved
    } else {
        index
    };
    if resolved < 0 || (resolved as u64) > len {
        None
    } else {
        Some(resolved as u64)
    }
}

pub fn resolve_insert_unsigned(len: u64, index: u128) -> Option<u64> {
    if index > u128::from(len) {
        None
    } else {
        Some(index as u64)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceFault {
    ZeroStep,
}

pub fn slice_indices(
    len: u64,
    start: Option<i128>,
    stop: Option<i128>,
    step: Option<i128>,
) -> Result<Vec<u64>, SliceFault> {
    let step = step.unwrap_or(1);
    if step == 0 {
        return Err(SliceFault::ZeroStep);
    }
    let len = len as i128;
    let start = match start {
        Some(start) => adjust_bound(len, start, step),
        None => {
            if step < 0 {
                len - 1
            } else {
                0
            }
        }
    };
    let stop = match stop {
        Some(stop) => adjust_bound(len, stop, step),
        None => {
            if step < 0 {
                -1
            } else {
                len
            }
        }
    };
    let count = slice_len(start, stop, step);
    let mut indices = Vec::with_capacity(count as usize);
    for offset in 0..count {
        let cursor = start + step * i128::from(offset);
        if cursor < 0 {
            break;
        }
        indices.push(cursor as u64);
    }
    Ok(indices)
}

pub fn unit_bounds(len: u64, start: Option<i128>, stop: Option<i128>) -> (u64, u64) {
    let len = len as i128;
    let start = adjust_bound(len, start.unwrap_or(0), 1);
    let mut stop = adjust_bound(len, stop.unwrap_or(len), 1);
    if stop < start {
        stop = start;
    }
    (start as u64, stop as u64)
}

fn adjust_bound(len: i128, mut value: i128, step: i128) -> i128 {
    if value < 0 {
        value += len;
        if value < 0 {
            return if step < 0 { -1 } else { 0 };
        }
    } else if value >= len {
        return if step < 0 { len - 1 } else { len };
    }
    value
}

fn slice_len(start: i128, stop: i128, step: i128) -> u64 {
    if (step < 0 && stop >= start) || (step > 0 && start >= stop) {
        return 0;
    }
    let count = if step < 0 {
        (stop - start + 1) / step + 1
    } else {
        (stop - start - 1) / step + 1
    };
    count as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Option<i128>, stop: Option<i128>, step: Option<i128>) -> Vec<i64> {
        let source: Vec<i64> = vec![10, 20, 30, 40, 50];
        let indices = slice_indices(source.len() as u64, start, stop, step).unwrap();
        indices
            .into_iter()
            .map(|index| source[index as usize])
            .collect()
    }

    #[test]
    fn slices_match_the_python_examples() {
        assert_eq!(at(None, Some(3), None), vec![10, 20, 30]);
        assert_eq!(at(Some(1), Some(4), None), vec![20, 30, 40]);
        assert_eq!(at(Some(2), None, None), vec![30, 40, 50]);
        assert_eq!(at(None, None, None), vec![10, 20, 30, 40, 50]);
        assert_eq!(at(None, None, Some(2)), vec![10, 30, 50]);
        assert_eq!(at(None, None, Some(-1)), vec![50, 40, 30, 20, 10]);
        assert_eq!(at(Some(4), Some(0), Some(-1)), vec![50, 40, 30, 20]);
        assert_eq!(at(None, Some(-1), None), vec![10, 20, 30, 40]);
        assert_eq!(at(Some(-3), None, None), vec![30, 40, 50]);
        assert_eq!(at(None, Some(-1), Some(-1)), Vec::<i64>::new());
        assert_eq!(at(Some(100), None, None), Vec::<i64>::new());
        assert_eq!(at(Some(-100), None, None), vec![10, 20, 30, 40, 50]);
        assert_eq!(at(None, None, Some(-1)), vec![50, 40, 30, 20, 10]);
        assert!(slice_indices(5, None, None, Some(0)).is_err());
    }

    #[test]
    fn unit_assignment_clamps_and_can_insert() {
        let mut items = vec![10, 20, 30, 40, 50];
        assign_unit(&mut items, Some(1), Some(3), &[200, 300]);
        assert_eq!(items, vec![10, 200, 300, 40, 50]);

        let mut items = vec![10, 20, 30, 40, 50];
        assign_unit(&mut items, Some(1), Some(2), &[7, 8, 9]);
        assert_eq!(items, vec![10, 7, 8, 9, 30, 40, 50]);

        let mut items = vec![10, 20, 30, 40, 50];
        assign_unit(&mut items, Some(1), Some(3), &[]);
        assert_eq!(items, vec![10, 40, 50]);

        let mut items = vec![10, 20, 30, 40, 50];
        assign_unit(&mut items, Some(3), Some(1), &[9]);
        assert_eq!(items, vec![10, 20, 30, 9, 40, 50]);

        let mut items = vec![10, 20, 30, 40, 50];
        assign_unit(&mut items, Some(2), Some(2), &[7, 8]);
        assert_eq!(items, vec![10, 20, 7, 8, 30, 40, 50]);
    }

    #[test]
    fn indexes_and_insertion_positions_cover_the_ends() {
        assert_eq!(resolve_index_signed(5, 0), Some(0));
        assert_eq!(resolve_index_signed(5, 4), Some(4));
        assert_eq!(resolve_index_signed(5, -1), Some(4));
        assert_eq!(resolve_index_signed(5, -5), Some(0));
        assert_eq!(resolve_index_signed(5, 5), None);
        assert_eq!(resolve_index_signed(5, -6), None);
        assert_eq!(resolve_index_signed(0, 0), None);
        assert_eq!(resolve_index_signed(5, i128::from(i64::MIN)), None);
        assert_eq!(resolve_index_signed(5, i128::from(i64::MAX)), None);
        assert_eq!(resolve_index_unsigned(5, 4), Some(4));
        assert_eq!(resolve_index_unsigned(5, 5), None);
        assert_eq!(resolve_insert_signed(5, 5), Some(5));
        assert_eq!(resolve_insert_signed(5, -1), Some(4));
        assert_eq!(resolve_insert_signed(5, 0), Some(0));
        assert_eq!(resolve_insert_signed(5, -5), Some(0));
        assert_eq!(resolve_insert_signed(5, 6), None);
        assert_eq!(resolve_insert_signed(5, -6), None);
        assert_eq!(resolve_insert_signed(0, 0), Some(0));
        assert_eq!(resolve_insert_signed(0, -1), None);
        assert_eq!(resolve_insert_unsigned(5, 5), Some(5));
        assert_eq!(resolve_insert_unsigned(5, 6), None);
    }
}
