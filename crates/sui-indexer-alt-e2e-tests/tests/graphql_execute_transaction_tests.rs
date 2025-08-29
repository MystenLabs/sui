// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use reqwest::Client;
use serde_json::{json, Value};
use simulacrum::Simulacrum;
use sui_indexer_alt::config::IndexerConfig;
use sui_indexer_alt_consistent_store::config::ServiceConfig as ConsistentConfig;
use sui_indexer_alt_e2e_tests::FullCluster;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_graphql::config::RpcConfig as GraphQlConfig;
use sui_indexer_alt_jsonrpc::config::RpcConfig as JsonRpcConfig;
use sui_indexer_alt_reader::full_node_client::FullNodeArgs;
use sui_macros::sim_test;
use sui_test_transaction_builder::make_transfer_sui_transaction;
use sui_types::base_types::SuiAddress;
use test_cluster::{TestCluster, TestClusterBuilder};
use tokio_util::sync::CancellationToken;

async fn create_graphql_test_cluster(validator_cluster: &TestCluster) -> FullCluster {
    FullCluster::new_with_configs(
        Simulacrum::new(),
        IndexerArgs::default(),
        IndexerArgs::default(),
        IndexerConfig::for_test(),
        ConsistentConfig::for_test(),
        JsonRpcConfig::default(),
        GraphQlConfig::default(),
        FullNodeArgs {
            full_node_rpc_url: Some(validator_cluster.rpc_url().to_string()),
            ..Default::default()
        },
        &Registry::new(),
        CancellationToken::new(),
    )
    .await
    .expect("Failed to create GraphQL test cluster")
}

#[sim_test]
async fn test_execute_transaction_mutation_schema() {
    let validator_cluster = TestClusterBuilder::new().build().await;
    let graphql_cluster = create_graphql_test_cluster(&validator_cluster).await;

    // Create a simple transfer transaction for testing
    let recipient = SuiAddress::random_for_testing_only();
    let signed_tx =
        make_transfer_sui_transaction(&validator_cluster.wallet, Some(recipient), Some(1_000_000))
            .await;
    let (tx_bytes, signatures) = signed_tx.to_tx_bytes_and_signatures();

    let client = Client::new();
    let response = client
        .post(graphql_cluster.graphql_url())
        .json(&json!({
            "query": r#"
            mutation($tx: TransactionExecutionInput!, $sigs: [String!]!) {
                executeTransaction(transaction: $tx, signatures: $sigs) {
                    effects {
                        digest
                        status
                        checkpoint {
                            sequenceNumber
                        }
                        transaction {
                            sender { address }
                            gasInput { gasBudget }
                            signatures {
                                signatureBytes
                            }
                        }
                    }
                    errors
                }
            }
        "#,
            "variables": {
                "tx": { "transactionDataBcs": tx_bytes.encoded() },
                "sigs": signatures.iter().map(|s| s.encoded()).collect::<Vec<_>>()
            }
        }))
        .send()
        .await
        .expect("GraphQL request failed");

    // Verify successful fresh execution
    let result: Value = response.json().await.expect("Failed to parse response");
    let data = result.get("data").expect("Should have data");
    let execute_result = data
        .get("executeTransaction")
        .expect("Should have executeTransaction");
    let effects = execute_result.get("effects").expect("Should have effects");
    let transaction = effects.get("transaction").expect("Should have transaction");

    let status = effects.get("status").unwrap().as_str().unwrap();
    let checkpoint = effects.get("checkpoint");
    let errors = execute_result.get("errors");
    let sender_address = transaction
        .get("sender")
        .unwrap()
        .get("address")
        .unwrap()
        .as_str()
        .unwrap();
    let gas_budget = transaction
        .get("gasInput")
        .unwrap()
        .get("gasBudget")
        .unwrap()
        .as_str()
        .unwrap();
    let returned_sigs = transaction.get("signatures").unwrap().as_array().unwrap();

    assert_eq!(status, "SUCCESS");
    assert_eq!(checkpoint, Some(&serde_json::Value::Null));
    assert_eq!(errors, Some(&serde_json::Value::Null));

    // Verify transaction data matches original
    assert_eq!(
        sender_address,
        validator_cluster.get_address_0().to_string()
    );
    assert_eq!(gas_budget, "10000000");
    assert_eq!(returned_sigs.len(), signatures.len());
    for (returned, original) in returned_sigs.iter().zip(signatures.iter()) {
        let sig_bytes = returned.get("signatureBytes").unwrap().as_str().unwrap();
        assert_eq!(sig_bytes, original.encoded());
    }
}

#[sim_test]
async fn test_execute_transaction_input_validation() {
    let validator_cluster = TestClusterBuilder::new().build().await;
    let graphql_cluster = create_graphql_test_cluster(&validator_cluster).await;
    let client = Client::new();

    // Test invalid Base64 transaction data
    let result = client
        .post(graphql_cluster.graphql_url())
        .json(&json!({
            "query": r#"
            mutation($tx: TransactionExecutionInput!, $sigs: [String!]!) {
                executeTransaction(transaction: $tx, signatures: $sigs) {
                    effects { digest }
                    errors
                }
            }
        "#,
            "variables": {
                "tx": { "transactionDataBcs": "invalid_base64!" },
                "sigs": ["invalidSignature"]
            }
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert!(result.get("errors").is_some());
}

#[sim_test]
async fn test_execute_transaction_with_events() {
    let validator_cluster = TestClusterBuilder::new()
        .enable_fullnode_events()
        .build()
        .await;
    let graphql_cluster = create_graphql_test_cluster(&validator_cluster).await;

    // Publish our test package which emits events in its init function
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/emit_event");
    let tx_data = validator_cluster
        .test_transaction_builder()
        .await
        .publish(path)
        .build();
    let signed_tx = validator_cluster.sign_transaction(&tx_data).await;
    let (tx_bytes, signatures) = signed_tx.to_tx_bytes_and_signatures();

    let client = Client::new();
    let response = client
        .post(graphql_cluster.graphql_url())
        .json(&json!({
            "query": r#"
            mutation($tx: TransactionExecutionInput!, $sigs: [String!]!) {
                executeTransaction(transaction: $tx, signatures: $sigs) {
                    effects {
                        digest
                        status
                        events {
                            nodes {
                                eventBcs
                                sender { address }
                            }
                        }
                    }
                    errors
                }
            }
        "#,
            "variables": {
                "tx": { "transactionDataBcs": tx_bytes.encoded() },
                "sigs": signatures.iter().map(|s| s.encoded()).collect::<Vec<_>>()
            }
        }))
        .send()
        .await
        .expect("GraphQL request failed");

    let result: Value = response.json().await.expect("Failed to parse response");
    let data = result.get("data").expect("Should have data");
    let execute_result = data
        .get("executeTransaction")
        .expect("Should have executeTransaction");
    let effects = execute_result.get("effects").expect("Should have effects");

    // Verify events structure and content
    let events = effects.get("events").expect("Should have events field");
    let event_nodes = events
        .get("nodes")
        .expect("Should have event nodes")
        .as_array()
        .unwrap();

    assert_eq!(effects.get("status").unwrap().as_str().unwrap(), "SUCCESS");
    assert!(
        !event_nodes.is_empty(),
        "Package publish should emit events from init function"
    );

    let sender_address = validator_cluster.get_address_0();
    for event_node in event_nodes {
        let sender = event_node.get("sender").expect("Event should have sender");
        let sender_addr = sender.get("address").unwrap().as_str().unwrap();

        assert!(
            event_node.get("eventBcs").is_some(),
            "Event should have eventBcs"
        );
        assert_eq!(
            sender_addr,
            sender_address.to_string(),
            "Event sender should match transaction sender"
        );
    }
}

#[sim_test]
async fn test_execute_transaction_grpc_errors() {
    let validator_cluster = TestClusterBuilder::new().build().await;
    let graphql_cluster = create_graphql_test_cluster(&validator_cluster).await;

    // Create signature mismatch scenario: use transaction data from one tx with signatures from another
    let recipient1 = SuiAddress::random_for_testing_only();
    let recipient2 = SuiAddress::random_for_testing_only();

    let signed_tx1 =
        make_transfer_sui_transaction(&validator_cluster.wallet, Some(recipient1), Some(1_000_000))
            .await;
    let signed_tx2 =
        make_transfer_sui_transaction(&validator_cluster.wallet, Some(recipient2), Some(2_000_000))
            .await;

    let (tx1_bytes, _) = signed_tx1.to_tx_bytes_and_signatures();
    let (_, tx2_signatures) = signed_tx2.to_tx_bytes_and_signatures();

    // This will pass GraphQL validation but fail at gRPC execution due to signature mismatch
    let client = Client::new();
    let response = client
        .post(graphql_cluster.graphql_url())
        .json(&json!({
            "query": r#"
            mutation($tx: TransactionExecutionInput!, $sigs: [String!]!) {
                executeTransaction(transaction: $tx, signatures: $sigs) {
                    effects {
                        digest
                        status
                    }
                    errors
                }
            }
        "#,
            "variables": {
                "tx": { "transactionDataBcs": tx1_bytes.encoded() },
                "sigs": tx2_signatures.iter().map(|s| s.encoded()).collect::<Vec<_>>()
            }
        }))
        .send()
        .await
        .expect("GraphQL request failed");

    let result: Value = response.json().await.expect("Failed to parse response");
    let data = result.get("data").expect("Should have data");
    let execute_result = data
        .get("executeTransaction")
        .expect("Should have executeTransaction");

    // Verify gRPC execution failure response structure
    let effects = execute_result.get("effects");
    assert_eq!(
        effects,
        Some(&serde_json::Value::Null),
        "Should have null effects on gRPC error"
    );

    let errors = execute_result
        .get("errors")
        .expect("Should have errors field");
    let error_array = errors.as_array().expect("Errors should be an array");
    assert!(!error_array.is_empty());
}
