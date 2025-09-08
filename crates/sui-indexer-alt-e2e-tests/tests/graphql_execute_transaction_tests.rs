// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use prometheus::Registry;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use sui_indexer_alt_graphql::{
    config::RpcConfig as GraphQlConfig, start_rpc as start_graphql, RpcArgs as GraphQlArgs,
};
use sui_indexer_alt_reader::{
    bigtable_reader::BigtableArgs, consistent_reader::ConsistentReaderArgs,
    fullnode_client::FullnodeArgs, system_package_task::SystemPackageTaskArgs,
};
use sui_macros::sim_test;
use sui_pg_db::{temp::get_available_port, DbArgs};
use sui_test_transaction_builder::make_transfer_sui_transaction;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use url::Url;

use sui_types::{
    base_types::SuiAddress, programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::ObjectArg,
};
use test_cluster::{TestCluster, TestClusterBuilder};

// Unified struct for all GraphQL transaction effects parsing
#[derive(Debug, Deserialize)]
struct TransactionEffects {
    status: String,
    checkpoint: Option<serde_json::Value>,
    transaction: Option<TransactionDetails>,
    events: Option<Events>,
    #[serde(rename = "unchangedConsensusObjects")]
    unchanged_consensus_objects: Option<UnchangedConsensusObjects>,
}

#[derive(Debug, Deserialize)]
struct TransactionDetails {
    sender: Sender,
    #[serde(rename = "gasInput")]
    gas_input: GasInput,
    signatures: Vec<Signature>,
}

#[derive(Debug, Deserialize)]
struct Sender {
    address: String,
}

#[derive(Debug, Deserialize)]
struct GasInput {
    #[serde(rename = "gasBudget")]
    gas_budget: String,
}

#[derive(Debug, Deserialize)]
struct Signature {
    #[serde(rename = "signatureBytes")]
    signature_bytes: String,
}

#[derive(Debug, Deserialize)]
struct Events {
    nodes: Vec<EventNode>,
}

#[derive(Debug, Deserialize)]
struct EventNode {
    #[serde(rename = "eventBcs")]
    event_bcs: String,
    sender: Sender,
}

#[derive(Debug, Deserialize)]
struct UnchangedConsensusObjects {
    edges: Vec<UnchangedConsensusObjectEdge>,
}

#[derive(Debug, Deserialize)]
struct UnchangedConsensusObjectEdge {
    node: UnchangedConsensusObjectNode,
}

#[derive(Debug, Deserialize)]
struct UnchangedConsensusObjectNode {
    #[serde(rename = "__typename")]
    typename: String,
    object: Option<ConsensusObject>,
}

#[derive(Debug, Deserialize)]
struct ConsensusObject {
    address: String,
    version: u64,
}

struct GraphQlTestCluster {
    url: Url,
    handle: JoinHandle<()>,
    cancel: CancellationToken,
}

impl GraphQlTestCluster {
    /// Execute a GraphQL mutation or query
    async fn execute_graphql(&self, query: &str, variables: Value) -> anyhow::Result<Value> {
        let request_body = json!({
            "query": query,
            "variables": variables
        });

        let client = Client::new();
        let response = client
            .post(self.url.clone())
            .json(&request_body)
            .send()
            .await
            .context("GraphQL request failed")?;

        let body: Value = response
            .json()
            .await
            .context("Failed to parse GraphQL response")?;

        Ok(body)
    }

    async fn stopped(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
    }
}

async fn create_graphql_test_cluster(validator_cluster: &TestCluster) -> GraphQlTestCluster {
    let graphql_port = get_available_port();
    let graphql_listen_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), graphql_port);

    let graphql_args = GraphQlArgs {
        rpc_listen_address: graphql_listen_address,
        no_ide: true,
    };

    let fullnode_args = FullnodeArgs {
        fullnode_rpc_url: Some(validator_cluster.rpc_url().to_string()),
    };

    let cancel = CancellationToken::new();

    // Start GraphQL server that connects directly to TestCluster's RPC
    let graphql_handle = start_graphql(
        None, // No database - GraphQL will use fullnode RPC for executeTransaction
        None, // No bigtable
        fullnode_args,
        DbArgs::default(),
        BigtableArgs::default(),
        ConsistentReaderArgs::default(),
        graphql_args,
        SystemPackageTaskArgs::default(),
        "0.0.0",
        GraphQlConfig::default(),
        vec![], // No pipelines since we're not using database
        &Registry::new(),
        cancel.child_token(),
    )
    .await
    .expect("Failed to start GraphQL server");

    let url = Url::parse(&format!("http://{}/graphql", graphql_listen_address))
        .expect("Failed to parse GraphQL URL");

    GraphQlTestCluster {
        url,
        handle: graphql_handle,
        cancel,
    }
}

#[sim_test]
async fn test_execute_transaction_mutation_schema() {
    let validator_cluster = TestClusterBuilder::new()
        .with_num_validators(1) // Reduce resource usage for CI
        .build()
        .await;

    let graphql_cluster = create_graphql_test_cluster(&validator_cluster).await;

    // Create a simple transfer transaction for testing
    let recipient = SuiAddress::random_for_testing_only();
    let signed_tx =
        make_transfer_sui_transaction(&validator_cluster.wallet, Some(recipient), Some(1_000_000))
            .await;
    let (tx_bytes, signatures) = signed_tx.to_tx_bytes_and_signatures();

    let result = graphql_cluster
        .execute_graphql(
            r#"
            mutation($txData: Base64!, $sigs: [Base64!]!) {
                executeTransaction(transactionDataBcs: $txData, signatures: $sigs) {
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
            json!({
                "txData": tx_bytes.encoded(),
                "sigs": signatures.iter().map(|s| s.encoded()).collect::<Vec<_>>()
            }),
        )
        .await
        .expect("GraphQL request failed");
    let effects: TransactionEffects = serde_json::from_value(
        result
            .pointer("/data/executeTransaction/effects")
            .unwrap()
            .clone(),
    )
    .unwrap();
    let errors = result.pointer("/data/executeTransaction/errors");

    assert_eq!(effects.status, "SUCCESS");
    assert_eq!(effects.checkpoint, None); // ExecutedTransaction has no checkpoint yet
    assert_eq!(errors, Some(&serde_json::Value::Null));

    // Verify transaction data matches original
    let transaction = effects.transaction.unwrap();
    assert_eq!(
        transaction.sender.address,
        validator_cluster.get_address_0().to_string()
    );
    assert_eq!(transaction.gas_input.gas_budget, "10000000");
    assert_eq!(transaction.signatures.len(), signatures.len());
    for (returned, original) in transaction.signatures.iter().zip(signatures.iter()) {
        assert_eq!(returned.signature_bytes, original.encoded());
    }

    graphql_cluster.stopped().await;
}

#[sim_test]
async fn test_execute_transaction_input_validation() {
    let validator_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let graphql_cluster = create_graphql_test_cluster(&validator_cluster).await;

    // Test invalid Base64 transaction data
    let result = graphql_cluster
        .execute_graphql(
            r#"
            mutation($txData: Base64!, $sigs: [Base64!]!) {
                executeTransaction(transactionDataBcs: $txData, signatures: $sigs) {
                    effects { digest }
                    errors
                }
            }
        "#,
            json!({
                "txData": "invalid_base64!",
                "sigs": ["invalidSignature"]
            }),
        )
        .await
        .unwrap();

    assert!(result.get("errors").is_some());

    graphql_cluster.stopped().await;
}

#[sim_test]
async fn test_execute_transaction_with_events() {
    let validator_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
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

    let result = graphql_cluster
        .execute_graphql(
            r#"
            mutation($txData: Base64!, $sigs: [Base64!]!) {
                executeTransaction(transactionDataBcs: $txData, signatures: $sigs) {
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
            json!({
                "txData": tx_bytes.encoded(),
                "sigs": signatures.iter().map(|s| s.encoded()).collect::<Vec<_>>()
            }),
        )
        .await
        .expect("GraphQL request failed");
    let effects: TransactionEffects = serde_json::from_value(
        result
            .pointer("/data/executeTransaction/effects")
            .unwrap()
            .clone(),
    )
    .unwrap();
    let errors = result.pointer("/data/executeTransaction/errors");

    assert_eq!(effects.status, "SUCCESS");
    assert_eq!(errors, Some(&serde_json::Value::Null));

    let events = effects.events.unwrap();
    assert!(
        !events.nodes.is_empty(),
        "Package publish should emit events from init function"
    );

    let sender_address = validator_cluster.get_address_0();
    for event_node in &events.nodes {
        assert!(
            !event_node.event_bcs.is_empty(),
            "Event should have eventBcs"
        );
        assert_eq!(
            event_node.sender.address,
            sender_address.to_string(),
            "Event sender should match transaction sender"
        );
    }

    graphql_cluster.stopped().await;
}

#[sim_test]
async fn test_execute_transaction_grpc_errors() {
    let validator_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
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
    let result = graphql_cluster
        .execute_graphql(
            r#"
            mutation($txData: Base64!, $sigs: [Base64!]!) {
                executeTransaction(transactionDataBcs: $txData, signatures: $sigs) {
                    effects {
                        digest
                        status
                    }
                    errors
                }
            }
        "#,
            json!({
                "txData": tx1_bytes.encoded(),
                "sigs": tx2_signatures.iter().map(|s| s.encoded()).collect::<Vec<_>>()
            }),
        )
        .await
        .expect("GraphQL request failed");
    let effects = result.pointer("/data/executeTransaction/effects");
    let errors = result.pointer("/data/executeTransaction/errors").unwrap();

    assert_eq!(
        effects,
        Some(&serde_json::Value::Null),
        "Should have null effects on gRPC error"
    );
    let error_array = errors.as_array().unwrap();
    assert!(!error_array.is_empty());

    graphql_cluster.stopped().await;
}

#[sim_test]
async fn test_execute_transaction_unchanged_consensus_objects() {
    let validator_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let graphql_cluster = create_graphql_test_cluster(&validator_cluster).await;

    // Create a read-only transaction that accesses the Clock object
    let mut ptb = ProgrammableTransactionBuilder::new();
    let clock_arg = ptb
        .obj(ObjectArg::SharedObject {
            id: sui_types::SUI_CLOCK_OBJECT_ID,
            initial_shared_version: sui_types::base_types::SequenceNumber::from_u64(1),
            mutable: false,
        })
        .unwrap();

    ptb.programmable_move_call(
        sui_types::SUI_FRAMEWORK_PACKAGE_ID,
        "clock".parse().unwrap(),
        "timestamp_ms".parse().unwrap(),
        vec![],
        vec![clock_arg],
    );

    let tx_data = validator_cluster
        .test_transaction_builder()
        .await
        .programmable(ptb.finish())
        .build();
    let signed_tx = validator_cluster.sign_transaction(&tx_data).await;
    let (tx_bytes, signatures) = signed_tx.to_tx_bytes_and_signatures();

    let result = graphql_cluster
        .execute_graphql(
            r#"
            mutation($txData: Base64!, $sigs: [Base64!]!) {
                executeTransaction(transactionDataBcs: $txData, signatures: $sigs) {
                    effects {
                        digest
                        status
                        unchangedConsensusObjects {
                            edges {
                                node {
                                    __typename
                                    ... on ConsensusObjectRead {
                                        object {
                                            address
                                            version
                                        }
                                    }
                                }
                            }
                        }
                    }
                    errors
                }
            }
        "#,
            json!({
                "txData": tx_bytes.encoded(),
                "sigs": signatures.iter().map(|s| s.encoded()).collect::<Vec<_>>()
            }),
        )
        .await
        .expect("GraphQL request failed");

    let effects: TransactionEffects = serde_json::from_value(
        result
            .pointer("/data/executeTransaction/effects")
            .unwrap()
            .clone(),
    )
    .unwrap();
    let errors = result.pointer("/data/executeTransaction/errors");

    // Verify the transaction succeeded
    assert_eq!(effects.status, "SUCCESS");
    assert_eq!(errors, Some(&serde_json::Value::Null));

    // Verify unchanged consensus objects are returned
    let consensus_objects = effects.unchanged_consensus_objects.unwrap();
    assert!(
        !consensus_objects.edges.is_empty(),
        "Clock read should create unchanged consensus objects"
    );

    // Verify the first edge is a ConsensusObjectRead with correct data
    let first_edge = &consensus_objects.edges[0];
    assert_eq!(first_edge.node.typename, "ConsensusObjectRead");

    let object = first_edge.node.object.as_ref().unwrap();
    assert_eq!(object.address, sui_types::SUI_CLOCK_OBJECT_ID.to_string());
    assert!(object.version > 0, "Version should be greater than 0");

    graphql_cluster.stopped().await;
}
