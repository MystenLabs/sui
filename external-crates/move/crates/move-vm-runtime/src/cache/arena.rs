// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(unsafe_code)]

use std::mem::MaybeUninit;

use bumpalo::Bump;

// -------------------------------------------------------------------------------------------------
// ARENA DEFINITIONS
// -------------------------------------------------------------------------------------------------

// -----------------------------------------------
// Types
// -----------------------------------------------

pub struct Arena(Bump);

/// An Arena Pointer, which allows conversion to references and const. Equality is defined as
/// pointer equality, and clone/copy are shallow.
pub struct ArenaPointer<T>(*const T);

// -----------------------------------------------
// Impls
// -----------------------------------------------

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

impl Arena {
    pub fn new() -> Self {
        Arena(Bump::new())
    }

    /// SAFETY: it is the caller's responsibility to ensure that `self` is not shared across
    /// threads during this call. This should be fine as the translation step that uses an arena
    /// should happen in a thread that holds that arena, with no other contention for allocation
    /// into it, and nothing should allocate into a LoadedModule after it is loaded.
    pub fn alloc_slice<T>(&self, items: impl ExactSizeIterator<Item = T>) -> *mut [T] {
        let slice = self.0.alloc_slice_fill_iter(items);
        slice as *mut [T]
    }
}

impl<T> ArenaPointer<T> {
    #[inline]
    pub fn new(value: *const T) -> Self {
        ArenaPointer(value)
    }

    #[allow(dead_code)]
    #[inline]
    pub fn to_const(self) -> *const T {
        self.0
    }

    #[inline]
    pub fn to_ref<'a>(self) -> &'a T {
        to_ref(self.ptr_clone().0)
    }

    #[inline]
    pub fn from_ref(t: &T) -> Self {
        Self(unsafe { t as *const T })
    }

    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> Self {
        std::ptr::eq(self.0, other.0)
    }

    #[inline]
    pub fn ptr_clone(&self) -> Self {
        Self(self.0)
    }

    #[inline]
    pub fn from_ref(t: &T) -> Self {
        Self(unsafe { t as *const T })
    }
}

// -----------------------------------------------
// Pointer Operations
// -----------------------------------------------

///// Returns a pointer to a slice, but nulled. This must be set before use.
#[inline]
pub fn null_ptr<T>() -> *const [T] {
    unsafe { MaybeUninit::<*const [T]>::zeroed().assume_init() }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[inline]
pub fn ref_slice<'a, T>(value: *const [T]) -> &'a [T] {
    unsafe { &*value }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[inline]
pub fn mut_to_ref_slice<'a, T>(value: *mut [T]) -> &'a [T] {
    unsafe { &*value }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[inline]
pub fn to_mut_ref_slice<'a, T>(value: *mut [T]) -> &'a mut [T] {
    unsafe { &mut *value }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[inline]
pub fn to_ref<'a, T>(value: *const T) -> &'a T {
    unsafe { &*value as &T }
}

// -----------------------------------------------
// Trait Implementations
// -----------------------------------------------

// SAFETY: these are okay, if callers follow the documented safety requirements for `Arena`'s
// unsafe methods.

unsafe impl Send for Arena {}
unsafe impl Sync for Arena {}

unsafe impl<T> Send for ArenaPointer<T> {}
unsafe impl<T> Sync for ArenaPointer<T> {}

impl<T: ::std::fmt::Debug> ::std::fmt::Debug for ArenaPointer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ptr->{:?}", to_ref(self.0))
    }
}

// Pointer equality
impl<T> PartialEq for ArenaPointer<T> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other)
    }
}

impl<T> Eq for ArenaPointer<T> {}

impl<T> Clone for ArenaPointer<T> {
    fn clone(&self) -> Self {
        self.ptr_clone()
    }
}

impl<T> Copy for ArenaPointer<T> {}
