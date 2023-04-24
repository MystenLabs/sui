// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::narwhal_manager::{NarwhalConfiguration, NarwhalManager, NarwhalManagerMetrics};
use bytes::Bytes;
use fastcrypto::bls12381;
use fastcrypto::traits::KeyPair;
use mysten_metrics::RegistryService;
use narwhal_config::{Epoch, WorkerCache};
use narwhal_executor::ExecutionState;
use narwhal_types::{BatchAPI, ConsensusOutput, TransactionProto, TransactionsClient};
use narwhal_worker::TrivialTransactionValidator;
use prometheus::Registry;
use std::sync::Arc;
use std::time::Duration;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use sui_types::sui_system_state::SuiSystemStateTrait;
use test_utils::authority::test_and_configure_authority_configs;
use tokio::sync::broadcast;
use tokio::time::{interval, sleep};

#[derive(Clone)]
struct NoOpExecutionState {
    epoch: Epoch,
}

#[async_trait::async_trait]
impl ExecutionState for NoOpExecutionState {
    async fn handle_consensus_output(&self, consensus_output: ConsensusOutput) {
        for (_, batches) in consensus_output.batches {
            for batch in batches {
                for transaction in batch.transactions().iter() {
                    assert_eq!(
                        transaction.clone(),
                        Bytes::from(self.epoch.to_be_bytes().to_vec())
                    );
                }
            }
        }
    }

    async fn last_executed_sub_dag_index(&self) -> u64 {
        0
    }
}

async fn send_transactions(
    name: &bls12381::min_sig::BLS12381PublicKey,
    worker_cache: WorkerCache,
    epoch: Epoch,
    mut rx_shutdown: broadcast::Receiver<()>,
) {
    let target = worker_cache
        .worker(name, /* id */ &0)
        .expect("Our key or worker id is not in the worker cache")
        .transactions;
    let config = mysten_network::config::Config::new();
    let channel = config.connect_lazy(&target).unwrap();
    let mut client = TransactionsClient::new(channel);
    // Make a transaction to submit forever.
    let tx = TransactionProto {
        transaction: Bytes::from(epoch.to_be_bytes().to_vec()),
    };
    // Repeatedly send transactions.
    let interval = interval(Duration::from_millis(1));

    tokio::pin!(interval);
    let mut succeeded_once = false;
    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Send a transactions.
                let result = client.submit_transaction(tx.clone()).await;
                if result.is_ok() {
                    succeeded_once = true;
                }

            },
            _ = rx_shutdown.recv() => {
                break
            }
        }
    }
    assert!(succeeded_once);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_narwhal_manager() {
    let configs = test_and_configure_authority_configs(1);
    let mut narwhal_managers = Vec::new();
    let mut shutdown_senders = Vec::new();

    for config in configs.validator_configs() {
        let consensus_config = config.consensus_config().unwrap();
        let registry_service = RegistryService::new(Registry::new());
        let secret = Arc::pin(config.protocol_key_pair().copy());
        let genesis = config.genesis().unwrap();

        let state = TestAuthorityBuilder::new()
            .with_genesis_and_keypair(genesis, &secret)
            .build()
            .await;

        let system_state = state
            .get_sui_system_state_object_for_testing()
            .expect("Reading Sui system state object cannot fail")
            .into_epoch_start_state();

        let transactions_addr = &config.consensus_config.as_ref().unwrap().address;
        let narwhal_committee = system_state.get_narwhal_committee();
        let worker_cache = system_state.get_narwhal_worker_cache(transactions_addr);

        let execution_state = Arc::new(NoOpExecutionState {
            epoch: narwhal_committee.epoch(),
        });

        let narwhal_config = NarwhalConfiguration {
            primary_keypair: config.protocol_key_pair().copy(),
            network_keypair: config.network_key_pair().copy(),
            worker_ids_and_keypairs: vec![(0, config.worker_key_pair().copy())],
            storage_base_path: consensus_config.db_path().to_path_buf(),
            parameters: consensus_config.narwhal_config().to_owned(),
            registry_service,
        };

        let metrics = NarwhalManagerMetrics::new(&Registry::new());

        let narwhal_manager = NarwhalManager::new(narwhal_config, metrics);

        // start narwhal
        narwhal_manager
            .start(
                narwhal_committee.clone(),
                worker_cache.clone(),
                Arc::new(execution_state.clone()),
                TrivialTransactionValidator::default(),
            )
            .await;

        let name = config.protocol_key_pair().public().clone();
        narwhal_managers.push((
            narwhal_manager,
            state,
            transactions_addr.clone(),
            name.clone(),
        ));

        // Send some transactions
        let (tx_shutdown, rx_shutdown) = broadcast::channel(1);
        tokio::spawn(async move {
            send_transactions(
                &name,
                worker_cache.clone(),
                narwhal_committee.epoch(),
                rx_shutdown,
            )
            .await
        });
        shutdown_senders.push(tx_shutdown);
    }

    sleep(Duration::from_secs(1)).await;
    for tr_shutdown in shutdown_senders {
        _ = tr_shutdown.send(());
    }
    let mut shutdown_senders = Vec::new();

    for (narwhal_manager, state, transactions_addr, name) in narwhal_managers {
        // stop narwhal instance
        narwhal_manager.shutdown().await;

        // ensure that no primary or worker node is running
        assert!(!narwhal_manager.primary_node.is_running().await);
        assert!(narwhal_manager
            .worker_nodes
            .workers_running()
            .await
            .is_empty());

        let system_state = state
            .get_sui_system_state_object_for_testing()
            .expect("Reading Sui system state object cannot fail")
            .into_epoch_start_state();
        let narwhal_committee = system_state.get_narwhal_committee();
        let worker_cache = system_state.get_narwhal_worker_cache(&transactions_addr);

        let execution_state = Arc::new(NoOpExecutionState {
            epoch: narwhal_committee.epoch(),
        });

        // start narwhal with advanced epoch
        narwhal_manager
            .start(
                narwhal_committee.clone(),
                worker_cache.clone(),
                Arc::new(execution_state.clone()),
                TrivialTransactionValidator::default(),
            )
            .await;

        // Send some transactions
        let (tr_shutdown, rx_shutdown) = broadcast::channel(1);
        tokio::spawn(async move {
            send_transactions(
                &name,
                worker_cache.clone(),
                narwhal_committee.epoch(),
                rx_shutdown,
            )
            .await
        });

        shutdown_senders.push(tr_shutdown);
    }
    sleep(Duration::from_secs(5)).await;
    for tr_shutdown in shutdown_senders {
        _ = tr_shutdown.send(());
    }
}
