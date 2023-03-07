// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use bytes::Bytes;
use std::time::Duration;
use test_utils::cluster::{setup_tracing, Cluster};
use types::TransactionProto;

type StringTransaction = String;

#[ignore]
#[tokio::test]
async fn test_restore_from_disk() {
    // Enabled debug tracing so we can easily observe the
    // nodes logs.
    let _guard = setup_tracing();

    let mut cluster = Cluster::new(None, true);

    // start the cluster
    cluster.start(Some(4), Some(1), None).await;

    let id = 0;
    let client = cluster.authority(0).new_transactions_client(&id).await;

    // Subscribe to the transaction confirmation channel
    let mut receiver = cluster
        .authority(0)
        .primary()
        .await
        .tx_transaction_confirmation
        .subscribe();

    // Create arbitrary transactions
    let mut total_tx = 3;
    for tx in [
        string_transaction(),
        string_transaction(),
        string_transaction(),
    ] {
        let mut c = client.clone();
        tokio::spawn(async move {
            let tr = bcs::to_bytes(&tx).unwrap();
            let txn = TransactionProto {
                transaction: Bytes::from(tr),
            };

            c.submit_transaction(txn).await.unwrap();
        });
    }

    // wait for transactions to complete
    loop {
        if let Ok(_result) = receiver.recv().await {
            total_tx -= 1;
            if total_tx < 1 {
                break;
            }
        }
    }

    // Now stop node 0
    cluster.stop_node(0).await;

    // Let other primaries advance and primary 0 releases its port.
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Now start the node 0 again
    cluster.start_node(0, true, Some(1)).await;

    // Let the node recover
    tokio::time::sleep(Duration::from_secs(2)).await;

    let node = cluster.authority(0);

    // Check the metrics to ensure the node was recovered from disk
    let _primary = node.primary().await;

    // let node_recovered_state =
    //     if let Some(metric) = primary.metric("recovered_consensus_state").await {
    //         let value = metric.get_counter().get_value();
    //         info!("Found metric for recovered consensus state.");

    //         value > 0.0
    //     } else {
    //         false
    //     };

    // assert!(node_recovered_state, "Node did not recover state from disk");
}

fn string_transaction() -> StringTransaction {
    StringTransaction::from("test transaction")
}

#[ignore]
#[tokio::test]
async fn test_read_causal_signed_certificates() {
    const CURRENT_ROUND_METRIC: &str = "current_round";

    // Enabled debug tracing so we can easily observe the
    // nodes logs.
    let _guard = setup_tracing();

    let mut cluster = Cluster::new(None, true);

    // start the cluster
    cluster.start(Some(4), Some(1), None).await;

    // Let primaries advance little bit
    tokio::time::sleep(Duration::from_secs(10)).await;

    // // Ensure all nodes advanced
    // for authority in cluster.authorities().await {
    //     if let Some(metric) = authority.primary().await.metric(CURRENT_ROUND_METRIC).await {
    //         let value = metric.get_gauge().get_value();

    //         info!("Metric -> {:?}", value);

    //         // If the current round is increasing then it means that the
    //         // node starts catching up and is proposing.
    //         assert!(value > 1.0, "Node didn't progress further than the round 1");
    //     }
    // }

    // Now stop node 0
    cluster.stop_node(0).await;

    // Let other primaries advance and primary 0 releases its port.
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Now start the validator 0 again
    cluster.start_node(0, true, Some(1)).await;

    // Now check that the current round advances. Give the opportunity with a few
    // iterations. If metric hasn't picked up then we know that node can't make
    // progress.
    let node_made_progress = false;
    let _node = cluster.authority(0).primary().await;

    // for _ in 0..10 {
    //     tokio::time::sleep(Duration::from_secs(1)).await;

    //     if let Some(metric) = node.metric(CURRENT_ROUND_METRIC).await {
    //         let value = metric.get_gauge().get_value();
    //         info!("Metric -> {:?}", value);

    //         // If the current round is increasing then it means that the
    //         // node starts catching up and is proposing.
    //         if value > 1.0 {
    //             node_made_progress = true;
    //             break;
    //         }
    //     }
    // }

    assert!(
        node_made_progress,
        "Node 0 didn't make progress - causal completion didn't succeed"
    );
}
