// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hash},
};

use lru::LruCache;
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::collections::HashMap;
use std::fmt::Debug;
use std::num::NonZeroUsize;

pub struct ShardedLruCache<K, V, S = RandomState> {
    shards: Vec<RwLock<LruCache<K, V>>>,
    hasher: S,
}

unsafe impl<K, V, S> Send for ShardedLruCache<K, V, S> {}
unsafe impl<K, V, S> Sync for ShardedLruCache<K, V, S> {}

impl<K, V> ShardedLruCache<K, V, RandomState>
where
    K: Send + Sync + Hash + Eq + Clone,
    V: Send + Sync + Clone,
{
    pub fn new(capacity: u64, num_shards: u64) -> Self {
        let cap_per_shard = (capacity + num_shards - 1) / num_shards;
        let hasher = RandomState::default();
        Self {
            hasher,
            shards: (0..num_shards)
                .map(|_| {
                    RwLock::new(LruCache::new(
                        NonZeroUsize::new(cap_per_shard as usize).unwrap(),
                    ))
                })
                .collect(),
        }
    }
}

impl<K, V, S> ShardedLruCache<K, V, S>
where
    K: Hash + Eq + Clone + Debug,
    V: Clone,
    S: BuildHasher,
{
    fn shard_id(&self, key: &K) -> usize {
        let h = self.hasher.hash_one(key) as usize;
        h % self.shards.len()
    }

    fn read_shard(&self, key: &K) -> RwLockReadGuard<'_, LruCache<K, V>> {
        let shard_idx = self.shard_id(key);
        self.shards[shard_idx].read()
    }

    fn write_shard(&self, key: &K) -> RwLockWriteGuard<'_, LruCache<K, V>> {
        let shard_idx = self.shard_id(key);
        self.shards[shard_idx].write()
    }

    pub fn invalidate(&self, key: &K) -> Option<V> {
        self.write_shard(key).pop(key)
    }

    pub fn batch_invalidate(&self, keys: impl IntoIterator<Item = K>) {
        let mut grouped = HashMap::new();
        for key in keys.into_iter() {
            let shard_idx = self.shard_id(&key);
            grouped.entry(shard_idx).or_insert(vec![]).push(key);
        }
        for (shard_idx, keys) in grouped.into_iter() {
            let mut lock = self.shards[shard_idx].write();
            for key in keys {
                lock.pop(&key);
            }
        }
    }

    pub fn merge(&self, key: K, value: &V, f: fn(&V, &V) -> V) {
        let mut shard = self.write_shard(&key);
        let old_value = shard.get(&key);
        if let Some(old_value) = old_value {
            let new_value = f(old_value, value);
            shard.put(key, new_value);
        }
    }

    pub fn batch_merge(&self, key_values: impl IntoIterator<Item = (K, V)>, f: fn(&V, &V) -> V) {
        let mut grouped = HashMap::new();
        for (key, value) in key_values.into_iter() {
            let shard_idx = self.shard_id(&key);
            grouped
                .entry(shard_idx)
                .or_insert(vec![])
                .push((key, value));
        }
        for (shard_idx, keys) in grouped.into_iter() {
            let mut shard = self.shards[shard_idx].write();
            for (key, value) in keys.into_iter() {
                let old_value = shard.get(&key);
                if let Some(old_value) = old_value {
                    let new_value = f(old_value, &value);
                    shard.put(key, new_value);
                }
            }
        }
    }

    pub fn get(&self, key: &K) -> Option<V> {
        self.read_shard(key).peek(key).cloned()
    }

    pub fn get_with(&self, key: K, init: impl FnOnce() -> V) -> V {
        let shard = self.read_shard(&key);
        let value = shard.peek(&key);
        if let Some(value) = value {
            return value.clone();
        }
        drop(shard);
        let mut shard = self.write_shard(&key);
        let value = shard.get(&key);
        if let Some(value) = value {
            return value.clone();
        }
        let value = init();
        let cloned_value = value.clone();
        shard.push(key, value);
        cloned_value
    }
}
