// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::hash_map::{DefaultHasher, RandomState};
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash, Hasher};
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

// MutexTable supports mutual exclusion on keys such as TransactionDigest or ObjectDigest
pub struct MutexTable<K: Hash> {
    random_state: RandomState,
    lock_table: Vec<RwLock<HashMap<K, Arc<tokio::sync::Mutex<()>>>>>,
    _k: std::marker::PhantomData<K>,
}

// Opaque struct to hide tokio::sync::MutexGuard.
pub struct LockGuard(tokio::sync::OwnedMutexGuard<()>);

impl<'b, K: Hash + std::cmp::Eq + 'b> MutexTable<K> {
    pub fn new(num_shards: usize, shard_size: usize) -> Self {
        Self {
            random_state: RandomState::new(),
            lock_table: (0..num_shards)
                .into_iter()
                .map(|_| RwLock::new(HashMap::with_capacity(shard_size)))
                .collect(),
            _k: std::marker::PhantomData {},
        }
    }

    fn get_lock_idx(&self, key: &K) -> usize {
        let mut hasher = if !cfg!(test) {
            self.random_state.build_hasher()
        } else {
            // be deterministic for tests
            DefaultHasher::new()
        };

        key.hash(&mut hasher);
        // unwrap ok - converting u64 -> usize
        let hash: usize = hasher.finish().try_into().unwrap();
        hash % self.lock_table.len()
    }

    pub async fn acquire_locks<I>(&self, object_iter: I) -> Vec<LockGuard>
    where
        I: Iterator<Item = K>,
    {
        let mut objects: Vec<K> = object_iter.into_iter().collect();
        objects.sort_by_key(|a| self.get_lock_idx(a));
        objects.dedup();

        let mut guards = Vec::with_capacity(objects.len());
        for object in objects.into_iter() {
            guards.push(self.acquire_lock(object).await);
        }
        guards
    }

    pub async fn acquire_lock(&self, k: K) -> LockGuard {
        let lock_idx = self.get_lock_idx(&k);
        let map = self.lock_table[lock_idx].read().await;
        if let Some(element) = map.get(&k) {
            LockGuard(element.clone().lock_owned().await)
        } else {
            // element doesn't exist
            drop(map);
            let mut map = self.lock_table[lock_idx].write().await;
            let element = map.entry(k).or_insert_with(|| Arc::new(Mutex::new(())));
            LockGuard(element.clone().lock_owned().await)
        }
    }
}
