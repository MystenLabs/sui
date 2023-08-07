// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;
use sui_macros::sim_test;
use test_utils::network::TestClusterBuilder;

#[sim_test]
async fn basic_checkpoints_integration_test() {
    let test_cluster = TestClusterBuilder::new().build().await.unwrap();
    let tx = test_cluster
        .wallet
        .make_transfer_sui_transaction(None, None)
        .await;
    let digest = *tx.digest();
    test_cluster.execute_transaction(tx).await.unwrap();

    for _ in 0..600 {
        let all_included = test_cluster
            .swarm
            .validator_node_handles()
            .into_iter()
            .all(|handle| {
                handle.with(|node| node.is_transaction_executed_in_checkpoint(&digest).unwrap())
            });
        if all_included {
            // success
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    panic!("Did not include transaction in checkpoint in 60 seconds");
}
