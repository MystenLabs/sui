// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{errors::PartialVMResult, partial_vm_error};
use std::{collections::HashMap, hash::Hash};

/// A fixed-capacity, insert-once map that is loud instead of lossy: [`insert`](Self::insert)
/// raises an invariant violation at capacity rather than evicting, and on a duplicate key
/// rather than overwriting, so nothing put in it can ever go missing, be replaced, or
/// overflow silently. The interface is intentionally spartan (in the mold of the dispatch
/// tables' `DefinitionMap`) -- keyed probes only, no iteration -- so unstable hashing order
/// can never influence a result.
pub(crate) struct BoundedMap<K, V> {
    map: HashMap<K, V>,
    capacity: usize,
    /// Names the map in the overflow error, so the violation says who filled up.
    label: &'static str,
}

impl<K: Hash + Eq, V> BoundedMap<K, V> {
    pub(crate) fn new(capacity: usize, label: &'static str) -> Self {
        Self {
            map: HashMap::new(),
            capacity,
            label,
        }
    }

    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        self.map.get(key)
    }

    pub(crate) fn contains_key(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    /// Insert, erroring if the map is at capacity or the key is already present. Nothing
    /// evicts, and a duplicate insert means the caller computed the same entry twice; both are
    /// logic errors worth surfacing, not silent updates.
    pub(crate) fn insert(&mut self, key: K, value: V) -> PartialVMResult<()> {
        if self.map.len() >= self.capacity {
            return Err(partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "{} exceeded its capacity of {}",
                self.label,
                self.capacity
            ));
        }
        if self.map.insert(key, value).is_some() {
            return Err(partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "{} inserted a duplicate key",
                self.label
            ));
        }
        Ok(())
    }
}
