// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use fastcrypto::traits::KeyPair;
use futures::FutureExt;
use mysten_metrics::RegistryService;
use prometheus::Registry;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSummary,
};
use tokio::{sync::mpsc, time::sleep};

use crate::{
    authority::{AuthorityState, test_authority_builder::TestAuthorityBuilder},
    checkpoints::{CheckpointMetrics, CheckpointService, CheckpointServiceNoop},
    consensus_handler::ConsensusHandlerInitializer,
    consensus_manager::{ConsensusManager, UpdatableConsensusClient},
    consensus_validator::{SuiTxValidator, SuiTxValidatorMetrics},
    global_state_hasher::GlobalStateHasher,
};
use mysten_network::Multiaddr;
use sui_network::endpoint_manager::{AddressSource, ConsensusAddressUpdater};
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;

pub fn checkpoint_service_for_testing(state: Arc<AuthorityState>) -> Arc<CheckpointService> {
    let (output, _result) = mpsc::channel::<(CheckpointContents, CheckpointSummary)>(10);
    let epoch_store = state.epoch_store_for_testing();
    let accumulator = Arc::new(GlobalStateHasher::new_for_tests(
        state.get_global_state_hash_store().clone(),
    ));
    let (certified_output, _certified_result) = mpsc::channel::<CertifiedCheckpointSummary>(10);

    let checkpoint_service = CheckpointService::build(
        state.clone(),
        state.get_checkpoint_store().clone(),
        epoch_store.clone(),
        state.get_transaction_cache_reader().clone(),
        Arc::downgrade(&accumulator),
        Box::new(output),
        Box::new(certified_output),
        CheckpointMetrics::new_for_tests(),
        3,
        100_000,
    );
    checkpoint_service
        .spawn(epoch_store.clone(), None)
        .now_or_never()
        .unwrap();
    checkpoint_service
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_consensus_manager() {
    // GIVEN
    let configs = ConfigBuilder::new_with_temp_dir()
        .committee_size(4.try_into().unwrap())
        .build();

    let config = &configs.validator_configs()[0];

    let consensus_config = config.consensus_config().unwrap();
    let registry_service = RegistryService::new(Registry::new());
    let secret = Arc::pin(config.protocol_key_pair().copy());
    let genesis = config.genesis().unwrap();

    let state = TestAuthorityBuilder::new()
        .with_genesis_and_keypair(genesis, &secret)
        .build()
        .await;

    let epoch_store = state.epoch_store_for_testing();
    let consensus_client = Arc::new(UpdatableConsensusClient::new());

    let manager = ConsensusManager::new(
        config,
        consensus_config,
        &registry_service,
        consensus_client,
    );

    let boot_counter = *manager.boot_counter.lock().await;
    assert_eq!(boot_counter, 0);

    for i in 1..=3 {
        let consensus_handler_initializer = ConsensusHandlerInitializer::new_for_testing(
            state.clone(),
            checkpoint_service_for_testing(state.clone()),
        );

        // WHEN start consensus
        manager
            .start(
                config,
                epoch_store.clone(),
                consensus_handler_initializer,
                SuiTxValidator::new(
                    state.clone(),
                    epoch_store.clone(),
                    Arc::new(CheckpointServiceNoop {}),
                    SuiTxValidatorMetrics::new(&Registry::new()),
                ),
            )
            .await;

        // THEN
        assert!(manager.is_running().await);

        let boot_counter = *manager.boot_counter.lock().await;
        if i == 1 || i == 2 {
            assert_eq!(boot_counter, 0);
        } else {
            assert_eq!(boot_counter, 1);
        }

        // Now try to shut it down
        sleep(Duration::from_secs(1)).await;

        // Simulate a commit by bumping the handled commit index so we can ensure that boot counter increments only after the first run.
        // Practically we want to simulate a case where consensus engine restarts when no commits have happened before for first run.
        if i > 1 {
            let monitor = manager
                .consumer_monitor
                .load_full()
                .expect("A consumer monitor should have been initialised");
            monitor.set_highest_handled_commit(100);
        }

        // WHEN
        manager.shutdown().await;

        // THEN
        assert!(!manager.is_running().await);
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_consensus_manager_address_update() {
    // GIVEN
    let configs = ConfigBuilder::new_with_temp_dir()
        .committee_size(4.try_into().unwrap())
        .build();

    let config = &configs.validator_configs()[0];
    let consensus_config = config.consensus_config().unwrap();
    let registry_service = RegistryService::new(Registry::new());
    let secret = Arc::pin(config.protocol_key_pair().copy());
    let genesis = config.genesis().unwrap();

    let state = TestAuthorityBuilder::new()
        .with_genesis_and_keypair(genesis, &secret)
        .build()
        .await;

    let epoch_store = state.epoch_store_for_testing();
    let consensus_client = Arc::new(UpdatableConsensusClient::new());

    let manager = Arc::new(ConsensusManager::new(
        config,
        consensus_config,
        &registry_service,
        consensus_client,
    ));

    // Start consensus
    let consensus_handler_initializer = ConsensusHandlerInitializer::new_for_testing(
        state.clone(),
        checkpoint_service_for_testing(state.clone()),
    );

    manager
        .start(
            config,
            epoch_store.clone(),
            consensus_handler_initializer,
            SuiTxValidator::new(
                state.clone(),
                epoch_store.clone(),
                Arc::new(CheckpointServiceNoop {}),
                SuiTxValidatorMetrics::new(&Registry::new()),
            ),
        )
        .await;

    assert!(manager.is_running().await);

    // Get a peer's network public key from the committee
    let committee = epoch_store.epoch_start_state().get_consensus_committee();
    let (_peer_index, peer_authority) = committee.authorities().nth(1).unwrap();
    let peer_network_pubkey = peer_authority.network_key.clone().into_inner();

    // Test 1: Update with Admin source
    let admin_address: Multiaddr = "/ip4/192.168.1.100/udp/8080".parse().unwrap();
    let manager_clone = manager.clone();
    let pubkey = peer_network_pubkey.clone();
    let addr = admin_address.clone();
    let result = tokio::task::spawn_blocking(move || {
        manager_clone.update_address(pubkey, AddressSource::Admin, vec![addr])
    })
    .await
    .unwrap();
    assert!(result.is_ok());

    // Test 2: Update with Config source (lower priority than Admin)
    let config_address: Multiaddr = "/ip4/192.168.1.101/udp/8081".parse().unwrap();
    let manager_clone = manager.clone();
    let pubkey = peer_network_pubkey.clone();
    let addr = config_address.clone();
    let result = tokio::task::spawn_blocking(move || {
        manager_clone.update_address(pubkey, AddressSource::Config, vec![addr])
    })
    .await
    .unwrap();
    assert!(result.is_ok());

    // Test 3: Clear Admin source - Config should become active
    let manager_clone = manager.clone();
    let pubkey = peer_network_pubkey.clone();
    let result = tokio::task::spawn_blocking(move || {
        manager_clone.update_address(pubkey, AddressSource::Admin, vec![])
    })
    .await
    .unwrap();
    assert!(result.is_ok());

    // Shutdown and restart to verify persistence
    manager.shutdown().await;
    assert!(!manager.is_running().await);

    // Add an address update before restart - should be persisted but return error
    let persistent_address: Multiaddr = "/ip4/192.168.1.103/udp/8083".parse().unwrap();
    let manager_clone = manager.clone();
    let pubkey = peer_network_pubkey.clone();
    let addr = persistent_address.clone();
    let result = tokio::task::spawn_blocking(move || {
        manager_clone.update_address(pubkey, AddressSource::Config, vec![addr])
    })
    .await
    .unwrap();
    // Should fail because consensus is not running, but the address is still persisted
    assert!(result.is_err());

    // Restart consensus
    let epoch_store = state.epoch_store_for_testing();
    let consensus_handler_initializer = ConsensusHandlerInitializer::new_for_testing(
        state.clone(),
        checkpoint_service_for_testing(state.clone()),
    );

    manager
        .start(
            config,
            epoch_store.clone(),
            consensus_handler_initializer,
            SuiTxValidator::new(
                state.clone(),
                epoch_store.clone(),
                Arc::new(CheckpointServiceNoop {}),
                SuiTxValidatorMetrics::new(&Registry::new()),
            ),
        )
        .await;

    assert!(manager.is_running().await);

    // The persisted address update from before restart should have been reapplied
    // We can't directly verify the internal state, but the test passes if no panics occur

    manager.shutdown().await;
}
