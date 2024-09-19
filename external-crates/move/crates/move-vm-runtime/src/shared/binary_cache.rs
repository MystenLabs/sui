// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, hash::Hash, sync::Arc};

use move_binary_format::errors::PartialVMResult;

// A simple cache that offers both a HashMap and a Vector lookup.
// Values are forced into a `Arc` so they can be used from multiple thread.
// Access to this cache is always under a `RwLock`.
#[derive(Debug)]
pub struct BinaryCache<K, V> {
    pub id_map: HashMap<K, usize>,
    pub binaries: Vec<Arc<V>>,
}

impl<K, V> BinaryCache<K, V>
where
    K: Eq + Hash,
{
    pub fn new() -> Self {
        Self {
            id_map: HashMap::new(),
            binaries: vec![],
        }
    }

    pub fn insert(&mut self, key: K, binary: V) -> PartialVMResult<&Arc<V>> {
        let idx = self.binaries.len();
        // Last write wins in the binary cache -- it's up to the callee to not make conflicting
        // writes.
        self.id_map.insert(key, idx);
        self.binaries.push(Arc::new(binary));
        Ok(&self.binaries[idx])
    }

    pub fn get_with_idx(&self, key: &K) -> Option<(usize, &Arc<V>)> {
        let idx = self.id_map.get(key)?;
        Some((*idx, self.binaries.get(*idx)?))
    }

    pub fn get(&self, key: &K) -> Option<&Arc<V>> {
        Some(self.get_with_idx(key)?.1)
    }

    pub fn get_by_id(&self, idx: &usize) -> Option<&Arc<V>> {
        self.binaries.get(*idx)
    }

    pub fn contains(&self, key: &K) -> bool {
        self.id_map.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.binaries.len()
    }
}
