//! Graphite `async<T>` runtime support.
//!
//! Async operations are type-erased, pinned futures that produce
//! `Result<T, RuntimeError>`. Locals and parameters use this alias so the
//! generated Rust can name the type; awaiting consumes the future.

use std::future::Future;
use std::pin::Pin;

use crate::diagnostic::RuntimeError;

/// Graphite `async<T>` — a single-consumption pending operation.
pub type Async<T> = Pin<Box<dyn Future<Output = Result<T, RuntimeError>> + Send>>;

/// Box an async function call into a Graphite `async<T>` value.
pub fn box_async<T, F>(future: F) -> Async<T>
where
    T: Send + 'static,
    F: Future<Output = Result<T, RuntimeError>> + Send + 'static,
{
    Box::pin(future)
}
