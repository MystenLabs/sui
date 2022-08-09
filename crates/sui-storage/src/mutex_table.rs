// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::hash_map::{DefaultHasher, RandomState};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::hash::{BuildHasher, Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{error, info};

type InnerLockTable<K> = HashMap<K, Arc<tokio::sync::Mutex<()>>>;
// MutexTable supports mutual exclusion on keys such as TransactionDigest or ObjectDigest
pub struct MutexTable<K: Hash> {
    random_state: RandomState,
    lock_table: Arc<Vec<RwLock<InnerLockTable<K>>>>,
    _k: std::marker::PhantomData<K>,
    _cleaner: JoinHandle<()>,
    stop: Arc<AtomicBool>,
}

#[derive(Debug)]
pub enum TryAcquireLockError {
    LockTableLocked,
    LockEntryLocked,
}

impl fmt::Display for TryAcquireLockError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "operation would block")
    }
}

impl Error for TryAcquireLockError {}
// Opaque struct to hide tokio::sync::MutexGuard.
pub struct LockGuard(tokio::sync::OwnedMutexGuard<()>);

impl<K: Hash + std::cmp::Eq + Send + Sync + 'static> MutexTable<K> {
    pub fn new_with_cleanup(
        num_shards: usize,
        shard_size: usize,
        cleanup_period: Duration,
        cleanup_initial_delay: Duration,
    ) -> Self {
        let lock_table: Arc<Vec<RwLock<InnerLockTable<K>>>> = Arc::new(
            (0..num_shards)
                .into_iter()
                .map(|_| RwLock::new(HashMap::with_capacity(shard_size)))
                .collect(),
        );
        let cloned = lock_table.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_cloned = stop.clone();
        Self {
            random_state: RandomState::new(),
            lock_table,
            _k: std::marker::PhantomData {},
            _cleaner: tokio::spawn(async move {
                tokio::time::sleep(cleanup_initial_delay).await;
                while !stop_cloned.load(Ordering::SeqCst) {
                    Self::cleanup(cloned.clone());
                    tokio::time::sleep(cleanup_period).await;
                }
                info!("Stopping mutex table cleanup!");
            }),
            stop,
        }
    }

    pub fn new(num_shards: usize, shard_size: usize) -> Self {
        Self::new_with_cleanup(
            num_shards,
            shard_size,
            Duration::from_secs(10),
            Duration::from_secs(10),
        )
    }

    pub fn cleanup(lock_table: Arc<Vec<RwLock<InnerLockTable<K>>>>) {
        for shard in lock_table.iter() {
            let map = shard.try_write();
            if map.is_err() {
                continue;
            }
            map.unwrap().retain(|_k, v| {
                let mutex_guard = v.try_lock();
                mutex_guard.is_err()
            });
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

    pub fn try_acquire_lock(&self, k: K) -> Result<LockGuard, TryAcquireLockError> {
        let lock_idx = self.get_lock_idx(&k);
        let res = self.lock_table[lock_idx].try_read();
        if res.is_err() {
            return Err(TryAcquireLockError::LockTableLocked);
        }
        let map = res.unwrap();
        if let Some(element) = map.get(&k) {
            let lock = element.clone().try_lock_owned();
            if lock.is_err() {
                return Err(TryAcquireLockError::LockEntryLocked);
            }
            Ok(LockGuard(lock.unwrap()))
        } else {
            // element doesn't exist
            drop(map);
            let res = self.lock_table[lock_idx].try_write();
            if res.is_err() {
                return Err(TryAcquireLockError::LockTableLocked);
            }
            let mut map = res.unwrap();
            let element = map.entry(k).or_insert_with(|| Arc::new(Mutex::new(())));
            let lock = element.clone().try_lock_owned();
            lock.map(LockGuard).map_err(|e| {
                error!("Failed to acquire lock after creation: {:?}", e);
                TryAcquireLockError::LockEntryLocked
            })
        }
    }
}

impl<K: Hash> Drop for MutexTable<K> {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn test_mutex_table() {
    // Disable bg cleanup with Duration.MAX for initial delay
    let mutex_table =
        MutexTable::<String>::new_with_cleanup(1, 128, Duration::from_secs(10), Duration::MAX);
    let john1 = mutex_table.try_acquire_lock("john".to_string());
    assert!(john1.is_ok());
    let john2 = mutex_table.try_acquire_lock("john".to_string());
    assert!(john2.is_err());
    drop(john1);
    let john2 = mutex_table.try_acquire_lock("john".to_string());
    assert!(john2.is_ok());
    let jane = mutex_table.try_acquire_lock("jane".to_string());
    assert!(jane.is_ok());
    MutexTable::cleanup(mutex_table.lock_table.clone());
    let map = mutex_table.lock_table.get(0).as_ref().unwrap().try_read();
    assert!(map.is_ok());
    assert_eq!(map.unwrap().len(), 2);
    drop(john2);
    MutexTable::cleanup(mutex_table.lock_table.clone());
    let map = mutex_table.lock_table.get(0).as_ref().unwrap().try_read();
    assert!(map.is_ok());
    assert_eq!(map.unwrap().len(), 1);
    drop(jane);
    MutexTable::cleanup(mutex_table.lock_table.clone());
    let map = mutex_table.lock_table.get(0).as_ref().unwrap().try_read();
    assert!(map.is_ok());
    assert!(map.unwrap().is_empty());
}

#[tokio::test]
async fn test_mutex_table_bg_cleanup() {
    let mutex_table = MutexTable::<String>::new_with_cleanup(
        1,
        128,
        Duration::from_secs(5),
        Duration::from_secs(1),
    );
    let lock1 = mutex_table.try_acquire_lock("lock1".to_string());
    let lock2 = mutex_table.try_acquire_lock("lock2".to_string());
    let lock3 = mutex_table.try_acquire_lock("lock3".to_string());
    let lock4 = mutex_table.try_acquire_lock("lock4".to_string());
    let lock5 = mutex_table.try_acquire_lock("lock5".to_string());
    assert!(lock1.is_ok());
    assert!(lock2.is_ok());
    assert!(lock3.is_ok());
    assert!(lock4.is_ok());
    assert!(lock5.is_ok());
    // Trigger cleanup
    MutexTable::cleanup(mutex_table.lock_table.clone());
    // Try acquiring locks again, these should still fail because locks have not been released
    let lock11 = mutex_table.try_acquire_lock("lock1".to_string());
    let lock22 = mutex_table.try_acquire_lock("lock2".to_string());
    let lock33 = mutex_table.try_acquire_lock("lock3".to_string());
    let lock44 = mutex_table.try_acquire_lock("lock4".to_string());
    let lock55 = mutex_table.try_acquire_lock("lock5".to_string());
    assert!(lock11.is_err());
    assert!(lock22.is_err());
    assert!(lock33.is_err());
    assert!(lock44.is_err());
    assert!(lock55.is_err());
    // drop all locks
    drop(lock1);
    drop(lock2);
    drop(lock3);
    drop(lock4);
    drop(lock5);
    // Wait for bg cleanup to be triggered
    tokio::time::sleep(Duration::from_secs(10)).await;
    for entry in mutex_table.lock_table.iter() {
        let locked = entry.read().await;
        assert!(locked.is_empty());
    }
}
