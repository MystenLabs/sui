// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(unsafe_code)]

use bumpalo::Bump;

// -------------------------------------------------------------------------------------------------
// Types - Arenas for Cache Allocations
// -------------------------------------------------------------------------------------------------

pub struct Arena(Bump);

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

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

// -----------------------------------------------
// Trait Implementations
// -----------------------------------------------

// SAFETY: these are okay, if callers follow the documented safety requirements for `Arena`'s
// unsafe methods.

unsafe impl Send for Arena {}
unsafe impl Sync for Arena {}
