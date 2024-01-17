// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::gas_pool::locked_gas_coins::CoinLockInfo;
use crate::metrics::GasPoolMetrics;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use sui_types::base_types::ObjectID;
use typed_store::metrics::SamplingInterval;
use typed_store::rocks::{DBMap, MetricConf};
use typed_store::traits::TableSummary;
use typed_store::traits::TypedStoreDebug;
use typed_store::Map;
use typed_store_derive::DBMapUtils;

pub struct GasPoolStore {
    tables: GasPoolDbTables,
    metrics: Arc<GasPoolMetrics>,
}

impl GasPoolStore {
    pub fn new(parent_path: &Path, metrics: Arc<GasPoolMetrics>) -> Self {
        Self {
            tables: GasPoolDbTables::open(parent_path),
            metrics,
        }
    }

    pub fn add_locked_gas_coins(&self, coin_lock_info: &CoinLockInfo) {
        let key = coin_lock_info.inner.objects.first().unwrap();
        if self.tables.locked_gas_coins.contains_key(key).unwrap() {
            self.metrics.num_invariant_violations.inc();
            #[cfg(debug_assertions)]
            panic!("CoinLockInfo already exists");
        }
        self.tables
            .locked_gas_coins
            .insert(key, coin_lock_info)
            .unwrap();
    }

    pub fn remove_locked_gas_coins(&self, coin_lock_info: &CoinLockInfo) {
        let key = coin_lock_info.inner.objects.first().unwrap();
        if !self.tables.locked_gas_coins.contains_key(key).unwrap() {
            self.metrics.num_invariant_violations.inc();
            #[cfg(debug_assertions)]
            panic!("CoinLockInfo does not exist");
        }
        self.tables.locked_gas_coins.remove(key).unwrap();
    }

    /// This function should only be called when during initialization of the gas station.
    pub fn get_all_locked_gas_coins_during_init(&self) -> Vec<CoinLockInfo> {
        self.tables
            .locked_gas_coins
            .unbounded_iter()
            .map(|(_, v)| v.clone())
            .collect()
    }
}

#[derive(DBMapUtils)]
struct GasPoolDbTables {
    /// A persisted table that stores all the CoinLockInfo. To avoid storing the same CoinLockInfo,
    /// we use the first ObjectID in the CoinLockInfo as the key for adding and removal.
    locked_gas_coins: DBMap<ObjectID, CoinLockInfo>,
}

impl GasPoolDbTables {
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
}
