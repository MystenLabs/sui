// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::hash::Hash;

use dashmap::DashMap;

/// A checkpoint-tagged in-memory map of streamed values not yet available from the persistent index.
/// Each entry records the checkpoint that introduced it, so evicting an older checkpoint leaves in
/// place a key that was re-introduced at a later checkpoint.
pub(crate) struct StreamedStore<K, V> {
    entries: DashMap<K, Checkpointed<V>>,
}

struct Checkpointed<V> {
    checkpoint: u64,
    value: V,
}

impl<K: Eq + Hash + Clone, V: Clone> StreamedStore<K, V> {
    pub(crate) fn new() -> Self {
        Self {
            entries: DashMap::new(),
        }
    }

    /// Insert `value` under `key`, tagged with the `checkpoint` that introduced it. Checkpoints are
    /// processed in order, so a later insert for the same key is the newer value and replaces it.
    pub(crate) fn insert(&self, checkpoint: u64, key: K, value: V) {
        self.entries.insert(key, Checkpointed { checkpoint, value });
    }

    /// The value held under `key`, if any.
    pub(crate) fn get(&self, key: &K) -> Option<V> {
        self.entries.get(key).map(|entry| entry.value.clone())
    }

    /// Drop entries introduced at a checkpoint at or below `indexed_checkpoint`. A key re-introduced
    /// at a later checkpoint survives, since its entry records that later checkpoint.
    pub(crate) fn evict_up_to(&self, indexed_checkpoint: u64) {
        self.entries
            .retain(|_, entry| entry.checkpoint > indexed_checkpoint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> StreamedStore<u8, String> {
        StreamedStore::new()
    }

    #[test]
    fn get_hits_inserted_entry() {
        let store = store();
        store.insert(5, 1, "a".to_owned());
        assert_eq!(store.get(&1), Some("a".to_owned()));
        assert_eq!(store.get(&2), None);
    }

    #[test]
    fn later_insert_replaces_earlier() {
        let store = store();
        store.insert(5, 1, "a".to_owned());
        store.insert(10, 1, "b".to_owned());
        assert_eq!(store.get(&1), Some("b".to_owned()));
    }

    #[test]
    fn evict_up_to_removes_entries_at_or_below() {
        let store = store();
        store.insert(3, 1, "a".to_owned());
        store.insert(5, 2, "b".to_owned());
        store.insert(7, 3, "c".to_owned());
        store.evict_up_to(5);
        assert_eq!(store.get(&1), None);
        assert_eq!(store.get(&2), None);
        assert_eq!(store.get(&3), Some("c".to_owned()));
    }

    #[test]
    fn evict_up_to_keeps_key_reintroduced_at_later_checkpoint() {
        let store = store();
        store.insert(5, 1, "a".to_owned());
        store.insert(10, 1, "b".to_owned());
        store.evict_up_to(5);
        assert_eq!(store.get(&1), Some("b".to_owned()));
    }
}
