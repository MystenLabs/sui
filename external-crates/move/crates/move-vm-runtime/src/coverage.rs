// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! AFL-style edge-coverage bitmap for the Move VM.
//!
//! When the `coverage` feature is enabled, every executed instruction records
//! an edge in a 64 KB bitmap. The fuzzer installs a fresh bitmap before each
//! execution, then retrieves it afterwards to detect new code paths.
//!
//! When the feature is disabled every public function is a no-op, so there is
//! zero runtime cost for normal (non-fuzzing) operation.

#[cfg(feature = "coverage")]
mod inner {
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicU8, Ordering};

    /// 64 KB edge-coverage bitmap.
    ///
    /// Each byte at index `i` counts how many times edge `i` was taken.
    pub struct CoverageBitmap {
        // Use Vec<AtomicU8> so initialization requires no unsafe code.
        map: Vec<AtomicU8>,
    }

    // CoverageBitmap contains Vec<AtomicU8>; Sync and Send are derived automatically.

    impl CoverageBitmap {
        const SIZE: usize = 65536;

        /// Allocate a zeroed bitmap.
        pub fn new() -> Self {
            let map: Vec<AtomicU8> = (0..Self::SIZE).map(|_| AtomicU8::new(0)).collect();
            CoverageBitmap { map }
        }

        /// Record a hit for the directed edge from `from` to `to`.
        ///
        /// Uses the AFL convention: edge index = (from >> 1) ^ to.
        #[inline(always)]
        pub fn record_edge(&self, from: u16, to: u16) {
            let idx = (((from >> 1) ^ to) as usize) & (Self::SIZE - 1);
            // Saturating increment without compare_exchange overhead:
            // load → if < 255 → store. Relaxed ordering is fine for profiling.
            let cur = self.map[idx].load(Ordering::Relaxed);
            if cur < 255 {
                self.map[idx].store(cur + 1, Ordering::Relaxed);
            }
        }

        /// Merge this bitmap into `global`.
        ///
        /// Returns `true` if any byte in this bitmap was non-zero and not yet
        /// seen in `global`, indicating new coverage was discovered.
        pub fn merge_into(&self, global: &CoverageBitmap) -> bool {
            let mut new_coverage = false;
            for i in 0..Self::SIZE {
                let local = self.map[i].load(Ordering::Relaxed);
                if local > 0 {
                    let prev = global.map[i].fetch_max(local, Ordering::Relaxed);
                    if prev == 0 {
                        new_coverage = true;
                    }
                }
            }
            new_coverage
        }

        /// Reset all counters to zero.
        pub fn reset(&self) {
            for cell in &self.map {
                cell.store(0, Ordering::Relaxed);
            }
        }

        /// Count the number of edges with at least one hit.
        pub fn count_edges(&self) -> usize {
            self.map
                .iter()
                .filter(|c| c.load(Ordering::Relaxed) > 0)
                .count()
        }
    }

    impl Default for CoverageBitmap {
        fn default() -> Self {
            Self::new()
        }
    }

    thread_local! {
        /// The active coverage bitmap for this thread. Installed before a
        /// fuzz execution; taken back afterwards.
        static COVERAGE_BITMAP: RefCell<Option<CoverageBitmap>> = RefCell::new(None);

        /// PC hash of the most recently executed instruction.
        static PREV_LOCATION: RefCell<u16> = RefCell::new(0);
    }

    /// Install `bitmap` as the active coverage sink for this thread.
    ///
    /// The bitmap takes ownership; retrieve it with `take_bitmap()` after execution.
    pub fn install_bitmap(bitmap: CoverageBitmap) {
        COVERAGE_BITMAP.with(|cell| *cell.borrow_mut() = Some(bitmap));
        PREV_LOCATION.with(|cell| *cell.borrow_mut() = 0);
    }

    /// Remove and return the active coverage bitmap from this thread.
    pub fn take_bitmap() -> Option<CoverageBitmap> {
        PREV_LOCATION.with(|cell| *cell.borrow_mut() = 0);
        COVERAGE_BITMAP.with(|cell| cell.borrow_mut().take())
    }

    /// Record an edge from the previous location to `(module_addr, func_idx, pc)`.
    ///
    /// Called from the interpreter's instruction loop. When no bitmap is
    /// installed this function returns after a single `borrow()` check.
    #[inline(always)]
    pub fn record_current_location(
        module_addr: &move_core_types::account_address::AccountAddress,
        func_idx: u16,
        pc: u16,
    ) {
        COVERAGE_BITMAP.with(|bitmap_cell| {
            let mut guard = bitmap_cell.borrow_mut();
            let Some(bitmap) = guard.as_mut() else { return };
            let to = location_hash(module_addr, func_idx, pc);
            PREV_LOCATION.with(|prev_cell| {
                let mut prev = prev_cell.borrow_mut();
                bitmap.record_edge(*prev, to);
                *prev = to;
            });
        });
    }

    /// FNV-1a hash of `(module_address, func_idx, pc)` → u16.
    #[inline(always)]
    fn location_hash(
        addr: &move_core_types::account_address::AccountAddress,
        func_idx: u16,
        pc: u16,
    ) -> u16 {
        const FNV_OFFSET: u32 = 2166136261;
        const FNV_PRIME: u32 = 16777619;
        let mut hash = FNV_OFFSET;
        for byte in addr.as_ref() {
            hash ^= *byte as u32;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash ^= func_idx as u32;
        hash = hash.wrapping_mul(FNV_PRIME);
        hash ^= pc as u32;
        hash = hash.wrapping_mul(FNV_PRIME);
        (hash ^ (hash >> 16)) as u16
    }
}

#[cfg(feature = "coverage")]
pub use inner::{CoverageBitmap, install_bitmap, record_current_location, take_bitmap};

/// No-op stub used when the `coverage` feature is disabled.
#[cfg(not(feature = "coverage"))]
pub struct CoverageBitmap;

#[cfg(not(feature = "coverage"))]
impl CoverageBitmap {
    pub fn new() -> Self {
        CoverageBitmap
    }
    pub fn merge_into(&self, _other: &CoverageBitmap) -> bool {
        false
    }
    pub fn reset(&self) {}
    pub fn count_edges(&self) -> usize {
        0
    }
}

#[cfg(not(feature = "coverage"))]
impl Default for CoverageBitmap {
    fn default() -> Self {
        Self::new()
    }
}
