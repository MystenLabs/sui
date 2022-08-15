// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Execute every entry function in Move framework and examples and ensure costs don't change
// To review snapshot changes, and fix snapshot differences,
// 0. Install cargo-insta
// 1. Run `cargo insta test --review` under `./sui-cost`.
// 2. Review, accept or reject changes.

use sui_cost::empirical_transaction_cost::run_common_tx_costs;

use insta::assert_yaml_snapshot;

#[tokio::test]
async fn test_good_snapshot() -> Result<(), anyhow::Error> {
    let common_costs = run_common_tx_costs().await?;
    assert_yaml_snapshot!(common_costs);

    Ok(())
}
