// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use fastcrypto::encoding::Base58;
use fastcrypto::encoding::Encoding;
use jsonrpsee::core::Serialize;
use reqwest::Client;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_json::json;
use sui_framework::BuiltInFramework;
use sui_indexer_alt::BootstrapGenesis;
use sui_indexer_alt::config::IndexerConfig;
use sui_indexer_alt::config::PipelineLayer;
use sui_indexer_alt_schema::checkpoints::StoredGenesis;
use sui_indexer_alt_schema::epochs::StoredEpochStart;
use sui_types::balance::Balance;
use sui_types::digests::Digest;
use sui_types::messages_checkpoint::CheckpointCommitment;
use sui_types::messages_checkpoint::ECMHLiveObjectSetDigest;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::sui_system_state::mock;
use sui_types::sui_system_state::sui_system_state_inner_v1::SuiSystemStateInnerV1;
use sui_types::sui_system_state::sui_system_state_inner_v2::SuiSystemStateInnerV2;
use sui_types::test_checkpoint_data_builder::AdvanceEpochConfig;
use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

use sui_indexer_alt_e2e_tests::OffchainCluster;
use sui_indexer_alt_e2e_tests::OffchainClusterConfig;
use sui_indexer_alt_e2e_tests::local_ingestion_client_args;
use sui_indexer_alt_e2e_tests::write_checkpoint;

const SAFE_MODE_QUERY: &str = r#"
query {
    epoch(epochId: 0) {
        systemState {
            safeMode: format(format: "{safe_mode:json}")
            computationRewards: format(format: "{safe_mode_computation_rewards:json}")
            storageRewards: format(format: "{safe_mode_storage_rewards:json}")
            storageRebates: format(format: "{safe_mode_storage_rebates:json}")
            nonRefundableStorageFee: format(format: "{safe_mode_non_refundable_storage_fee:json}")
        }
    }
}
"#;

const ENABLED: bool = true;
const COMPUTATION_COST: u64 = 100;
const STORAGE_COST: u64 = 200;
const STORAGE_REBATE: u64 = 300;
const NON_REFUNDABLE_STORAGE_FEE: u64 = 400;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EpochSystemState {
    system_state: SafeMode,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SafeMode {
    safe_mode: bool,
    computation_rewards: String,
    storage_rewards: String,
    storage_rebates: String,
    non_refundable_storage_fee: String,
}

#[tokio::test]
async fn safe_mode_system_state_v1() {
    let sui_system_state = SuiSystemState::V1(SuiSystemStateInnerV1 {
        safe_mode: ENABLED,
        safe_mode_computation_rewards: Balance::new(COMPUTATION_COST),
        safe_mode_storage_rewards: Balance::new(STORAGE_COST),
        safe_mode_storage_rebates: STORAGE_REBATE,
        safe_mode_non_refundable_storage_fee: NON_REFUNDABLE_STORAGE_FEE,
        ..mock::sui_system_state_inner_v1()
    });

    let EpochSystemState {
        system_state:
            SafeMode {
                safe_mode,
                computation_rewards,
                storage_rewards,
                storage_rebates,
                non_refundable_storage_fee,
            },
    } = test_graphql(
        SAFE_MODE_QUERY,
        sui_system_state.clone(),
        AdvanceEpochConfig {
            output_objects: mock::system_state_output_objects(sui_system_state),
            ..Default::default()
        },
    )
    .await;

    assert_eq!(safe_mode, ENABLED);
    assert_eq!(computation_rewards, COMPUTATION_COST.to_string());
    assert_eq!(storage_rewards, STORAGE_COST.to_string());
    assert_eq!(storage_rebates, STORAGE_REBATE.to_string());
    assert_eq!(
        non_refundable_storage_fee,
        NON_REFUNDABLE_STORAGE_FEE.to_string()
    );
}

#[tokio::test]
async fn safe_mode_system_state_v2() {
    let sui_system_state = SuiSystemState::V2(SuiSystemStateInnerV2 {
        safe_mode: ENABLED,
        safe_mode_computation_rewards: Balance::new(COMPUTATION_COST),
        safe_mode_storage_rewards: Balance::new(STORAGE_COST),
        safe_mode_storage_rebates: STORAGE_REBATE,
        safe_mode_non_refundable_storage_fee: NON_REFUNDABLE_STORAGE_FEE,
        ..mock::sui_system_state_inner_v2()
    });

    let EpochSystemState {
        system_state:
            SafeMode {
                safe_mode,
                computation_rewards,
                storage_rewards,
                storage_rebates,
                non_refundable_storage_fee,
            },
    } = test_graphql(
        SAFE_MODE_QUERY,
        sui_system_state.clone(),
        AdvanceEpochConfig {
            output_objects: mock::system_state_output_objects(sui_system_state),
            ..Default::default()
        },
    )
    .await;

    assert_eq!(safe_mode, ENABLED);
    assert_eq!(computation_rewards, COMPUTATION_COST.to_string());
    assert_eq!(storage_rewards, STORAGE_COST.to_string());
    assert_eq!(storage_rebates, STORAGE_REBATE.to_string());
    assert_eq!(
        non_refundable_storage_fee,
        NON_REFUNDABLE_STORAGE_FEE.to_string()
    );
}

#[tokio::test]
async fn live_object_set_digest() {
    let sui_system_state = SuiSystemState::V2(mock::sui_system_state_inner_v2());
    const LIVE_OBJECT_SET_DIGEST_QUERY: &str = r#"
query {
    epoch(epochId: 0) {
        liveObjectSetDigest
    }
}
"#;

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct LiveObjectsSetDigestEpoch {
        live_object_set_digest: String,
    }

    let expected_digest = [1u8; 32];
    let LiveObjectsSetDigestEpoch {
        live_object_set_digest: actual_digest,
    } = test_graphql(
        LIVE_OBJECT_SET_DIGEST_QUERY,
        sui_system_state.clone(),
        AdvanceEpochConfig {
            output_objects: mock::system_state_output_objects(sui_system_state),
            epoch_commitments: vec![CheckpointCommitment::ECMHLiveObjectSetDigest(
                ECMHLiveObjectSetDigest {
                    digest: Digest::new(expected_digest),
                },
            )],
            ..Default::default()
        },
    )
    .await;

    assert_eq!(actual_digest, Base58::encode(expected_digest));
}

async fn test_graphql<T: DeserializeOwned>(
    query: &str,
    genesis_sui_system_state: SuiSystemState,
    mut advance_epoch_config: AdvanceEpochConfig,
) -> T {
    telemetry_subscribers::init_for_testing();
    let (client_args, temp_dir) = local_ingestion_client_args();
    let offchain = OffchainCluster::new(
        client_args,
        OffchainClusterConfig {
            indexer_config: IndexerConfig {
                pipeline: PipelineLayer {
                    cp_sequence_numbers: Some(Default::default()),
                    kv_epoch_ends: Some(Default::default()),
                    kv_epoch_starts: Some(Default::default()),
                    kv_packages: Some(Default::default()),
                    ..Default::default()
                },
                ..Default::default()
            },
            bootstrap_genesis: Some(BootstrapGenesis {
                stored_genesis: StoredGenesis {
                    genesis_digest: [1u8; 32].to_vec(),
                    initial_protocol_version: 0,
                },
                stored_epoch_start: StoredEpochStart {
                    epoch: 0,
                    protocol_version: 0,
                    cp_lo: 0,
                    start_timestamp_ms: 0,
                    reference_gas_price: 0,
                    system_state: bcs::to_bytes(&genesis_sui_system_state).unwrap(),
                },
            }),
            ..Default::default()
        },
        &prometheus::Registry::new(),
    )
    .await
    .unwrap();

    // Publish the framework.
    advance_epoch_config
        .output_objects
        .extend(BuiltInFramework::genesis_objects());

    let checkpoint = TestCheckpointBuilder::new(0).advance_epoch(advance_epoch_config);
    write_checkpoint(temp_dir.path(), checkpoint).await.unwrap();

    offchain
        .wait_for_graphql(0, Duration::from_secs(10))
        .await
        .unwrap();

    let query = json!({"query": query});
    let client = Client::new();

    let request = client.post(offchain.graphql_url()).json(&query);
    let response = request.send().await.unwrap();

    let value: Value = response.json().await.unwrap();
    let Some(system_state) = value.pointer("/data/epoch") else {
        panic!("System state not found");
    };

    serde_json::from_value(system_state.clone())
        .unwrap_or_else(|e| panic!("Error: {e}\nValue: {system_state}"))
}
