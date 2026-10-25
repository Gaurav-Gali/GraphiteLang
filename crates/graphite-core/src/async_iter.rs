//! Graphite `asyncIter<T>` — pull-based async item streams.
//!
//! Protocol mirrors sync `iter<T>`: `.iter()`, `.hasNext()`, `.next()`,
//! `.value()`. `hasNext` / `next` / `value` may suspend until the producer
//! emits. Cloning aliases the same stream state.

use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};

use crate::convert::Site;
use crate::diagnostic::{self, RuntimeError};

/// Graphite `asyncIter<T>`.
#[derive(Clone)]
pub struct AsyncIter<T> {
    inner: Arc<Mutex<Inner<T>>>,
}

struct Inner<T> {
    rx: Option<mpsc::Receiver<Result<T, RuntimeError>>>,
    /// `None` = not peeked. `Some(None)` = peeked exhaustion. `Some(Some(v))` = cached.
    peeked: Option<Option<T>>,
}

impl<T> AsyncIter<T>
where
    T: Clone + Send + 'static,
{
    /// Build an async iterator from a producer future that sends items on `tx`.
    ///
    /// The producer is spawned on the current Tokio runtime. Capacity `1`
    /// gives backpressure: the next `emit` waits until the consumer pulls.
    pub fn spawn<F>(producer: F) -> Self
    where
        F: FnOnce(mpsc::Sender<Result<T, RuntimeError>>) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
            + Send
            + 'static,
    {
        let (tx, rx) = mpsc::channel(1);
        tokio::spawn(producer(tx));
        Self {
            inner: Arc::new(Mutex::new(Inner {
                rx: Some(rx),
                peeked: None,
            })),
        }
    }

    /// `.iter()` on an async iterator returns the same shared state.
    pub fn iter(&self) -> Self {
        self.clone()
    }

    /// Await whether another element remains; may prefetch and cache it.
    pub async fn has_next(&self) -> Result<bool, RuntimeError> {
        let mut inner = self.inner.lock().await;
        if let Some(peeked) = &inner.peeked {
            return Ok(peeked.is_some());
        }
        let next = Self::pull(&mut inner).await?;
        let available = next.is_some();
        inner.peeked = Some(next);
        Ok(available)
    }

    /// Await and consume the next element; runtime error if exhausted.
    pub async fn next(&self, site: Site<'_>) -> Result<T, RuntimeError> {
        let mut inner = self.inner.lock().await;
        if let Some(peeked) = inner.peeked.take() {
            return peeked.ok_or_else(|| diagnostic::iterator_exhausted(site));
        }
        Self::pull(&mut inner)
            .await?
            .ok_or_else(|| diagnostic::iterator_exhausted(site))
    }

    /// Current peeked element without consuming. Prefetches if needed.
    pub async fn value(&self, site: Site<'_>) -> Result<T, RuntimeError> {
        let mut inner = self.inner.lock().await;
        if inner.peeked.is_none() {
            let next = Self::pull(&mut inner).await?;
            inner.peeked = Some(next);
        }
        match &inner.peeked {
            Some(Some(value)) => Ok(value.clone()),
            _ => Err(diagnostic::iterator_exhausted(site)),
        }
    }

    /// Low-level pull used by `loop (await<T> …)` codegen.
    pub async fn try_next(&self) -> Result<Option<T>, RuntimeError> {
        let mut inner = self.inner.lock().await;
        if let Some(peeked) = inner.peeked.take() {
            return Ok(peeked);
        }
        Self::pull(&mut inner).await
    }

    async fn pull(inner: &mut Inner<T>) -> Result<Option<T>, RuntimeError> {
        let Some(rx) = inner.rx.as_mut() else {
            return Ok(None);
        };
        match rx.recv().await {
            Some(Ok(value)) => Ok(Some(value)),
            Some(Err(error)) => Err(error),
            None => {
                inner.rx = None;
                Ok(None)
            }
        }
    }
}
