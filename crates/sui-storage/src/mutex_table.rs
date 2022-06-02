// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use blake2::{Blake2s, Digest};
use rand::rngs::OsRng;
use rand::RngCore;

// MutexTable supports mutual exclusion on keys such as TransactionDigest or ObjectDigest
pub struct MutexTable<K: std::convert::AsRef<[u8]>> {
    hash_secret: [u8; 16],
    lock_table: Vec<tokio::sync::Mutex<()>>,
    _k: std::marker::PhantomData<K>,
}

impl<K: std::convert::AsRef<[u8]>> MutexTable<K> {
    pub fn new(size: usize) -> Self {
        let hash_secret = if !cfg!(test) {
            let mut rng = OsRng;
            let mut hash_secret = [0u8; 16];
            rng.fill_bytes(&mut hash_secret);
            hash_secret
        } else {
            [0u8; 16] // be deterministic for tests
        };

        Self {
            hash_secret,
            lock_table: (0..size)
                .into_iter()
                .map(|_| tokio::sync::Mutex::new(()))
                .collect(),
            _k: std::marker::PhantomData {},
        }
    }

    fn get_lock_idx(&self, key: &K) -> usize {
        let mut hasher = Blake2s::new();
        hasher.update(self.hash_secret);
        hasher.update(key);
        let digest = hasher.finalize();
        usize::from_le_bytes(digest[0..8].try_into().unwrap()) % self.lock_table.len()
    }

    pub async fn acquire_locks<'a>(
        &'a self,
        objects: &[K],
    ) -> Vec<tokio::sync::MutexGuard<'a, ()>> {
        let mut locks: Vec<usize> = objects.iter().map(|o| self.get_lock_idx(o)).collect();

        locks.sort_unstable();
        locks.dedup();

        let mut guards = Vec::with_capacity(locks.len());
        for lock_idx in locks {
            guards.push(self.lock_table[lock_idx].lock().await);
        }
        guards
    }

    pub async fn acquire_lock<'a>(&'a self, k: &K) -> tokio::sync::MutexGuard<'a, ()> {
        let lock_idx = self.get_lock_idx(k);
        self.lock_table[lock_idx].lock().await
    }
}
