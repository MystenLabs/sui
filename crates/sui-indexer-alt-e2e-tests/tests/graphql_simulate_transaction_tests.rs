// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use prometheus::Registry;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use sui_indexer_alt::{config::IndexerConfig, setup_indexer};
use sui_indexer_alt_framework::{
    IndexerArgs,
    ingestion::{ClientArgs, ingestion_client::IngestionClientArgs},
};
use sui_indexer_alt_graphql::{
    RpcArgs as GraphQlArgs, args::KvArgs as GraphQlKvArgs, config::RpcConfig as GraphQlConfig,
    start_rpc as start_graphql,
};
use sui_indexer_alt_reader::{
    consistent_reader::ConsistentReaderArgs, fullnode_client::FullnodeArgs,
    system_package_task::SystemPackageTaskArgs,
};
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_pg_db::{
    DbArgs,
    temp::{TempDb, get_available_port},
};
use sui_test_transaction_builder::make_transfer_sui_transaction;
use sui_types::gas_coin::GasCoin;

use sui_futures::service::Service;
use url::Url;

use sui_types::base_types::SuiAddress;
use test_cluster::{TestCluster, TestClusterBuilder};

// Structs for parsing command results
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandResult {
    return_values: Option<Vec<CommandOutput>>,
    mutated_references: Option<Vec<CommandOutput>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandOutput {
    argument: Option<TransactionArgument>,
    value: Option<MoveValue>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveValue {
    #[serde(rename = "type")]
    type_: MoveType,
    bcs: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveType {
    repr: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransactionArgument {
    #[serde(flatten)]
    kind: ArgumentKind,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
#[allow(dead_code)]
enum ArgumentKind {
    TxResult { cmd: Option<u16>, ix: Option<u16> },
    Input { ix: Option<u16> },
    GasCoin {},
}

// Struct for parsing SimulationResult from GraphQL response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SimulationResult {
    effects: Option<TransactionEffects>,
    outputs: Option<Vec<CommandResult>>,
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

#[derive(Debug, Deserialize)]
struct FieldLayout {
    name: String,
    layout: Value,
}

struct GraphQlTestCluster {
    url: Url,
    /// Hold on to the service so it doesn't get dropped (and therefore aborted) until the cluster
    /// goes out of scope.
    #[allow(unused)]
    service: Service,
    /// Hold on to the database so it doesn't get dropped until the cluster is stopped.
    #[allow(unused)]
    database: TempDb,
}

impl GraphQlTestCluster {
    async fn new(validator_cluster: &TestCluster) -> Self {
        let graphql_port = get_available_port();
        let graphql_listen_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), graphql_port);

        let database = TempDb::new().expect("Failed to create temp database");
        let database_url = database.database().url().clone();

        let fullnode_args = FullnodeArgs {
            fullnode_rpc_url: Some(validator_cluster.rpc_url().to_string()),
        };
        let client_args = ClientArgs {
            ingestion: IngestionClientArgs {
                rpc_api_url: Some(
                    Url::parse(validator_cluster.rpc_url()).expect("Invalid RPC URL"),
                ),
                ..Default::default()
            },
            ..Default::default()
        };

        let indexer = setup_indexer(
            database_url.clone(),
            DbArgs::default(),
            IndexerArgs::default(),
            client_args,
            IndexerConfig::for_test(),
            None,
            &Registry::new(),
        )
        .await
        .expect("Failed to setup indexer");

        let pipelines: Vec<String> = indexer.pipelines().map(|s| s.to_string()).collect();
        let s_indexer = indexer.run().await.expect("Failed to start indexer");

        let s_graphql = start_graphql(
            Some(database_url),
            fullnode_args,
            DbArgs::default(),
            GraphQlKvArgs::default(),
            ConsistentReaderArgs::default(),
            GraphQlArgs {
                rpc_listen_address: graphql_listen_address,
                no_ide: true,
            },
            SystemPackageTaskArgs::default(),
            "0.0.0",
            GraphQlConfig::default(),
            pipelines,
            &Registry::new(),
        )
        .await
        .expect("Failed to start GraphQL server");

        let url = Url::parse(&format!("http://{}/graphql", graphql_listen_address))
            .expect("Failed to parse GraphQL URL");

        Self {
            url,
            service: s_graphql.merge(s_indexer),
            database,
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
}

#[tokio::test]
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
            query($txData: JSON!) {
                simulateTransaction(transaction: $txData) {
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
                "txData": {
                    "bcs": {
                        "value": tx_bytes.encoded()
                    }
                }
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
    assert_eq!(transaction.gas_input.gas_budget, "5000000000");

    // For simulation, signatures should be empty since we don't provide them
    assert_eq!(transaction.signatures.len(), 0);
}

#[tokio::test]
async fn test_simulate_transaction_with_events() {
    let validator_cluster = TestClusterBuilder::new().build().await;
    let graphql_cluster = GraphQlTestCluster::new(&validator_cluster).await;

    // Publish our test package which emits events in its init function
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/emit_event");
    let tx_data = validator_cluster
        .test_transaction_builder()
        .await
        .publish_async(path)
        .await
        .build();
    let signed_tx = validator_cluster.sign_transaction(&tx_data).await;
    let (tx_bytes, _signatures) = signed_tx.to_tx_bytes_and_signatures();

    let result = graphql_cluster
        .execute_graphql(
            r#"
            query($txData: JSON!) {
                simulateTransaction(transaction: $txData) {
                    effects {
                        status
                        events {
                            nodes {
                                timestamp
                                contents {
                                    json
                                }
                                transactionModule {
                                    package {
                                        version
                                        modules {
                                            nodes {
                                                name
                                            }
                                        }
                                    }
                                    name
                                }
                            }
                        }
                    }
                    error
                }
            }
        "#,
            json!({
                "txData": {
                    "bcs": {
                        "value": tx_bytes.encoded()
                    }
                }
            }),
        )
        .await
        .expect("GraphQL request failed");

    // Verify package version and digest are populated correctly from execution context
    insta::assert_json_snapshot!(result.pointer("/data/simulateTransaction"), @r#"
    {
      "effects": {
        "status": "SUCCESS",
        "events": {
          "nodes": [
            {
              "timestamp": null,
              "contents": {
                "json": {
                  "message": "Package published successfully!",
                  "value": "42"
                }
              },
              "transactionModule": {
                "package": {
                  "version": 1,
                  "modules": {
                    "nodes": [
                      {
                        "name": "emit_event"
                      }
                    ]
                  }
                },
                "name": "emit_event"
              }
            }
          ]
        }
      },
      "error": null
    }
    "#);
}

#[tokio::test]
async fn test_simulate_transaction_input_validation() {
    let validator_cluster = TestClusterBuilder::new().build().await;
    let graphql_cluster = GraphQlTestCluster::new(&validator_cluster).await;

    // Test invalid Base64 transaction data
    let result = graphql_cluster
        .execute_graphql(
            r#"
            query($txData: JSON!) {
                simulateTransaction(transaction: $txData) {
                    effects { digest }
                    error
                }
            }
        "#,
            json!({
                "txData": {
                    "bcs": {
                        "value": "invalid_base64!"
                    }
                }
            }),
        )
        .await
        .unwrap();

    // Should return GraphQL errors for invalid input
    assert!(result.get("errors").is_some());
}

#[tokio::test]
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
            query($txData: JSON!) {
                simulateTransaction(transaction: $txData) {
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
                "txData": {
                    "bcs": {
                        "value": tx_bytes.encoded()
                    }
                }
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
}

#[tokio::test]
async fn test_simulate_transaction_command_results() {
    let validator_cluster = TestClusterBuilder::new().build().await;
    let graphql_cluster = GraphQlTestCluster::new(&validator_cluster).await;

    // First, publish the command_results package
    let package_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/command_results");
    let publish_tx = validator_cluster
        .test_transaction_builder()
        .await
        .publish_async(package_path)
        .await
        .build();
    let signed_tx = validator_cluster.sign_transaction(&publish_tx).await;
    let publish_result = validator_cluster.execute_transaction(signed_tx).await;

    // Find the published package ID from created objects
    let package_id = publish_result
        .effects
        .unwrap()
        .created()
        .iter()
        .find(|obj| obj.owner.is_immutable())
        .unwrap()
        .reference
        .object_id;

    // Now create a programmable transaction that calls our Move functions exactly like move_call.move:
    // Command 0: create_test_object(Input(42)) -> TestObject
    // Command 1: get_object_value(Result(0)) -> u64 (should return 42)
    // Command 2: check_gas_coin(Gas) -> u64 (gas coin value)
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::transaction::{Argument, CallArg, Command};

    let mut ptb = ProgrammableTransactionBuilder::new();

    // Input 0: Pure value 42
    ptb.input(CallArg::Pure(bcs::to_bytes(&42u64).unwrap()))
        .unwrap();

    // Input 1: Pure value 100 (for mutation)
    ptb.input(CallArg::Pure(bcs::to_bytes(&100u64).unwrap()))
        .unwrap();

    // Command 0: create_test_object(Input(42))
    ptb.command(Command::move_call(
        package_id,
        move_core_types::ident_str!("test_commands").to_owned(),
        move_core_types::ident_str!("create_test_object").to_owned(),
        vec![],
        vec![Argument::Input(0)], // Input(42)
    ));

    // Command 1: update_object_value(&mut Result(0), Input(100)) - MUTATES the object!
    ptb.command(Command::move_call(
        package_id,
        move_core_types::ident_str!("test_commands").to_owned(),
        move_core_types::ident_str!("update_object_value").to_owned(),
        vec![],
        vec![Argument::Result(0), Argument::Input(1)], // Mutate Result(0) with Input(100)
    ));

    // Command 2: get_object_value(&Result(0)) - should now return 100 after mutation
    ptb.command(Command::move_call(
        package_id,
        move_core_types::ident_str!("test_commands").to_owned(),
        move_core_types::ident_str!("get_object_value").to_owned(),
        vec![],
        vec![Argument::Result(0)], // Result(0) - the mutated TestObject
    ));

    // Command 3: check_gas_coin(&Gas) - takes gas coin by reference
    ptb.command(Command::move_call(
        package_id,
        move_core_types::ident_str!("test_commands").to_owned(),
        move_core_types::ident_str!("check_gas_coin").to_owned(),
        vec![],
        vec![Argument::GasCoin], // GasCoin
    ));

    // Build the programmable transaction
    let pt = ptb.finish();
    let gas_objects = validator_cluster
        .wallet
        .get_gas_objects_owned_by_address(validator_cluster.get_address_0(), None)
        .await
        .unwrap();
    let tx_data = sui_types::transaction::TransactionData::new_programmable(
        validator_cluster.get_address_0(),
        vec![gas_objects[0]],
        pt,
        10_000_000, // gas budget
        validator_cluster.get_reference_gas_price().await,
    );

    let signed_tx = validator_cluster.sign_transaction(&tx_data).await;
    let (tx_bytes, _) = signed_tx.to_tx_bytes_and_signatures();

    let result = graphql_cluster
        .execute_graphql(
            r#"
            query($txData: JSON!) {
                simulateTransaction(transaction: $txData) {
                    effects { status }
                    outputs {
                        returnValues {
                            argument {
                                ... on Input { ix }
                                ... on TxResult { cmd ix }
                                ... on GasCoin { _ }
                            }
                            value {
                                type { repr }
                                bcs
                            }
                        }
                        mutatedReferences {
                            argument {
                                ... on Input { ix }
                                ... on TxResult { cmd ix }
                                ... on GasCoin { _ }
                            }
                            value {
                                type { repr }
                                bcs
                            }
                        }
                    }
                }
            }
        "#,
            json!({
                "txData": {
                    "bcs": {
                        "value": tx_bytes.encoded()
                    }
                }
            }),
        )
        .await
        .unwrap();

    let simulation: SimulationResult =
        serde_json::from_value(result.pointer("/data/simulateTransaction").unwrap().clone())
            .unwrap();

    assert_eq!(simulation.effects.as_ref().unwrap().status, "SUCCESS");

    let results = simulation.outputs.expect("outputs field should be present");
    assert_eq!(results.len(), 4, "Should have exactly 4 commands");

    // Verify command result structure with specific expectations per command
    for (i, cmd) in results.iter().enumerate() {
        match i {
            0 => {
                // Command 0: create_test_object(Input(42)) -> TestObject
                let returns = cmd.return_values.as_ref().unwrap();
                assert_eq!(returns.len(), 1);
                let value = returns[0].value.as_ref().unwrap();
                assert!(value.type_.repr.contains("TestObject"));
                assert!(!value.bcs.is_empty());
                assert!(cmd.mutated_references.as_ref().unwrap().is_empty());
            }
            1 => {
                // Command 1: update_object_value(&mut Result(0), Input(100))
                let mutated = cmd.mutated_references.as_ref().unwrap();
                assert_eq!(mutated.len(), 1);
                let mutated_ref = &mutated[0];

                let ArgumentKind::TxResult { cmd, ix } =
                    &mutated_ref.argument.as_ref().unwrap().kind
                else {
                    panic!("Expected TxResult argument");
                };
                assert_eq!(*cmd, Some(0));
                assert_eq!(*ix, Some(0));

                let value = mutated_ref.value.as_ref().unwrap();
                assert!(value.type_.repr.contains("TestObject"));
                assert!(!value.bcs.is_empty());
            }
            2 => {
                // Command 2: get_object_value(&Result(0)) -> u64 (should return 100 after mutation)
                let returns = cmd.return_values.as_ref().unwrap();
                assert_eq!(returns.len(), 1);

                let value = returns[0].value.as_ref().unwrap();
                assert_eq!(value.type_.repr, "u64");
                assert!(!value.bcs.is_empty());
                assert!(returns[0].argument.is_none());
                assert!(cmd.mutated_references.as_ref().unwrap().is_empty());
            }
            3 => {
                // Command 3: check_gas_coin(&Gas) -> u64
                let returns = cmd.return_values.as_ref().unwrap();
                assert_eq!(returns.len(), 1);

                let value = returns[0].value.as_ref().unwrap();
                assert_eq!(value.type_.repr, "u64");
                assert!(!value.bcs.is_empty());
                assert!(returns[0].argument.is_none());
                assert!(cmd.mutated_references.as_ref().unwrap().is_empty());
            }
            _ => panic!("Unexpected command index: {}", i),
        }
    }
}

#[tokio::test]
async fn test_simulate_transaction_json_transfer() {
    let validator_cluster = TestClusterBuilder::new().build().await;
    let graphql_cluster = GraphQlTestCluster::new(&validator_cluster).await;

    let sender = validator_cluster.get_address_0();
    let recipient = SuiAddress::random_for_testing_only();

    // Create a JSON Transaction following the proto schema for a simple transfer
    let tx_json = json!({
        "sender": sender.to_string(),
         "gas_payment": {
            "budget": 3000000,
            "owner": sender.to_string()
        },
        "kind": {
            "programmable_transaction": {
                "inputs": [
                    {
                        "literal": 1000000  // Amount to transfer as a number literal
                    },
                    {
                        "literal": recipient.to_string()  // Recipient address as string literal
                    }
                ],
                "commands": [
                    {
                        "split_coins": {
                            "coin": {
                                "kind": "GAS"
                            },
                            "amounts": [
                                {
                                    "kind": "INPUT",
                                    "input": 0
                                }
                            ]
                        }
                    },
                    {
                        "transfer_objects": {
                            "objects": [
                                {
                                    "kind": "RESULT",
                                    "result": 0,
                                    "subresult": 0
                                }
                            ],
                            "address": {
                                "kind": "INPUT",
                                "input": 1
                            }
                        }
                    }
                ]
            }
        }
    });

    let result = graphql_cluster
        .execute_graphql(
            r#"
            query($txJson: JSON!) {
                simulateTransaction(transaction: $txJson) {
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
                "txJson": tx_json
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
    assert_eq!(transaction.gas_input.gas_budget, "3000000");

    // For simulation, signatures should be empty since we don't provide them
    assert_eq!(transaction.signatures.len(), 0);
}

#[tokio::test]
async fn test_package_resolver_finds_newly_published_package() {
    let validator_cluster = TestClusterBuilder::new().build().await;
    let graphql_cluster = GraphQlTestCluster::new(&validator_cluster).await;

    // Publish the test package
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["packages", "package_resolver_test"]);
    let tx_data = validator_cluster
        .test_transaction_builder()
        .await
        .publish(path)
        .build();
    let signed_tx = validator_cluster.sign_transaction(&tx_data).await;
    let (tx_bytes, _signatures) = signed_tx.to_tx_bytes_and_signatures();

    // Execute the publish transaction and query for type LAYOUT
    // The layout query will trigger PackageResolver::fetch() for the new package
    let result = graphql_cluster
        .execute_graphql(
            r#"
            query($txData: JSON!) {
                simulateTransaction(transaction: $txData) {
                    effects {
                        status
                        objectChanges {
                            nodes {
                                outputState {
                                    address
                                    asMoveObject {
                                        contents {
                                            type {
                                                repr
                                                layout
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        "#,
            json!({
                "txData": {
                    "bcs": {
                        "value": tx_bytes.encoded()
                    }
                }
            }),
        )
        .await
        .expect("GraphQL request failed");

    // Find the SimpleObject created by the package's init function
    let object_changes = result
        .pointer("/data/simulateTransaction/effects/objectChanges/nodes")
        .unwrap()
        .as_array()
        .unwrap();

    // Look for the SimpleObject - its type resolution requires fetching the newly published package
    let simple_object = object_changes
        .iter()
        .find(|node| {
            node.pointer("/outputState/asMoveObject/contents/type/repr")
                .and_then(|t| t.as_str())
                .map(|type_str| type_str.contains("::resolver_test::SimpleObject"))
                .unwrap_or(false)
        })
        .unwrap();
    let fields: Vec<FieldLayout> = serde_json::from_value(
        simple_object
            .pointer("/outputState/asMoveObject/contents/type/layout/struct/fields")
            .unwrap()
            .clone(),
    )
    .unwrap();

    assert_eq!(
        fields.len(),
        2,
        "SimpleObject should have 2 fields (id and value)"
    );

    // Verify the 'id' field is of type UID
    assert_eq!(fields[0].name, "id");
    assert_eq!(
        fields[0]
            .layout
            .pointer("/struct/type")
            .unwrap()
            .as_str()
            .unwrap(),
        "0x0000000000000000000000000000000000000000000000000000000000000002::object::UID"
    );

    // Verify the 'value' field is of type NestedObject
    assert_eq!(fields[1].name, "value");
    assert!(
        fields[1]
            .layout
            .pointer("/struct/type")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("::resolver_test::NestedObject")
    );
}

#[tokio::test]
async fn test_simulate_transaction_balance_changes() {
    let validator_cluster = TestClusterBuilder::new().build().await;
    let graphql_cluster = GraphQlTestCluster::new(&validator_cluster).await;

    // Create a transfer transaction that will cause balance changes
    let recipient = SuiAddress::random_for_testing_only();
    let transfer_amount = 1_000_000u64;

    let signed_tx = make_transfer_sui_transaction(
        &validator_cluster.wallet,
        Some(recipient),
        Some(transfer_amount),
    )
    .await;
    let (tx_bytes, _signatures) = signed_tx.to_tx_bytes_and_signatures();

    let result = graphql_cluster
        .execute_graphql(
            r#"
            query($txData: JSON!) {
                simulateTransaction(transaction: $txData) {
                    effects {
                        status
                        balanceChanges {
                            nodes {
                                coinType {
                                    repr
                                }
                                amount
                            }
                        }
                    }
                    error
                }
            }
        "#,
            json!({
                "txData": {
                    "bcs": {
                        "value": tx_bytes.encoded()
                    }
                }
            }),
        )
        .await
        .expect("GraphQL request failed");

    // Verify balance changes are populated from execution context
    let mut balance_changes: Vec<_> = result
        .pointer("/data/simulateTransaction/effects/balanceChanges/nodes")
        .expect("balanceChanges should be present")
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v["coinType"]["repr"].as_str().unwrap(),
                v["amount"].as_str().unwrap(),
            )
        })
        .collect();

    // Sort for deterministic ordering (order depends on address which varies between runs)
    balance_changes.sort();

    // Should have balance changes for both sender and recipient
    assert_eq!(balance_changes.len(), 2, "Should have 2 balance changes");

    // Verify structure matches expected format
    assert_eq!(
        balance_changes,
        vec![
            (
                "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI",
                "-3976000"
            ),
            (
                "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI",
                "1000000"
            ),
        ]
    );
}
