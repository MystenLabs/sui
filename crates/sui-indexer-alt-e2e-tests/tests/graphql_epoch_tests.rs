// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::core::Serialize;
use serde::Deserialize;
use std::time::Duration;
use sui_indexer_alt::config::{IndexerConfig, PipelineLayer};
use sui_indexer_alt::BootstrapGenesis;
use sui_indexer_alt_e2e_tests::mock::{sui_system_state_inner_v1, sui_system_state_inner_v2};
use sui_indexer_alt_e2e_tests::{mock, OffchainCluster, OffchainClusterConfig};
use sui_types::balance::Balance;
use sui_types::sui_system_state::sui_system_state_inner_v1::SuiSystemStateInnerV1;
use sui_types::sui_system_state::sui_system_state_inner_v2::SuiSystemStateInnerV2;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::test_checkpoint_data_builder::{AdvanceEpochConfig, TestCheckpointDataBuilder};

const ENABLED: bool = true;
const COMPUTATION_COST: u64 = 100;
const STORAGE_COST: u64 = 200;
const STORAGE_REBATE: u64 = 300;
const NON_REFUNDABLE_STORAGE_FEE: u64 = 400;

#[tokio::test]
async fn safe_mode_system_state_v1() -> Result<(), anyhow::Error> {
    let SafeMode {
        enabled,
        gas_summary:
            GasSummary {
                computation_cost,
                storage_cost,
                storage_rebate,
                non_refundable_storage_fee,
            },
    } = test_safe_mode(SuiSystemState::V1(SuiSystemStateInnerV1 {
        safe_mode: ENABLED,
        safe_mode_computation_rewards: Balance::new(COMPUTATION_COST),
        safe_mode_storage_rewards: Balance::new(STORAGE_COST),
        safe_mode_storage_rebates: STORAGE_REBATE,
        safe_mode_non_refundable_storage_fee: NON_REFUNDABLE_STORAGE_FEE,
        ..sui_system_state_inner_v1()
    }))
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
    let SafeMode {
        enabled,
        gas_summary:
            GasSummary {
                computation_cost,
                storage_cost,
                storage_rebate,
                non_refundable_storage_fee,
            },
    } = test_safe_mode(SuiSystemState::V2(SuiSystemStateInnerV2 {
        safe_mode: ENABLED,
        safe_mode_computation_rewards: Balance::new(COMPUTATION_COST),
        safe_mode_storage_rewards: Balance::new(STORAGE_COST),
        safe_mode_storage_rebates: STORAGE_REBATE,
        safe_mode_non_refundable_storage_fee: NON_REFUNDABLE_STORAGE_FEE,
        ..sui_system_state_inner_v2()
    }))
    .await?;
    assert_eq!(enabled, ENABLED);
    assert_eq!(computation_cost, COMPUTATION_COST);
    assert_eq!(storage_cost, STORAGE_COST);
    assert_eq!(storage_rebate, STORAGE_REBATE);
    assert_eq!(non_refundable_storage_fee, NON_REFUNDABLE_STORAGE_FEE);
    Ok(())
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

async fn test_safe_mode(sui_system_state: SuiSystemState) -> anyhow::Result<SafeMode> {
    telemetry_subscribers::init_for_testing();

    let offchain = OffchainCluster::new(OffchainClusterConfig {
        indexer_config: IndexerConfig {
            pipeline: PipelineLayer {
                cp_sequence_numbers: Some(Default::default()),
                kv_epoch_ends: Some(Default::default()),
                kv_epoch_starts: Some(Default::default()),
                ..Default::default()
            },
            ..IndexerConfig::default()
        },
        bootstrap_genesis: Some(BootstrapGenesis {
            stored_genesis: mock::stored_genesis(),
            sui_system_state: sui_system_state.clone(),
        }),
        ..OffchainClusterConfig::with_local_ingestion()
    })
    .await?;

    let checkpoint_data = TestCheckpointDataBuilder::new(0).advance_epoch(AdvanceEpochConfig {
        output_objects: mock::genesis_output_objects(sui_system_state),
        ..AdvanceEpochConfig::default()
    });
    offchain.write_checkpoint(checkpoint_data).await?;

    offchain
        .wait_for_graphql(0, Duration::from_secs(10))
        .await?;

    #[derive(Serialize, Deserialize)]
    struct Data {
        epoch: Epoch,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Epoch {
        safe_mode: SafeMode,
    }

    let query = "query {
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
    }";

    let data: Data = offchain.query_graphql(query).await?;

    offchain.stopped().await;

    Ok(data.epoch.safe_mode)
}
