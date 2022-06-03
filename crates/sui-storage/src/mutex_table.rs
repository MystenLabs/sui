// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::hash_map::{DefaultHasher, RandomState};
use std::hash::{BuildHasher, Hash, Hasher};

// MutexTable supports mutual exclusion on keys such as TransactionDigest or ObjectDigest
pub struct MutexTable<K: Hash> {
    random_state: RandomState,
    lock_table: Vec<tokio::sync::Mutex<()>>,
    _k: std::marker::PhantomData<K>,
}

// Opaque struct to hide tokio::sync::MutexGuard.
pub struct LockGuard<'a> {
    _guard: tokio::sync::MutexGuard<'a, ()>,
}

impl<'b, K: Hash + 'b> MutexTable<K> {
    pub fn new(size: usize) -> Self {
        Self {
            random_state: RandomState::new(),
            lock_table: (0..size)
                .into_iter()
                .map(|_| tokio::sync::Mutex::new(()))
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

    pub async fn acquire_locks<'a, I>(&'a self, object_iter: I) -> Vec<LockGuard<'a>>
    where
        I: IntoIterator<Item = &'b K>,
    {
        let mut locks: Vec<usize> = object_iter
            .into_iter()
            .map(|o| self.get_lock_idx(o))
            .collect();

        locks.sort_unstable();
        locks.dedup();

        let mut guards = Vec::with_capacity(locks.len());
        for lock_idx in locks {
            guards.push(LockGuard {
                _guard: self.lock_table[lock_idx].lock().await,
            });
        }
        guards
    }

    pub async fn acquire_lock<'a>(&'a self, k: &K) -> LockGuard<'a> {
        let lock_idx = self.get_lock_idx(k);
        LockGuard {
            _guard: self.lock_table[lock_idx].lock().await,
        }
    }
}
