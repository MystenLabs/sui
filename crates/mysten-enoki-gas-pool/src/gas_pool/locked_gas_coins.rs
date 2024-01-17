// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::gas_pool::gas_pool_db::GasPoolStore;
use crate::metrics::GasPoolMetrics;
use crate::types::GasCoin;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeSet, BinaryHeap, HashMap};
use std::ops::Add;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_types::base_types::{ObjectID, SuiAddress};
use tracing::debug;

pub struct LockedGasCoins {
    // TODO: The mutex can be sharded among sponsor addresses.
    mutex: Mutex<LockedGasCoinsInner>,
    persisted_store: GasPoolStore,
}

#[derive(Default)]
struct LockedGasCoinsInner {
    /// A lookup map from each ObjectID to the corresponding lock information.
    locked_gas_coins: HashMap<ObjectID, CoinLockInfo>,
    // Use Reverse so that it becomes a min-heap.
    unlock_queue: BinaryHeap<Reverse<CoinLockInfo>>,
}

impl LockedGasCoinsInner {
    pub fn add_locked_coins(&mut self, lock_info: &CoinLockInfo) {
        self.locked_gas_coins.extend(
            lock_info
                .inner
                .objects
                .iter()
                .map(|id| (*id, lock_info.clone())),
        );
        self.unlock_queue.push(Reverse(lock_info.clone()));
    }

    /// If any coin is not currently locked, or they are not all locked by the
    /// same transaction, returns error.
    pub fn remove_locked_coins(&mut self, gas_coins: &[ObjectID]) -> anyhow::Result<CoinLockInfo> {
        if gas_coins.is_empty() {
            anyhow::bail!("No gas coin provided");
        }
        let unique_gas_coins = gas_coins.iter().cloned().collect::<BTreeSet<_>>();
        let mut unique_lock_info = None;
        for c in gas_coins {
            if let Some(lock_info) = self.locked_gas_coins.get(c) {
                if let Some(unique_lock_info) = unique_lock_info {
                    // TODO: Make the comparison more efficient.
                    if unique_lock_info != lock_info {
                        anyhow::bail!("Some gas coins are locked by different transaction")
                    }
                }
                unique_lock_info = Some(lock_info);
            } else {
                anyhow::bail!("Coin {} is not locked", c)
            }
        }
        let unique_lock_info = unique_lock_info.unwrap().clone();
        // unwrap safe because we have checked that gas_coins is not empty,
        // and one iteration will either return early or set unique_lock_info.
        if unique_lock_info.inner.objects != unique_gas_coins {
            anyhow::bail!("Gas coins provided are inconsistent with the locked ones");
        }
        debug!(
            "Removing locked gas coins belong to the lock info: {:?}",
            unique_lock_info
        );
        for c in gas_coins {
            self.locked_gas_coins.remove(c);
        }
        Ok(unique_lock_info)
        // We don't remove them in the unlock queue because it's too expensive to remove
        // by ID from a heap. Eventually they will pop out and be removed anyway.
    }

    pub fn unlock_if_expired(&mut self) -> Vec<CoinLockInfo> {
        let now = Utc::now();
        let mut unlocked_coins = Vec::new();
        while let Some(coin_info) = self.unlock_queue.peek() {
            if coin_info.0.inner.unlock_time <= now {
                let coin_info = self.unlock_queue.pop().unwrap().0;
                // If we fail to remove these coins from the locked coins, it means they have already
                // been released proactively.
                // Only return them if we can remove them from the locked coins.
                if self
                    .remove_locked_coins(
                        &coin_info.inner.objects.iter().cloned().collect::<Vec<_>>(),
                    )
                    .is_ok()
                {
                    debug!(
                        "Coins {:?} will be unlocked since its lock time expired. Current time: {:?}",
                        coin_info.inner, now
                    );
                    unlocked_coins.push(coin_info);
                }
            } else {
                break;
            }
        }
        unlocked_coins
    }

    #[cfg(test)]
    pub fn get_locked_coins_and_check_consistency(&self) -> Vec<CoinLockInfo> {
        let mut unique_lock_infos = BTreeSet::new();
        for (id, lock_info) in &self.locked_gas_coins {
            assert!(lock_info.inner.objects.contains(id));
            for id in &lock_info.inner.objects {
                assert_eq!(self.locked_gas_coins.get(id), Some(lock_info));
            }
            unique_lock_infos.insert(lock_info.inner.objects.first().unwrap());
        }
        let mut results = vec![];
        for Reverse(queued_lock_info) in &self.unlock_queue {
            let first_id = queued_lock_info.inner.objects.first().unwrap();
            let Some(lock_info) = self.locked_gas_coins.get(first_id) else {
                assert!(queued_lock_info.inner.unlock_time <= Utc::now());
                continue;
            };
            assert_eq!(lock_info, queued_lock_info);
            results.push(lock_info.clone());
        }
        assert_eq!(unique_lock_infos.len(), results.len());
        results
    }
}

impl LockedGasCoins {
    pub fn new(local_db_path: PathBuf, metrics: Arc<GasPoolMetrics>) -> Self {
        let mut inner = LockedGasCoinsInner::default();
        let persisted_store = GasPoolStore::new(&local_db_path, metrics);
        let recovered_locked_coins = persisted_store.get_all_locked_gas_coins_during_init();
        debug!("Recovered locked coins: {:?}", recovered_locked_coins);
        for lock_info in recovered_locked_coins {
            for id in &lock_info.inner.objects {
                inner.locked_gas_coins.insert(*id, lock_info.clone());
            }
            inner.unlock_queue.push(Reverse(lock_info));
        }
        Self {
            mutex: Mutex::new(inner),
            persisted_store,
        }
    }

    pub fn add_locked_coins(
        &self,
        sponsor: SuiAddress,
        gas_coins: &[GasCoin],
        lock_duration: Duration,
    ) {
        let unlock_time = Utc::now().add(lock_duration);
        let lock_info = CoinLockInfo::new(
            sponsor,
            gas_coins.iter().map(|c| c.object_ref.0).collect(),
            unlock_time,
        );
        self.mutex.lock().add_locked_coins(&lock_info);
        self.persisted_store.add_locked_gas_coins(&lock_info);
        debug!("Added coin lock info: {:?}", lock_info);
    }

    pub fn unlock_if_expired(&self) -> Vec<CoinLockInfo> {
        let lock_infos = self.mutex.lock().unlock_if_expired();
        for lock_info in &lock_infos {
            self.persisted_store.remove_locked_gas_coins(lock_info);
        }
        lock_infos
    }

    pub fn remove_locked_coins(&self, gas_coins: &[ObjectID]) -> anyhow::Result<()> {
        let lock_info = self.mutex.lock().remove_locked_coins(gas_coins)?;
        self.persisted_store.remove_locked_gas_coins(&lock_info);
        Ok(())
    }

    #[cfg(test)]
    pub fn get_locked_coins_and_check_consistency(&self) -> Vec<CoinLockInfo> {
        self.mutex.lock().get_locked_coins_and_check_consistency()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CoinLockInfo {
    pub inner: Arc<CoinLockInfoInner>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoinLockInfoInner {
    pub sponsor: SuiAddress,
    pub objects: BTreeSet<ObjectID>,
    pub unlock_time: DateTime<Utc>,
}

impl CoinLockInfo {
    pub fn new(
        sponsor: SuiAddress,
        objects: BTreeSet<ObjectID>,
        unlock_time: DateTime<Utc>,
    ) -> Self {
        Self {
            inner: Arc::new(CoinLockInfoInner {
                sponsor,
                objects,
                unlock_time,
            }),
        }
    }
}

impl PartialOrd<Self> for CoinLockInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CoinLockInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        self.inner.unlock_time.cmp(&other.inner.unlock_time)
    }
}
