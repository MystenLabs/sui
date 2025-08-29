// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::Context;
use reqwest::Client;
use serde_json::{json, Value};
use sui_indexer_alt_e2e_tests::FullCluster;
use sui_macros::sim_test;
use sui_types::{
    base_types::SuiAddress,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{Transaction, TransactionData},
};

const DEFAULT_GAS_BUDGET: u64 = 1_000_000_000;

struct GraphQlExecuteTestCluster {
    cluster: FullCluster,
    client: Client,
    graphql_url: String,
}

impl GraphQlExecuteTestCluster {
    async fn new() -> anyhow::Result<Self> {
        // Use FullCluster which includes Simulacrum + all indexer services
        // This automatically configures all services including GraphQL with gRPC support
        let cluster = FullCluster::new()
            .await
            .context("Failed to create FullCluster")?;

        let graphql_url = cluster.graphql_url().to_string();
        
        Ok(Self {
            cluster,
            client: Client::new(),
            graphql_url,
        })
    }

    /// Execute a GraphQL mutation and return the response
    async fn execute_graphql_mutation(
        &self,
        query: &str,
        variables: Value,
    ) -> anyhow::Result<Value> {
        let body = json!({
            "query": query,
            "variables": variables
        });

        let response = self
            .client
            .post(&self.graphql_url)
            .json(&body)
            .send()
            .await
            .context("Failed to send GraphQL request")?;

        let result: Value = response
            .json()
            .await
            .context("Failed to parse GraphQL response")?;

        if let Some(errors) = result.get("errors") {
            anyhow::bail!("GraphQL errors: {}", errors);
        }

        Ok(result["data"].clone())
    }

    /// Create a simple transfer transaction for testing
    async fn create_transfer_transaction(&mut self) -> anyhow::Result<(String, Vec<String>)> {
        // Get funded accounts from FullCluster
        let (sender, sender_kp, gas) = self
            .cluster
            .funded_account(DEFAULT_GAS_BUDGET + 1000)
            .context("Failed to fund sender account")?;

        // Use a fixed recipient address for simplicity
        let recipient_address = SuiAddress::random_for_testing_only();

        // Build transfer transaction
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.transfer_sui(recipient_address, Some(1000));

        let data = TransactionData::new_programmable(
            sender,
            vec![gas],
            builder.finish(),
            DEFAULT_GAS_BUDGET,
            self.cluster.reference_gas_price(),
        );

        // Sign transaction
        let transaction = Transaction::from_data_and_signer(data, vec![&sender_kp]);
        let (tx_bytes, signatures) = transaction.to_tx_bytes_and_signatures();

        // Convert to base64 strings for GraphQL
        let tx_data_bcs = tx_bytes.encoded();
        let signature_strings: Vec<String> = signatures
            .iter()
            .map(|sig| sig.encoded())
            .collect();

        println!("Generated {} signatures", signature_strings.len());
        for (i, sig) in signature_strings.iter().enumerate() {
            println!("Signature {}: {} (length: {})", i, sig, sig.len());
        }

        Ok((tx_data_bcs, signature_strings))
    }

    /// Query a transaction by digest after it's been indexed
    async fn query_transaction(&self, digest: &str) -> anyhow::Result<Value> {
        let query = r#"
            query($digest: String!) {
                transaction(digest: $digest) {
                    digest
                    sender {
                        address
                    }
                    effects {
                        status
                        checkpoint {
                            sequenceNumber
                        }
                        events {
                            nodes {
                                type {
                                    repr
                                }
                            }
                        }
                    }
                }
            }
        "#;

        let variables = json!({
            "digest": digest
        });

        self.execute_graphql_mutation(query, variables).await
    }
}

#[sim_test]
async fn test_execute_transaction_success() {
    // Initialize logging for debugging
    telemetry_subscribers::init_for_testing();

    let mut test_cluster = GraphQlExecuteTestCluster::new()
        .await
        .expect("Failed to create test cluster");

    // Create a transfer transaction
    let (tx_data_bcs, signatures) = test_cluster
        .create_transfer_transaction()
        .await
        .expect("Failed to create transfer transaction");

    // Execute transaction via GraphQL mutation
    let mutation = r#"
        mutation($transactionData: TransactionExecutionInput!, $signatures: [String!]!) {
            executeTransaction(
                transaction: $transactionData,
                signatures: $signatures
            ) {
                effects {
                    digest
                    status
                    checkpoint {
                        sequenceNumber
                    }
                }
                errors
            }
        }
    "#;

    let variables = json!({
        "transactionData": {
            "transactionDataBcs": tx_data_bcs
        },
        "signatures": signatures
    });

    let response = test_cluster
        .execute_graphql_mutation(mutation, variables)
        .await
        .expect("Failed to execute GraphQL mutation");

    println!("Execute transaction response: {:#}", response);

    // Verify the response structure
    let execute_result = response["executeTransaction"].as_object()
        .expect("executeTransaction should be an object");

    // Check that we got effects (successful execution)
    let effects = execute_result["effects"].as_object()
        .expect("effects should be present for successful execution");

    // Check that there are no errors
    assert!(
        execute_result["errors"].is_null(),
        "Expected no errors, got: {}",
        execute_result["errors"]
    );

    // Verify fresh transaction effects
    let digest = effects["digest"].as_str()
        .expect("digest should be present");
    assert!(!digest.is_empty(), "digest should not be empty");

    let status = effects["status"].as_str()
        .expect("status should be present");
    assert_eq!(status, "success", "transaction should be successful");

    // Verify fresh data characteristics (no checkpoint yet)
    assert!(
        effects["checkpoint"].is_null(),
        "Fresh transaction should not have checkpoint: {}",
        effects["checkpoint"]
    );

    // Verify that we can access events from fresh execution
    let events = &effects["events"]["nodes"];
    assert!(events.is_array(), "events should be accessible from fresh execution");

    println!("✅ Fresh transaction executed successfully!");
    println!("   Digest: {}", digest);
    println!("   Status: {}", status);
    println!("   Events: {} events found", events.as_array().unwrap().len());

    // Optional: Wait for indexing and verify consistency
    println!("⏳ Waiting for indexing to complete...");
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Query the same transaction after indexing
    let indexed_response = test_cluster
        .query_transaction(digest)
        .await
        .expect("Failed to query indexed transaction");

    let indexed_tx = indexed_response["transaction"].as_object()
        .expect("transaction should be found after indexing");

    let indexed_effects = indexed_tx["effects"].as_object()
        .expect("indexed transaction should have effects");

    // Verify consistency between fresh and indexed data
    assert_eq!(
        indexed_effects["digest"].as_str().unwrap(),
        digest,
        "Digest should be consistent between fresh and indexed"
    );

    assert_eq!(
        indexed_effects["status"].as_str().unwrap(),
        status,
        "Status should be consistent between fresh and indexed"
    );

    // Now the indexed version should have a checkpoint
    assert!(
        !indexed_effects["checkpoint"].is_null(),
        "Indexed transaction should have checkpoint"
    );

    println!("✅ Indexed transaction data is consistent with fresh execution!");
}

#[sim_test]
async fn test_execute_transaction_grpc_error() {
    telemetry_subscribers::init_for_testing();

    let mut test_cluster = GraphQlExecuteTestCluster::new()
        .await
        .expect("Failed to create test cluster");

    // Create invalid transaction data (empty signatures to trigger error)
    let (tx_data_bcs, _) = test_cluster
        .create_transfer_transaction()
        .await
        .expect("Failed to create transfer transaction");

    let mutation = r#"
        mutation($transactionData: TransactionExecutionInput!, $signatures: [String!]!) {
            executeTransaction(
                transaction: $transactionData,
                signatures: $signatures
            ) {
                effects {
                    digest
                }
                errors
            }
        }
    "#;

    let variables = json!({
        "transactionData": {
            "transactionDataBcs": tx_data_bcs
        },
        "signatures": [] // Empty signatures should cause gRPC error
    });

    let response = test_cluster
        .execute_graphql_mutation(mutation, variables)
        .await
        .expect("Failed to execute GraphQL mutation");

    println!("Error response: {:#}", response);

    let execute_result = response["executeTransaction"].as_object()
        .expect("executeTransaction should be an object");

    // For gRPC errors, we should get errors instead of effects
    assert!(
        execute_result["effects"].is_null(),
        "Should not have effects for failed execution"
    );

    let errors = execute_result["errors"].as_array()
        .expect("Should have errors for failed execution");

    assert!(!errors.is_empty(), "Should have at least one error");
    
    let error_msg = errors[0].as_str()
        .expect("Error should be a string");
    
    println!("✅ gRPC error handled correctly: {}", error_msg);
}
