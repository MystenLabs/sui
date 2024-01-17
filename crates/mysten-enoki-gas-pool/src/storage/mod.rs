// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::GasPoolStorageConfig;
use crate::metrics::StoragePoolMetrics;
use crate::storage::rocksdb::rocksdb_rpc_client::RocksDbRpcClient;
use crate::storage::rocksdb::RocksDBStorage;
use crate::types::GasCoin;
use std::sync::Arc;
use sui_types::base_types::{ObjectID, SuiAddress};

pub mod rocksdb;

pub const MAX_GAS_PER_QUERY: usize = 256;

/// Defines the trait for a storage that manages gas coins.
/// It is expected to support concurrent access and manage atomicity internally.
/// It supports multiple addresses each with its own gas coin queue.
#[async_trait::async_trait]
pub trait Storage: Sync + Send {
    /// Reserve gas coins for the given sponsor address, with total coin balance >= target_budget.
    /// If there is not enough balance, returns error.
    /// The implementation is required to guarantee that:
    /// 1. It never returns the same coin to multiple callers.
    /// 2. It keeps a record of the reserved coins with timestamp, so that in the case
    ///    when caller forgets to release them, some cleanup process can clean them up latter.
    /// 3. It should never return more than 256 coins at a time since that's the upper bound of gas.
    async fn reserve_gas_coins(
        &self,
        sponsor_address: SuiAddress,
        target_budget: u64,
    ) -> anyhow::Result<Vec<GasCoin>>;

    /// Release reserved gas coins.
    /// \released_gas_coins contain previously reserved coins that are now released.
    /// \deleted_gas_coins contain previously reserved coins that are now deleted.
    /// This function can be called both when the pool is being initialized, or when
    /// gas coins are being released after usage.
    /// It assumes that the caller is responsible for ensuring that they never call
    /// this function with the same coin twice without re-reservation.
    /// The implementation needs to guarantee that:
    /// 1. Released gas coins become available for reservation immediately.
    /// 2. Deleted gas coins are no longer available for reservation.
    async fn update_gas_coins(
        &self,
        sponsor_address: SuiAddress,
        released_gas_coins: Vec<GasCoin>,
        deleted_gas_coins: Vec<ObjectID>,
    ) -> anyhow::Result<()>;

    async fn check_health(&self) -> anyhow::Result<()>;

    #[cfg(test)]
    async fn get_available_coin_count(&self, sponsor_address: SuiAddress) -> usize;

    #[cfg(test)]
    async fn get_total_available_coin_balance(&self, sponsor_address: SuiAddress) -> u64;

    #[cfg(test)]
    async fn get_reserved_coin_count(&self) -> usize;

    // TODO: Add APIs to support collecting coins that were forgotten to be released.
}

pub async fn connect_storage(config: &GasPoolStorageConfig) -> Arc<dyn Storage> {
    let storage: Arc<dyn Storage> = match config {
        GasPoolStorageConfig::LocalRocksDbForTesting { db_path } => Arc::new(RocksDBStorage::new(
            db_path,
            StoragePoolMetrics::new_for_testing(),
        )),
        GasPoolStorageConfig::RemoteRocksDb { db_rpc_url } => {
            Arc::new(RocksDbRpcClient::new(db_rpc_url.clone()))
        }
    };
    storage
        .check_health()
        .await
        .expect("Unable to connect to the storage layer");
    storage
}

#[cfg(test)]
mod tests {
    use crate::storage::rocksdb::rocksdb_rpc_server::RocksDbServer;
    use crate::storage::{Storage, MAX_GAS_PER_QUERY};
    use crate::types::GasCoin;
    use std::sync::Arc;
    use sui_types::base_types::{random_object_ref, SuiAddress};

    async fn assert_coin_count(
        sponsor_address: SuiAddress,
        storage: &Arc<dyn Storage>,
        available: usize,
        reserved: usize,
    ) {
        assert_eq!(
            storage.get_available_coin_count(sponsor_address).await,
            available
        );
        assert_eq!(storage.get_reserved_coin_count().await, reserved);
    }

    async fn setup(init_balance: Vec<(SuiAddress, Vec<u64>)>) -> Arc<dyn Storage> {
        let storage = RocksDbServer::start_storage_server_for_testing().await;
        for (sponsor, amounts) in init_balance {
            let gas_coins = amounts
                .into_iter()
                .map(|amount| GasCoin {
                    object_ref: random_object_ref(),
                    balance: amount,
                })
                .collect::<Vec<_>>();
            for chunk in gas_coins.chunks(5000) {
                storage
                    .update_gas_coins(sponsor, chunk.to_vec(), vec![])
                    .await
                    .unwrap();
            }
        }
        storage
    }

    #[tokio::test]
    async fn test_successful_reservation() {
        // Create a gas pool of 100000 coins, each with balance of 1.
        let sponsor = SuiAddress::random_for_testing_only();
        let storage = setup(vec![(sponsor, vec![1; 100000])]).await;
        assert_coin_count(sponsor, &storage, 100000, 0).await;
        let mut cur_available = 100000;
        for i in 1..=MAX_GAS_PER_QUERY {
            let reserved_gas_coins = storage.reserve_gas_coins(sponsor, i as u64).await.unwrap();
            assert_eq!(reserved_gas_coins.len(), i);
            cur_available -= i;
        }
        assert_coin_count(sponsor, &storage, cur_available, 100000 - cur_available).await;
    }

    #[tokio::test]
    async fn test_max_gas_coin_per_query() {
        let sponsor = SuiAddress::random_for_testing_only();
        let storage = setup(vec![(sponsor, vec![1; MAX_GAS_PER_QUERY + 1])]).await;
        assert!(storage
            .reserve_gas_coins(sponsor, (MAX_GAS_PER_QUERY + 1) as u64)
            .await
            .is_err());
        assert_coin_count(sponsor, &storage, MAX_GAS_PER_QUERY + 1, 0).await;
    }

    #[tokio::test]
    async fn test_insufficient_pool_budget() {
        let sponsor = SuiAddress::random_for_testing_only();
        let storage = setup(vec![(sponsor, vec![1; 100])]).await;
        assert!(storage.reserve_gas_coins(sponsor, 101).await.is_err());
        assert_coin_count(sponsor, &storage, 100, 0).await;
    }

    #[tokio::test]
    async fn test_coin_release() {
        let sponsor = SuiAddress::random_for_testing_only();
        let storage = setup(vec![(sponsor, vec![1; 100])]).await;
        for _ in 0..100 {
            // Keep reserving and putting them back.
            // Should be able to repeat this process indefinitely if balance are not changed.
            let reserved_gas_coins = storage.reserve_gas_coins(sponsor, 99).await.unwrap();
            assert_eq!(reserved_gas_coins.len(), 99);
            assert_coin_count(sponsor, &storage, 1, 99).await;
            storage
                .update_gas_coins(sponsor, reserved_gas_coins, vec![])
                .await
                .unwrap();
            assert_coin_count(sponsor, &storage, 100, 0).await;
        }
    }

    #[tokio::test]
    async fn test_coin_release_with_updated_balance() {
        let sponsor = SuiAddress::random_for_testing_only();
        let storage = setup(vec![(sponsor, vec![1; 100])]).await;
        for _ in 0..10 {
            let mut reserved_gas_coins = storage.reserve_gas_coins(sponsor, 10).await.unwrap();
            assert_eq!(
                reserved_gas_coins.iter().map(|c| c.balance).sum::<u64>(),
                10
            );
            for reserved_gas_coin in reserved_gas_coins.iter_mut() {
                if reserved_gas_coin.balance > 0 {
                    reserved_gas_coin.balance -= 1;
                }
            }
            storage
                .update_gas_coins(sponsor, reserved_gas_coins, vec![])
                .await
                .unwrap();
        }
        assert_coin_count(sponsor, &storage, 100, 0).await;
        assert_eq!(storage.get_total_available_coin_balance(sponsor).await, 0);
        assert!(storage.reserve_gas_coins(sponsor, 1).await.is_err());
    }

    #[tokio::test]
    async fn test_multiple_sponsors() {
        let sponsors = (0..10)
            .map(|_| SuiAddress::random_for_testing_only())
            .collect::<Vec<_>>();
        let storage = setup(
            sponsors
                .iter()
                .map(|sponsor| (*sponsor, vec![1; 100]))
                .collect(),
        )
        .await;
        for sponsor in sponsors.iter() {
            let gas_coins = storage.reserve_gas_coins(*sponsor, 50).await.unwrap();
            assert_eq!(gas_coins.len(), 50);
        }
        for sponsor in sponsors.iter() {
            assert_coin_count(*sponsor, &storage, 50, 500).await;
        }
    }

    #[tokio::test]
    async fn test_invalid_sponsor() {
        let sponsor = SuiAddress::random_for_testing_only();
        let storage = setup(vec![(sponsor, vec![1; 100])]).await;
        assert!(storage
            .reserve_gas_coins(SuiAddress::random_for_testing_only(), 1)
            .await
            .is_err());
        assert_eq!(
            storage.reserve_gas_coins(sponsor, 1).await.unwrap().len(),
            1
        )
    }

    #[tokio::test]
    async fn test_deleted_objects() {
        let sponsor = SuiAddress::random_for_testing_only();
        let storage = setup(vec![(sponsor, vec![1; 100])]).await;
        let mut reserved_gas_coins = storage.reserve_gas_coins(sponsor, 100).await.unwrap();
        assert_eq!(reserved_gas_coins.len(), 100);
        let deleted_gas_coins = reserved_gas_coins
            .drain(0..50)
            .map(|c| c.object_ref.0)
            .collect::<Vec<_>>();
        storage
            .update_gas_coins(sponsor, reserved_gas_coins, deleted_gas_coins)
            .await
            .unwrap();
        assert_coin_count(sponsor, &storage, 50, 0).await;
    }

    #[tokio::test]
    async fn test_concurrent_reservation() {
        let sponsor = SuiAddress::random_for_testing_only();
        let storage = setup(vec![(sponsor, vec![1; 100000])]).await;
        let mut handles = vec![];
        for _ in 0..10 {
            let storage = storage.clone();
            handles.push(tokio::spawn(async move {
                let mut reserved_gas_coins = vec![];
                for _ in 0..100 {
                    reserved_gas_coins.extend(storage.reserve_gas_coins(sponsor, 3).await.unwrap());
                }
                reserved_gas_coins
            }));
        }
        let mut reserved_gas_coins = vec![];
        for handle in handles {
            reserved_gas_coins.extend(handle.await.unwrap());
        }
        let count = reserved_gas_coins.len();
        // Check that all object IDs are unique in all reservations.
        reserved_gas_coins.sort_by_key(|c| c.object_ref.0);
        reserved_gas_coins.dedup_by_key(|c| c.object_ref.0);
        assert_eq!(reserved_gas_coins.len(), count);
    }
}
