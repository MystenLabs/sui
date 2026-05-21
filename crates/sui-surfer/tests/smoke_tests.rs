// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::time::Duration;
use sui_macros::sim_test;

#[sim_test]
async fn smoke_test() {
    // This test makes sure that the sui surfer runs.
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "move_building_blocks"]);
    let results =
        sui_surfer::run(Duration::from_secs(30), Duration::from_secs(15), vec![path]).await;
    results.print_stats();
    assert!(results.num_successful_transactions > 0);
    // The building-block package exposes many modules (objects, collections,
    // crypto, dynamic fields, coins, events, receiving, party, etc.). Make sure a
    // broad set of them is actually exercised, not just a couple.
    assert!(
        results.unique_move_functions_called.len() >= 30,
        "expected the surfer to exercise many functions, only saw {}",
        results.unique_move_functions_called.len()
    );
    // Make sure the newer transaction/object features are reachable: gasless and
    // address-balance gas, object receiving, and party (consensus-address-owned)
    // objects. At least one of these should fire in a run of this length.
    let advanced_feature_txns = results.num_gasless_transactions
        + results.num_address_balance_gas_transactions
        + results.num_receiving_transactions
        + results.num_party_object_transactions;
    assert!(
        advanced_feature_txns > 0,
        "expected the surfer to exercise gasless / address-balance / receiving / party features"
    );
}
