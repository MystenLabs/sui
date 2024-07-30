// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Arena Definitions

use std::mem::MaybeUninit;

use bumpalo::Bump;

pub struct Arena(Bump);

impl Arena {
    pub fn new() -> Self {
        Arena(Bump::new())
    }

    /// SAFETY: it is the caller's responsibility to ensure that `self` is not shared across
    /// threads during this call. This should be fine as the loader should run in lock-isoltion and
    /// nothing should allocate into a LoadedModule after it is loaded.
    pub fn alloc_slice<T>(&self, items: impl ExactSizeIterator<Item = T>) -> *mut [T] {
        let slice = self.0.alloc_slice_fill_iter(items);
        slice as *mut [T]
    }
}

// SAFETY: these are okay, if callers follow the documented safety requirements for `Arena`'s
// unsafe methods.
unsafe impl Send for Arena {}
unsafe impl Sync for Arena {}

/// Returns a pointer to a slice, but nulled. This must be set before use.
pub fn null_ptr<T>() -> *const [T] {
    unsafe { MaybeUninit::<*const [T]>::zeroed().assume_init() }
}

pub fn ref_slice<'a, T>(value: *const [T]) -> &'a [T] {
    unsafe { &*value }
}

pub fn mut_to_ref_slice<'a, T>(value: *mut [T]) -> &'a [T] {
    unsafe { &*value }
}

pub fn to_mut_ref_slice<'a, T>(value: *mut [T]) -> &'a mut [T] {
    unsafe { &mut *value }
}

pub fn to_ref<'a, T>(value: *const T) -> &'a T {
    unsafe { &*value as &T }
}

#[derive(Clone, Copy)]
pub struct ArenaPointer<T>(*const T);

impl<T> ArenaPointer<T> {
    pub fn new(value: *const T) -> Self {
        ArenaPointer(value)
    }

    pub fn to_const(&self) -> *const T {
        self.0
    }

    pub fn to_ref<'a>(&self) -> &'a T {
        to_ref(self.0)
    }
}

impl<T: ::std::fmt::Debug> ::std::fmt::Debug for ArenaPointer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ptr->{:?}", to_ref(self.0))
    }
}

unsafe impl<T> Send for ArenaPointer<T> {}
unsafe impl<T> Sync for ArenaPointer<T> {}
