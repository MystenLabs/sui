// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::collections::hash_map::{DefaultHasher, RandomState};
use std::error::Error;
use std::fmt;
use std::hash::{BuildHasher, Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use parking_lot::{ArcMutexGuard, ArcRwLockReadGuard, ArcRwLockWriteGuard, Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::info;

use mysten_common::sync::execution_permit::release_execution_permit;
#[cfg(test)]
use mysten_common::sync::execution_permit::set_execution_permit;
use mysten_metrics::spawn_monitored_task;

type OwnedMutexGuard<T> = ArcMutexGuard<parking_lot::RawMutex, T>;
type OwnedRwLockReadGuard<T> = ArcRwLockReadGuard<parking_lot::RawRwLock, T>;
type OwnedRwLockWriteGuard<T> = ArcRwLockWriteGuard<parking_lot::RawRwLock, T>;

pub trait Lock: Send + Sync + Default {
    type Guard;
    type ReadGuard;
    fn lock_owned(self: Arc<Self>) -> Self::Guard;
    fn try_lock_owned(self: Arc<Self>) -> Option<Self::Guard>;
    fn read_lock_owned(self: Arc<Self>) -> Self::ReadGuard;
}

impl Lock for Mutex<()> {
    type Guard = OwnedMutexGuard<()>;
    type ReadGuard = Self::Guard;

    fn lock_owned(self: Arc<Self>) -> Self::Guard {
        self.lock_arc()
    }

    fn try_lock_owned(self: Arc<Self>) -> Option<Self::Guard> {
        self.try_lock_arc()
    }

    fn read_lock_owned(self: Arc<Self>) -> Self::ReadGuard {
        self.lock_arc()
    }
}

impl Lock for RwLock<()> {
    type Guard = OwnedRwLockWriteGuard<()>;
    type ReadGuard = OwnedRwLockReadGuard<()>;

    fn lock_owned(self: Arc<Self>) -> Self::Guard {
        self.write_arc()
    }

    fn try_lock_owned(self: Arc<Self>) -> Option<Self::Guard> {
        self.try_write_arc()
    }

    fn read_lock_owned(self: Arc<Self>) -> Self::ReadGuard {
        self.read_arc()
    }
}

type InnerLockTable<K, L> = HashMap<K, Arc<L>>;
// MutexTable supports mutual exclusion on keys such as TransactionDigest or ObjectDigest
pub struct LockTable<K: Hash, L: Lock> {
    random_state: RandomState,
    lock_table: Arc<Vec<RwLock<InnerLockTable<K, L>>>>,
    _k: std::marker::PhantomData<K>,
    _cleaner: JoinHandle<()>,
    stop: Arc<AtomicBool>,
    size: Arc<AtomicUsize>,
}

pub type MutexTable<K> = LockTable<K, Mutex<()>>;
pub type RwLockTable<K> = LockTable<K, RwLock<()>>;

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
pub type MutexGuard = OwnedMutexGuard<()>;
pub type RwLockGuard = OwnedRwLockReadGuard<()>;

impl<K: Hash + Eq + Send + Sync + 'static, L: Lock + 'static> LockTable<K, L> {
    pub fn new_with_cleanup(
        num_shards: usize,
        cleanup_period: Duration,
        cleanup_initial_delay: Duration,
        cleanup_entries_threshold: usize,
    ) -> Self {
        let num_shards = if cfg!(msim) { 4 } else { num_shards };

        let lock_table: Arc<Vec<RwLock<InnerLockTable<K, L>>>> = Arc::new(
            (0..num_shards)
                .map(|_| RwLock::new(HashMap::new()))
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

    pub fn new(num_shards: usize) -> Self {
        Self::new_with_cleanup(
            num_shards,
            Duration::from_secs(10),
            Duration::from_secs(10),
            10_000,
        )
    }

    pub fn size(&self) -> usize {
        self.size.load(Ordering::SeqCst)
    }

    pub fn cleanup(lock_table: Arc<Vec<RwLock<InnerLockTable<K, L>>>>) -> usize {
        let mut num_removed: usize = 0;
        for shard in lock_table.iter() {
            let map = shard.try_write();
            if map.is_none() {
                continue;
            }
            map.unwrap().retain(|_k, v| {
                // MutexMap::(try_|)acquire_locks will lock the map and call Arc::clone on the entry
                // This check ensures that we only drop entry from the map if this is the only mutex copy
                // This check is also likely sufficient e.g. you don't even need try_lock below, but keeping it just in case
                if Arc::strong_count(v) == 1 {
                    num_removed += 1;
                    false
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

    pub fn acquire_locks<I>(&self, object_iter: I) -> Vec<L::Guard>
    where
        I: Iterator<Item = K>,
        K: Ord,
    {
        let mut objects: Vec<K> = object_iter.into_iter().collect();
        objects.sort_unstable();
        objects.dedup();

        let mut guards = Vec::with_capacity(objects.len());
        for object in objects.into_iter() {
            guards.push(self.acquire_lock(object));
        }
        guards
    }

    pub fn acquire_read_locks(&self, mut objects: Vec<K>) -> Vec<L::ReadGuard>
    where
        K: Ord,
    {
        objects.sort_unstable();
        objects.dedup();
        let mut guards = Vec::with_capacity(objects.len());
        for object in objects.into_iter() {
            guards.push(self.get_lock(object).read_lock_owned());
        }
        guards
    }

    pub fn get_lock(&self, k: K) -> Arc<L> {
        let lock_idx = self.get_lock_idx(&k);
        let element = {
            let map = self.lock_table[lock_idx].read();
            map.get(&k).cloned()
        };
        if let Some(element) = element {
            element
        } else {
            // element doesn't exist

            {
                let mut map = self.lock_table[lock_idx].write();
                map.entry(k)
                    .or_insert_with(|| {
                        self.size.fetch_add(1, Ordering::SeqCst);
                        Arc::new(L::default())
                    })
                    .clone()
            }
        }
    }

    /// Acquires the lock for `k`.
    ///
    /// This is a blocking API. Attempting a contended acquisition from an async
    /// runtime worker is a caller bug in both native and msim builds. Callers that
    /// may contend must use a blocking context, such as `spawn_blocking`.
    ///
    /// On contention, any installed execution permit is released before waiting;
    /// this is a no-op for callers outside execution. Under msim, the blocking
    /// thread yields between attempts so the lock holder can make progress.
    pub fn acquire_lock(&self, k: K) -> L::Guard {
        let lock = self.get_lock(k);
        if let Some(guard) = lock.clone().try_lock_owned() {
            return guard;
        }

        release_execution_permit();

        #[cfg(msim)]
        loop {
            msim::task::yield_blocking();
            if let Some(guard) = lock.clone().try_lock_owned() {
                return guard;
            }
        }

        #[cfg(not(msim))]
        lock.lock_owned()
    }

    pub fn try_acquire_lock(&self, k: K) -> Result<L::Guard, TryAcquireLockError> {
        let lock_idx = self.get_lock_idx(&k);
        let element = {
            let map = self.lock_table[lock_idx]
                .try_read()
                .ok_or(TryAcquireLockError::LockTableLocked)?;
            map.get(&k).cloned()
        };
        if let Some(element) = element {
            let lock = element.try_lock_owned();
            lock.ok_or(TryAcquireLockError::LockEntryLocked)
        } else {
            // element doesn't exist
            let element = {
                let mut map = self.lock_table[lock_idx]
                    .try_write()
                    .ok_or(TryAcquireLockError::LockTableLocked)?;
                map.entry(k)
                    .or_insert_with(|| {
                        self.size.fetch_add(1, Ordering::SeqCst);
                        Arc::new(L::default())
                    })
                    .clone()
            };
            let lock = element.try_lock_owned();
            lock.ok_or(TryAcquireLockError::LockEntryLocked)
        }
    }
}

impl<K: Hash, L: Lock> Drop for LockTable<K, L> {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
struct DropFlag(Arc<AtomicBool>);

#[cfg(test)]
impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn test_acquire_lock_keeps_execution_permit_when_uncontended() {
    let mutex_table = MutexTable::<String>::new(1);
    let released = Arc::new(AtomicBool::new(false));
    let _permit = set_execution_permit(Box::new(DropFlag(released.clone())));

    let _guard = mutex_table.acquire_lock("key".to_string());

    assert!(
        !released.load(Ordering::SeqCst),
        "permit must be kept when the lock is immediately available"
    );
}

#[cfg(not(msim))]
#[tokio::test]
async fn test_acquire_lock_releases_execution_permit_when_contended() {
    let mutex_table = Arc::new(MutexTable::<String>::new(1));
    let holder = mutex_table.acquire_lock("key".to_string());
    let released = Arc::new(AtomicBool::new(false));

    let waiter = std::thread::spawn({
        let mutex_table = mutex_table.clone();
        let released = released.clone();
        move || {
            let _permit = set_execution_permit(Box::new(DropFlag(released)));
            drop(mutex_table.acquire_lock("key".to_string()));
        }
    });

    while !released.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(1));
    }
    drop(holder);
    waiter.join().unwrap();
}

#[cfg(all(msim, test))]
#[sui_macros::sim_test]
async fn test_contended_acquire_lock_releases_execution_permit_and_yields() {
    let mutex_table = Arc::new(MutexTable::<String>::new(1));
    let holder_acquired = Arc::new(AtomicBool::new(false));
    let release_holder = Arc::new(AtomicBool::new(false));

    let holder = tokio::task::spawn_blocking({
        let mutex_table = mutex_table.clone();
        let holder_acquired = holder_acquired.clone();
        let release_holder = release_holder.clone();
        move || {
            let guard = mutex_table.acquire_lock("key".to_string());
            holder_acquired.store(true, Ordering::SeqCst);
            while !release_holder.load(Ordering::SeqCst) {
                msim::task::yield_blocking();
            }
            drop(guard);
        }
    });

    while !holder_acquired.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    let released = Arc::new(AtomicBool::new(false));
    let waiter = tokio::task::spawn_blocking({
        let released = released.clone();
        move || {
            let _permit = set_execution_permit(Box::new(DropFlag(released)));
            drop(mutex_table.acquire_lock("key".to_string()));
        }
    });

    while !released.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    release_holder.store(true, Ordering::SeqCst);

    holder.await.unwrap();
    waiter.await.unwrap();
}

#[cfg(test)]
#[sui_macros::sui_test]
// Tests that mutex table provides parallelism on the individual mutex level,
// e.g. that locks for different entries do not block entire bucket if it needs to wait on individual lock
async fn test_mutex_table_concurrent_in_same_bucket() {
    let mutex_table = Arc::new(MutexTable::<String>::new(1));
    let holder_acquired = Arc::new(AtomicBool::new(false));
    let release_holder = Arc::new(AtomicBool::new(false));

    let holder = tokio::task::spawn_blocking({
        let mutex_table = mutex_table.clone();
        let holder_acquired = holder_acquired.clone();
        let release_holder = release_holder.clone();
        move || {
            let guard = mutex_table.acquire_lock("john".to_string());
            holder_acquired.store(true, Ordering::SeqCst);
            while !release_holder.load(Ordering::SeqCst) {
                #[cfg(msim)]
                msim::task::yield_blocking();
                #[cfg(not(msim))]
                std::thread::yield_now();
            }
            drop(guard);
        }
    });

    while !holder_acquired.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    let waiter_contended = Arc::new(AtomicBool::new(false));

    let waiter = tokio::task::spawn_blocking({
        let mutex_table = mutex_table.clone();
        let waiter_contended = waiter_contended.clone();
        move || {
            let _permit = set_execution_permit(Box::new(DropFlag(waiter_contended)));
            drop(mutex_table.acquire_lock("john".to_string()));
        }
    });

    while !waiter_contended.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    assert!(
        mutex_table.try_acquire_lock("jane".to_string()).is_ok(),
        "a waiter on one key must not hold the shared shard lock"
    );

    release_holder.store(true, Ordering::SeqCst);
    holder.await.unwrap();
    waiter.await.unwrap();
}

#[tokio::test]
async fn test_mutex_table() {
    // Disable bg cleanup with Duration.MAX for initial delay
    let mutex_table =
        MutexTable::<String>::new_with_cleanup(1, Duration::from_secs(10), Duration::MAX, 1000);
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
    let map = mutex_table.lock_table.first().as_ref().unwrap().try_read();
    assert!(map.is_some());
    assert_eq!(map.unwrap().len(), 2);
    drop(john2);
    MutexTable::cleanup(mutex_table.lock_table.clone());
    let map = mutex_table.lock_table.first().as_ref().unwrap().try_read();
    assert!(map.is_some());
    assert_eq!(map.unwrap().len(), 1);
    drop(jane);
    MutexTable::cleanup(mutex_table.lock_table.clone());
    let map = mutex_table.lock_table.first().as_ref().unwrap().try_read();
    assert!(map.is_some());
    assert!(map.unwrap().is_empty());
}

#[tokio::test]
async fn test_acquire_locks() {
    let mutex_table =
        RwLockTable::<String>::new_with_cleanup(1, Duration::from_secs(10), Duration::MAX, 1000);
    let object_1 = "object 1".to_string();
    let object_2 = "object 2".to_string();
    let object_3 = "object 3".to_string();

    // ensure even with duplicate objects we succeed acquiring their locks
    let objects = vec![
        object_1.clone(),
        object_2.clone(),
        object_2,
        object_1.clone(),
        object_3,
        object_1,
    ];

    let locks = mutex_table.acquire_locks(objects.clone().into_iter());
    assert_eq!(locks.len(), 3);

    for object in objects.clone() {
        assert!(mutex_table.try_acquire_lock(object).is_err());
    }

    drop(locks);
    let locks = mutex_table.acquire_locks(objects.into_iter());
    assert_eq!(locks.len(), 3);
}

#[tokio::test]
async fn test_read_locks() {
    let mutex_table =
        RwLockTable::<String>::new_with_cleanup(1, Duration::from_secs(10), Duration::MAX, 1000);
    let lock = "lock".to_string();
    let locks1 = mutex_table.acquire_read_locks(vec![lock.clone()]);
    assert!(mutex_table.try_acquire_lock(lock.clone()).is_err());
    let locks2 = mutex_table.acquire_read_locks(vec![lock.clone()]);
    drop(locks1);
    drop(locks2);
    assert!(mutex_table.try_acquire_lock(lock.clone()).is_ok());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_mutex_table_bg_cleanup() {
    let mutex_table = MutexTable::<String>::new_with_cleanup(
        1,
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
        let locked = entry.read();
        assert!(locked.is_empty());
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_mutex_table_bg_cleanup_with_size_threshold() {
    // set up the table to never trigger cleanup because of time period but only size threshold
    let mutex_table =
        MutexTable::<String>::new_with_cleanup(1, Duration::MAX, Duration::from_secs(1), 5);
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
        let locked = entry.read();
        assert!(locked.is_empty());
    }
}
