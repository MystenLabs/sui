// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use crate::authority::AuthorityStore;
use crate::epoch::committee_store::CommitteeStore;
use crate::narwhal_manager::{
    run_narwhal_manager, NarwhalConfiguration, NarwhalManager, NarwhalStartMessage,
};
use bytes::Bytes;
use fastcrypto::bls12381;
use mysten_metrics::RegistryService;
use narwhal_config::{Epoch, SharedWorkerCache};
use narwhal_executor::ExecutionState;
use narwhal_types::{ConsensusOutput, TransactionProto, TransactionsClient};
use narwhal_worker::TrivialTransactionValidator;
use prometheus::Registry;
use std::sync::Arc;
use std::time::Duration;
use sui_config::node::AuthorityStorePruningConfig;
use sui_types::crypto::KeypairTraits;
use test_utils::authority::test_and_configure_authority_configs;
use tokio::sync::broadcast;
use tokio::sync::mpsc::channel;
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
                for transaction in batch.transactions.into_iter() {
                    assert_eq!(transaction, Bytes::from(self.epoch.to_be_bytes().to_vec()));
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
    worker_cache: SharedWorkerCache,
    epoch: Epoch,
    mut rx_shutdown: broadcast::Receiver<()>,
) {
    let target = worker_cache
        .load()
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

    for config in &(*configs.validator_configs()).clone() {
        let consensus_config = config.consensus_config().unwrap();
        let registry_service = RegistryService::new(Registry::new());
        let secret = Arc::pin(config.protocol_key_pair().copy());
        let genesis = config.genesis().unwrap();
        let genesis_committee = genesis.committee().unwrap();
        let committee_store = Arc::new(CommitteeStore::new(
            config.db_path().join("epochs"),
            &genesis_committee,
            None,
        ));

        let store = Arc::new(
            AuthorityStore::open(
                &config.db_path().join("store"),
                None,
                genesis,
                &committee_store,
                &AuthorityStorePruningConfig::default(),
            )
            .await
            .unwrap(),
        );

        let state = AuthorityState::new(
            config.protocol_public_key(),
            secret,
            store,
            committee_store.clone(),
            None,
            None,
            None,
            &registry_service.default_registry(),
        )
        .await;

        let system_state = state
            .get_sui_system_state_object()
            .expect("Reading Sui system state object cannot fail");

        let transactions_addr = &config.consensus_config.as_ref().unwrap().address;
        let narwhal_committee = system_state.get_current_epoch_narwhal_committee();
        let worker_cache = system_state.get_current_epoch_narwhal_worker_cache(transactions_addr);

        let execution_state = Arc::new(NoOpExecutionState {
            epoch: narwhal_committee.epoch,
        });

        let narwhal_config = NarwhalConfiguration {
            primary_keypair: config.protocol_key_pair().copy(),
            network_keypair: config.network_key_pair().copy(),
            worker_ids_and_keypairs: vec![(0, config.worker_key_pair().copy())],
            storage_base_path: consensus_config.db_path().to_path_buf(),
            parameters: consensus_config.narwhal_config().to_owned(),
            tx_validator: TrivialTransactionValidator::default(),
            registry_service,
        };

        let (tx_start, tr_start) = channel(1);
        let (tx_stop, tr_stop) = channel(1);
        let join_handle = tokio::spawn(run_narwhal_manager(narwhal_config, tr_start, tr_stop));

        let narwhal_manager = NarwhalManager {
            join_handle,
            tx_start,
            tx_stop,
        };

        // start narwhal
        let shared_worker_cache = SharedWorkerCache::from(worker_cache.clone());
        assert!(narwhal_manager
            .tx_start
            .send(NarwhalStartMessage {
                committee: Arc::new(narwhal_committee.clone()),
                shared_worker_cache: shared_worker_cache.clone(),
                execution_state: Arc::new(execution_state.clone())
            })
            .await
            .is_ok());

        let name = config.protocol_key_pair().public().clone();
        narwhal_managers.push((
            narwhal_manager,
            state,
            transactions_addr.clone(),
            name.clone(),
        ));

        // Send some transactions
        let (tr_shutdown, rx_shutdown) = broadcast::channel(1);
        tokio::spawn(async move {
            send_transactions(
                &name,
                shared_worker_cache,
                narwhal_committee.epoch,
                rx_shutdown,
            )
            .await
        });
        shutdown_senders.push(tr_shutdown);
    }

    sleep(Duration::from_secs(1)).await;
    for tr_shutdown in shutdown_senders {
        _ = tr_shutdown.send(());
    }
    let mut shutdown_senders = Vec::new();

    for (narwhal_manager, state, transactions_addr, name) in narwhal_managers {
        // stop narwhal instance
        assert!(narwhal_manager.tx_stop.send(()).await.is_ok());

        let system_state = state
            .get_sui_system_state_object()
            .expect("Reading Sui system state object cannot fail");

        let mut narwhal_committee = system_state.get_current_epoch_narwhal_committee();
        let mut worker_cache =
            system_state.get_current_epoch_narwhal_worker_cache(&transactions_addr);

        // advance epoch
        narwhal_committee.epoch = 1;
        worker_cache.epoch = 1;

        let execution_state = Arc::new(NoOpExecutionState {
            epoch: narwhal_committee.epoch,
        });

        // start narwhal with advanced epoch
        let shared_worker_cache = SharedWorkerCache::from(worker_cache.clone());
        assert!(narwhal_manager
            .tx_start
            .send(NarwhalStartMessage {
                committee: Arc::new(narwhal_committee.clone()),
                shared_worker_cache: shared_worker_cache.clone(),
                execution_state: Arc::new(execution_state.clone())
            })
            .await
            .is_ok());

        // Send some transactions
        let (tr_shutdown, rx_shutdown) = broadcast::channel(1);
        tokio::spawn(async move {
            send_transactions(
                &name,
                shared_worker_cache,
                narwhal_committee.epoch,
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
