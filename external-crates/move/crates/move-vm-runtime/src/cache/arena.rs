// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(unsafe_code)]

use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::vm_status::StatusCode;

use bumpalo::Bump;

// -------------------------------------------------------------------------------------------------
// Types - Arenas for Cache Allocations
// -------------------------------------------------------------------------------------------------

pub struct Arena(Bump);

/// Size of a package arena.
/// This is 10 megabytes, which should be more than enough room for any pacakge on chain.
/// FIXME: Test this limit and validate. See how large packages are in backtesting and double that
/// limit, setting it here.
const ARENA_SIZE: usize = 10_000_000;

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl Default for Arena {
    fn default() -> Self {
        let bump = Bump::new();
        bump.set_allocation_limit(Some(ARENA_SIZE));
        Arena(bump)
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
    pub fn alloc_slice<T>(
        &self,
        items: impl ExactSizeIterator<Item = T>,
    ) -> PartialVMResult<*mut [T]> {
        if let Ok(slice) = self.0.try_alloc_slice_fill_iter(items) {
            Ok(slice)
        } else {
            Err(PartialVMError::new(StatusCode::PACKAGE_ARENA_LIMIT_REACHED))
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
