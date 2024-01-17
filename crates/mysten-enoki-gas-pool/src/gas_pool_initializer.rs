// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::GasPoolStorageConfig;
use crate::storage::{connect_storage, Storage};
use crate::sui_client::SuiClient;
use crate::types::GasCoin;
use parking_lot::Mutex;
use std::cmp::min;
use std::collections::VecDeque;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::Duration;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_types::base_types::SuiAddress;
use sui_types::coin::{PAY_MODULE_NAME, PAY_SPLIT_N_FUNC_NAME};
use sui_types::crypto::SuiKeyPair;
use sui_types::gas_coin::GAS;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{Argument, Transaction, TransactionData};
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{debug, error};

pub struct GasPoolInitializer {}

#[derive(Clone)]
struct CoinSplitEnv {
    target_init_coin_balance: u64,
    gas_cost_per_object: u64,
    sponsor_address: SuiAddress,
    keypair: Arc<SuiKeyPair>,
    sui_client: SuiClient,
    task_queue: Arc<Mutex<VecDeque<JoinHandle<Vec<GasCoin>>>>>,
    total_coin_count: Arc<AtomicUsize>,
    rgp: u64,
}

impl CoinSplitEnv {
    fn enqueue_task(&self, coin: GasCoin) -> Option<GasCoin> {
        if coin.balance <= (self.gas_cost_per_object + self.target_init_coin_balance) * 2 {
            debug!(
                "Skip splitting coin {:?} because it has small balance",
                coin
            );
            return Some(coin);
        }
        let env = self.clone();
        let task = tokio::task::spawn(async move { env.split_one_gas_coin(coin).await });
        self.task_queue.lock().push_back(task);
        None
    }

    fn increment_total_coin_count_by(&self, delta: usize) {
        println!(
            "Number of coins got so far: {}",
            self.total_coin_count
                .fetch_add(delta, std::sync::atomic::Ordering::Relaxed)
                + delta
        );
    }

    async fn split_one_gas_coin(self, coin: GasCoin) -> Vec<GasCoin> {
        let rgp = self.rgp;
        let split_count = min(
            // Max number of object mutations per transaction is 2048.
            2000,
            coin.balance / (self.gas_cost_per_object + self.target_init_coin_balance),
        );
        debug!(
            "Evenly splitting coin {:?} into {} coins",
            coin, split_count
        );
        let mut pt_builder = ProgrammableTransactionBuilder::new();
        let pure_arg = pt_builder.pure(split_count).unwrap();
        pt_builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            PAY_MODULE_NAME.into(),
            PAY_SPLIT_N_FUNC_NAME.into(),
            vec![GAS::type_tag()],
            vec![Argument::GasCoin, pure_arg],
        );
        let pt = pt_builder.finish();
        let budget = self.gas_cost_per_object * split_count;
        let tx = TransactionData::new_programmable(
            self.sponsor_address,
            vec![coin.object_ref],
            pt,
            budget,
            rgp,
        );
        let tx = Transaction::from_data_and_signer(tx, vec![self.keypair.as_ref()]);
        debug!(
            "Sending transaction for execution. Tx digest: {:?}",
            tx.digest()
        );
        let effects = self
            .sui_client
            .execute_transaction(tx.clone(), Duration::from_secs(10))
            .await
            .expect("Failed to execute transaction after retries, give up");
        assert!(
            effects.status().is_ok(),
            "Transaction failed. This should never happen. Tx: {:?}, effects: {:?}",
            tx,
            effects
        );
        let mut result = vec![];
        let new_coin_balance = (coin.balance - budget) / split_count;
        for created in effects.created() {
            result.extend(self.enqueue_task(GasCoin {
                object_ref: created.reference.to_object_ref(),
                balance: new_coin_balance,
            }));
        }
        let remaining_coin_balance = (coin.balance - new_coin_balance * (split_count - 1)) as i64
            - effects.gas_cost_summary().net_gas_usage();
        result.extend(self.enqueue_task(GasCoin {
            object_ref: effects.gas_object().reference.to_object_ref(),
            balance: remaining_coin_balance as u64,
        }));
        self.increment_total_coin_count_by(result.len() - 1);
        result
    }
}

impl GasPoolInitializer {
    async fn split_gas_coins(coins: Vec<GasCoin>, env: CoinSplitEnv) -> Vec<GasCoin> {
        let total_balance: u64 = coins.iter().map(|c| c.balance).sum();
        println!(
            "Splitting {} coins with total balance of {} into smaller coins with target balance of {}. This will result in close to {} coins",
            coins.len(),
            total_balance,
            env.target_init_coin_balance,
            total_balance / env.target_init_coin_balance,
        );
        let mut result = vec![];
        for coin in coins {
            result.extend(env.enqueue_task(coin));
        }
        loop {
            let Some(task) = env.task_queue.lock().pop_front() else {
                break;
            };
            result.extend(task.await.unwrap());
        }
        let new_total_balance: u64 = result.iter().map(|c| c.balance).sum();
        println!(
            "Splitting finished. Got {} coins. New total balance: {}. Spent {} gas in total",
            result.len(),
            new_total_balance,
            total_balance - new_total_balance
        );
        result
    }

    pub async fn run(
        fullnode_url: &str,
        gas_pool_config: &GasPoolStorageConfig,
        target_init_coin_balance: u64,
        keypair: Arc<SuiKeyPair>,
    ) -> Arc<dyn Storage> {
        let start = Instant::now();
        let sui_client = SuiClient::new(fullnode_url).await;
        let storage = connect_storage(gas_pool_config).await;
        let sponsor_address = (&keypair.public()).into();
        let coins = sui_client.get_all_owned_sui_coins(sponsor_address).await;
        let total_coin_count = Arc::new(AtomicUsize::new(coins.len()));
        let rgp = sui_client.get_reference_gas_price().await;
        if coins.is_empty() {
            error!("The account doesn't own any gas coins");
            return storage;
        }
        let gas_cost_per_object = sui_client
            .calibrate_gas_cost_per_object(sponsor_address, &coins[0])
            .await;
        debug!("Calibrated gas cost per object: {:?}", gas_cost_per_object);
        let result = Self::split_gas_coins(
            coins,
            CoinSplitEnv {
                target_init_coin_balance,
                gas_cost_per_object,
                sponsor_address,
                keypair,
                sui_client,
                task_queue: Default::default(),
                total_coin_count,
                rgp,
            },
        )
        .await;
        for chunk in result.chunks(5000) {
            storage
                .update_gas_coins(sponsor_address, chunk.to_vec(), vec![])
                .await
                .unwrap();
        }
        println!("Pool initialization took {:?}s", start.elapsed().as_secs());
        storage
    }
}

#[cfg(test)]
mod tests {
    use crate::config::GasStationConfig;
    use crate::gas_pool_initializer::GasPoolInitializer;
    use crate::test_env::start_sui_cluster;
    use std::sync::Arc;
    use sui_types::gas_coin::MIST_PER_SUI;

    // TODO: Add more accurate tests.

    #[tokio::test]
    async fn test_basic_init_flow() {
        telemetry_subscribers::init_for_testing();
        let (_cluster, config) = start_sui_cluster(vec![1000 * MIST_PER_SUI]).await;
        let GasStationConfig {
            keypair,
            gas_pool_config,
            fullnode_url,
            ..
        } = config;
        let sponsor = (&keypair.public()).into();
        let keypair = Arc::new(keypair);
        let storage = GasPoolInitializer::run(
            fullnode_url.as_str(),
            &gas_pool_config,
            MIST_PER_SUI,
            keypair,
        )
        .await;
        assert!(storage.get_available_coin_count(sponsor).await > 900);
    }

    #[tokio::test]
    async fn test_init_non_even_split() {
        telemetry_subscribers::init_for_testing();
        let (_cluster, config) = start_sui_cluster(vec![10000000 * MIST_PER_SUI]).await;
        let GasStationConfig {
            keypair,
            gas_pool_config,
            fullnode_url,
            ..
        } = config;
        let sponsor = (&keypair.public()).into();
        let keypair = Arc::new(keypair);
        let target_init_coin_balance = 12345 * MIST_PER_SUI;
        let storage = GasPoolInitializer::run(
            fullnode_url.as_str(),
            &gas_pool_config,
            target_init_coin_balance,
            keypair,
        )
        .await;
        assert!(storage.get_available_coin_count(sponsor).await > 800);
    }
}
