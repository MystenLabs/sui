// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use sui_json_rpc_api::ReadApiClient;
use sui_test_transaction_builder::publish_package;
use sui_types::transaction::CallArg;
use test_cluster::TestClusterBuilder;

#[tokio::test]
async fn test_multi_get_transaction_blocks() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new()
        .with_indexer_backed_rpc()
        .build()
        .await;

    // publish package
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    let move_package = publish_package(&cluster.wallet, path).await.0;

    // execute a transaction
    let function = "emit_3";
    let arguments = vec![CallArg::Pure(bcs::to_bytes(&5u64).unwrap())];
    let transaction = cluster
        .test_transaction_builder()
        .await
        .move_call(move_package, "events_queries", function, arguments)
        .build();
    let signed_transaction = cluster.wallet.sign_transaction(&transaction);
    let first_tx = cluster.execute_transaction(signed_transaction).await;

    // another transaction
    let function = "emit_3";
    let arguments = vec![CallArg::Pure(bcs::to_bytes(&5u64).unwrap())];
    let transaction = cluster
        .test_transaction_builder()
        .await
        .move_call(move_package, "events_queries", function, arguments)
        .build();
    let signed_transaction = cluster.wallet.sign_transaction(&transaction);
    let second_tx = cluster.execute_transaction(signed_transaction).await;

    let http_client = cluster.rpc_client();

    // individual queries
    let fetched_first_tx = http_client
        .get_transaction_block(first_tx.digest, None)
        .await?;
    assert_eq!(first_tx.digest, fetched_first_tx.digest);
    let fetched_second_tx = http_client
        .get_transaction_block(second_tx.digest, None)
        .await?;
    assert_eq!(second_tx.digest, fetched_second_tx.digest);

    // multi-get query
    let fetched_txs = http_client
        .multi_get_transaction_blocks(vec![first_tx.digest, second_tx.digest], None)
        .await?;

    // First verify we got exactly 2 transactions
    assert_eq!(fetched_txs.len(), 2);

    // Then verify that both original digests are present exactly once
    let fetched_digests: Vec<_> = fetched_txs.iter().map(|tx| tx.digest).collect();

    // Verify first_tx.digest is present exactly once
    assert!(fetched_digests.contains(&first_tx.digest));
    assert_eq!(
        fetched_digests
            .iter()
            .filter(|&&d| d == first_tx.digest)
            .count(),
        1
    );

    // Verify second_tx.digest is present exactly once
    assert!(fetched_digests.contains(&second_tx.digest));
    assert_eq!(
        fetched_digests
            .iter()
            .filter(|&&d| d == second_tx.digest)
            .count(),
        1
    );

    Ok(())
}
