// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::identifier::Identifier;
use std::num::NonZeroU32;
use std::time::Duration;
use sui_config::node::ExecutionTimeObserverConfig;
use sui_core::authority::execution_time_estimator::{
    EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_CHUNK_COUNT_KEY, EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_KEY,
};
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_keys::keystore::AccountKeystore;
use sui_macros::sim_test;
use sui_protocol_config::{
    ExecutionTimeEstimateParams, PerObjectCongestionControlMode, ProtocolConfig,
};
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::dynamic_field::get_dynamic_field_from_store;
use sui_types::execution::ExecutionTimeObservationChunkKey;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::sui_system_state;
use sui_types::transaction::{
    SharedObjectMutability, StoredExecutionTimeObservations, TransactionData,
};
use test_cluster::{TestCluster, TestClusterBuilder};

async fn setup_test_cluster_with_chunking() -> TestCluster {
    // set thresholds low to ensure we share observations
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.set_per_object_congestion_control_mode_for_testing(
            PerObjectCongestionControlMode::ExecutionTimeEstimate(ExecutionTimeEstimateParams {
                target_utilization: 0,
                allowed_txn_cost_overage_burst_limit_us: 500_000,
                randomness_scalar: 20,
                max_estimate_us: 1_500_000,
                stored_observations_num_included_checkpoints: 200,
                stored_observations_limit: 200,
                stake_weighted_median_threshold: 0,
                default_none_duration_for_new_keys: true,
                observations_chunk_size: Some(2),
            }),
        );
        cfg
    });

    let observer_config = ExecutionTimeObserverConfig {
        observation_sharing_object_utilization_threshold: Some(Duration::from_micros(1)),
        observation_sharing_diff_threshold: Some(0.01),
        observation_sharing_min_interval: Some(Duration::from_millis(1)),
        observation_sharing_rate_limit: Some(NonZeroU32::new(1000).unwrap()),
        observation_sharing_burst_limit: Some(NonZeroU32::new(1000).unwrap()),
        ..Default::default()
    };

    #[allow(unused_mut)]
    let mut builder = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_epoch_duration_ms(120_000)
        .with_execution_time_observer_config(observer_config);

    #[cfg(msim)]
    {
        builder = builder.with_synthetic_execution_time_injection();
    }

    builder.build().await
}

async fn validate_chunked_execution_time_storage(test_cluster: &TestCluster) {
    let validator_handles = test_cluster.all_validator_handles();
    let validator_node = &validator_handles[0];
    let validator_state = validator_node.state();
    let object_store = validator_state.get_object_store();

    let system_state = sui_system_state::get_sui_system_state(object_store.as_ref())
        .expect("System state must exist");

    let extra_fields_id = match &system_state {
        sui_types::sui_system_state::SuiSystemState::V2(system_state) => {
            system_state.extra_fields.id.id.bytes
        }
        #[cfg(msim)]
        sui_types::sui_system_state::SuiSystemState::SimTestDeepV2(system_state) => {
            system_state.extra_fields.id.id.bytes
        }
        #[cfg(msim)]
        sui_types::sui_system_state::SuiSystemState::SimTestShallowV2(system_state) => {
            system_state.extra_fields.id.id.bytes
        }
        sui_types::sui_system_state::SuiSystemState::V1(_) => {
            panic!("SuiSystemState V1 not supported for chunking validation");
        }
        #[cfg(msim)]
        sui_types::sui_system_state::SuiSystemState::SimTestV1(_) => {
            panic!("SuiSystemState V1 not supported for chunking validation");
        }
    };

    let old_format_result: Result<Vec<u8>, _> = get_dynamic_field_from_store(
        object_store.as_ref(),
        extra_fields_id,
        &EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_KEY,
    );

    if old_format_result.is_ok() {
        panic!(
            "Old execution time estimates storage format found - should not exist with chunking enabled"
        );
    }

    let chunk_count: u64 = get_dynamic_field_from_store(
        object_store.as_ref(),
        extra_fields_id,
        &EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_CHUNK_COUNT_KEY,
    )
    .expect("Chunk count must exist - ensure enough observations were collected before reconfig");

    for chunk_index in 0..chunk_count {
        let chunk_key = ExecutionTimeObservationChunkKey { chunk_index };
        let chunk_bytes_result: Result<Vec<u8>, _> =
            get_dynamic_field_from_store(object_store.as_ref(), extra_fields_id, &chunk_key);

        match chunk_bytes_result {
            Ok(chunk_bytes) => {
                let _chunk: StoredExecutionTimeObservations = bcs::from_bytes(&chunk_bytes)
                    .expect("Failed to deserialize stored execution time estimates chunk");
            }
            Err(_) => {
                panic!(
                    "Could not find stored execution time observation chunk {}",
                    chunk_index
                );
            }
        }
    }

    assert!(
        chunk_count > 1,
        "Expected more than 1 chunk to validate chunking, got {}",
        chunk_count
    );
}

#[sim_test]
async fn test_chunked_execution_time() {
    sui_protocol_config::ProtocolConfig::poison_get_for_min_version();

    let test_cluster = setup_test_cluster_with_chunking().await;

    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/data/execution_time_test");
    let (package_id, _, _) =
        sui_test_transaction_builder::publish_package(&test_cluster.wallet, path).await;

    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let (counter_id, initial_shared_version) =
        create_shared_counter(&test_cluster, sender, package_id).await;

    send_transactions(
        &test_cluster,
        sender,
        package_id,
        counter_id,
        initial_shared_version,
    )
    .await;

    test_cluster.trigger_reconfiguration().await;

    validate_chunked_execution_time_storage(&test_cluster).await;
}

async fn create_shared_counter(
    test_cluster: &TestCluster,
    sender: SuiAddress,
    package_id: ObjectID,
) -> (ObjectID, SequenceNumber) {
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .wallet
        .gas_objects(sender)
        .await
        .expect("Failed to get gas objects")
        .pop()
        .expect("No gas objects available")
        .1
        .object_ref();

    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.programmable_move_call(
        package_id,
        Identifier::new("compute").unwrap(),
        Identifier::new("create_counter").unwrap(),
        vec![],
        vec![],
    );

    let tx_data = TransactionData::new(
        sui_types::transaction::TransactionKind::ProgrammableTransaction(ptb.finish()),
        sender,
        gas,
        10_000_000,
        rgp,
    );

    let signed_tx = test_cluster.wallet.sign_transaction(&tx_data).await;
    let response = test_cluster.execute_transaction(signed_tx).await;

    let created_obj = response
        .effects
        .unwrap()
        .created()
        .iter()
        .find(|obj| obj.owner.is_shared())
        .unwrap()
        .clone();

    (
        created_obj.reference.object_id,
        created_obj.reference.version,
    )
}

async fn send_transactions(
    test_cluster: &TestCluster,
    sender: SuiAddress,
    package_id: ObjectID,
    counter_id: ObjectID,
    initial_shared_version: SequenceNumber,
) {
    let rgp = test_cluster.get_reference_gas_price().await;

    for i in 0..6 {
        let gas = test_cluster
            .wallet
            .gas_objects(sender)
            .await
            .expect("Failed to get gas objects")
            .pop()
            .expect("No gas objects available")
            .1
            .object_ref();

        let mut ptb = ProgrammableTransactionBuilder::new();

        let counter_arg = ptb
            .obj(sui_types::transaction::ObjectArg::SharedObject {
                id: counter_id,
                initial_shared_version,
                mutability: SharedObjectMutability::Mutable,
            })
            .unwrap();

        let function = match i % 3 {
            0 => "increment_a",
            1 => "increment_b",
            _ => "increment_c",
        };

        ptb.programmable_move_call(
            package_id,
            Identifier::new("compute").unwrap(),
            Identifier::new(function).unwrap(),
            vec![],
            vec![counter_arg],
        );

        let tx_data = TransactionData::new(
            sui_types::transaction::TransactionKind::ProgrammableTransaction(ptb.finish()),
            sender,
            gas,
            10_000_000,
            rgp,
        );

        let signed_tx = test_cluster.wallet.sign_transaction(&tx_data).await;

        let res = test_cluster.execute_transaction(signed_tx).await;
        assert_eq!(
            res.effects.unwrap().executed_epoch(),
            0,
            "all txns to execute in epoch 0"
        );
    }
}
