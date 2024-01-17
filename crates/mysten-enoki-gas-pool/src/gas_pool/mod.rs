// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod gas_pool_core;
mod gas_pool_db;
mod locked_gas_coins;

#[cfg(test)]
mod tests {
    use crate::config::GasStationConfig;
    use crate::gas_pool::gas_pool_core::GasPoolContainer;
    use crate::gas_pool_initializer::GasPoolInitializer;
    use crate::metrics::GasPoolMetrics;
    use crate::test_env::{create_test_transaction, start_gas_station, start_sui_cluster};
    use std::sync::Arc;
    use std::time::Duration;
    use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
    use sui_types::gas_coin::MIST_PER_SUI;
    use tokio::time;

    #[tokio::test]
    async fn test_station_reserve_gas() {
        let (_test_cluster, container) =
            start_gas_station(vec![MIST_PER_SUI; 10], MIST_PER_SUI).await;
        let station = container.get_gas_pool_arc();
        let (sponsor1, gas_coins) = station
            .reserve_gas(None, MIST_PER_SUI * 3, Duration::from_secs(10))
            .await
            .unwrap();
        assert_eq!(gas_coins.len(), 3);
        assert_eq!(station.query_pool_available_coin_count(sponsor1).await, 7);
        let (sponsor2, gas_coins) = station
            .reserve_gas(None, MIST_PER_SUI * 7, Duration::from_secs(10))
            .await
            .unwrap();
        assert_eq!(gas_coins.len(), 7);
        assert_eq!(sponsor1, sponsor2);
        assert_eq!(station.query_pool_available_coin_count(sponsor1).await, 0);
        assert!(station
            .reserve_gas(None, 1, Duration::from_secs(10))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_e2e_gas_station_flow() {
        let (test_cluster, container) = start_gas_station(vec![MIST_PER_SUI], MIST_PER_SUI).await;
        let station = container.get_gas_pool_arc();
        assert!(station
            .reserve_gas(None, MIST_PER_SUI + 1, Duration::from_secs(10))
            .await
            .is_err());

        let (sponsor, gas_coins) = station
            .reserve_gas(None, MIST_PER_SUI, Duration::from_secs(10))
            .await
            .unwrap();
        assert_eq!(gas_coins.len(), 1);
        assert_eq!(station.query_pool_available_coin_count(sponsor).await, 0);
        assert!(station
            .reserve_gas(None, 1, Duration::from_secs(10))
            .await
            .is_err());

        let (tx_data, user_sig) = create_test_transaction(&test_cluster, sponsor, gas_coins).await;
        let effects = station
            .execute_transaction(tx_data, user_sig)
            .await
            .unwrap();
        assert!(effects.status().is_ok());
        assert_eq!(station.query_pool_available_coin_count(sponsor).await, 1);
    }

    #[tokio::test]
    async fn test_coin_expiration() {
        let (test_cluster, container) = start_gas_station(vec![MIST_PER_SUI], MIST_PER_SUI).await;
        let station = container.get_gas_pool_arc();
        let (sponsor, gas_coins) = station
            .reserve_gas(None, MIST_PER_SUI, Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(gas_coins.len(), 1);
        assert_eq!(station.query_pool_available_coin_count(sponsor).await, 0);
        assert!(station
            .reserve_gas(None, 1, Duration::from_secs(1))
            .await
            .is_err());
        // Sleep a little longer to give it enough time to expire.
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert_eq!(station.query_pool_available_coin_count(sponsor).await, 1);
        let (tx_data, user_sig) = create_test_transaction(&test_cluster, sponsor, gas_coins).await;
        assert!(station
            .execute_transaction(tx_data, user_sig)
            .await
            .is_err());
        station
            .reserve_gas(None, 1, Duration::from_secs(1))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_incomplete_gas_usage() {
        let (test_cluster, container) =
            start_gas_station(vec![MIST_PER_SUI; 10], MIST_PER_SUI).await;
        let station = container.get_gas_pool_arc();
        let (sponsor, gas_coins) = station
            .reserve_gas(None, MIST_PER_SUI * 3, Duration::from_secs(10))
            .await
            .unwrap();
        assert_eq!(gas_coins.len(), 3);

        // Remove one gas object from the reserved list and only use the two.
        let mut incomplete_gas_coins = gas_coins.clone();
        incomplete_gas_coins.pop().unwrap();
        let (tx_data, user_sig) =
            create_test_transaction(&test_cluster, sponsor, incomplete_gas_coins).await;
        // It should fail because it's inconsistent with the reservation.
        assert!(station
            .execute_transaction(tx_data, user_sig)
            .await
            .is_err());

        let (tx_data, user_sig) = create_test_transaction(&test_cluster, sponsor, gas_coins).await;
        let effects = station
            .execute_transaction(tx_data, user_sig)
            .await
            .unwrap();
        assert!(effects.status().is_ok());
    }

    #[tokio::test]
    async fn test_mixed_up_gas_coins() {
        let (test_cluster, container) =
            start_gas_station(vec![MIST_PER_SUI; 10], MIST_PER_SUI).await;
        let station = container.get_gas_pool_arc();
        let (sponsor, gas_coins1) = station
            .reserve_gas(None, MIST_PER_SUI * 3, Duration::from_secs(10))
            .await
            .unwrap();
        assert_eq!(gas_coins1.len(), 3);
        let (_, gas_coins2) = station
            .reserve_gas(None, MIST_PER_SUI, Duration::from_secs(10))
            .await
            .unwrap();
        assert_eq!(gas_coins2.len(), 1);

        // Mix up gas coins from two reservations.
        let mut mixed_up_gas_coins = gas_coins1.clone();
        mixed_up_gas_coins[0] = gas_coins2[0];
        let (tx_data, user_sig) =
            create_test_transaction(&test_cluster, sponsor, mixed_up_gas_coins).await;
        assert!(station
            .execute_transaction(tx_data, user_sig)
            .await
            .is_err());

        let (tx_data, user_sig) = create_test_transaction(&test_cluster, sponsor, gas_coins1).await;
        let effects = station
            .execute_transaction(tx_data, user_sig)
            .await
            .unwrap();
        assert!(effects.status().is_ok());
    }

    #[tokio::test]
    async fn test_crash_restart() {
        let (_test_cluster, config) = start_sui_cluster(vec![MIST_PER_SUI; 10]).await;
        let GasStationConfig {
            keypair,
            gas_pool_config,
            fullnode_url,
            local_db_path,
            ..
        } = config;
        let keypair = Arc::new(keypair);
        let storage = GasPoolInitializer::run(
            fullnode_url.as_str(),
            &gas_pool_config,
            MIST_PER_SUI,
            keypair.clone(),
        )
        .await;

        {
            let container = GasPoolContainer::new(
                keypair.clone(),
                storage.clone(),
                fullnode_url.as_str(),
                GasPoolMetrics::new_for_testing(),
                local_db_path.clone(),
            )
            .await;

            let station = container.get_gas_pool_arc();
            let (_sponsor, gas_coins) = station
                .reserve_gas(None, MIST_PER_SUI * 3, Duration::from_secs(5))
                .await
                .unwrap();
            assert_eq!(gas_coins.len(), 3);
            let (_, gas_coins2) = station
                .reserve_gas(None, MIST_PER_SUI * 5, Duration::from_secs(5))
                .await
                .unwrap();
            assert_eq!(gas_coins2.len(), 5);
        }
        // Drop the station so we can restart it.

        {
            let container = GasPoolContainer::new(
                keypair.clone(),
                storage.clone(),
                fullnode_url.as_str(),
                GasPoolMetrics::new_for_testing(),
                local_db_path.clone(),
            )
            .await;
            let locked_gas = container
                .get_gas_pool_arc()
                .get_locked_coins_and_check_consistency();
            assert_eq!(locked_gas.len(), 2);
            assert_eq!(
                locked_gas[0].inner.objects.len() + locked_gas[1].inner.objects.len(),
                8
            );
            time::sleep(Duration::from_secs(10)).await;
        }
        // Drop the station and restart it again.

        let container = GasPoolContainer::new(
            keypair,
            storage,
            fullnode_url.as_str(),
            GasPoolMetrics::new_for_testing(),
            local_db_path,
        )
        .await;
        let locked_gas = container
            .get_gas_pool_arc()
            .get_locked_coins_and_check_consistency();
        assert!(locked_gas.is_empty());
    }
}
