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
use sui_types::gas_coin::GasCoin;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use url::Url;

use sui_types::base_types::SuiAddress;
use test_cluster::{TestCluster, TestClusterBuilder};

// Struct for parsing SimulationResult from GraphQL response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SimulationResult {
    effects: Option<TransactionEffects>,
    events: Option<Events>,
    error: Option<String>,
}

// Reuse TransactionEffects from execute_transaction tests
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransactionEffects {
    status: String,
    transaction: Option<TransactionDetails>,
}

#[derive(Debug, Deserialize)]
struct TransactionDetails {
    sender: Sender,
    #[serde(rename = "gasInput")]
    gas_input: GasInput,
    signatures: Vec<Value>,
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

// Events is now a Vec<EventNode> directly, not wrapped in nodes
type Events = Vec<EventNode>;

#[derive(Debug, Deserialize)]
struct EventNode {
    #[serde(rename = "eventBcs")]
    event_bcs: String,
    sender: Sender,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ObjectChangeNode {
    id_created: bool,
    id_deleted: bool,
    input_state: Option<ObjectState>,
    output_state: Option<ObjectState>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ObjectState {
    version: u64,
    as_move_object: Option<Value>,
}

struct GraphQlTestCluster {
    url: Url,
    handle: JoinHandle<()>,
    cancel: CancellationToken,
}

impl GraphQlTestCluster {
    async fn new(validator_cluster: &TestCluster) -> Self {
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
            None, // No database - GraphQL will use fullnode RPC for simulateTransaction
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

        Self {
            url,
            handle: graphql_handle,
            cancel,
        }
    }

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

#[sim_test]
async fn test_simulate_transaction_basic() {
    let validator_cluster = TestClusterBuilder::new().build().await;

    let graphql_cluster = GraphQlTestCluster::new(&validator_cluster).await;

    // Create a simple transfer transaction for simulation (no signatures needed!)
    let recipient = SuiAddress::random_for_testing_only();
    let signed_tx =
        make_transfer_sui_transaction(&validator_cluster.wallet, Some(recipient), Some(1_000_000))
            .await;
    let (tx_bytes, _signatures) = signed_tx.to_tx_bytes_and_signatures();

    let result = graphql_cluster
        .execute_graphql(
            r#"
            query($txData: Base64!) {
                simulateTransaction(transactionDataBcs: $txData) {
                    effects {
                        digest
                        status
                        transaction {
                            sender { address }
                            gasInput { gasBudget }
                            signatures {
                                signatureBytes
                            }
                        }
                    }
                    error
                }
            }
        "#,
            json!({
                "txData": tx_bytes.encoded()
            }),
        )
        .await
        .expect("GraphQL request failed");

    let simulation_result: SimulationResult =
        serde_json::from_value(result.pointer("/data/simulateTransaction").unwrap().clone())
            .unwrap();

    // Verify simulation was successful
    let effects = simulation_result.effects.unwrap();
    assert_eq!(effects.status, "SUCCESS");
    assert!(simulation_result.error.is_none());

    // Verify transaction data matches original
    let transaction = effects.transaction.unwrap();
    assert_eq!(
        transaction.sender.address,
        validator_cluster.get_address_0().to_string()
    );
    assert_eq!(transaction.gas_input.gas_budget, "10000000");

    // For simulation, signatures should be empty since we don't provide them
    assert_eq!(transaction.signatures.len(), 0);

    graphql_cluster.stopped().await;
}

#[sim_test]
async fn test_simulate_transaction_with_events() {
    let validator_cluster = TestClusterBuilder::new().build().await;
    let graphql_cluster = GraphQlTestCluster::new(&validator_cluster).await;

    // Publish our test package which emits events in its init function
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/emit_event");
    let tx_data = validator_cluster
        .test_transaction_builder()
        .await
        .publish(path)
        .build();
    let signed_tx = validator_cluster.sign_transaction(&tx_data).await;
    let (tx_bytes, _signatures) = signed_tx.to_tx_bytes_and_signatures();

    let result = graphql_cluster
        .execute_graphql(
            r#"
            query($txData: Base64!) {
                simulateTransaction(transactionDataBcs: $txData) {
                    effects {
                        digest
                        status
                    }
                    events {
                        eventBcs
                        sender { address }
                    }
                    error
                }
            }
        "#,
            json!({
                "txData": tx_bytes.encoded()
            }),
        )
        .await
        .expect("GraphQL request failed");

    let simulation_result: SimulationResult =
        serde_json::from_value(result.pointer("/data/simulateTransaction").unwrap().clone())
            .unwrap();

    // Verify events were simulated
    let events = simulation_result.events.unwrap();
    assert!(!events.is_empty());

    let sender_address = validator_cluster.get_address_0();
    for event_node in &events {
        assert!(!event_node.event_bcs.is_empty());
        assert_eq!(event_node.sender.address, sender_address.to_string());
    }

    graphql_cluster.stopped().await;
}

#[sim_test]
async fn test_simulate_transaction_input_validation() {
    let validator_cluster = TestClusterBuilder::new().build().await;
    let graphql_cluster = GraphQlTestCluster::new(&validator_cluster).await;

    // Test invalid Base64 transaction data
    let result = graphql_cluster
        .execute_graphql(
            r#"
            query($txData: Base64!) {
                simulateTransaction(transactionDataBcs: $txData) {
                    effects { digest }
                    error
                }
            }
        "#,
            json!({
                "txData": "invalid_base64!"
            }),
        )
        .await
        .unwrap();

    // Should return GraphQL errors for invalid input
    assert!(result.get("errors").is_some());

    graphql_cluster.stopped().await;
}

#[sim_test]
async fn test_simulate_transaction_object_changes() {
    let validator_cluster = TestClusterBuilder::new().build().await;
    let graphql_cluster = GraphQlTestCluster::new(&validator_cluster).await;

    // Create a transfer transaction that will modify objects
    let recipient = SuiAddress::random_for_testing_only();
    let signed_tx =
        make_transfer_sui_transaction(&validator_cluster.wallet, Some(recipient), Some(1_000_000))
            .await;
    let (tx_bytes, _signatures) = signed_tx.to_tx_bytes_and_signatures();

    let result = graphql_cluster
        .execute_graphql(
            r#"
            query($txData: Base64!) {
                simulateTransaction(transactionDataBcs: $txData) {
                    effects {
                        digest
                        status
                        objectChanges {
                            nodes {
                                idCreated
                                idDeleted
                                inputState {
                                    version
                                    asMoveObject {
                                        contents {
                                            type {
                                                repr
                                            }
                                        }
                                    }
                                }
                                outputState {
                                    version
                                    asMoveObject {
                                        contents {
                                            type {
                                                repr
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    error
                }
            }
        "#,
            json!({
                "txData": tx_bytes.encoded()
            }),
        )
        .await
        .expect("GraphQL request failed");

    let simulation_result: SimulationResult =
        serde_json::from_value(result.pointer("/data/simulateTransaction").unwrap().clone())
            .unwrap();

    let effects = simulation_result.effects.unwrap();
    assert_eq!(effects.status, "SUCCESS");

    // Use pointer to navigate to object changes and deserialize directly
    let object_changes_value = result
        .pointer("/data/simulateTransaction/effects/objectChanges/nodes")
        .unwrap();
    let nodes: Vec<ObjectChangeNode> =
        serde_json::from_value(object_changes_value.clone()).unwrap();

    // There should be 2 objects (gas coin + newly created coin)
    assert_eq!(nodes.len(), 2);

    // Filter out the gas object (modified, has both input and output states)
    let gas_coin = nodes
        .iter()
        .find(|node| !node.id_created && !node.id_deleted)
        .unwrap();
    let input_state = gas_coin.input_state.as_ref().unwrap();
    let output_state = gas_coin.output_state.as_ref().unwrap();
    let input_move_obj = input_state.as_move_object.as_ref().unwrap();
    let output_move_obj = output_state.as_move_object.as_ref().unwrap();
    let input_type = input_move_obj
        .pointer("/contents/type/repr")
        .unwrap()
        .as_str()
        .unwrap();
    let output_type = output_move_obj
        .pointer("/contents/type/repr")
        .unwrap()
        .as_str()
        .unwrap();
    let sui_coin_type = GasCoin::type_().to_canonical_string(true);

    // Gas coin versions should be simulated correctly
    assert_eq!(input_state.version, 1);
    assert_eq!(output_state.version, 2);

    // Both should be SUI coins
    assert_eq!(input_type, sui_coin_type);
    assert_eq!(output_type, sui_coin_type);

    // Filter out the newly created coin (created for recipient)
    let created_coin = nodes.iter().find(|node| node.id_created).unwrap();
    let created_output = created_coin.output_state.as_ref().unwrap();

    // Created coin should only have output state
    assert!(created_coin.input_state.is_none());
    assert!(created_coin.output_state.is_some());
    assert_eq!(created_output.version, 2);

    // Created object should be SUI coins
    let created_move_obj = created_output.as_move_object.as_ref().unwrap();
    let created_type = created_move_obj
        .pointer("/contents/type/repr")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(created_type, sui_coin_type);

    graphql_cluster.stopped().await;
}
