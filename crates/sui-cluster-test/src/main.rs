// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use sui_cluster_test::{
    config::ClusterTestOpt,
    test_case::{
        call_contract_test::CallContractTest, coin_merge_split_test::CoinMergeSplitTest,
        native_transfer_test::NativeTransferTest, shared_object_test::SharedCounterTest,
    },
    *,
};
use tracing::info;

#[tokio::main]
async fn main() {
    let _guard = telemetry_subscribers::TelemetryConfig::new(env!("CARGO_BIN_NAME"))
        .with_env()
        .init();

    let options = ClusterTestOpt::parse();
    let mut ctx = TestContext::setup(options)
        .await
        .unwrap_or_else(|e| panic!("Failed to set up TestContext, e: {e}"));

    let tests = vec![
        TestCase::new(NativeTransferTest {}),
        TestCase::new(CoinMergeSplitTest {}),
        TestCase::new(CallContractTest {}),
        TestCase::new(SharedCounterTest {}),
    ];

    // TODO: improve the runner parallelism for efficiency
    // For now we run tests serially
    let mut success_cnt = 0;
    let total_cnt = tests.len() as i32;
    for t in tests {
        let is_sucess = t.run(&mut ctx).await as i32;
        success_cnt += is_sucess;
    }
    if success_cnt < total_cnt {
        // If any test failed, panic to bubble up the signal
        panic!("{success_cnt} of {total_cnt} tests passed.");
    }
    info!("{success_cnt} of {total_cnt} tests passed.");
}
