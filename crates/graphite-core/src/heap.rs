//! Thread-local mark-sweep heap for Graphite class instances.
//!
//! A Graphite class value is a [`Rooted`] handle. Cloning a root registers
//! another root; dropping it removes that root. Objects stored inside other
//! objects keep raw [`Handle`]s and are kept alive only by tracing, so an
//! unreachable cycle is reclaimed.
//!
//! Collection runs from [`alloc`] and [`collect_now`]. `with` / `with_mut`
//! borrow the heap for the callback only. Re-entering the heap from a
//! callback returns [`RuntimeInternalError`] instead of panicking. Root
//! registration that happens while the heap is borrowed is deferred and
//! applied before the next collection.

use std::any::Any;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::convert::Site;
use crate::diagnostic::{self, RuntimeError};

/// Identity of one class instance. Not a Graphite value and not a stable
/// address. Equality is object identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle {
    index: u32,
    generation: u32,
}

impl Handle {
    pub fn invalid() -> Self {
        Self {
            index: u32::MAX,
            generation: 0,
        }
    }

    pub fn is_invalid(self) -> bool {
        self.index == u32::MAX
    }
}

impl Default for Handle {
    fn default() -> Self {
        Self::invalid()
    }
}

/// A stack, record, array, or map root. Class fields store [`Handle`], not
/// `Rooted`, so a cycle of fields is not a root cycle.
pub struct Rooted {
    handle: Handle,
    rooted: bool,
}

impl std::fmt::Debug for Rooted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rooted")
            .field("rooted", &self.rooted)
            .finish()
    }
}

impl PartialEq for Rooted {
    fn eq(&self, other: &Self) -> bool {
        self.handle == other.handle
    }
}

impl Eq for Rooted {}

impl Default for Rooted {
    fn default() -> Self {
        Self {
            handle: Handle::invalid(),
            rooted: false,
        }
    }
}

impl Clone for Rooted {
    fn clone(&self) -> Self {
        if self.handle.is_invalid() {
            return Self::default();
        }
        register_root(self.handle);
        Self {
            handle: self.handle,
            rooted: true,
        }
    }
}

impl Drop for Rooted {
    fn drop(&mut self) {
        if self.rooted {
            unregister_root(self.handle);
            self.rooted = false;
        }
    }
}

impl Rooted {
    pub fn handle(&self) -> Handle {
        self.handle
    }

    /// Copy the handle without registering a root. Used for values stored
    /// inside a class object, which are traced rather than rooted.
    pub fn duplicate_unrooted(&self) -> Self {
        Self {
            handle: self.handle,
            rooted: false,
        }
    }

    pub fn disarm(&mut self) {
        if self.rooted {
            unregister_root(self.handle);
            self.rooted = false;
        }
    }

    pub fn promote(&self) -> Self {
        self.clone()
    }
}

/// Object stored in the heap. Generated class structs implement this.
pub trait GcObject: Any + Send {
    fn trace(&self, visit: &mut dyn FnMut(Handle));
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn grf_type_id(&self) -> u32 {
        0
    }
}

/// Dynamic class tag stored on a generated object. Used for overriding and
/// interface dispatch. Objects that are not generated classes report `0`.
pub fn type_id(handle: Handle) -> Result<u32, RuntimeError> {
    with_heap(|heap| {
        let Some(slot) = heap.slot_mut(handle) else {
            return Err(diagnostic::runtime_internal("invalid class handle"));
        };
        let Some(object) = slot.data.as_ref() else {
            return Err(diagnostic::runtime_internal("invalid class handle"));
        };
        Ok(object.grf_type_id())
    })?
}

struct Slot {
    generation: u32,
    marked: bool,
    frozen: bool,
    data: Option<Box<dyn GcObject>>,
}

struct Heap {
    slots: Vec<Slot>,
    free: Vec<u32>,
    roots: HashMap<Handle, u32>,
}

impl Heap {
    fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            roots: HashMap::new(),
        }
    }

    fn add_root(&mut self, handle: Handle) {
        *self.roots.entry(handle).or_insert(0) += 1;
    }

    fn remove_root(&mut self, handle: Handle) {
        if let Some(count) = self.roots.get_mut(&handle) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.roots.remove(&handle);
            }
        }
    }

    fn insert(&mut self, value: Box<dyn GcObject>) -> Handle {
        let generation_slot = if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.generation = slot.generation.wrapping_add(1);
            if slot.generation == 0 {
                slot.generation = 1;
            }
            slot.marked = false;
            slot.frozen = false;
            slot.data = Some(value);
            (index, slot.generation)
        } else {
            let index = self.slots.len() as u32;
            self.slots.push(Slot {
                generation: 1,
                marked: false,
                frozen: false,
                data: Some(value),
            });
            (index, 1)
        };
        Handle {
            index: generation_slot.0,
            generation: generation_slot.1,
        }
    }

    fn slot_mut(&mut self, handle: Handle) -> Option<&mut Slot> {
        let slot = self.slots.get_mut(handle.index as usize)?;
        if slot.generation != handle.generation || slot.data.is_none() {
            return None;
        }
        Some(slot)
    }

    fn collect(&mut self) {
        for slot in &mut self.slots {
            slot.marked = false;
        }
        let roots = self.roots.keys().copied().collect::<Vec<_>>();
        for root in roots {
            self.mark(root);
        }
        for index in 0..self.slots.len() {
            if self.slots[index].data.is_none() || self.slots[index].marked {
                continue;
            }
            self.slots[index].data = None;
            self.slots[index].frozen = false;
            self.free.push(index as u32);
        }
    }

    fn mark(&mut self, handle: Handle) {
        if handle.is_invalid() {
            return;
        }
        let Some(slot) = self.slots.get_mut(handle.index as usize) else {
            return;
        };
        if slot.generation != handle.generation || slot.data.is_none() || slot.marked {
            return;
        }
        slot.marked = true;
        let mut children = Vec::new();
        if let Some(object) = slot.data.as_ref() {
            object.trace(&mut |child| children.push(child));
        }
        for child in children {
            self.mark(child);
        }
    }

    fn live_count(&self) -> usize {
        self.slots.iter().filter(|slot| slot.data.is_some()).count()
    }
}

enum Deferred {
    Root(Handle),
    Unroot(Handle),
}

thread_local! {
    static HEAP: RefCell<Heap> = RefCell::new(Heap::new());
    static DEFERRED: RefCell<Vec<Deferred>> = const { RefCell::new(Vec::new()) };
}

fn register_root(handle: Handle) {
    if handle.is_invalid() {
        return;
    }
    let queued = HEAP.with(|cell| {
        if let Ok(mut heap) = cell.try_borrow_mut() {
            heap.add_root(handle);
            false
        } else {
            true
        }
    });
    if queued {
        DEFERRED.with(|cell| cell.borrow_mut().push(Deferred::Root(handle)));
    }
}

fn unregister_root(handle: Handle) {
    if handle.is_invalid() {
        return;
    }
    let queued = HEAP.with(|cell| {
        if let Ok(mut heap) = cell.try_borrow_mut() {
            flush_into(&mut heap);
            heap.remove_root(handle);
            false
        } else {
            true
        }
    });
    if queued {
        DEFERRED.with(|cell| cell.borrow_mut().push(Deferred::Unroot(handle)));
    }
}

fn flush_into(heap: &mut Heap) {
    let pending = DEFERRED.with(|cell| std::mem::take(&mut *cell.borrow_mut()));
    for item in pending {
        match item {
            Deferred::Root(handle) => heap.add_root(handle),
            Deferred::Unroot(handle) => heap.remove_root(handle),
        }
    }
}

fn flush_deferred() {
    HEAP.with(|cell| {
        if let Ok(mut heap) = cell.try_borrow_mut() {
            flush_into(&mut heap);
        }
    });
}

fn with_heap<R>(body: impl FnOnce(&mut Heap) -> R) -> Result<R, RuntimeError> {
    flush_deferred();
    HEAP.with(|cell| {
        let Ok(mut heap) = cell.try_borrow_mut() else {
            return Err(diagnostic::runtime_internal("heap reentered"));
        };
        Ok(body(&mut heap))
    })
}

/// Allocate a class object and root it. Unreachable objects, including
/// cycles, are reclaimed first.
pub fn alloc<T: GcObject + 'static>(value: T) -> Result<Rooted, RuntimeError> {
    let rooted = with_heap(|heap| {
        heap.collect();
        let handle = heap.insert(Box::new(value));
        heap.add_root(handle);
        Rooted {
            handle,
            rooted: true,
        }
    })?;
    flush_deferred();
    Ok(rooted)
}

/// Root an existing handle. Fails for an invalid or uninitialized handle.
pub fn root(handle: Handle) -> Result<Rooted, RuntimeError> {
    if handle.is_invalid() {
        return Err(diagnostic::runtime_internal("uninitialized class field"));
    }
    let live = with_heap(|heap| heap.slot_mut(handle).is_some())?;
    if !live {
        return Err(diagnostic::runtime_internal("invalid class handle"));
    }
    register_root(handle);
    Ok(Rooted {
        handle,
        rooted: true,
    })
}

pub fn with<T: GcObject + 'static, R>(
    handle: Handle,
    body: impl FnOnce(&T) -> R,
) -> Result<R, RuntimeError> {
    with_heap(|heap| {
        let Some(slot) = heap.slot_mut(handle) else {
            return Err(diagnostic::runtime_internal("invalid class handle"));
        };
        let Some(object) = slot.data.as_ref() else {
            return Err(diagnostic::runtime_internal("invalid class handle"));
        };
        let Some(typed) = object.as_any().downcast_ref::<T>() else {
            return Err(diagnostic::runtime_internal("class type mismatch"));
        };
        Ok(body(typed))
    })?
}

pub fn with_mut<T: GcObject + 'static, R>(
    handle: Handle,
    site: Site<'_>,
    body: impl FnOnce(&mut T) -> R,
) -> Result<R, RuntimeError> {
    with_heap(|heap| {
        let Some(slot) = heap.slot_mut(handle) else {
            return Err(diagnostic::runtime_internal("invalid class handle"));
        };
        if slot.frozen {
            return Err(diagnostic::const_violation(site));
        }
        let Some(object) = slot.data.as_mut() else {
            return Err(diagnostic::runtime_internal("invalid class handle"));
        };
        let Some(typed) = object.as_any_mut().downcast_mut::<T>() else {
            return Err(diagnostic::runtime_internal("class type mismatch"));
        };
        Ok(body(typed))
    })?
}

/// Freeze this object and every class instance reachable from it.
/// Traversal is cycle-safe. Freezing is monotonic.
pub fn freeze(handle: Handle) {
    if handle.is_invalid() {
        return;
    }
    let mut pending = vec![handle];
    let mut seen = HashSet::new();
    while let Some(current) = pending.pop() {
        if current.is_invalid() || !seen.insert(current) {
            continue;
        }
        let children = with_heap(|heap| {
            let Some(slot) = heap.slot_mut(current) else {
                return Vec::new();
            };
            slot.frozen = true;
            let mut found = Vec::new();
            if let Some(object) = slot.data.as_ref() {
                object.trace(&mut |child| found.push(child));
            }
            found
        })
        .unwrap_or_default();
        pending.extend(children);
    }
}

pub fn collect_now() {
    let _ = with_heap(|heap| heap.collect());
    flush_deferred();
}

pub fn live_count() -> usize {
    flush_deferred();
    HEAP.with(|cell| cell.borrow().live_count())
}

pub fn is_frozen(handle: Handle) -> bool {
    HEAP.with(|cell| {
        cell.borrow()
            .slots
            .get(handle.index as usize)
            .is_some_and(|slot| slot.generation == handle.generation && slot.frozen)
    })
}

#[cfg(test)]
fn reset_heap() {
    DEFERRED.with(|cell| cell.borrow_mut().clear());
    HEAP.with(|cell| *cell.borrow_mut() = Heap::new());
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Node {
        next: Handle,
        value: i64,
    }

    impl GcObject for Node {
        fn trace(&self, visit: &mut dyn FnMut(Handle)) {
            visit(self.next);
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    fn site() -> Site<'static> {
        Site {
            file: "test.grf",
            line: 1,
            column: 1,
            end_column: 2,
            excerpt: "x",
        }
    }

    #[test]
    fn reachable_objects_stay_and_unreachable_objects_are_reclaimed() {
        reset_heap();
        let live = alloc(Node {
            next: Handle::invalid(),
            value: 1,
        })
        .unwrap();
        collect_now();
        assert_eq!(live_count(), 1);
        drop(live);
        collect_now();
        assert_eq!(live_count(), 0);
    }

    #[test]
    fn cloned_handles_keep_one_object_alive() {
        reset_heap();
        let first = alloc(Node {
            next: Handle::invalid(),
            value: 1,
        })
        .unwrap();
        let second = first.clone();
        assert_eq!(first, second);
        drop(first);
        collect_now();
        assert_eq!(live_count(), 1);
        drop(second);
        collect_now();
        assert_eq!(live_count(), 0);
    }

    #[test]
    fn unreachable_cycles_are_reclaimed() {
        reset_heap();
        let left = alloc(Node {
            next: Handle::invalid(),
            value: 1,
        })
        .unwrap();
        let right = alloc(Node {
            next: left.handle(),
            value: 2,
        })
        .unwrap();
        with_mut::<Node, ()>(left.handle(), site(), |node| {
            node.next = right.handle();
        })
        .unwrap();
        assert_eq!(live_count(), 2);
        drop(left);
        drop(right);
        collect_now();
        assert_eq!(live_count(), 0);
    }

    #[test]
    fn nested_handles_stay_reachable_through_their_parent() {
        reset_heap();
        let child = alloc(Node {
            next: Handle::invalid(),
            value: 7,
        })
        .unwrap();
        let parent = alloc(Node {
            next: child.handle(),
            value: 1,
        })
        .unwrap();
        drop(child);
        collect_now();
        assert_eq!(live_count(), 2);
        let nested = with::<Node, Result<i64, RuntimeError>>(parent.handle(), |node| {
            with::<Node, i64>(node.next, |child| child.value)
        });
        // `with` cannot call `with` because the heap is borrowed.
        assert!(nested.unwrap().is_err());
        let child_handle = with::<Node, Handle>(parent.handle(), |node| node.next).unwrap();
        let value = with::<Node, i64>(child_handle, |child| child.value).unwrap();
        assert_eq!(value, 7);
        drop(parent);
        collect_now();
        assert_eq!(live_count(), 0);
    }

    #[test]
    fn failed_constructor_does_not_keep_the_object() {
        reset_heap();
        let failed: Result<Rooted, RuntimeError> = (|| {
            let object = alloc(Node {
                next: Handle::invalid(),
                value: 1,
            })?;
            let _ = object;
            Err(diagnostic::runtime_internal("constructor failed"))
        })();
        assert!(failed.is_err());
        collect_now();
        assert_eq!(live_count(), 0);
    }

    #[test]
    fn freeze_is_cycle_safe_and_rejects_mutation() {
        reset_heap();
        let left = alloc(Node {
            next: Handle::invalid(),
            value: 1,
        })
        .unwrap();
        let right = alloc(Node {
            next: left.handle(),
            value: 2,
        })
        .unwrap();
        with_mut::<Node, ()>(left.handle(), site(), |node| {
            node.next = right.handle();
        })
        .unwrap();
        freeze(left.handle());
        assert!(is_frozen(left.handle()));
        assert!(is_frozen(right.handle()));
        let error = with_mut::<Node, ()>(right.handle(), site(), |node| {
            node.value = 3;
        })
        .unwrap_err();
        assert_eq!(error.kind_name(), "ConstViolationError");
        assert_eq!(error.code, "GRF-RUN-014");
        assert_eq!(error.message_text(), "mutation of frozen value");
        let value = with::<Node, i64>(right.handle(), |node| node.value).unwrap();
        assert_eq!(value, 2);
        drop(left);
        drop(right);
        collect_now();
        assert_eq!(live_count(), 0);
    }
}
