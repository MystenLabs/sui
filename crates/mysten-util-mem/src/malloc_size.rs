// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright 2016-2017 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A crate for measuring the heap usage of data structures in a way that
//! integrates with Firefox's memory reporting, particularly the use of
//! mozjemalloc and DMD. In particular, it has the following features.
//! - It isn't bound to a particular heap allocator.
//! - It provides traits for both "shallow" and "deep" measurement, which gives
//!   flexibility in the cases where the traits can't be used.
//! - It allows for measuring blocks even when only an interior pointer can be
//!   obtained for heap allocations, e.g. `HashSet` and `HashMap`. (This relies
//!   on the heap allocator having suitable support, which mozjemalloc has.)
//! - It allows handling of types like `Rc` and `Arc` by providing traits that
//!   are different to the ones for non-graph structures.
//!
//! Suggested uses are as follows.
//! - When possible, use the `MallocSizeOf` trait. (Deriving support is
//!   provided by the `malloc_size_of_derive` crate.)
//! - If you need an additional synchronization argument, provide a function
//!   that is like the standard trait method, but with the extra argument.
//! - If you need multiple measurements for a type, provide a function named
//!   `add_size_of` that takes a mutable reference to a struct that contains
//!   the multiple measurement fields.
//! - When deep measurement (via `MallocSizeOf`) cannot be implemented for a
//!   type, shallow measurement (via `MallocShallowSizeOf`) in combination with
//!   iteration can be a useful substitute.
//! - `Rc` and `Arc` are always tricky, which is why `MallocSizeOf` is not (and
//!   should not be) implemented for them.
//! - If an `Rc` or `Arc` is known to be a "primary" reference and can always
//!   be measured, it should be measured via the `MallocUnconditionalSizeOf`
//!   trait.
//! - Using universal function call syntax is a good idea when measuring boxed
//!   fields in structs, because it makes it clear that the Box is being
//!   measured as well as the thing it points to. E.g.
//!   `<Box<_> as MallocSizeOf>::size_of(field, ops)`.

//! This is an extended version of the Servo internal malloc_size crate.
//! We should occasionally track the upstream changes/fixes and reintroduce them here, whenever applicable.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
#[cfg(feature = "std")]
mod rstd {
    pub use std::*;
}
#[cfg(not(feature = "std"))]
mod rstd {
    pub use core::*;
    pub mod collections {
        pub use alloc::collections::*;
        pub use vec_deque::VecDeque;
    }
}

#[cfg(feature = "std")]
use std::sync::Arc;

#[cfg(not(feature = "std"))]
pub use alloc::boxed::Box;
#[cfg(not(feature = "std"))]
use core::ffi::c_void;
#[cfg(feature = "std")]
use rstd::hash::Hash;
use rstd::{
    marker::PhantomData,
    mem::size_of,
    ops::{Deref, DerefMut, Range},
};
#[cfg(feature = "std")]
use std::hash::BuildHasher;
#[cfg(feature = "std")]
use std::os::raw::c_void;

/// A C function that takes a pointer to a heap allocation and returns its size.
pub type VoidPtrToSizeFn = unsafe extern "C" fn(ptr: *const c_void) -> usize;

/// A closure implementing a stateful predicate on pointers.
pub type VoidPtrToBoolFnMut = dyn FnMut(*const c_void) -> bool;

/// Operations used when measuring heap usage of data structures.
pub struct MallocSizeOfOps {
    /// A function that returns the size of a heap allocation.
    size_of_op: VoidPtrToSizeFn,

    /// Like `size_of_op`, but can take an interior pointer. Optional because
    /// not all allocators support this operation. If it's not provided, some
    /// memory measurements will actually be computed estimates rather than
    /// real and accurate measurements.
    enclosing_size_of_op: Option<VoidPtrToSizeFn>,

    /// Check if a pointer has been seen before, and remember it for next time.
    /// Useful when measuring `Rc`s and `Arc`s. Optional, because many places
    /// don't need it.
    have_seen_ptr_op: Option<Box<VoidPtrToBoolFnMut>>,
}

impl MallocSizeOfOps {
    pub fn new(
        size_of: VoidPtrToSizeFn,
        malloc_enclosing_size_of: Option<VoidPtrToSizeFn>,
        have_seen_ptr: Option<Box<VoidPtrToBoolFnMut>>,
    ) -> Self {
        MallocSizeOfOps {
            size_of_op: size_of,
            enclosing_size_of_op: malloc_enclosing_size_of,
            have_seen_ptr_op: have_seen_ptr,
        }
    }

    /// Check if an allocation is empty. This relies on knowledge of how Rust
    /// handles empty allocations, which may change in the future.
    fn is_empty<T: ?Sized>(ptr: *const T) -> bool {
        // The correct condition is this:
        //   `ptr as usize <= ::std::mem::align_of::<T>()`
        // But we can't call align_of() on a ?Sized T. So we approximate it
        // with the following. 256 is large enough that it should always be
        // larger than the required alignment, but small enough that it is
        // always in the first page of memory and therefore not a legitimate
        // address.
        return ptr as *const usize as usize <= 256;
    }

    /// Call `size_of_op` on `ptr`, first checking that the allocation isn't
    /// empty, because some types (such as `Vec`) utilize empty allocations.
    pub unsafe fn malloc_size_of<T: ?Sized>(&self, ptr: *const T) -> usize {
        if MallocSizeOfOps::is_empty(ptr) {
            0
        } else {
            (self.size_of_op)(ptr as *const c_void)
        }
    }

    /// Is an `enclosing_size_of_op` available?
    pub fn has_malloc_enclosing_size_of(&self) -> bool {
        self.enclosing_size_of_op.is_some()
    }

    /// Call `enclosing_size_of_op`, which must be available, on `ptr`, which
    /// must not be empty.
    pub unsafe fn malloc_enclosing_size_of<T>(&self, ptr: *const T) -> usize {
        assert!(!MallocSizeOfOps::is_empty(ptr));
        (self.enclosing_size_of_op.unwrap())(ptr as *const c_void)
    }

    /// Call `have_seen_ptr_op` on `ptr`.
    pub fn have_seen_ptr<T>(&mut self, ptr: *const T) -> bool {
        let have_seen_ptr_op = self
            .have_seen_ptr_op
            .as_mut()
            .expect("missing have_seen_ptr_op");
        have_seen_ptr_op(ptr as *const c_void)
    }
}

/// Trait for measuring the "deep" heap usage of a data structure. This is the
/// most commonly-used of the traits.
pub trait MallocSizeOf {
    /// Measure the heap usage of all descendant heap-allocated structures, but
    /// not the space taken up by the value itself.
    /// If `T::size_of` is a constant, consider implementing `constant_size` as well.
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize;

    /// Used to optimize `MallocSizeOf` implementation for collections
    /// like `Vec` and `HashMap` to avoid iterating over them unnecessarily.
    /// The `Self: Sized` bound is for object safety.
    fn constant_size() -> Option<usize>
    where
        Self: Sized,
    {
        None
    }
}

/// Trait for measuring the "shallow" heap usage of a container.
pub trait MallocShallowSizeOf {
    /// Measure the heap usage of immediate heap-allocated descendant
    /// structures, but not the space taken up by the value itself. Anything
    /// beyond the immediate descendants must be measured separately, using
    /// iteration.
    fn shallow_size_of(&self, ops: &mut MallocSizeOfOps) -> usize;
}

/// Like `MallocSizeOf`, but with a different name so it cannot be used
/// accidentally with derive(MallocSizeOf). For use with types like `Rc` and
/// `Arc` when appropriate (e.g. when measuring a "primary" reference).
pub trait MallocUnconditionalSizeOf {
    /// Measure the heap usage of all heap-allocated descendant structures, but
    /// not the space taken up by the value itself.
    fn unconditional_size_of(&self, ops: &mut MallocSizeOfOps) -> usize;
}

/// `MallocUnconditionalSizeOf` combined with `MallocShallowSizeOf`.
pub trait MallocUnconditionalShallowSizeOf {
    /// `unconditional_size_of` combined with `shallow_size_of`.
    fn unconditional_shallow_size_of(&self, ops: &mut MallocSizeOfOps) -> usize;
}

impl<'a, T: ?Sized> MallocSizeOf for &'a T {
    fn size_of(&self, _ops: &mut MallocSizeOfOps) -> usize {
        // Zero makes sense for a non-owning reference.
        0
    }
    fn constant_size() -> Option<usize> {
        Some(0)
    }
}

impl<T: MallocSizeOf + ?Sized> MallocSizeOf for Box<T> {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        self.shallow_size_of(ops) + (**self).size_of(ops)
    }
}

#[impl_trait_for_tuples::impl_for_tuples(12)]
impl MallocSizeOf for Tuple {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        let mut result = 0;
        for_tuples!( #( result += Tuple.size_of(ops); )* );
        result
    }
    fn constant_size() -> Option<usize> {
        let mut result = Some(0);
        for_tuples!( #( result = result.and_then(|s| Tuple::constant_size().map(|t| s + t)); )* );
        result
    }
}

impl<T: MallocSizeOf> MallocSizeOf for Option<T> {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        if let Some(val) = self.as_ref() {
            val.size_of(ops)
        } else {
            0
        }
    }
    fn constant_size() -> Option<usize> {
        T::constant_size().filter(|s| *s == 0)
    }
}

impl<T: MallocSizeOf, E: MallocSizeOf> MallocSizeOf for Result<T, E> {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        match *self {
            Ok(ref x) => x.size_of(ops),
            Err(ref e) => e.size_of(ops),
        }
    }
    fn constant_size() -> Option<usize> {
        // Result<T, E> has constant size iff T::constant_size == E::constant_size
        T::constant_size().and_then(|t| E::constant_size().filter(|e| *e == t))
    }
}

impl<T: MallocSizeOf + Copy> MallocSizeOf for rstd::cell::Cell<T> {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        self.get().size_of(ops)
    }
    fn constant_size() -> Option<usize> {
        T::constant_size()
    }
}

impl<T: MallocSizeOf> MallocSizeOf for rstd::cell::RefCell<T> {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        self.borrow().size_of(ops)
    }
    fn constant_size() -> Option<usize> {
        T::constant_size()
    }
}

#[cfg(feature = "std")]
impl<'a, B: ?Sized + ToOwned> MallocSizeOf for std::borrow::Cow<'a, B>
where
    B::Owned: MallocSizeOf,
{
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        match *self {
            std::borrow::Cow::Borrowed(_) => 0,
            std::borrow::Cow::Owned(ref b) => b.size_of(ops),
        }
    }
}

impl<T: MallocSizeOf> MallocSizeOf for [T] {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        let mut n = 0;
        if let Some(t) = T::constant_size() {
            n += self.len() * t;
        } else {
            n = self.iter().fold(n, |acc, elem| acc + elem.size_of(ops))
        }
        n
    }
}

impl<T: MallocSizeOf> MallocSizeOf for Vec<T> {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        let mut n = self.shallow_size_of(ops);
        if let Some(t) = T::constant_size() {
            n += self.len() * t;
        } else {
            n = self.iter().fold(n, |acc, elem| acc + elem.size_of(ops))
        }
        n
    }
}

impl<T> MallocShallowSizeOf for rstd::collections::VecDeque<T> {
    fn shallow_size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        if ops.has_malloc_enclosing_size_of() {
            if let Some(front) = self.front() {
                // The front element is an interior pointer.
                unsafe { ops.malloc_enclosing_size_of(&*front) }
            } else {
                // This assumes that no memory is allocated when the VecDeque is empty.
                0
            }
        } else {
            // An estimate.
            self.capacity() * size_of::<T>()
        }
    }
}

impl<T: MallocSizeOf> MallocSizeOf for rstd::collections::VecDeque<T> {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        let mut n = self.shallow_size_of(ops);
        if let Some(t) = T::constant_size() {
            n += self.len() * t;
        } else {
            n = self.iter().fold(n, |acc, elem| acc + elem.size_of(ops))
        }
        n
    }
}

#[cfg(feature = "std")]
impl<T, S> MallocShallowSizeOf for std::collections::HashSet<T, S>
where
    T: Eq + Hash,
    S: BuildHasher,
{
    fn shallow_size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        if ops.has_malloc_enclosing_size_of() {
            // The first value from the iterator gives us an interior pointer.
            // `ops.malloc_enclosing_size_of()` then gives us the storage size.
            // This assumes that the `HashSet`'s contents (values and hashes)
            // are all stored in a single contiguous heap allocation.
            self.iter()
                .next()
                .map_or(0, |t| unsafe { ops.malloc_enclosing_size_of(t) })
        } else {
            // An estimate.
            self.capacity() * (size_of::<T>() + size_of::<usize>())
        }
    }
}

#[cfg(feature = "std")]
impl<T, S> MallocSizeOf for std::collections::HashSet<T, S>
where
    T: Eq + Hash + MallocSizeOf,
    S: BuildHasher,
{
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        let mut n = self.shallow_size_of(ops);
        if let Some(t) = T::constant_size() {
            n += self.len() * t;
        } else {
            n = self.iter().fold(n, |acc, elem| acc + elem.size_of(ops))
        }
        n
    }
}

impl<I: MallocSizeOf> MallocSizeOf for rstd::cmp::Reverse<I> {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        self.0.size_of(ops)
    }
    fn constant_size() -> Option<usize> {
        I::constant_size()
    }
}

#[cfg(feature = "std")]
impl<K, V, S> MallocShallowSizeOf for std::collections::HashMap<K, V, S> {
    fn shallow_size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        // See the implementation for std::collections::HashSet for details.
        if ops.has_malloc_enclosing_size_of() {
            self.values()
                .next()
                .map_or(0, |v| unsafe { ops.malloc_enclosing_size_of(v) })
        } else {
            self.capacity() * (size_of::<V>() + size_of::<K>() + size_of::<usize>())
        }
    }
}

#[cfg(feature = "std")]
impl<K, V, S> MallocSizeOf for std::collections::HashMap<K, V, S>
where
    K: MallocSizeOf,
    V: MallocSizeOf,
{
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        let mut n = self.shallow_size_of(ops);
        if let (Some(k), Some(v)) = (K::constant_size(), V::constant_size()) {
            n += self.len() * (k + v)
        } else {
            n = self
                .iter()
                .fold(n, |acc, (k, v)| acc + k.size_of(ops) + v.size_of(ops))
        }
        n
    }
}

impl<K, V> MallocShallowSizeOf for rstd::collections::BTreeMap<K, V> {
    fn shallow_size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        if ops.has_malloc_enclosing_size_of() {
            self.values()
                .next()
                .map_or(0, |v| unsafe { ops.malloc_enclosing_size_of(v) })
        } else {
            self.len() * (size_of::<V>() + size_of::<K>() + size_of::<usize>())
        }
    }
}

impl<K, V> MallocSizeOf for rstd::collections::BTreeMap<K, V>
where
    K: MallocSizeOf,
    V: MallocSizeOf,
{
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        let mut n = self.shallow_size_of(ops);
        if let (Some(k), Some(v)) = (K::constant_size(), V::constant_size()) {
            n += self.len() * (k + v)
        } else {
            n = self
                .iter()
                .fold(n, |acc, (k, v)| acc + k.size_of(ops) + v.size_of(ops))
        }
        n
    }
}

impl<T> MallocShallowSizeOf for rstd::collections::BTreeSet<T> {
    fn shallow_size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        if ops.has_malloc_enclosing_size_of() {
            // See implementation for HashSet how this works.
            self.iter()
                .next()
                .map_or(0, |t| unsafe { ops.malloc_enclosing_size_of(t) })
        } else {
            // An estimate.
            self.len() * (size_of::<T>() + size_of::<usize>())
        }
    }
}

impl<T> MallocSizeOf for rstd::collections::BTreeSet<T>
where
    T: MallocSizeOf,
{
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        let mut n = self.shallow_size_of(ops);
        if let Some(t) = T::constant_size() {
            n += self.len() * t;
        } else {
            n = self.iter().fold(n, |acc, elem| acc + elem.size_of(ops))
        }
        n
    }
}

// XXX: we don't want MallocSizeOf to be defined for Rc and Arc. If negative
// trait bounds are ever allowed, this code should be uncommented.
// (We do have a compile-fail test for this:
// rc_arc_must_not_derive_malloc_size_of.rs)
// impl<T> !MallocSizeOf for Arc<T> { }
// impl<T> !MallocShallowSizeOf for Arc<T> { }

#[cfg(feature = "std")]
impl<T: MallocSizeOf> MallocUnconditionalSizeOf for Arc<T> {
    fn unconditional_size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        self.unconditional_shallow_size_of(ops) + (**self).size_of(ops)
    }
}

/// If a mutex is stored directly as a member of a data type that is being measured,
/// it is the unique owner of its contents and deserves to be measured.
///
/// If a mutex is stored inside of an Arc value as a member of a data type that is being measured,
/// the Arc will not be automatically measured so there is no risk of overcounting the mutex's
/// contents.
///
/// The same reasoning applies to RwLock.
#[cfg(feature = "std")]
impl<T: MallocSizeOf> MallocSizeOf for std::sync::Mutex<T> {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        self.lock().unwrap().size_of(ops)
    }
}

#[cfg(feature = "std")]
impl<T: MallocSizeOf> MallocSizeOf for parking_lot::Mutex<T> {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        self.lock().size_of(ops)
    }
}

#[cfg(feature = "std")]
impl<T: MallocSizeOf> MallocSizeOf for once_cell::sync::OnceCell<T> {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        self.get().map(|v| v.size_of(ops)).unwrap_or(0)
    }
}

#[cfg(feature = "std")]
impl<T: MallocSizeOf> MallocSizeOf for std::sync::RwLock<T> {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        self.read().unwrap().size_of(ops)
    }
}

#[cfg(feature = "std")]
impl<T: MallocSizeOf> MallocSizeOf for parking_lot::RwLock<T> {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        self.read().size_of(ops)
    }
}

/// Implement notion of 0 allocation size for some type(s).
///
/// if used for generics, by default it will require that generaic arguments
/// should implement `MallocSizeOf`. This can be avoided with passing "any: "
/// in front of type list.
///
/// ```rust
/// use mysten_util_mem::{malloc_size, malloc_size_of_is_0};
///
/// struct Data<P> {
/// 	phantom: std::marker::PhantomData<P>,
/// }
///
/// malloc_size_of_is_0!(any: Data<P>);
///
/// // MallocSizeOf is NOT implemented for [u8; 333]
/// assert_eq!(malloc_size(&Data::<[u8; 333]> { phantom: std::marker::PhantomData }), 0);
/// ```
///
/// and when no "any: "
///
/// ```rust
/// use mysten_util_mem::{malloc_size, malloc_size_of_is_0};
///
/// struct Data<T>(pub T);
///
/// // generic argument (`T`) must be `impl MallocSizeOf`
/// malloc_size_of_is_0!(Data<u8>);
///
/// assert_eq!(malloc_size(&Data(0u8)), 0);
/// ```
#[macro_export]
macro_rules! malloc_size_of_is_0(
	($($ty:ty),+) => (
		$(
			impl $crate::MallocSizeOf for $ty {
				#[inline(always)]
				fn size_of(&self, _: &mut $crate::MallocSizeOfOps) -> usize {
					0
				}
				#[inline(always)]
				fn constant_size() -> Option<usize> { Some(0) }
			}
		)+
	);
	(any: $($ty:ident<$($gen:ident),+>),+) => (
		$(
			impl<$($gen),+> $crate::MallocSizeOf for $ty<$($gen),+> {
				#[inline(always)]
				fn size_of(&self, _: &mut $crate::MallocSizeOfOps) -> usize {
					0
				}
				#[inline(always)]
				fn constant_size() -> Option<usize> { Some(0) }
			}
		)+
	);
	($($ty:ident<$($gen:ident),+>),+) => (
		$(
			impl<$($gen: $crate::MallocSizeOf),+> $crate::MallocSizeOf for $ty<$($gen),+> {
				#[inline(always)]
				fn size_of(&self, _: &mut $crate::MallocSizeOfOps) -> usize {
					0
				}
				#[inline(always)]
				fn constant_size() -> Option<usize> { Some(0) }
			}
		)+
	);
);

malloc_size_of_is_0!(bool, char, str);
malloc_size_of_is_0!(u8, u16, u32, u64, u128, usize);
malloc_size_of_is_0!(i8, i16, i32, i64, i128, isize);
malloc_size_of_is_0!(f32, f64);

malloc_size_of_is_0!(rstd::sync::atomic::AtomicBool);
malloc_size_of_is_0!(rstd::sync::atomic::AtomicIsize);
malloc_size_of_is_0!(rstd::sync::atomic::AtomicUsize);

malloc_size_of_is_0!(Range<u8>, Range<u16>, Range<u32>, Range<u64>, Range<usize>);
malloc_size_of_is_0!(Range<i8>, Range<i16>, Range<i32>, Range<i64>, Range<isize>);
malloc_size_of_is_0!(Range<f32>, Range<f64>);
malloc_size_of_is_0!(any: PhantomData<T>);

/// Measurable that defers to inner value and used to verify MallocSizeOf implementation in a
/// struct.
#[derive(Clone)]
pub struct Measurable<T: MallocSizeOf>(pub T);

impl<T: MallocSizeOf> Deref for Measurable<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: MallocSizeOf> DerefMut for Measurable<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

#[cfg(feature = "hashbrown")]
impl<K, V, S> MallocShallowSizeOf for hashbrown::HashMap<K, V, S> {
    fn shallow_size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        // See the implementation for std::collections::HashSet for details.
        if ops.has_malloc_enclosing_size_of() {
            self.values()
                .next()
                .map_or(0, |v| unsafe { ops.malloc_enclosing_size_of(v) })
        } else {
            self.capacity() * (size_of::<V>() + size_of::<K>() + size_of::<usize>())
        }
    }
}

#[cfg(feature = "hashbrown")]
impl<K, V, S> MallocSizeOf for hashbrown::HashMap<K, V, S>
where
    K: MallocSizeOf,
    V: MallocSizeOf,
{
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        let mut n = self.shallow_size_of(ops);
        if let (Some(k), Some(v)) = (K::constant_size(), V::constant_size()) {
            n += self.len() * (k + v)
        } else {
            n = self
                .iter()
                .fold(n, |acc, (k, v)| acc + k.size_of(ops) + v.size_of(ops))
        }
        n
    }
}

malloc_size_of_is_0!(
    [u8; 1], [u8; 2], [u8; 3], [u8; 4], [u8; 5], [u8; 6], [u8; 7], [u8; 8], [u8; 9], [u8; 10],
    [u8; 11], [u8; 12], [u8; 13], [u8; 14], [u8; 15], [u8; 16], [u8; 17], [u8; 18], [u8; 19],
    [u8; 20], [u8; 21], [u8; 22], [u8; 23], [u8; 24], [u8; 25], [u8; 26], [u8; 27], [u8; 28],
    [u8; 29], [u8; 30], [u8; 31], [u8; 32]
);

macro_rules! impl_smallvec {
    ($size: expr) => {
        #[cfg(feature = "smallvec")]
        impl<T> MallocSizeOf for smallvec::SmallVec<[T; $size]>
        where
            T: MallocSizeOf,
        {
            fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
                let mut n = if self.spilled() {
                    self.capacity() * core::mem::size_of::<T>()
                } else {
                    0
                };
                if let Some(t) = T::constant_size() {
                    n += self.len() * t;
                } else {
                    n = self.iter().fold(n, |acc, elem| acc + elem.size_of(ops))
                }
                n
            }
        }
    };
}

impl_smallvec!(32); // kvdb uses this
impl_smallvec!(36); // trie-db uses this

#[cfg(feature = "std")]
malloc_size_of_is_0!(std::time::Instant);
#[cfg(feature = "std")]
malloc_size_of_is_0!(std::time::Duration);

#[cfg(all(test, feature = "std"))] // tests are using std implementations
mod tests {
    use crate::{allocators::new_malloc_size_ops, MallocSizeOf, MallocSizeOfOps};
    use smallvec::SmallVec;
    use std::{collections::BTreeSet, mem};
    impl_smallvec!(3);

    #[test]
    fn test_smallvec_stack_allocated_type() {
        let mut v: SmallVec<[u8; 3]> = SmallVec::new();
        let mut ops = new_malloc_size_ops();
        assert_eq!(v.size_of(&mut ops), 0);
        v.push(1);
        v.push(2);
        v.push(3);
        assert_eq!(v.size_of(&mut ops), 0);
        assert!(!v.spilled());
        v.push(4);
        assert!(
            v.spilled(),
            "SmallVec spills when going beyond the capacity of the inner backing array"
        );
        assert_eq!(v.size_of(&mut ops), 4); // 4 u8s on the heap
    }

    #[test]
    fn test_smallvec_boxed_stack_allocated_type() {
        let mut v: SmallVec<[Box<u8>; 3]> = SmallVec::new();
        let mut ops = new_malloc_size_ops();
        assert_eq!(v.size_of(&mut ops), 0);
        v.push(Box::new(1u8));
        v.push(Box::new(2u8));
        v.push(Box::new(3u8));
        assert!(v.size_of(&mut ops) >= 3);
        assert!(!v.spilled());
        v.push(Box::new(4u8));
        assert!(
            v.spilled(),
            "SmallVec spills when going beyond the capacity of the inner backing array"
        );
        let mut ops = new_malloc_size_ops();
        let expected_min_allocs = mem::size_of::<Box<u8>>() * 4 + 4;
        assert!(v.size_of(&mut ops) >= expected_min_allocs);
    }

    #[test]
    fn test_smallvec_heap_allocated_type() {
        let mut v: SmallVec<[String; 3]> = SmallVec::new();
        let mut ops = new_malloc_size_ops();
        assert_eq!(v.size_of(&mut ops), 0);
        v.push("COW".into());
        v.push("PIG".into());
        v.push("DUCK".into());
        assert!(!v.spilled());
        assert!(v.size_of(&mut ops) >= "COW".len() + "PIG".len() + "DUCK".len());
        v.push("ÖWL".into());
        assert!(v.spilled());
        let mut ops = new_malloc_size_ops();
        let expected_min_allocs =
            mem::size_of::<String>() * 4 + "ÖWL".len() + "COW".len() + "PIG".len() + "DUCK".len();
        assert!(v.size_of(&mut ops) >= expected_min_allocs);
    }

    #[test]
    fn test_large_vec() {
        const N: usize = 128 * 1024 * 1024;
        let val = vec![1u8; N];
        let mut ops = new_malloc_size_ops();
        assert!(val.size_of(&mut ops) >= N);
        assert!(val.size_of(&mut ops) < 2 * N);
    }

    #[test]
    fn btree_set() {
        let mut set = BTreeSet::new();
        for t in 0..100 {
            set.insert(vec![t]);
        }
        // ~36 per value
        assert!(crate::malloc_size(&set) > 3000);
    }

    #[test]
    fn special_malloc_size_of_0() {
        struct Data<P> {
            phantom: std::marker::PhantomData<P>,
        }

        malloc_size_of_is_0!(any: Data<P>);

        // MallocSizeOf is not implemented for [u8; 333]
        assert_eq!(
            crate::malloc_size(&Data::<[u8; 333]> {
                phantom: std::marker::PhantomData
            }),
            0
        );
    }

    #[test]
    fn constant_size() {
        struct AlwaysTwo(Vec<u8>);

        impl MallocSizeOf for AlwaysTwo {
            fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
                self.0.size_of(ops)
            }
            fn constant_size() -> Option<usize> {
                Some(2)
            }
        }

        assert_eq!(AlwaysTwo::constant_size(), Some(2));
        assert_eq!(std::cmp::Reverse::<u8>::constant_size(), Some(0));
        assert_eq!(std::cell::RefCell::<u8>::constant_size(), Some(0));
        assert_eq!(std::cell::Cell::<u8>::constant_size(), Some(0));
        assert_eq!(Result::<(), ()>::constant_size(), Some(0));
        assert_eq!(
            <(AlwaysTwo, (), [u8; 32], AlwaysTwo)>::constant_size(),
            Some(2 + 2)
        );
        assert_eq!(Option::<u8>::constant_size(), Some(0));
        assert_eq!(<&String>::constant_size(), Some(0));

        assert_eq!(<String>::constant_size(), None);
        assert_eq!(std::borrow::Cow::<String>::constant_size(), None);
        assert_eq!(Result::<(), String>::constant_size(), None);
        assert_eq!(Option::<AlwaysTwo>::constant_size(), None);
    }
}
