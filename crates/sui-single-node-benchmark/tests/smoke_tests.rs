// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_single_node_benchmark::execution::{
    benchmark_move_transactions, benchmark_simple_transfer,
};

#[sim_test]
async fn benchmark_simple_transfer_smoke_test() {
    // This test makes sure that the benchmark runs.
    benchmark_simple_transfer(10, false).await;
    benchmark_simple_transfer(10, true).await;
}

#[sim_test]
async fn benchmark_move_transactions_smoke_test() {
    // This test makes sure that the benchmark runs.
    benchmark_move_transactions(10, false, 2, 1, 1).await;
    benchmark_move_transactions(10, true, 2, 1, 1).await;
}
