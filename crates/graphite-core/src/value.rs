//! Values that can be stored in a generic class and traced by the heap.
//!
//! Generated generic structs are bounded by this trait. Plain values trace
//! nothing. Handles, arrays, maps, options, and records trace the class
//! references they contain.

use std::collections::HashMap;

use crate::diagnostic::RuntimeError;
use crate::heap::{Handle, Rooted};

pub trait GrfValue: Clone + Send + 'static {
    fn grf_trace(&self, visit: &mut dyn FnMut(Handle));

    /// Prepare a value for storage inside a heap object.
    fn grf_to_heap(&self) -> Self {
        self.clone()
    }

    /// Restore a heap-stored value for use on the stack.
    fn grf_from_heap(self) -> Self {
        self
    }
}

macro_rules! plain_value {
    ($($ty:ty),* $(,)?) => {$(
        impl GrfValue for $ty {
            fn grf_trace(&self, _visit: &mut dyn FnMut(Handle)) {}
        }
    )*};
}

plain_value!(
    i16,
    i32,
    i64,
    u16,
    u32,
    u64,
    f32,
    f64,
    bool,
    char,
    String,
    ()
);

impl<T: GrfValue> GrfValue for Vec<T> {
    fn grf_trace(&self, visit: &mut dyn FnMut(Handle)) {
        for item in self {
            item.grf_trace(visit);
        }
    }

    fn grf_to_heap(&self) -> Self {
        self.iter().map(GrfValue::grf_to_heap).collect()
    }

    fn grf_from_heap(self) -> Self {
        self.into_iter().map(GrfValue::grf_from_heap).collect()
    }
}

impl<T: GrfValue> GrfValue for Option<T> {
    fn grf_trace(&self, visit: &mut dyn FnMut(Handle)) {
        if let Some(item) = self {
            item.grf_trace(visit);
        }
    }

    fn grf_to_heap(&self) -> Self {
        self.as_ref().map(GrfValue::grf_to_heap)
    }

    fn grf_from_heap(self) -> Self {
        self.map(GrfValue::grf_from_heap)
    }
}

impl<K, V> GrfValue for HashMap<K, V>
where
    K: Clone + Eq + std::hash::Hash + Send + 'static,
    V: GrfValue,
{
    fn grf_trace(&self, visit: &mut dyn FnMut(Handle)) {
        for value in self.values() {
            value.grf_trace(visit);
        }
    }

    fn grf_to_heap(&self) -> Self {
        self.iter()
            .map(|(key, value)| (key.clone(), value.grf_to_heap()))
            .collect()
    }

    fn grf_from_heap(self) -> Self {
        self.into_iter()
            .map(|(key, value)| (key, value.grf_from_heap()))
            .collect()
    }
}

impl GrfValue for Handle {
    fn grf_trace(&self, visit: &mut dyn FnMut(Handle)) {
        visit(*self);
    }
}

impl GrfValue for Rooted {
    fn grf_trace(&self, visit: &mut dyn FnMut(Handle)) {
        visit(self.handle());
    }

    fn grf_to_heap(&self) -> Self {
        self.duplicate_unrooted()
    }

    fn grf_from_heap(self) -> Self {
        self.promote()
    }
}

macro_rules! function_value {
    ($($param:ident),*) => {
        impl<R: GrfValue, $($param: GrfValue),*> GrfValue for fn($($param),*) -> Result<R, RuntimeError> {
            fn grf_trace(&self, _visit: &mut dyn FnMut(Handle)) {}
        }
    };
}

impl<R: GrfValue> GrfValue for fn() -> Result<R, RuntimeError> {
    fn grf_trace(&self, _visit: &mut dyn FnMut(Handle)) {}
}

function_value!(A);
function_value!(A, B);
function_value!(A, B, C);
