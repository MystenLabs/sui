// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use fastcrypto::encoding::{Base58, Encoding};
use jsonrpsee::core::Serialize;
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use sui_indexer_alt::{
    config::{IndexerConfig, PipelineLayer},
    BootstrapGenesis,
};
use sui_indexer_alt_e2e_tests::{
    local_ingestion_client_args, write_checkpoint, OffchainCluster, OffchainClusterConfig,
};
use sui_indexer_alt_schema::{checkpoints::StoredGenesis, epochs::StoredEpochStart};
use sui_types::{
    balance::Balance,
    digests::Digest,
    messages_checkpoint::{CheckpointCommitment, ECMHLiveObjectSetDigest},
    sui_system_state::{
        mock, sui_system_state_inner_v1::SuiSystemStateInnerV1,
        sui_system_state_inner_v2::SuiSystemStateInnerV2, SuiSystemState,
    },
    test_checkpoint_data_builder::{AdvanceEpochConfig, TestCheckpointDataBuilder},
};
use tokio_util::sync::CancellationToken;

const SAFE_MODE_QUERY: &str = r#"
query {
    epoch(epochId: 0) {
        safeMode {
            enabled
            gasSummary {
                computationCost
                storageCost
                storageRebate
                nonRefundableStorageFee
            }
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
struct SafeModeEpoch {
    safe_mode: SafeMode,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SafeMode {
    enabled: bool,
    gas_summary: GasSummary,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GasSummary {
    computation_cost: u64,
    storage_cost: u64,
    storage_rebate: u64,
    non_refundable_storage_fee: u64,
}

#[tokio::test]
async fn safe_mode_system_state_v1() -> Result<(), anyhow::Error> {
    let sui_system_state = SuiSystemState::V1(SuiSystemStateInnerV1 {
        safe_mode: ENABLED,
        safe_mode_computation_rewards: Balance::new(COMPUTATION_COST),
        safe_mode_storage_rewards: Balance::new(STORAGE_COST),
        safe_mode_storage_rebates: STORAGE_REBATE,
        safe_mode_non_refundable_storage_fee: NON_REFUNDABLE_STORAGE_FEE,
        ..mock::sui_system_state_inner_v1()
    });
    let SafeModeEpoch {
        safe_mode:
            SafeMode {
                enabled,
                gas_summary:
                    GasSummary {
                        computation_cost,
                        storage_cost,
                        storage_rebate,
                        non_refundable_storage_fee,
                    },
            },
    } = test_graphql(
        SAFE_MODE_QUERY,
        sui_system_state.clone(),
        AdvanceEpochConfig {
            output_objects: mock::system_state_output_objects(sui_system_state),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(enabled, ENABLED);
    assert_eq!(computation_cost, COMPUTATION_COST);
    assert_eq!(storage_cost, STORAGE_COST);
    assert_eq!(storage_rebate, STORAGE_REBATE);
    assert_eq!(non_refundable_storage_fee, NON_REFUNDABLE_STORAGE_FEE);
    Ok(())
}

#[tokio::test]
async fn safe_mode_system_state_v2() -> Result<(), anyhow::Error> {
    let sui_system_state = SuiSystemState::V2(SuiSystemStateInnerV2 {
        safe_mode: ENABLED,
        safe_mode_computation_rewards: Balance::new(COMPUTATION_COST),
        safe_mode_storage_rewards: Balance::new(STORAGE_COST),
        safe_mode_storage_rebates: STORAGE_REBATE,
        safe_mode_non_refundable_storage_fee: NON_REFUNDABLE_STORAGE_FEE,
        ..mock::sui_system_state_inner_v2()
    });
    let SafeModeEpoch {
        safe_mode:
            SafeMode {
                enabled,
                gas_summary:
                    GasSummary {
                        computation_cost,
                        storage_cost,
                        storage_rebate,
                        non_refundable_storage_fee,
                    },
            },
    } = test_graphql(
        SAFE_MODE_QUERY,
        sui_system_state.clone(),
        AdvanceEpochConfig {
            output_objects: mock::system_state_output_objects(sui_system_state),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(enabled, ENABLED);
    assert_eq!(computation_cost, COMPUTATION_COST);
    assert_eq!(storage_cost, STORAGE_COST);
    assert_eq!(storage_rebate, STORAGE_REBATE);
    assert_eq!(non_refundable_storage_fee, NON_REFUNDABLE_STORAGE_FEE);
    Ok(())
}

#[tokio::test]
async fn live_object_set_digest() -> Result<(), anyhow::Error> {
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
    .await?;

    assert_eq!(actual_digest, Base58::encode(expected_digest));
    Ok(())
}

async fn test_graphql<T: DeserializeOwned>(
    query: &str,
    genesis_sui_system_state: SuiSystemState,
    advance_epoch_config: AdvanceEpochConfig,
) -> anyhow::Result<T> {
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
                    system_state: bcs::to_bytes(&genesis_sui_system_state)?,
                },
            }),
            ..Default::default()
        },
        &prometheus::Registry::new(),
        CancellationToken::new(),
    )
    .await?;

    let checkpoint_data = TestCheckpointDataBuilder::new(0).advance_epoch(advance_epoch_config);
    write_checkpoint(temp_dir.path(), checkpoint_data).await?;

    offchain
        .wait_for_graphql(0, Duration::from_secs(10))
        .await?;

    let query = json!({"query": query});
    let client = Client::new();

    let request = client.post(offchain.graphql_url()).json(&query);
    let response = request.send().await?;

    offchain.stopped().await;

    let value: Value = response.json().await?;
    let Some(epoch) = value.pointer("/data/epoch") else {
        return Err(anyhow::Error::msg("epoch not found"));
    };
    Ok(serde_json::from_value(epoch.clone())?)
}
