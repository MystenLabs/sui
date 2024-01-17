// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod rocksdb_rpc_client;
pub mod rocksdb_rpc_server;
mod rocksdb_rpc_types;

use crate::metrics::StoragePoolMetrics;
use crate::storage::{Storage, MAX_GAS_PER_QUERY};
use crate::types::GasCoin;
use anyhow::bail;
use chrono::Utc;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use sui_types::base_types::{ObjectID, SuiAddress};
use tracing::warn;
use typed_store::metrics::SamplingInterval;
use typed_store::rocks::{DBMap, MetricConf};
use typed_store::traits::{TableSummary, TypedStoreDebug};
use typed_store::Map;
use typed_store_derive::DBMapUtils;

// TODO: Add more logging and metrics

pub struct RocksDBStorage {
    tables: Arc<RocksDBStorageTables>,
    /// The RwLock is to allow adding new addresses.
    /// The inner mutex is used to ensure operations on the available coin queue of each address are atomic.
    mutexes: RwLock<HashMap<SuiAddress, Mutex<PerAddressState>>>,
    pub metrics: Arc<StoragePoolMetrics>,
}

#[derive(Default)]
struct PerAddressState {
    first_available_coin_index: Option<u64>,
    next_new_gas_coin_index: u64,
}

type ReserveTimeMs = u64;

#[derive(DBMapUtils)]
struct RocksDBStorageTables {
    available_gas_coins: DBMap<(SuiAddress, u64), GasCoin>,
    /// Gas Stations are expected to call `put_back_gas_coin` within a certain amount of time after calling `take_first_available_gas_coin`.
    /// We still keep the reserved_gas_coins list such that even if in the event that
    /// sometimes Gas Stations crash and don't release them, we can run a GC process to release them
    /// from time to time.
    reserved_gas_coins: DBMap<ObjectID, ReserveTimeMs>,
}

impl RocksDBStorageTables {
    pub fn path(parent_path: &Path) -> PathBuf {
        parent_path.join("gas_pool")
    }

    pub fn open(parent_path: &Path) -> Self {
        Self::open_tables_read_write(
            Self::path(parent_path),
            MetricConf::default().with_sampling(SamplingInterval::new(Duration::from_secs(60), 0)),
            None,
            None,
        )
    }

    /// Take the first available gas coins with the smallest index,
    /// until we have enough gas coins to satisfy the target budget, or we have gone
    /// over the per-query limit.
    /// If successful, we put them in the reserved gas coins table.
    pub fn take_first_available_gas_coins(
        &self,
        mutex: &Mutex<PerAddressState>,
        sponsor_address: SuiAddress,
        target_budget: u64,
    ) -> anyhow::Result<Vec<GasCoin>> {
        let mut guard = mutex.lock();
        let Some(mut first_available_coin_index) = guard.first_available_coin_index else {
            bail!("No available gas coins for sponsor {:?}", sponsor_address);
        };

        let mut indexes = vec![];
        let mut coins = vec![];
        let mut total_balance = 0;
        while total_balance < target_budget && coins.len() < MAX_GAS_PER_QUERY {
            let key = (sponsor_address, first_available_coin_index);
            if let Some(coin) = self.available_gas_coins.get(&key)? {
                total_balance += coin.balance;
                coins.push(coin);
                indexes.push(key);
                first_available_coin_index += 1;
            } else {
                // The first available coin must always be available because otherwise
                // first_available_coin_index would have been set to None.
                assert!(!indexes.is_empty());
                break;
            }
        }

        if total_balance < target_budget {
            warn!(
                "After taking {} gas coins, total balance {} is still less than target budget {}",
                coins.len(),
                total_balance,
                target_budget
            );
            bail!("Unable to find enough gas coins to meet the budget");
        }
        let cur_time = Utc::now().timestamp_millis() as u64;
        let mut batch = self.available_gas_coins.batch();
        batch.delete_batch(&self.available_gas_coins, indexes)?;
        batch.insert_batch(
            &self.reserved_gas_coins,
            coins.iter().map(|c| (c.object_ref.0, cur_time)),
        )?;
        batch.write()?;
        guard.first_available_coin_index =
            if first_available_coin_index == guard.next_new_gas_coin_index {
                None
            } else {
                Some(first_available_coin_index)
            };
        Ok(coins)
    }

    /// Add a list of available gas coins, and put it at the end of the available gas coins queue.
    /// This is done by getting the next index in the available gas coins table, and inserting the coin at that index.
    /// This function can be used both for releasing reserved gas coins and adding new gas coins.
    pub fn update_gas_coins(
        &self,
        mutex: &Mutex<PerAddressState>,
        sponsor_address: SuiAddress,
        released_gas_coins: Vec<GasCoin>,
        deleted_gas_coins: Vec<ObjectID>,
    ) -> anyhow::Result<()> {
        let mut guard = mutex.lock();
        let next_index = guard.next_new_gas_coin_index;
        let mut batch = self.available_gas_coins.batch();
        batch.insert_batch(
            &self.available_gas_coins,
            released_gas_coins
                .iter()
                .enumerate()
                .map(|(offset, c)| ((sponsor_address, next_index + offset as u64), c)),
        )?;
        batch.delete_batch(
            &self.reserved_gas_coins,
            released_gas_coins
                .iter()
                .map(|c| c.object_ref.0)
                .chain(deleted_gas_coins),
        )?;
        batch.write()?;
        guard.next_new_gas_coin_index = next_index + released_gas_coins.len() as u64;
        if guard.first_available_coin_index.is_none() {
            guard.first_available_coin_index = Some(next_index);
        }
        Ok(())
    }

    #[cfg(test)]
    fn iter_available_gas_coins(
        &self,
        sponsor_address: SuiAddress,
    ) -> impl Iterator<Item = ((SuiAddress, u64), GasCoin)> + '_ {
        self.available_gas_coins.iter_with_bounds(
            Some((sponsor_address, 0)),
            Some((sponsor_address, u64::MAX)),
        )
    }
}

impl RocksDBStorage {
    pub fn new(parent_path: &Path, metrics: Arc<StoragePoolMetrics>) -> Self {
        Self {
            tables: Arc::new(RocksDBStorageTables::open(parent_path)),
            mutexes: RwLock::new(HashMap::new()),
            metrics,
        }
    }
}

#[async_trait::async_trait]
impl Storage for RocksDBStorage {
    async fn reserve_gas_coins(
        &self,
        sponsor_address: SuiAddress,
        target_budget: u64,
    ) -> anyhow::Result<Vec<GasCoin>> {
        if target_budget == 0 {
            bail!("Target budget must be positive");
        }
        if let Some(mutex) = self.mutexes.read().get(&sponsor_address) {
            let gas_coins = self.tables.take_first_available_gas_coins(
                mutex,
                sponsor_address,
                target_budget,
            )?;
            self.metrics
                .cur_num_available_gas_coins
                .sub(gas_coins.len() as i64);
            self.metrics
                .cur_num_reserved_gas_coins
                .add(gas_coins.len() as i64);
            self.metrics
                .cur_total_available_gas_balance
                .sub(gas_coins.iter().map(|c| c.balance as i64).sum());
            Ok(gas_coins)
        } else {
            bail!("Invalid sponsor address: {:?}", sponsor_address)
        }
    }

    async fn update_gas_coins(
        &self,
        sponsor_address: SuiAddress,
        released_gas_coins: Vec<GasCoin>,
        deleted_gas_coins: Vec<ObjectID>,
    ) -> anyhow::Result<()> {
        if !self.mutexes.read().contains_key(&sponsor_address) {
            self.mutexes
                .write()
                .insert(sponsor_address, Mutex::new(PerAddressState::default()));
        }
        let released_gas_coins_len = released_gas_coins.len() as i64;
        let released_gas_coin_balance = released_gas_coins.iter().map(|c| c.balance as i64).sum();
        let deleted_gas_coins_len = deleted_gas_coins.len() as i64;
        self.tables.update_gas_coins(
            // unwrap safe because we would add it above if not exist.
            self.mutexes.read().get(&sponsor_address).unwrap(),
            sponsor_address,
            released_gas_coins,
            deleted_gas_coins,
        )?;
        self.metrics
            .cur_num_available_gas_coins
            .add(released_gas_coins_len);
        self.metrics
            .cur_num_reserved_gas_coins
            .sub(released_gas_coins_len + deleted_gas_coins_len);
        self.metrics
            .cur_total_available_gas_balance
            .add(released_gas_coin_balance);
        Ok(())
    }

    async fn check_health(&self) -> anyhow::Result<()> {
        Ok(())
    }

    #[cfg(test)]
    async fn get_available_coin_count(&self, sponsor_address: SuiAddress) -> usize {
        self.tables
            .iter_available_gas_coins(sponsor_address)
            .count()
    }

    #[cfg(test)]
    async fn get_total_available_coin_balance(&self, sponsor_address: SuiAddress) -> u64 {
        self.tables
            .iter_available_gas_coins(sponsor_address)
            .map(|(_, c)| c.balance)
            .sum()
    }

    #[cfg(test)]
    async fn get_reserved_coin_count(&self) -> usize {
        self.tables.reserved_gas_coins.unbounded_iter().count()
    }
}
