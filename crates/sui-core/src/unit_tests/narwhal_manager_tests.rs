// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use crate::authority::AuthorityStore;
use crate::epoch::committee_store::CommitteeStore;
use crate::narwhal_manager::{
    run_narwhal_manager, NarwhalConfiguration, NarwhalManager, NarwhalStartMessage,
};
use mysten_metrics::RegistryService;
use narwhal_config::SharedWorkerCache;
use narwhal_executor::ExecutionState;
use narwhal_types::ConsensusOutput;
use narwhal_worker::TrivialTransactionValidator;
use prometheus::Registry;
use std::sync::Arc;
use std::time::Duration;
use sui_types::crypto::KeypairTraits;
use test_utils::authority::test_and_configure_authority_configs;
use tokio::sync::mpsc::channel;

#[derive(Clone)]
struct NoOpExecutionState {}

#[async_trait::async_trait]
impl ExecutionState for NoOpExecutionState {
    async fn handle_consensus_output(&self, _consensus_output: ConsensusOutput) {}

    async fn last_executed_sub_dag_index(&self) -> u64 {
        0
    }
}

#[tokio::test]
async fn test_narwhal_manager() {
    let configs = test_and_configure_authority_configs(1);
    let registry_service = RegistryService::new(Registry::new());

    let config = configs.validator_configs()[0].clone();
    let consensus_config = config.consensus_config().unwrap();

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
    let mut narwhal_committee = system_state.get_current_epoch_narwhal_committee();
    let mut worker_cache = system_state.get_current_epoch_narwhal_worker_cache(transactions_addr);

    let execution_state = Arc::new(NoOpExecutionState {});

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
    assert!(narwhal_manager
        .tx_start
        .send(NarwhalStartMessage {
            committee: Arc::new(narwhal_committee.clone()),
            shared_worker_cache: SharedWorkerCache::from(worker_cache.clone()),
            execution_state: Arc::new(execution_state.clone())
        })
        .await
        .is_ok());

    tokio::time::sleep(Duration::from_millis(500)).await;

    // stop narwhal
    assert!(narwhal_manager.tx_stop.send(()).await.is_ok());

    // advance epoch
    narwhal_committee.epoch = 1;
    worker_cache.epoch = 1;

    // start narwhal with advanced epoch
    assert!(narwhal_manager
        .tx_start
        .send(NarwhalStartMessage {
            committee: Arc::new(narwhal_committee.clone()),
            shared_worker_cache: SharedWorkerCache::from(worker_cache.clone()),
            execution_state: Arc::new(execution_state)
        })
        .await
        .is_ok());
}
