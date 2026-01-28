// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use reqwest::Client;
use serde_json::json;
use sui_indexer_alt_graphql::config::Limits;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::Argument;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;

use sui_indexer_alt_e2e_tests::FullCluster;

/// Gas budget for transactions
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

/// GraphQL query to fetch objects owned by senders of the last N transactions
const QUERY_TEMPLATE: &str = r#"
query ($last: Int!) {
  transactions(last: $last, filter: { kind: PROGRAMMABLE_TX }) {
    nodes {
      sender {
        objects {
          nodes {
            address
          }
        }
      }
    }
  }
}
"#;

/// Run the rich query test with a given `last` value
async fn test_rich_query(cluster: &FullCluster, last: u64) -> serde_json::Value {
    let client = Client::new();
    let url = cluster.graphql_url();

    let query = json!({
        "query": QUERY_TEMPLATE,
        "variables": { "last": last }
    });

    client
        .post(url.as_str())
        .json(&query)
        .send()
        .await
        .expect("Request to GraphQL server failed")
        .json()
        .await
        .expect("Failed to parse GraphQL response")
}

/// Count the number of RESOURCE_EXHAUSTED errors in a GraphQL response
fn count_resource_exhausted_errors(response: &serde_json::Value) -> usize {
    response
        .pointer("/errors")
        .and_then(|errors| errors.as_array())
        .map(|errors| {
            errors
                .iter()
                .filter(|error| {
                    error
                        .pointer("/extensions/code")
                        .and_then(|code| code.as_str())
                        .is_some_and(|code| code == "RESOURCE_EXHAUSTED")
                })
                .count()
        })
        .unwrap_or(0)
}

#[tokio::test]
async fn test_rich_query_below_limit() {
    let mut cluster = FullCluster::new().await.expect("Failed to create cluster");

    let max_rich_queries = Limits::default().max_rich_queries;
    for _ in 0..max_rich_queries - 1 {
        let mut builder = ProgrammableTransactionBuilder::new();
        let (sender, kp, gas) = cluster
            .funded_account(DEFAULT_GAS_BUDGET)
            .expect("Failed to get funded account");

        builder.transfer_args(sender, vec![Argument::GasCoin]);
        let data = TransactionData::new_programmable(
            sender,
            vec![gas],
            builder.finish(),
            DEFAULT_GAS_BUDGET,
            cluster.reference_gas_price(),
        );

        let tx = Transaction::from_data_and_signer(data, vec![&kp]);
        cluster.execute_transaction(tx).expect("Transaction failed");
    }

    cluster.create_checkpoint().await;

    let last = (max_rich_queries - 1) as u64;
    let response = test_rich_query(&cluster, last).await;

    let error_count = count_resource_exhausted_errors(&response);
    assert_eq!(0, error_count);

    let data = response.pointer("/data");
    assert!(data.is_some());
}

#[tokio::test]
async fn test_rich_query_at_limit() {
    let mut cluster = FullCluster::new().await.expect("Failed to create cluster");

    let max_rich_queries = Limits::default().max_rich_queries;
    for _ in 0..max_rich_queries {
        let mut builder = ProgrammableTransactionBuilder::new();
        let (sender, kp, gas) = cluster
            .funded_account(DEFAULT_GAS_BUDGET)
            .expect("Failed to get funded account");

        builder.transfer_args(sender, vec![Argument::GasCoin]);
        let data = TransactionData::new_programmable(
            sender,
            vec![gas],
            builder.finish(),
            DEFAULT_GAS_BUDGET,
            cluster.reference_gas_price(),
        );

        let tx = Transaction::from_data_and_signer(data, vec![&kp]);
        cluster.execute_transaction(tx).expect("Transaction failed");
    }

    cluster.create_checkpoint().await;

    let last = max_rich_queries as u64;
    let response = test_rich_query(&cluster, last).await;

    // The `transactions` query is also a rich query, so that plus the exact number of owned object
    // queries nested underneath it will exceed the limit.
    let error_count = count_resource_exhausted_errors(&response);
    assert!(error_count > 0);
}

#[tokio::test]
async fn test_rich_query_above_limit() {
    let mut cluster = FullCluster::new().await.expect("Failed to create cluster");

    let max_rich_queries = Limits::default().max_rich_queries;
    for _ in 0..max_rich_queries + 1 {
        let mut builder = ProgrammableTransactionBuilder::new();
        let (sender, kp, gas) = cluster
            .funded_account(DEFAULT_GAS_BUDGET)
            .expect("Failed to get funded account");

        builder.transfer_args(sender, vec![Argument::GasCoin]);
        let data = TransactionData::new_programmable(
            sender,
            vec![gas],
            builder.finish(),
            DEFAULT_GAS_BUDGET,
            cluster.reference_gas_price(),
        );

        let tx = Transaction::from_data_and_signer(data, vec![&kp]);
        cluster.execute_transaction(tx).expect("Transaction failed");
    }

    cluster.create_checkpoint().await;

    let last = (max_rich_queries + 1) as u64;
    let response = test_rich_query(&cluster, last).await;

    let error_count = count_resource_exhausted_errors(&response);
    assert!(error_count > 0);
}
