// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright 2020 Parity Technologies
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[cfg(feature = "std")]
use crate::malloc_size::MallocUnconditionalSizeOf;
use crate::malloc_size::{MallocSizeOf, MallocSizeOfOps, VoidPtrToSizeFn};
#[cfg(not(feature = "std"))]
use core::ffi::c_void;
#[cfg(feature = "std")]
use std::os::raw::c_void;

mod usable_size {

    use super::*;

    cfg_if::cfg_if! {

        if #[cfg(any(
            target_arch = "wasm32",
            feature = "estimate-heapsize",
        ))] {

            // do not try system allocator

            /// Warning this is for compatibility only.
            /// This function does panic: `estimate-heapsize` feature needs to be activated
            /// to avoid this function call.
            pub unsafe extern "C" fn malloc_usable_size(_ptr: *const c_void) -> usize {
                unreachable!("estimate heapsize only")
            }

        } else if #[cfg(any(
            target_os = "linux",
            target_os = "android",
            target_os = "freebsd",
        ))] {
            // Linux/BSD call system allocator (currently malloc).
            extern "C" {
                pub fn malloc_usable_size(ptr: *const c_void) -> usize;
            }

        } else {
            // default allocator for non linux or windows system use estimate
            pub unsafe extern "C" fn malloc_usable_size(_ptr: *const c_void) -> usize {
                unreachable!("estimate heapsize or feature allocator needed")
            }

        }

    }

    /// No enclosing function defined.
    #[inline]
    pub fn new_enclosing_size_fn() -> Option<VoidPtrToSizeFn> {
        None
    }
}

/// Get a new instance of a MallocSizeOfOps
pub fn new_malloc_size_ops() -> MallocSizeOfOps {
    MallocSizeOfOps::new(
        usable_size::malloc_usable_size,
        usable_size::new_enclosing_size_fn(),
        None,
    )
}

/// Extension methods for `MallocSizeOf` trait, do not implement
/// directly.
/// It allows getting heapsize without exposing `MallocSizeOfOps`
/// (a single default `MallocSizeOfOps` is used for each call).
pub trait MallocSizeOfExt: MallocSizeOf {
    /// Method to launch a heapsize measurement with a
    /// fresh state.
    fn malloc_size_of(&self) -> usize {
        let mut ops = new_malloc_size_ops();
        <Self as MallocSizeOf>::size_of(self, &mut ops)
    }
}

impl<T: MallocSizeOf> MallocSizeOfExt for T {}

#[cfg(feature = "std")]
impl<T: MallocSizeOf> MallocSizeOf for std::sync::Arc<T> {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        self.unconditional_size_of(ops)
    }
}
