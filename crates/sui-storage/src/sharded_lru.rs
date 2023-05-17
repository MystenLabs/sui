// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::hash_map::RandomState,
    hash::{self, BuildHasher, Hash},
};

use hash::Hasher;
use lru::LruCache;
use std::collections::HashMap;
use std::fmt::Debug;
use std::future::Future;
use std::num::NonZeroUsize;
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

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
        let mut hasher = self.hasher.build_hasher();
        key.hash(&mut hasher);
        let h = hasher.finish() as usize;
        h % self.shards.len()
    }

    async fn read_shard(&self, key: &K) -> RwLockReadGuard<'_, LruCache<K, V>> {
        let shard_idx = self.shard_id(key);
        self.shards[shard_idx].read().await
    }

    async fn write_shard(&self, key: &K) -> RwLockWriteGuard<'_, LruCache<K, V>> {
        let shard_idx = self.shard_id(key);
        self.shards[shard_idx].write().await
    }

    pub async fn invalidate(&self, key: &K) -> Option<V> {
        self.write_shard(key).await.pop(key)
    }

    pub async fn batch_invalidate(&self, keys: impl IntoIterator<Item = K>) {
        let mut grouped = HashMap::new();
        for key in keys.into_iter() {
            let shard_idx = self.shard_id(&key);
            grouped.entry(shard_idx).or_insert(vec![]).push(key);
        }
        for (shard_idx, keys) in grouped.into_iter() {
            let mut lock = self.shards[shard_idx].write().await;
            for key in keys {
                lock.pop(&key);
            }
        }
    }

    pub async fn merge(&self, key: K, value: &V, f: fn(&V, &V) -> V) {
        let mut shard = self.write_shard(&key).await;
        let old_value = shard.get(&key);
        if let Some(old_value) = old_value {
            let new_value = f(old_value, value);
            shard.put(key, new_value);
        }
    }

    pub async fn batch_merge(
        &self,
        key_values: impl IntoIterator<Item = (K, V)>,
        f: fn(&V, &V) -> V,
    ) {
        let mut grouped = HashMap::new();
        for (key, value) in key_values.into_iter() {
            let shard_idx = self.shard_id(&key);
            grouped
                .entry(shard_idx)
                .or_insert(vec![])
                .push((key, value));
        }
        for (shard_idx, keys) in grouped.into_iter() {
            let mut shard = self.shards[shard_idx].write().await;
            for (key, value) in keys.into_iter() {
                let old_value = shard.get(&key);
                if let Some(old_value) = old_value {
                    let new_value = f(old_value, &value);
                    shard.put(key, new_value);
                }
            }
        }
    }

    pub async fn get(&self, key: &K) -> Option<V> {
        self.read_shard(key).await.peek(key).cloned()
    }

    pub async fn get_with(&self, key: K, init: impl Future<Output = V>) -> V {
        let shard = self.read_shard(&key).await;
        let value = shard.peek(&key);
        if value.is_some() {
            return value.unwrap().clone();
        }
        drop(shard);
        let mut shard = self.write_shard(&key).await;
        let value = shard.get(&key);
        if value.is_some() {
            return value.unwrap().clone();
        }
        let value = init.await;
        let cloned_value = value.clone();
        shard.push(key, value);
        cloned_value
    }
}
