// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use serde_json::Value;
use serde_json::json;
use sui_indexer_alt_e2e_tests::graphql::IndexedGraphQlCluster;
use sui_test_transaction_builder::make_transfer_sui_transaction;
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEffectsAPI;
use test_cluster::TestClusterBuilder;

#[tokio::test]
async fn test_transaction_query_insufficient_funds_error_metadata() {
    let validator_cluster = TestClusterBuilder::new().build().await;
    let graphql_cluster = IndexedGraphQlCluster::new(&validator_cluster).await;

    let recipient = SuiAddress::random_for_testing_only();
    // Split more SUI than the gas coin can hold so execution fails with InsufficientCoinBalance.
    let signed_tx =
        make_transfer_sui_transaction(&validator_cluster.wallet, Some(recipient), Some(u64::MAX))
            .await;
    let executed_tx = validator_cluster
        .wallet
        .execute_transaction_may_fail(signed_tx)
        .await
        .expect("Failed to execute transaction");

    assert!(
        executed_tx.effects.status().is_err(),
        "Transaction should fail with insufficient funds"
    );

    let digest = executed_tx.effects.transaction_digest().to_string();
    let query = r#"
        query($digest: String!) {
            transaction(digest: $digest) {
                effects {
                    status
                    executionError {
                        message
                        metadata
                    }
                    effectsJson
                }
            }
        }
    "#;

    let result = tokio::time::timeout(Duration::from_secs(60), async {
        let mut interval = tokio::time::interval(Duration::from_millis(200));
        loop {
            interval.tick().await;
            let result = graphql_cluster
                .execute_graphql(query, json!({ "digest": &digest }))
                .await
                .expect("GraphQL request failed");

            if result.get("errors").is_some()
                || result.pointer("/data/transaction/effects").is_some()
            {
                break result;
            }
        }
    })
    .await
    .expect("Timed out waiting for GraphQL to index transaction");

    assert!(
        result.get("errors").is_none(),
        "Unexpected GraphQL errors: {result:#}"
    );
    assert_eq!(
        result.pointer("/data/transaction/effects/status"),
        Some(&json!("FAILURE"))
    );

    let metadata = result
        .pointer("/data/transaction/effects/executionError/metadata")
        .expect("executionError.metadata should be present");
    let metadata_message = metadata
        .pointer("/message")
        .and_then(Value::as_str)
        .expect("executionError.metadata.message should be present");

    assert!(
        metadata_message.contains("balance:"),
        "unexpected metadata message: {metadata_message}"
    );
    assert!(
        metadata_message.contains("required:"),
        "unexpected metadata message: {metadata_message}"
    );

    assert_eq!(
        result
            .pointer("/data/transaction/effects/effectsJson/status/error/metadata")
            .expect("effectsJson status error metadata should be present"),
        metadata
    );
}
