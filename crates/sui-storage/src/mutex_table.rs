// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::hash_map::{DefaultHasher, RandomState};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::hash::{BuildHasher, Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::info;

use sui_metrics::spawn_monitored_task;

type InnerLockTable<K> = HashMap<K, Arc<Mutex<()>>>;
// MutexTable supports mutual exclusion on keys such as TransactionDigest or ObjectDigest
pub struct MutexTable<K: Hash> {
    random_state: RandomState,
    lock_table: Arc<Vec<RwLock<InnerLockTable<K>>>>,
    _k: std::marker::PhantomData<K>,
    _cleaner: JoinHandle<()>,
    stop: Arc<AtomicBool>,
    size: Arc<AtomicUsize>,
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

impl<K: Hash + Eq + Send + Sync + 'static> MutexTable<K> {
    pub fn new_with_cleanup(
        num_shards: usize,
        shard_size: usize,
        cleanup_period: Duration,
        cleanup_initial_delay: Duration,
        cleanup_entries_threshold: usize,
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
        let size: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let size_cloned = size.clone();
        Self {
            random_state: RandomState::new(),
            lock_table,
            _k: std::marker::PhantomData {},
            _cleaner: spawn_monitored_task!(async move {
                tokio::time::sleep(cleanup_initial_delay).await;
                let mut previous_cleanup_instant = Instant::now();
                while !stop_cloned.load(Ordering::SeqCst) {
                    if size_cloned.load(Ordering::SeqCst) >= cleanup_entries_threshold
                        || previous_cleanup_instant.elapsed() >= cleanup_period
                    {
                        let num_removed = Self::cleanup(cloned.clone());
                        size_cloned.fetch_sub(num_removed, Ordering::SeqCst);
                        previous_cleanup_instant = Instant::now();
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                info!("Stopping mutex table cleanup!");
            }),
            stop,
            size,
        }
    }

    pub fn new(num_shards: usize, shard_size: usize) -> Self {
        Self::new_with_cleanup(
            num_shards,
            shard_size,
            Duration::from_secs(10),
            Duration::from_secs(10),
            10_000,
        )
    }

    pub fn size(&self) -> usize {
        self.size.load(Ordering::SeqCst)
    }

    pub fn cleanup(lock_table: Arc<Vec<RwLock<InnerLockTable<K>>>>) -> usize {
        let mut num_removed: usize = 0;
        for shard in lock_table.iter() {
            let map = shard.try_write();
            if map.is_err() {
                continue;
            }
            map.unwrap().retain(|_k, v| {
                // MutexMap::(try_|)acquire_locks will lock the map and call Arc::clone on the entry
                // This check ensures that we only drop entry from the map if this is the only mutex copy
                // This check is also likely sufficient e.g. you don't even need try_lock below, but keeping it just in case
                if Arc::strong_count(v) == 1 {
                    let mutex_guard = v.try_lock();
                    if mutex_guard.is_ok() {
                        num_removed += 1;
                        false
                    } else {
                        true
                    }
                } else {
                    true
                }
            });
        }
        num_removed
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
        K: Ord,
    {
        let mut objects: Vec<K> = object_iter.into_iter().collect();
        objects.sort_unstable();
        objects.dedup();

        let mut guards = Vec::with_capacity(objects.len());
        for object in objects.into_iter() {
            guards.push(self.acquire_lock(object).await);
        }
        guards
    }

    pub async fn acquire_lock(&self, k: K) -> LockGuard {
        let lock_idx = self.get_lock_idx(&k);
        let element = {
            let map = self.lock_table[lock_idx].read().await;
            map.get(&k).cloned()
        };
        if let Some(element) = element {
            LockGuard(element.lock_owned().await)
        } else {
            // element doesn't exist
            let element = {
                let mut map = self.lock_table[lock_idx].write().await;
                map.entry(k)
                    .or_insert_with(|| {
                        self.size.fetch_add(1, Ordering::SeqCst);
                        Arc::new(Mutex::new(()))
                    })
                    .clone()
            };
            LockGuard(element.lock_owned().await)
        }
    }

    pub fn try_acquire_lock(&self, k: K) -> Result<LockGuard, TryAcquireLockError> {
        let lock_idx = self.get_lock_idx(&k);
        let element = {
            let map = self.lock_table[lock_idx]
                .try_read()
                .map_err(|_| TryAcquireLockError::LockTableLocked)?;
            map.get(&k).cloned()
        };
        if let Some(element) = element {
            let lock = element.try_lock_owned();
            Ok(LockGuard(
                lock.map_err(|_| TryAcquireLockError::LockEntryLocked)?,
            ))
        } else {
            // element doesn't exist
            let element = {
                let mut map = self.lock_table[lock_idx]
                    .try_write()
                    .map_err(|_| TryAcquireLockError::LockTableLocked)?;
                map.entry(k)
                    .or_insert_with(|| {
                        self.size.fetch_add(1, Ordering::SeqCst);
                        Arc::new(Mutex::new(()))
                    })
                    .clone()
            };
            let lock = element.try_lock_owned();
            lock.map(LockGuard)
                .map_err(|_| TryAcquireLockError::LockEntryLocked)
        }
    }
}

impl<K: Hash> Drop for MutexTable<K> {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
// Tests that mutex table provides parallelism on the individual mutex level,
// e.g. that locks for different entries do not block entire bucket if it needs to wait on individual lock
async fn test_mutex_table_concurrent_in_same_bucket() {
    use tokio::time::{sleep, timeout};
    let mutex_table = Arc::new(MutexTable::<String>::new(1, 128));
    let john = mutex_table.try_acquire_lock("john".to_string());
    john.unwrap();
    {
        let mutex_table = mutex_table.clone();
        spawn_monitored_task!(async move {
            mutex_table.acquire_lock("john".to_string()).await;
        });
    }
    sleep(Duration::from_millis(50)).await;
    let jane = mutex_table.try_acquire_lock("jane".to_string());
    jane.unwrap();

    let mutex_table = Arc::new(MutexTable::<String>::new(1, 128));
    let _john = mutex_table.acquire_lock("john".to_string()).await;
    {
        let mutex_table = mutex_table.clone();
        spawn_monitored_task!(async move {
            mutex_table.acquire_lock("john".to_string()).await;
        });
    }
    sleep(Duration::from_millis(50)).await;
    let jane = timeout(
        Duration::from_secs(1),
        mutex_table.acquire_lock("jane".to_string()),
    )
    .await;
    jane.unwrap();
}

#[tokio::test]
async fn test_mutex_table() {
    // Disable bg cleanup with Duration.MAX for initial delay
    let mutex_table = MutexTable::<String>::new_with_cleanup(
        1,
        128,
        Duration::from_secs(10),
        Duration::MAX,
        1000,
    );
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

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_mutex_table_bg_cleanup() {
    let mutex_table = MutexTable::<String>::new_with_cleanup(
        1,
        128,
        Duration::from_secs(5),
        Duration::from_secs(1),
        1000,
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

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_mutex_table_bg_cleanup_with_size_threshold() {
    // set up the table to never trigger cleanup because of time period but only size threshold
    let mutex_table =
        MutexTable::<String>::new_with_cleanup(1, 128, Duration::MAX, Duration::from_secs(1), 5);
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
    assert_eq!(mutex_table.size(), 5);
    // drop all locks
    drop(lock1);
    drop(lock2);
    drop(lock3);
    drop(lock4);
    drop(lock5);
    tokio::task::yield_now().await;
    // Wait for bg cleanup to be triggered because of size threshold
    tokio::time::advance(Duration::from_secs(5)).await;
    tokio::task::yield_now().await;
    assert_eq!(mutex_table.size(), 0);
    for entry in mutex_table.lock_table.iter() {
        let locked = entry.read().await;
        assert!(locked.is_empty());
    }
}
