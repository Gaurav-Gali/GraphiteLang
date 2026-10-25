//! Graphite `iter<T>` runtime adapter.
//!
//! Wraps a Rust [`Iterator`] so Graphite can use `.hasNext()` / `.next()`
//! without exposing `Option`. Exhaustion of `.next()` is a runtime error.
//! `.hasNext()` may peek and cache the next element so checking availability
//! does not consume it.
//!
//! Generators are driven by [`Iter::from_fn`]: each call to the resume
//! closure yields at most one element (`Ok(Some(v))`), signals completion
//! with `Ok(None)`, or propagates a [`RuntimeError`].

use std::sync::{Arc, Mutex};

use crate::convert::Site;
use crate::diagnostic::{self, RuntimeError};
use crate::heap::Handle;
use crate::value::GrfValue;

enum Source<T> {
    Ready(Box<dyn Iterator<Item = T> + Send>),
    /// Streaming generator / resume function. Returns the next element,
    /// `None` when exhausted, or a runtime error.
    Resume(Box<dyn FnMut() -> Result<Option<T>, RuntimeError> + Send>),
}

struct Inner<T> {
    source: Source<T>,
    /// `None` = not peeked. `Some(None)` = peeked exhaustion. `Some(Some(v))` = cached.
    peeked: Option<Option<T>>,
}

/// Shared, heap-backed Graphite iterator. Cloning aliases the same state.
#[derive(Clone)]
pub struct Iter<T> {
    inner: Arc<Mutex<Inner<T>>>,
}

impl<T> Iter<T>
where
    T: Clone + Send + 'static,
{
    pub fn from_iter<I>(iter: I) -> Self
    where
        I: Iterator<Item = T> + Send + 'static,
    {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                source: Source::Ready(Box::new(iter)),
                peeked: None,
            })),
        }
    }

    pub fn from_vec(items: Vec<T>) -> Self {
        Self::from_iter(items.into_iter())
    }

    /// Build a streaming iterator from a resume function. The function is
    /// invoked on each advance (`hasNext` / `next`) until it returns
    /// `Ok(None)` or an error. This is the lowering target for generators.
    pub fn from_fn<F>(f: F) -> Self
    where
        F: FnMut() -> Result<Option<T>, RuntimeError> + Send + 'static,
    {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                source: Source::Resume(Box::new(f)),
                peeked: None,
            })),
        }
    }

    /// Snapshot an array into a fresh iterator (replay-safe vs the collection).
    pub fn from_array(items: &[T]) -> Self {
        Self::from_vec(items.to_vec())
    }

    /// `.iter()` on an iterator returns the same shared state.
    pub fn iter(&self) -> Self {
        self.clone()
    }

    pub fn has_next(&self) -> Result<bool, RuntimeError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| diagnostic::runtime_internal("iterator lock poisoned"))?;
        if let Some(peeked) = &inner.peeked {
            return Ok(peeked.is_some());
        }
        let next = Self::pull(&mut inner)?;
        let available = next.is_some();
        inner.peeked = Some(next);
        Ok(available)
    }

    pub fn next(&self, site: Site<'_>) -> Result<T, RuntimeError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| diagnostic::runtime_internal("iterator lock poisoned"))?;
        if let Some(peeked) = inner.peeked.take() {
            return peeked.ok_or_else(|| diagnostic::iterator_exhausted(site));
        }
        Self::pull(&mut inner)?.ok_or_else(|| diagnostic::iterator_exhausted(site))
    }

    fn pull(inner: &mut Inner<T>) -> Result<Option<T>, RuntimeError> {
        match &mut inner.source {
            Source::Ready(iter) => Ok(iter.next()),
            Source::Resume(resume) => resume(),
        }
    }
}

impl<T: GrfValue> GrfValue for Iter<T> {
    fn grf_trace(&self, visit: &mut dyn FnMut(Handle)) {
        if let Ok(inner) = self.inner.lock() {
            if let Some(Some(value)) = &inner.peeked {
                value.grf_trace(visit);
            }
        }
    }
}

/// Iterate a string as Unicode scalar values (`chr`), lazily by byte index.
pub fn from_str_chars(text: &str) -> Iter<char> {
    Iter::from_iter(CharCursor {
        text: text.to_string(),
        index: 0,
    })
}

struct CharCursor {
    text: String,
    index: usize,
}

impl Iterator for CharCursor {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        let rest = self.text.get(self.index..)?;
        let ch = rest.chars().next()?;
        self.index += ch.len_utf8();
        Some(ch)
    }
}

/// Built-in Graphite `range(start, end, step)` — lazy, end-exclusive.
pub fn range(start: i64, end: i64, step: i64, site: Site<'_>) -> Result<Iter<i64>, RuntimeError> {
    if step == 0 {
        return Err(diagnostic::zero_step(site));
    }
    Ok(Iter::from_iter(RangeCursor {
        current: start,
        end,
        step,
    }))
}

struct RangeCursor {
    current: i64,
    end: i64,
    step: i64,
}

impl Iterator for RangeCursor {
    type Item = i64;

    fn next(&mut self) -> Option<i64> {
        if self.step > 0 {
            if self.current >= self.end {
                return None;
            }
        } else if self.current <= self.end {
            return None;
        }
        let value = self.current;
        match self.current.checked_add(self.step) {
            Some(next) => self.current = next,
            None => {
                // Overflow ends the range rather than wrapping.
                self.current = self.end;
            }
        }
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::Site;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn site() -> Site<'static> {
        Site {
            file: "test.grf",
            line: 1,
            column: 1,
            end_column: 1,
            excerpt: "",
        }
    }

    #[test]
    fn has_next_does_not_consume() {
        let it = Iter::from_vec(vec![1i64, 2]);
        assert!(it.has_next().unwrap());
        assert!(it.has_next().unwrap());
        assert_eq!(it.next(site()).unwrap(), 1);
        assert_eq!(it.next(site()).unwrap(), 2);
        assert!(!it.has_next().unwrap());
        assert!(it.next(site()).is_err());
    }

    #[test]
    fn iter_aliases_state() {
        let a = Iter::from_vec(vec![10i64, 20]);
        let b = a.iter();
        assert_eq!(a.next(site()).unwrap(), 10);
        assert_eq!(b.next(site()).unwrap(), 20);
    }

    #[test]
    fn from_fn_is_lazy_and_streaming() {
        static STEPS: AtomicUsize = AtomicUsize::new(0);
        let mut n = 0i64;
        let it = Iter::from_fn(move || {
            STEPS.fetch_add(1, Ordering::SeqCst);
            if n >= 3 {
                return Ok(None);
            }
            n += 1;
            Ok(Some(n))
        });
        assert_eq!(STEPS.load(Ordering::SeqCst), 0);
        assert!(it.has_next().unwrap());
        assert_eq!(STEPS.load(Ordering::SeqCst), 1);
        assert_eq!(it.next(site()).unwrap(), 1);
        assert_eq!(it.next(site()).unwrap(), 2);
        assert_eq!(it.next(site()).unwrap(), 3);
        assert!(!it.has_next().unwrap());
        assert_eq!(STEPS.load(Ordering::SeqCst), 4); // 3 yields + 1 exhaustion peek
    }

    #[test]
    fn from_str_chars_unicode_scalars() {
        let it = from_str_chars("a😀b");
        assert_eq!(it.next(site()).unwrap(), 'a');
        assert_eq!(it.next(site()).unwrap(), '😀');
        assert_eq!(it.next(site()).unwrap(), 'b');
        assert!(!it.has_next().unwrap());
    }

    #[test]
    fn range_is_end_exclusive_and_lazy() {
        let it = range(1, 5, 1, site()).unwrap();
        assert_eq!(it.next(site()).unwrap(), 1);
        assert_eq!(it.next(site()).unwrap(), 2);
        assert_eq!(it.next(site()).unwrap(), 3);
        assert_eq!(it.next(site()).unwrap(), 4);
        assert!(!it.has_next().unwrap());
        assert!(range(0, 10, 0, site()).is_err());
        let stepped = range(1, 10, 2, site()).unwrap();
        let mut values = Vec::new();
        while stepped.has_next().unwrap() {
            values.push(stepped.next(site()).unwrap());
        }
        assert_eq!(values, vec![1, 3, 5, 7, 9]);
    }
}
