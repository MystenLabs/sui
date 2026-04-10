// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Fixed-capacity reorder buffer for items keyed by a monotonic
//! `CommitGen`. Backed by a `VecDeque<Option<T>>` where the front slot
//! corresponds to `base`. Entries fill in potentially out of order; they
//! graduate in strict key order via [`ReorderBuffer::pop_front_if`].

use std::collections::VecDeque;

use crate::handlers::bitmap::async_pipeline::CommitGen;

/// Maximum number of in-flight commit gens. Exceeding this is a
/// correctness bug (the pipeline is making forward progress out of order
/// beyond any expected gap); we panic rather than silently overwrite.
const REORDER_BUFFER_CAPACITY: usize = 256;

pub(crate) struct ReorderBuffer<T> {
    ring: VecDeque<Option<T>>,
    base: CommitGen,
}

impl<T> ReorderBuffer<T> {
    pub(crate) fn new() -> Self {
        Self {
            ring: VecDeque::new(),
            base: 0,
        }
    }

    pub(crate) fn insert(&mut self, key: CommitGen, v: T) {
        assert!(
            key >= self.base,
            "ReorderBuffer::insert below base: key={key}, base={base}",
            base = self.base,
        );
        let idx = (key - self.base) as usize;
        assert!(
            idx < REORDER_BUFFER_CAPACITY,
            "ReorderBuffer overflow: key={key} base={base} idx={idx} cap={cap}",
            base = self.base,
            cap = REORDER_BUFFER_CAPACITY,
        );
        while self.ring.len() <= idx {
            self.ring.push_back(None);
        }
        self.ring[idx] = Some(v);
    }

    pub(crate) fn get_mut(&mut self, key: CommitGen) -> Option<&mut T> {
        if key < self.base {
            return None;
        }
        let idx = (key - self.base) as usize;
        self.ring.get_mut(idx).and_then(|s| s.as_mut())
    }

    /// Pop and return the front entry with its key if it is present
    /// and `pred` holds.
    pub(crate) fn pop_front_if<F: FnOnce(&T) -> bool>(
        &mut self,
        pred: F,
    ) -> Option<(CommitGen, T)> {
        match self.ring.front() {
            Some(Some(v)) if pred(v) => {
                let v = self.ring.pop_front().unwrap().unwrap();
                let key = self.base;
                self.base += 1;
                Some((key, v))
            }
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }
}
