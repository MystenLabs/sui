// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use strum::IntoEnumIterator;
use sui_macros::sim_test;
use sui_single_node_benchmark::command::Component;
use sui_single_node_benchmark::execution::{
    benchmark_move_transactions, benchmark_simple_transfer,
};

#[sim_test]
async fn benchmark_simple_transfer_smoke_test() {
    // This test makes sure that the benchmark runs.
    for component in Component::iter() {
        benchmark_simple_transfer(10, component).await;
    }
}

#[sim_test]
async fn benchmark_move_transactions_smoke_test() {
    // This test makes sure that the benchmark runs.
    for component in Component::iter() {
        benchmark_move_transactions(10, component, 2, 1, 1).await;
    }
}
