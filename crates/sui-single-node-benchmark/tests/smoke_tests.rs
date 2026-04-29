// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use strum::IntoEnumIterator;
use sui_macros::sim_test;
use sui_single_node_benchmark::command::{Component, WorkloadKind};
use sui_single_node_benchmark::run_benchmark;
use sui_single_node_benchmark::workload::Workload;

#[sim_test]
async fn benchmark_non_move_transactions_smoke_test() {
    for component in Component::iter() {
        run_benchmark(
            Workload::new(
                10,
                WorkloadKind::PTB {
                    num_transfers: 2,
                    use_native_transfer: true,
                    num_dynamic_fields: 0,
                    computation: 0,
                    num_shared_objects: 0,
                    num_mints: 0,
                    nft_size: 528,
                    use_batch_mint: false,
                },
            ),
            component,
            1000,
            false,
        )
        .await;
    }
}

#[sim_test]
async fn benchmark_move_transactions_smoke_test() {
    for component in Component::iter() {
        run_benchmark(
            Workload::new(
                10,
                WorkloadKind::PTB {
                    num_transfers: 2,
                    use_native_transfer: false,
                    num_dynamic_fields: 1,
                    computation: 1,
                    num_shared_objects: 2,
                    num_mints: 2,
                    nft_size: 528,
                    use_batch_mint: false,
                },
            ),
            component,
            1000,
            false,
        )
        .await;
    }
}

#[sim_test]
async fn benchmark_batch_mint_smoke_test() {
    for component in Component::iter() {
        run_benchmark(
            Workload::new(
                10,
                WorkloadKind::PTB {
                    num_transfers: 0,
                    use_native_transfer: false,
                    num_dynamic_fields: 0,
                    computation: 0,
                    num_shared_objects: 0,
                    num_mints: 10,
                    nft_size: 256,
                    use_batch_mint: true,
                },
            ),
            component,
            1000,
            false,
        )
        .await;
    }
}

#[sim_test]
async fn benchmark_publish_from_source() {
    // This test makes sure that the benchmark runs.
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend([
        "tests",
        "data",
        "package_publish_from_source",
        "manifest.json",
    ]);
    for component in Component::iter() {
        run_benchmark(
            Workload::new(
                10,
                WorkloadKind::Publish {
                    manifest_file: path.clone(),
                },
            ),
            component,
            1000,
            false,
        )
        .await;
    }
}

#[sim_test]
async fn benchmark_send_funds_smoke_test() {
    // SendFunds requires address-balance gas payments, which are not yet enabled
    // on every chain (e.g. mainnet). Use the chain override env var (set by the
    // simtest CI matrix) to determine the effective chain and skip the test when
    // the feature is disabled.
    let chain = match std::env::var("SUI_PROTOCOL_CONFIG_CHAIN_OVERRIDE").as_deref() {
        Ok("mainnet") => sui_protocol_config::Chain::Mainnet,
        Ok("testnet") => sui_protocol_config::Chain::Testnet,
        _ => sui_protocol_config::Chain::Unknown,
    };
    let protocol_config = sui_protocol_config::ProtocolConfig::get_for_version(
        sui_protocol_config::ProtocolVersion::MAX,
        chain,
    );
    if !protocol_config.enable_address_balance_gas_payments() {
        return;
    }

    // SendFunds uses address-balance gas which requires accumulators.
    // Only test Baseline component — other components may not support this.
    run_benchmark(
        Workload::new(
            10,
            WorkloadKind::SendFunds {
                seed_amount: 100_000_000_000,
                transfer_amount: 1000,
            },
        ),
        Component::Baseline,
        1000,
        false,
    )
    .await;
}

#[sim_test]
async fn benchmark_publish_from_bytecode() {
    // This test makes sure that the benchmark runs.
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend([
        "tests",
        "data",
        "package_publish_from_bytecode",
        "manifest.json",
    ]);
    for component in Component::iter() {
        run_benchmark(
            Workload::new(
                10,
                WorkloadKind::Publish {
                    manifest_file: path.clone(),
                },
            ),
            component,
            1000,
            false,
        )
        .await;
    }
}
