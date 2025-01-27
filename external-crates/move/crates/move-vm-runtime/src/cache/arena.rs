// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(unsafe_code)]

use std::alloc::Layout;

use bumpalo::Bump;
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::vm_status::StatusCode;

// -------------------------------------------------------------------------------------------------
// Types - Arenas for Cache Allocations
// -------------------------------------------------------------------------------------------------

pub struct Arena(Bump);

/// Size of a given arena -- a loaded package must fit in this.
const ARENA_SIZE: usize = 10_000_000;

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
        let bump = Bump::new();
        bump.set_allocation_limit(Some(ARENA_SIZE));
        Arena(bump)
    }

    /// SAFETY: it is the caller's responsibility to ensure that `self` is not shared across
    /// threads during this call. This should be fine as the translation step that uses an arena
    /// should happen in a thread that holds that arena, with no other contention for allocation
    /// into it, and nothing should allocate into a LoadedModule after it is loaded.
    pub fn alloc_slice<T>(&self, items: Vec<T>) -> PartialVMResult<*mut [T]> {
        let len = items.len();
        let layout = Layout::array::<T>(len).map_err(|_| {
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message("Could not compute type layout".to_string())
        })?;

        // Allocate memory for the slice
        if let Ok(ptr) = self.0.try_alloc_layout(layout) {
            unsafe {
                let slice_ptr = ptr.as_ptr() as *mut T;

                // Move the items into the allocated memory
                std::ptr::copy_nonoverlapping(items.as_ptr(), slice_ptr, len);

                // Prevent the original vector from dropping its data
                std::mem::forget(items);

                // Return the raw pointer to the allocated slice
                Ok(std::slice::from_raw_parts_mut(slice_ptr, len) as *mut [T])
            }
        } else {
            Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Could not allocate memory for slice".to_string()),
            )
        }
    }

    /// SAFETY: it is the caller's responsibility to ensure that `self` is not shared across
    /// threads during this call. This should be fine as the translation step that uses an arena
    /// should happen in a thread that holds that arena, with no other contention for allocation
    /// into it, and nothing should allocate into a LoadedModule after it is loaded.
    pub fn alloc_item<T>(&self, item: T) -> PartialVMResult<*mut T> {
        if let Ok(ptr) = self.0.try_alloc(item) {
            Ok(ptr as *mut T)
        } else {
            Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Could not allocate memory for slice".to_string()),
            )
        }
    }
}

// -----------------------------------------------
// Trait Implementations
// -----------------------------------------------

// SAFETY: these are okay, if callers follow the documented safety requirements for `Arena`'s
// unsafe methods.

unsafe impl Send for Arena {}
unsafe impl Sync for Arena {}
