// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use std::time::Duration;

use insta::assert_json_snapshot;
use move_core_types::ident_str;
use reqwest::Client;
use serde_json::{Value, json};
use simulacrum::Simulacrum;

use sui_indexer_alt::config::{ConcurrentLayer, IndexerConfig, PipelineLayer, PrunerLayer};
use sui_indexer_alt_e2e_tests::{FullCluster, OffchainClusterConfig, find::address_owned};
use sui_indexer_alt_graphql::config::{RpcConfig as GraphQlConfig, WatermarkConfig};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    crypto::{Signature, Signer, get_account_key_pair},
    digests::TransactionDigest,
    effects::TransactionEffectsAPI,
    object::Owner,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{Transaction, TransactionData},
};

/// 5 SUI gas budget
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

// GraphQL query constants
const AVAILABLE_RANGE_QUERY: &str = r#"
    query($type: String!, $field: String, $filters: [String]) {
        serviceConfig {
            availableRange(type: $type, field: $field, filters: $filters) {
                first { sequenceNumber }
                last { sequenceNumber }
            }
        }
    }
"#;

const TRANSACTIONS_QUERY: &str = r#"
    query($filter: TransactionFilter!, $first: Int) {
        transactions(filter: $filter, first: $first) {
            nodes {
                digest
            }
        }
    }
"#;

const EVENTS_QUERY: &str = r#"
    query($filter: EventFilter!, $first: Int) {
        events(filter: $filter, first: $first) {
            nodes {
                transaction { digest }
                sequenceNumber
            }
        }
    }
"#;

const CHECKPOINT_QUERY: &str = r#"
    query {
        checkpoints(first: 50) {
            nodes {
                sequenceNumber
            }
        }
    }
"#;

/// Test available range queries with retention configurations
#[tokio::test]
async fn test_available_range_with_pipelines() {
    let mut cluster = cluster_with_pipelines(PipelineLayer {
        tx_affected_addresses: Some(concurrent_pipeline(5)),
        tx_digests: Some(concurrent_pipeline(10)),
        cp_sequence_numbers: Some(ConcurrentLayer::default()),
        ..Default::default()
    })
    .await;

    let (a, akp) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // (1) Create 4 checkpoints with transactions from `a` and `b`. Each checkpoint contains one
    // transactions from `a` to `b`.  At this point we have 5 total checkpoints (Cp 0 is created by the cluster),
    // nothing should be pruned on either side.
    for _ in 1..=4 {
        transfer_dust(&mut cluster, a, &akp, b);
        cluster.create_checkpoint().await;
    }

    let affected_addresses_available_range =
        query_available_range(&cluster, "transactions", Some(&["affectedAddress"])).await;

    let (first, last) = collect_sequence_numbers(&affected_addresses_available_range);

    assert_eq!(first, 0);
    assert_eq!(last, 4);

    // (2) Add 1 more checkpoint, now the affected_addresses table is pruned, but the digests are not.
    transfer_dust(&mut cluster, a, &akp, b);
    cluster.create_checkpoint().await;

    cluster
        .wait_for_pruner("tx_affected_addresses", 1, Duration::from_secs(10))
        .await
        .unwrap();

    let affected_addresses_available_range =
        query_available_range(&cluster, "transactions", Some(&["affectedAddress"])).await;

    let (first, last) = collect_sequence_numbers(&affected_addresses_available_range);
    assert_eq!(first, 1);
    assert_eq!(last, 5);

    // (3) Add 5 more checkpoints, now both tables have been pruned.
    for _ in 6..=10 {
        transfer_dust(&mut cluster, a, &akp, b);
        cluster.create_checkpoint().await;
    }

    cluster
        .wait_for_pruner("tx_digests", 1, Duration::from_secs(10))
        .await
        .unwrap();

    cluster
        .wait_for_pruner("tx_affected_addresses", 6, Duration::from_secs(10))
        .await
        .unwrap();

    let transasction_available_range = query_available_range(&cluster, "transactions", None).await;

    let (first, last) = collect_sequence_numbers(&transasction_available_range);
    assert_eq!(first, 1);
    assert_eq!(last, 10);
}

/// Test that querying available range for a pipeline that is not enabled returns an error
#[tokio::test]
async fn test_available_range_pipeline_unavailable() {
    let cluster = cluster_with_pipelines(PipelineLayer {
        cp_sequence_numbers: Some(ConcurrentLayer::default()),
        ..Default::default()
    })
    .await;

    let response = query_available_range(&cluster, "objects", None).await;
    assert_json_snapshot!(response["errors"], @r###"
    [
      {
        "message": "consistent queries across objects and balances not available",
        "locations": [
          {
            "line": 4,
            "column": 13
          }
        ],
        "path": [
          "serviceConfig",
          "availableRange"
        ],
        "extensions": {
          "code": "FEATURE_UNAVAILABLE"
        }
      }
    ]"###);
}

/// Test available range queries with retention configurations
#[tokio::test]
async fn test_transaction_pagination_pruning() {
    let mut cluster = cluster_with_pipelines(PipelineLayer {
        tx_affected_addresses: Some(concurrent_pipeline(5)),
        tx_digests: Some(concurrent_pipeline(10)),
        kv_transactions: Some(ConcurrentLayer::default()),
        cp_sequence_numbers: Some(ConcurrentLayer::default()),
        ..Default::default()
    })
    .await;

    let (a, akp) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    let mut a_txs = vec![];

    // (1) Create 4 checkpoints with transactions from `a` and `b`. Each checkpoint contains one
    // transactions from `a` to `b`.  We have 4 total transactions, one in each of the 4 checkpoints.
    for _ in 1..=4 {
        a_txs.push(transfer_dust(&mut cluster, a, &akp, b));
        cluster.create_checkpoint().await;
    }

    let transactions_in_range = query_transactions(&cluster, b).await;
    let actual = collect_digests(&transactions_in_range);
    assert_eq!(&a_txs, &actual);

    // (2) Add 2 more checkpoints, with 1 transaction each. The affected_addresses table is pruned, but the digests are not.
    for _ in 5..=6 {
        a_txs.push(transfer_dust(&mut cluster, a, &akp, b));
        cluster.create_checkpoint().await;
    }

    cluster
        .wait_for_pruner("tx_affected_addresses", 2, Duration::from_secs(10))
        .await
        .unwrap();

    let transactions_in_range = query_transactions(&cluster, b).await;
    let actual = collect_digests(&transactions_in_range);
    assert_eq!(&a_txs[1..], &actual);

    // (3) Add 5 more checkpoints, now both tables have been pruned but we take the max reader_lo of the pipelines.
    for _ in 6..=10 {
        a_txs.push(transfer_dust(&mut cluster, a, &akp, b));
        cluster.create_checkpoint().await;
    }

    cluster
        .wait_for_pruner("tx_digests", 2, Duration::from_secs(10))
        .await
        .unwrap();

    cluster
        .wait_for_pruner("tx_affected_addresses", 7, Duration::from_secs(10))
        .await
        .unwrap();

    let transactions_in_range = query_transactions(&cluster, b).await;
    let actual = collect_digests(&transactions_in_range);
    assert_eq!(&a_txs[6..], &actual);
}

#[tokio::test]
async fn test_events_pagination_pruning() {
    let mut cluster = cluster_with_pipelines(PipelineLayer {
        tx_affected_addresses: Some(concurrent_pipeline(10)),
        tx_digests: Some(concurrent_pipeline(10)),
        ev_emit_mod: Some(concurrent_pipeline(5)),
        ev_struct_inst: Some(concurrent_pipeline(10)),
        kv_transactions: Some(ConcurrentLayer::default()),
        cp_sequence_numbers: Some(ConcurrentLayer::default()),
        ..Default::default()
    })
    .await;

    let pkg = publish_emit_event(&mut cluster).await;

    let mut tx_digests = vec![];

    // 1) Create checkpoints 1 through 9, each containing one event emitted by the test package.
    // The ev_emit_mod pipeline is pruned at this point, but the ev_struct_inst pipeline is not.
    for _ in 1..=9 {
        tx_digests.push(emit_test_event(&mut cluster, &pkg).await);
        cluster.create_checkpoint().await;
    }

    cluster
        .wait_for_pruner("ev_emit_mod", 4, Duration::from_secs(10))
        .await
        .unwrap();

    let events_in_range = query_events(&cluster, json!({ "module": pkg.to_string() })).await;
    let actual = collect_digests(&events_in_range);
    assert_eq!(&tx_digests[4..], &actual);

    let events_in_range = query_events(&cluster, json!({ "type": pkg.to_string() })).await;
    let actual = collect_digests(&events_in_range);
    assert_eq!(&tx_digests, &actual);
}

#[tokio::test]
async fn test_checkpoint_pagination_pruning() {
    let mut cluster = cluster_with_pipelines(PipelineLayer {
        cp_sequence_numbers: Some(concurrent_pipeline(5)),
        ..Default::default()
    })
    .await;

    let mut cp_sequence_numbers = vec![];

    // Create checkpoints 1 through 9
    for _ in 1..=9 {
        cp_sequence_numbers.push(cluster.create_checkpoint().await.sequence_number);
    }

    // We only retain 5 checkpoints so only checkpoints 5 through 9 should be available after pruning.
    cluster
        .wait_for_pruner("cp_sequence_numbers", 4, Duration::from_secs(10))
        .await
        .unwrap();

    let checkpoints_in_range = execute_graphql_query(&cluster, CHECKPOINT_QUERY, None).await;
    let checkpoints = checkpoints_in_range["data"]["checkpoints"]["nodes"]
        .as_array()
        .unwrap();

    assert_eq!(
        checkpoints[0]["sequenceNumber"].as_u64().unwrap(),
        cp_sequence_numbers[4]
    );
    assert_eq!(
        checkpoints[3]["sequenceNumber"].as_u64().unwrap(),
        cp_sequence_numbers[7]
    );
}

/// Set-up a cluster with a custom configuration for pipelines.
async fn cluster_with_pipelines(pipeline: PipelineLayer) -> FullCluster {
    FullCluster::new_with_configs(
        Simulacrum::new(),
        OffchainClusterConfig {
            indexer_config: IndexerConfig {
                pipeline,
                ..IndexerConfig::for_test()
            },
            graphql_config: GraphQlConfig {
                watermark: WatermarkConfig {
                    watermark_polling_interval: Duration::from_millis(50),
                },
                ..Default::default()
            },
            ..Default::default()
        },
        &prometheus::Registry::new(),
    )
    .await
    .expect("Failed to create cluster")
}

fn collect_digests(resp: &Value) -> Vec<TransactionDigest> {
    const RESPONSE_PATHS: &[(&str, &str)] = &[
        ("/data/events/nodes", "/transaction/digest"),
        ("/data/transactions/nodes", "/digest"),
    ];

    let (nodes, digest_path) = RESPONSE_PATHS
        .iter()
        .find_map(|(nodes_path, digest_path)| {
            resp.pointer(nodes_path).map(|nodes| (nodes, *digest_path))
        })
        .expect("Response must contain events or transactions");

    nodes
        .as_array()
        .expect("nodes must be an array")
        .iter()
        .map(|node| {
            let digest = node
                .pointer(digest_path)
                .and_then(Value::as_str)
                .expect("node must contain digest");
            TransactionDigest::from_str(digest).expect("invalid digest format")
        })
        .collect()
}

fn collect_sequence_numbers(resp: &Value) -> (u64, u64) {
    let range = &resp["data"]["serviceConfig"]["availableRange"];
    (
        range["first"]["sequenceNumber"].as_u64().unwrap(),
        range["last"]["sequenceNumber"].as_u64().unwrap(),
    )
}

async fn publish_emit_event(cluster: &mut FullCluster) -> ObjectID {
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/event/emit_test_event");
    // Create an address and fund it to run the following transactions.
    let (sender, kp, gas) = cluster.funded_account(1000 * DEFAULT_GAS_BUDGET).unwrap();

    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(
            TestTransactionBuilder::new(sender, gas, 1000)
                .with_gas_budget(DEFAULT_GAS_BUDGET)
                .publish(path)
                .build(),
            vec![&kp],
        ))
        .expect("Failed to execute publish transaction");
    cluster.create_checkpoint().await;

    fx.created()
        .into_iter()
        .find_map(|((pkg, v, _), owner)| {
            (v.value() == 1 && matches!(owner, Owner::Immutable)).then_some(pkg)
        })
        .expect("Failed to find package ID")
}

async fn emit_test_event(cluster: &mut FullCluster, pkg: &ObjectID) -> TransactionDigest {
    let (sender, kp, gas) = cluster.funded_account(1000 * DEFAULT_GAS_BUDGET).unwrap();
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.programmable_move_call(
        *pkg,
        ident_str!("emit_test_event").to_owned(),
        ident_str!("emit_test_event").to_owned(),
        vec![],
        vec![],
    );
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    let digest = data.digest();
    let (effects, error) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .expect("Emit failed");
    assert!(error.is_none(), "Emit failed: {error:?}");
    assert!(effects.status().is_ok(), "Emit failed");
    digest
}

/// Create a configuration for a concurrent pipeline with pruning configured to retain `retention`
/// checkpoints.
fn concurrent_pipeline(retention: u64) -> ConcurrentLayer {
    ConcurrentLayer {
        pruner: Some(PrunerLayer {
            retention: Some(retention),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Request gas from the "faucet" in `cluster`, and craft a transaction transferring 1 MIST from
/// `sender` (signed for with `signer`) to `recipient`, and returns the digest of the transaction as
/// long as it succeeded.
fn transfer_dust(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    signer: &dyn Signer<Signature>,
    recipient: SuiAddress,
) -> TransactionDigest {
    let fx = cluster
        .request_gas(sender, DEFAULT_GAS_BUDGET + 1)
        .expect("Failed to request gas");

    let gas = address_owned(&fx).expect("Failed to find gas object");

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(recipient, Some(1));

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    let digest = data.digest();
    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![signer]))
        .expect("Failed to execute transaction");

    assert!(fx.status().is_ok());
    digest
}

async fn execute_graphql_query(
    cluster: &FullCluster,
    query: &str,
    variables: Option<Value>,
) -> serde_json::Value {
    Client::new()
        .post(cluster.graphql_url().as_str())
        .json(&json!({
            "query": query,
            "variables": variables.unwrap_or_default()
        }))
        .send()
        .await
        .expect("Failed to send request")
        .json()
        .await
        .expect("Failed to parse response")
}

async fn query_available_range(
    cluster: &FullCluster,
    field: &str,
    filters: Option<&[&str]>,
) -> Value {
    let filters = filters
        .unwrap_or_default()
        .iter()
        .map(|&s| s.to_string())
        .collect::<Vec<_>>();
    execute_graphql_query(
        cluster,
        AVAILABLE_RANGE_QUERY,
        Some(json!({ "type": "Query", "field": field, "filters": filters })),
    )
    .await
}

async fn query_transactions(cluster: &FullCluster, affected_address: SuiAddress) -> Value {
    execute_graphql_query(
        cluster,
        TRANSACTIONS_QUERY,
        Some(json!({
            "filter": { "affectedAddress": affected_address.to_string() },
            "first": 50
        })),
    )
    .await
}

async fn query_events(cluster: &FullCluster, filter: Value) -> Value {
    execute_graphql_query(
        cluster,
        EVENTS_QUERY,
        Some(json!({
            "filter": filter,
            "first": 50
        })),
    )
    .await
}
