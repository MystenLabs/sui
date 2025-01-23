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
    authority::{test_authority_builder::TestAuthorityBuilder, AuthorityState},
    checkpoints::{CheckpointMetrics, CheckpointService, CheckpointServiceNoop},
    consensus_adapter::NoopConsensusOverloadChecker,
    consensus_handler::ConsensusHandlerInitializer,
    consensus_manager::{
        mysticeti_manager::MysticetiManager, ConsensusManagerMetrics, ConsensusManagerTrait,
    },
    consensus_validator::{SuiTxValidator, SuiTxValidatorMetrics},
    mysticeti_adapter::LazyMysticetiClient,
    state_accumulator::StateAccumulator,
};

pub fn checkpoint_service_for_testing(state: Arc<AuthorityState>) -> Arc<CheckpointService> {
    let (output, _result) = mpsc::channel::<(CheckpointContents, CheckpointSummary)>(10);
    let epoch_store = state.epoch_store_for_testing();
    let accumulator = Arc::new(StateAccumulator::new_for_tests(
        state.get_accumulator_store().clone(),
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
    checkpoint_service.spawn().now_or_never().unwrap();
    checkpoint_service
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_mysticeti_manager() {
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

    let metrics = Arc::new(ConsensusManagerMetrics::new(&Registry::new()));
    let epoch_store = state.epoch_store_for_testing();
    let client = Arc::new(LazyMysticetiClient::default());

    let manager = MysticetiManager::new(
        config.worker_key_pair().copy(),
        config.network_key_pair().copy(),
        consensus_config.db_path().to_path_buf(),
        registry_service,
        metrics,
        client,
    );

    let boot_counter = *manager.boot_counter.lock().await;
    assert_eq!(boot_counter, 0);

    for i in 1..=3 {
        let consensus_handler_initializer = ConsensusHandlerInitializer::new_for_testing(
            state.clone(),
            checkpoint_service_for_testing(state.clone()),
        );

        // WHEN start mysticeti
        manager
            .start(
                config,
                epoch_store.clone(),
                consensus_handler_initializer,
                SuiTxValidator::new(
                    state.clone(),
                    Arc::new(NoopConsensusOverloadChecker {}),
                    Arc::new(CheckpointServiceNoop {}),
                    state.transaction_manager().clone(),
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
