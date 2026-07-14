// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Context;
use fastcrypto::encoding::Base64;
use fastcrypto::encoding::Encoding;
use move_core_types::ident_str;
use prometheus::Registry;
use reqwest::Client;
use serde_json::Value;
use serde_json::json;
use sui_futures::service::Service;
use sui_indexer_alt::config::IndexerConfig;
use sui_indexer_alt::setup_indexer;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs;
use sui_indexer_alt_jsonrpc::NodeArgs;
use sui_indexer_alt_jsonrpc::RpcArgs;
use sui_indexer_alt_jsonrpc::args::SystemPackageTaskArgs;
use sui_indexer_alt_jsonrpc::config::RpcConfig;
use sui_indexer_alt_jsonrpc::start_rpc;
use sui_indexer_alt_reader::consistent_reader::ConsistentReaderArgs;
use sui_indexer_alt_reader::kv_loader::KvArgs;
use sui_pg_db::DbArgs;
use sui_pg_db::temp::TempDb;
use sui_pg_db::temp::get_available_port;
use sui_swarm_config::genesis_config::AccountConfig;
use sui_types::MOVE_STDLIB_PACKAGE_ID;
use sui_types::TypeTag;
use sui_types::crypto::ToFromBytes as _;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::signature::GenericSignature;
use sui_types::transaction::SenderSignedData;
use sui_types::transaction::TransactionData;
use sui_types::transaction::TransactionDataAPI;
use sui_types::transaction::TransactionKind;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use url::Url;

struct WriteTestCluster {
    onchain_cluster: TestCluster,
    rpc_url: Url,
    _service: Service,
    _database: TempDb,
    client: Client,
}

impl WriteTestCluster {
    async fn new() -> anyhow::Result<Self> {
        let onchain_cluster = TestClusterBuilder::new()
            .with_num_validators(1)
            .with_epoch_duration_ms(300_000)
            .with_accounts(vec![
                AccountConfig {
                    address: None,
                    gas_amounts: vec![1_000_000_000_000; 2],
                };
                4
            ])
            .build()
            .await;

        let fullnode_grpc_url = onchain_cluster.rpc_url().to_string();

        let database = TempDb::new().context("Failed to create database")?;
        let database_url = database.database().url().clone();

        let client_args = ClientArgs {
            ingestion: IngestionClientArgs {
                rpc_api_url: Some(Url::parse(onchain_cluster.rpc_url()).expect("Invalid RPC URL")),
                ..Default::default()
            },
            ..Default::default()
        };

        let registry = Registry::new();

        let indexer = setup_indexer(
            database_url.clone(),
            DbArgs::default(),
            IndexerArgs::default(),
            client_args,
            IndexerConfig::for_test(),
            None,
            &registry,
        )
        .await
        .context("Failed to setup indexer")?;

        let indexer_service = indexer.run().await.context("Failed to start indexer")?;

        let rpc_listen_address =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), get_available_port());
        let rpc_url = Url::parse(&format!("http://{}/", rpc_listen_address))
            .expect("Failed to parse RPC URL");

        let rpc_service = start_rpc(
            Some(database_url),
            DbArgs::default(),
            KvArgs::default(),
            ConsistentReaderArgs::default(),
            RpcArgs {
                rpc_listen_address,
                ..Default::default()
            },
            NodeArgs {
                fullnode_grpc_url: Some(fullnode_grpc_url),
            },
            SystemPackageTaskArgs::default(),
            RpcConfig::default(),
            &registry,
        )
        .await
        .context("Failed to start JSON-RPC server")?;

        let cluster = Self {
            onchain_cluster,
            rpc_url,
            _service: rpc_service.merge(indexer_service),
            _database: database,
            client: Client::new(),
        };

        // Dev-inspect reads its gas defaults from `kv_epoch_starts` and `kv_protocol_configs`.
        // The latter is only populated once the indexer's pipeline processes the genesis
        // checkpoint, so wait for it before handing the cluster to a test.
        tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                let response = cluster
                    .execute_jsonrpc("sui_getProtocolConfig", json!([]))
                    .await;
                if matches!(&response, Ok(r) if r["error"].is_null()) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        })
        .await
        .context("Timed out waiting for the genesis protocol config to be indexed")?;

        Ok(cluster)
    }

    async fn transfer_transaction(&self) -> anyhow::Result<(String, String, Vec<String>)> {
        let addresses = self.onchain_cluster.wallet.get_addresses();
        let recipient = addresses[1];
        let tx = self
            .onchain_cluster
            .test_transaction_builder()
            .await
            .transfer_sui(Some(1_000), recipient)
            .build();
        let tx_digest = tx.digest().to_string();
        let signed_tx = self.onchain_cluster.wallet.sign_transaction(&tx).await;
        let (tx_bytes, sigs) = signed_tx.to_tx_bytes_and_signatures();
        Ok((
            tx_digest,
            tx_bytes.encoded(),
            sigs.iter().map(|sig| sig.encoded()).collect(),
        ))
    }

    async fn privileged_transaction(&self) -> anyhow::Result<(String, String, Vec<String>)> {
        let tx: sui_types::transaction::TransactionData = self
            .onchain_cluster
            .test_transaction_builder()
            .await
            .call_request_remove_validator()
            .build();
        let tx_digest = tx.digest().to_string();
        let signed_tx = self.onchain_cluster.wallet.sign_transaction(&tx).await;
        let (tx_bytes, sigs) = signed_tx.to_tx_bytes_and_signatures();
        Ok((
            tx_digest,
            tx_bytes.encoded(),
            sigs.iter().map(|sig| sig.encoded()).collect(),
        ))
    }

    async fn execute_jsonrpc(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        self.execute_jsonrpc_at(self.rpc_url.clone(), method, params)
            .await
    }

    /// Send a JSON-RPC request to the fullnode's legacy JSON-RPC server (rather than the
    /// indexer's JSON-RPC server), to compare implementations of the same method.
    async fn execute_fullnode_jsonrpc(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        let url = Url::parse(self.onchain_cluster.rpc_url()).context("Invalid fullnode URL")?;
        self.execute_jsonrpc_at(url, method, params).await
    }

    async fn execute_jsonrpc_at(
        &self,
        url: Url,
        method: &str,
        params: Value,
    ) -> anyhow::Result<Value> {
        let query = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        let response = self
            .client
            .post(url)
            .json(&query)
            .send()
            .await
            .context("Request to JSON-RPC server failed")?;

        response
            .json()
            .await
            .context("Failed to parse JSON-RPC response")
    }
}

/// BCS- and Base64-encode a `TransactionKind`, the input format for
/// `sui_devInspectTransactionBlock`.
fn encode_transaction_kind(kind: &TransactionKind) -> String {
    Base64::encode(bcs::to_bytes(kind).expect("Failed to serialize TransactionKind"))
}

/// Extract a byte vector from a JSON array of numbers.
fn json_bytes(value: &Value) -> Vec<u8> {
    value
        .as_array()
        .expect("Expected a JSON array of bytes")
        .iter()
        .map(|b| b.as_u64().unwrap() as u8)
        .collect()
}

/// A transaction that calls the non-entry function `std::option::none<u64>()` and leaves its
/// result unused -- only valid under dev-inspect.
fn option_none_transaction_kind() -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.programmable_move_call(
        MOVE_STDLIB_PACKAGE_ID,
        ident_str!("option").to_owned(),
        ident_str!("none").to_owned(),
        vec![TypeTag::U64],
        vec![],
    );
    TransactionKind::programmable(builder.finish())
}

/// A transaction that fails execution with an arithmetic error (division by zero).
fn divide_by_zero_transaction_kind() -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    let one = builder.pure(1u64).expect("Failed to create pure input");
    let zero = builder.pure(0u64).expect("Failed to create pure input");
    builder.programmable_move_call(
        MOVE_STDLIB_PACKAGE_ID,
        ident_str!("u64").to_owned(),
        ident_str!("divide_and_round_up").to_owned(),
        vec![],
        vec![one, zero],
    );
    TransactionKind::programmable(builder.finish())
}

#[tokio::test]
async fn test_execute_transfer_correctness() {
    telemetry_subscribers::init_for_testing();
    let cluster = WriteTestCluster::new().await.unwrap();

    let sender = cluster.onchain_cluster.wallet.get_addresses()[0];
    let recipient = cluster.onchain_cluster.wallet.get_addresses()[1];
    let (tx_digest, tx_bytes, sigs) = cluster.transfer_transaction().await.unwrap();

    let response = cluster
        .execute_jsonrpc(
            "sui_executeTransactionBlock",
            json!({
                "tx_bytes": tx_bytes,
                "signatures": sigs,
                "options": {
                    "showInput": true,
                    "showRawInput": true,
                    "showEffects": true,
                    "showRawEffects": true,
                    "showEvents": true,
                    "showObjectChanges": true,
                    "showBalanceChanges": true,
                },
            }),
        )
        .await
        .unwrap();

    let result = &response["result"];
    let sender_str = sender.to_string();
    let recipient_str = recipient.to_string();

    assert_eq!(result["digest"], tx_digest);

    // -- input --
    assert_eq!(result["transaction"]["data"]["sender"], sender_str);
    let tx_kind = &result["transaction"]["data"]["transaction"];
    assert_eq!(tx_kind["kind"].as_str().unwrap(), "ProgrammableTransaction");
    assert!(
        !result["transaction"]["txSignatures"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // -- raw input --
    let raw_transaction = Base64::decode(result["rawTransaction"].as_str().unwrap()).unwrap();
    let actual: SenderSignedData = bcs::from_bytes(&raw_transaction).unwrap();
    let tx_data: TransactionData = bcs::from_bytes(&Base64::decode(&tx_bytes).unwrap()).unwrap();
    let tx_signatures = sigs
        .iter()
        .map(|sig| GenericSignature::from_bytes(&Base64::decode(sig).unwrap()).unwrap())
        .collect();
    assert_eq!(actual, SenderSignedData::new(tx_data, tx_signatures));

    // -- effects --
    let effects = &result["effects"];
    assert_eq!(effects["status"]["status"], "success");
    assert_eq!(effects["transactionDigest"], tx_digest);

    let gas_used = &effects["gasUsed"];
    let computation: u64 = gas_used["computationCost"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let storage: u64 = gas_used["storageCost"].as_str().unwrap().parse().unwrap();
    let rebate: u64 = gas_used["storageRebate"].as_str().unwrap().parse().unwrap();
    let _non_refundable: u64 = gas_used["nonRefundableStorageFee"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(computation > 0);
    assert!(storage > 0);

    // Gas object reference belongs to sender.
    let gas_obj = &effects["gasObject"];
    assert_eq!(
        gas_obj["owner"]["AddressOwner"].as_str().unwrap(),
        sender_str
    );

    // Mutated set contains the gas coin.
    let mutated = effects["mutated"].as_array().unwrap();
    assert!(
        mutated.iter().any(|m| m["owner"]["AddressOwner"]
            .as_str()
            .is_some_and(|a| a == sender_str)),
        "mutated set should contain sender's gas coin"
    );

    // Created set contains the new coin for recipient.
    let created = effects["created"].as_array().unwrap();
    assert!(
        created.iter().any(|c| c["owner"]["AddressOwner"]
            .as_str()
            .is_some_and(|a| a == recipient_str)),
        "created set should contain recipient's new coin"
    );

    // -- raw effects --
    assert!(!result["rawEffects"].as_array().unwrap().is_empty());

    // -- balance changes --
    let balance_changes = result["balanceChanges"].as_array().unwrap();
    assert_eq!(balance_changes.len(), 2);

    let find_balance = |addr: &str| -> (i128, String) {
        let bc = balance_changes
            .iter()
            .find(|bc| {
                bc["owner"]["AddressOwner"]
                    .as_str()
                    .is_some_and(|a| a == addr)
            })
            .unwrap_or_else(|| panic!("balance change not found for {addr}"));
        let amount: i128 = bc["amount"].as_str().unwrap().parse().unwrap();
        let coin_type = bc["coinType"].as_str().unwrap().to_string();
        (amount, coin_type)
    };

    let (sender_amount, sender_coin) = find_balance(&sender_str);
    let (recipient_amount, recipient_coin) = find_balance(&recipient_str);

    assert_eq!(sender_coin, "0x2::sui::SUI");
    assert_eq!(recipient_coin, "0x2::sui::SUI");
    assert_eq!(recipient_amount, 1_000);

    let gas_total = computation as i128 + storage as i128 - rebate as i128;
    assert_eq!(sender_amount, -(1_000 + gas_total));

    // -- object changes --
    let object_changes = result["objectChanges"].as_array().unwrap();
    assert_eq!(object_changes.len(), 2);

    let mutated_change = object_changes
        .iter()
        .find(|c| c["type"].as_str() == Some("mutated"))
        .expect("should have a mutated object change");
    assert_eq!(
        mutated_change["owner"]["AddressOwner"].as_str().unwrap(),
        sender_str
    );
    assert!(
        mutated_change["objectType"]
            .as_str()
            .unwrap()
            .contains("Coin<0x2::sui::SUI>")
    );
    assert_eq!(mutated_change["sender"].as_str().unwrap(), sender_str);
    assert!(!mutated_change["digest"].as_str().unwrap().is_empty());
    assert!(mutated_change["version"].as_str().is_some());
    assert!(mutated_change["previousVersion"].as_str().is_some());

    let created_change = object_changes
        .iter()
        .find(|c| c["type"].as_str() == Some("created"))
        .expect("should have a created object change");
    assert_eq!(
        created_change["owner"]["AddressOwner"].as_str().unwrap(),
        recipient_str
    );
    assert!(
        created_change["objectType"]
            .as_str()
            .unwrap()
            .contains("Coin<0x2::sui::SUI>")
    );
    assert_eq!(created_change["sender"].as_str().unwrap(), sender_str);
    assert!(!created_change["digest"].as_str().unwrap().is_empty());
    assert!(created_change["version"].as_str().is_some());
}

#[tokio::test]
async fn test_execute_no_options_omits_fields() {
    telemetry_subscribers::init_for_testing();
    let cluster = WriteTestCluster::new().await.unwrap();
    let (tx_digest, tx_bytes, sigs) = cluster.transfer_transaction().await.unwrap();

    let response = cluster
        .execute_jsonrpc(
            "sui_executeTransactionBlock",
            json!({
                "tx_bytes": tx_bytes,
                "signatures": sigs,
            }),
        )
        .await
        .unwrap();

    let result = &response["result"];
    assert_eq!(result["digest"], tx_digest);
    assert!(result["transaction"].is_null());
    assert!(result["effects"].is_null());
    assert!(result["events"].is_null());
    assert!(result["objectChanges"].is_null());
    assert!(result["balanceChanges"].is_null());
}

#[tokio::test]
async fn test_execute_aborted_tx() {
    telemetry_subscribers::init_for_testing();
    let cluster = WriteTestCluster::new().await.unwrap();
    let (_, tx_bytes, sigs) = cluster.privileged_transaction().await.unwrap();

    let response = cluster
        .execute_jsonrpc(
            "sui_executeTransactionBlock",
            json!({
                "tx_bytes": tx_bytes,
                "signatures": sigs,
                "options": { "showEffects": true },
            }),
        )
        .await
        .unwrap();

    assert_eq!(response["result"]["effects"]["status"]["status"], "failure");
}

#[tokio::test]
async fn test_execute_deprecated_mode() {
    telemetry_subscribers::init_for_testing();
    let cluster = WriteTestCluster::new().await.unwrap();
    let (_, tx_bytes, sigs) = cluster.transfer_transaction().await.unwrap();

    let response = cluster
        .execute_jsonrpc(
            "sui_executeTransactionBlock",
            json!({
                "tx_bytes": tx_bytes,
                "signatures": sigs,
                "request_type": "WaitForLocalExecution",
            }),
        )
        .await
        .unwrap();

    assert_eq!(response["error"]["code"], -32602);
}

#[tokio::test]
async fn test_execute_empty_sigs() {
    telemetry_subscribers::init_for_testing();
    let cluster = WriteTestCluster::new().await.unwrap();
    let (_, tx_bytes, _) = cluster.transfer_transaction().await.unwrap();

    let response = cluster
        .execute_jsonrpc(
            "sui_executeTransactionBlock",
            json!({
                "tx_bytes": tx_bytes,
                "signatures": [],
            }),
        )
        .await
        .unwrap();

    assert_eq!(response["error"]["code"], -32602);
}

#[tokio::test]
async fn test_dry_run_transfer_correctness() {
    telemetry_subscribers::init_for_testing();
    let cluster = WriteTestCluster::new().await.unwrap();

    let sender = cluster.onchain_cluster.wallet.get_addresses()[0];
    let recipient = cluster.onchain_cluster.wallet.get_addresses()[1];
    let (_, tx_bytes, _) = cluster.transfer_transaction().await.unwrap();

    let response = cluster
        .execute_jsonrpc(
            "sui_dryRunTransactionBlock",
            json!({ "tx_bytes": tx_bytes }),
        )
        .await
        .unwrap();

    let result = &response["result"];
    let sender_str = sender.to_string();
    let recipient_str = recipient.to_string();

    // -- input --
    assert_eq!(result["input"]["sender"], sender_str);
    let tx_kind = &result["input"]["transaction"];
    assert!(tx_kind["kind"].as_str().unwrap() == "ProgrammableTransaction");

    // -- effects --
    let effects = &result["effects"];
    assert_eq!(effects["status"]["status"], "success");
    assert!(!effects["transactionDigest"].as_str().unwrap().is_empty());

    let gas_used = &effects["gasUsed"];
    let computation: u64 = gas_used["computationCost"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let storage: u64 = gas_used["storageCost"].as_str().unwrap().parse().unwrap();
    let rebate: u64 = gas_used["storageRebate"].as_str().unwrap().parse().unwrap();
    let _non_refundable: u64 = gas_used["nonRefundableStorageFee"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(computation > 0);
    assert!(storage > 0);

    // Gas object reference belongs to sender.
    let gas_obj = &effects["gasObject"];
    assert_eq!(
        gas_obj["owner"]["AddressOwner"].as_str().unwrap(),
        sender_str
    );

    // Mutated set contains the gas coin.
    let mutated = effects["mutated"].as_array().unwrap();
    assert!(
        mutated.iter().any(|m| m["owner"]["AddressOwner"]
            .as_str()
            .is_some_and(|a| a == sender_str)),
        "mutated set should contain sender's gas coin"
    );

    // Created set contains the new coin for recipient.
    let created = effects["created"].as_array().unwrap();
    assert!(
        created.iter().any(|c| c["owner"]["AddressOwner"]
            .as_str()
            .is_some_and(|a| a == recipient_str)),
        "created set should contain recipient's new coin"
    );

    // -- balance changes --
    let balance_changes = result["balanceChanges"].as_array().unwrap();
    assert_eq!(balance_changes.len(), 2);

    let find_balance = |addr: &str| -> (i128, String) {
        let bc = balance_changes
            .iter()
            .find(|bc| {
                bc["owner"]["AddressOwner"]
                    .as_str()
                    .is_some_and(|a| a == addr)
            })
            .unwrap_or_else(|| panic!("balance change not found for {addr}"));
        let amount: i128 = bc["amount"].as_str().unwrap().parse().unwrap();
        let coin_type = bc["coinType"].as_str().unwrap().to_string();
        (amount, coin_type)
    };

    let (sender_amount, sender_coin) = find_balance(&sender_str);
    let (recipient_amount, recipient_coin) = find_balance(&recipient_str);

    assert_eq!(sender_coin, "0x2::sui::SUI");
    assert_eq!(recipient_coin, "0x2::sui::SUI");
    assert_eq!(recipient_amount, 1_000);

    let gas_total = computation as i128 + storage as i128 - rebate as i128;
    assert_eq!(sender_amount, -(1_000 + gas_total));

    // -- object changes --
    let object_changes = result["objectChanges"].as_array().unwrap();
    assert_eq!(object_changes.len(), 2);

    let mutated_change = object_changes
        .iter()
        .find(|c| c["type"].as_str() == Some("mutated"))
        .expect("should have a mutated object change");
    assert_eq!(
        mutated_change["owner"]["AddressOwner"].as_str().unwrap(),
        sender_str
    );
    assert!(
        mutated_change["objectType"]
            .as_str()
            .unwrap()
            .contains("Coin<0x2::sui::SUI>")
    );
    assert!(!mutated_change["digest"].as_str().unwrap().is_empty());
    assert!(mutated_change["version"].as_str().is_some());
    assert!(mutated_change["previousVersion"].as_str().is_some());

    let created_change = object_changes
        .iter()
        .find(|c| c["type"].as_str() == Some("created"))
        .expect("should have a created object change");
    assert_eq!(
        created_change["owner"]["AddressOwner"].as_str().unwrap(),
        recipient_str
    );
    assert!(
        created_change["objectType"]
            .as_str()
            .unwrap()
            .contains("Coin<0x2::sui::SUI>")
    );
    assert!(!created_change["digest"].as_str().unwrap().is_empty());
    assert!(created_change["version"].as_str().is_some());
}

#[tokio::test]
async fn test_dry_run_aborted_tx() {
    telemetry_subscribers::init_for_testing();
    let cluster = WriteTestCluster::new().await.unwrap();
    let (_, tx_bytes, _) = cluster.privileged_transaction().await.unwrap();

    let response = cluster
        .execute_jsonrpc(
            "sui_dryRunTransactionBlock",
            json!({ "tx_bytes": tx_bytes }),
        )
        .await
        .unwrap();

    assert_eq!(response["result"]["effects"]["status"]["status"], "failure");
}

#[tokio::test]
async fn test_dry_run_invalid_tx() {
    telemetry_subscribers::init_for_testing();
    let cluster = WriteTestCluster::new().await.unwrap();

    let response = cluster
        .execute_jsonrpc(
            "sui_dryRunTransactionBlock",
            json!({ "tx_bytes": "invalid_tx_bytes" }),
        )
        .await
        .unwrap();

    assert_eq!(response["error"]["code"], -32602);
}

#[tokio::test]
async fn test_execute_and_dry_run_gas_costs_agree() {
    telemetry_subscribers::init_for_testing();
    let cluster = WriteTestCluster::new().await.unwrap();
    let (_, tx_bytes, sigs) = cluster.transfer_transaction().await.unwrap();

    let dry_run = cluster
        .execute_jsonrpc(
            "sui_dryRunTransactionBlock",
            json!({ "tx_bytes": tx_bytes }),
        )
        .await
        .unwrap();

    let execute = cluster
        .execute_jsonrpc(
            "sui_executeTransactionBlock",
            json!({
                "tx_bytes": tx_bytes,
                "signatures": sigs,
                "options": { "showEffects": true },
            }),
        )
        .await
        .unwrap();

    assert_eq!(
        dry_run["result"]["effects"]["gasUsed"],
        execute["result"]["effects"]["gasUsed"],
    );
}

#[tokio::test]
async fn test_dev_inspect_returns_values() {
    let cluster = WriteTestCluster::new().await.unwrap();
    let sender = cluster.onchain_cluster.wallet.get_addresses()[0];

    let response = cluster
        .execute_jsonrpc(
            "sui_devInspectTransactionBlock",
            json!({
                "sender_address": sender.to_string(),
                "tx_bytes": encode_transaction_kind(&option_none_transaction_kind()),
            }),
        )
        .await
        .unwrap();

    assert!(
        response["error"].is_null(),
        "RPC error: {}",
        response["error"]
    );

    let result = &response["result"];
    assert_eq!(result["effects"]["status"]["status"], "success");
    assert!(result["error"].is_null());

    // One command, returning `std::option::Option<u64>`. The BCS encoding of `none` is a single
    // zero byte (an empty vector).
    let results = result["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);

    let return_values = results[0]["returnValues"].as_array().unwrap();
    assert_eq!(return_values.len(), 1);
    assert_eq!(return_values[0][0], json!([0]));
    assert!(
        return_values[0][1]
            .as_str()
            .unwrap()
            .contains("option::Option<u64>"),
        "Unexpected return type: {}",
        return_values[0][1],
    );

    // Raw transaction data and effects were not requested.
    assert!(result["rawTxnData"].is_null());
    assert!(result["rawEffects"].is_null());
}

#[tokio::test]
async fn test_dev_inspect_synthesized_transaction_defaults() {
    let cluster = WriteTestCluster::new().await.unwrap();
    let sender = cluster.onchain_cluster.wallet.get_addresses()[0];
    let reference_gas_price = cluster.onchain_cluster.get_reference_gas_price().await;

    let response = cluster
        .execute_jsonrpc(
            "sui_devInspectTransactionBlock",
            json!({
                "sender_address": sender.to_string(),
                "tx_bytes": encode_transaction_kind(&option_none_transaction_kind()),
                "additional_args": { "showRawTxnDataAndEffects": true },
            }),
        )
        .await
        .unwrap();

    let result = &response["result"];
    assert_eq!(result["effects"]["status"]["status"], "success");

    // The raw transaction data reflects the synthesized `TransactionData`: gas price defaults to
    // the reference gas price, the sender sponsors the gas, and the gas payment is left empty (a
    // mock gas coin is injected fullnode-side).
    let tx_data: TransactionData = bcs::from_bytes(&json_bytes(&result["rawTxnData"])).unwrap();
    assert_eq!(tx_data.sender(), sender);

    let gas_data = tx_data.gas_data();
    assert_eq!(gas_data.owner, sender);
    assert_eq!(gas_data.price, reference_gas_price);
    assert!(gas_data.payment.is_empty());
    assert!(gas_data.budget > 0);

    assert!(!result["rawEffects"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_dev_inspect_with_checks_enabled() {
    let cluster = WriteTestCluster::new().await.unwrap();
    let sender = cluster.onchain_cluster.wallet.get_addresses()[0];

    let response = cluster
        .execute_jsonrpc(
            "sui_devInspectTransactionBlock",
            json!({
                "sender_address": sender.to_string(),
                "tx_bytes": encode_transaction_kind(&option_none_transaction_kind()),
                "additional_args": { "skipChecks": false },
            }),
        )
        .await
        .unwrap();

    assert!(
        response["error"].is_null(),
        "RPC error: {}",
        response["error"]
    );

    let result = &response["result"];
    assert_eq!(result["effects"]["status"]["status"], "success");
    assert_eq!(result["results"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_dev_inspect_execution_failure() {
    let cluster = WriteTestCluster::new().await.unwrap();
    let sender = cluster.onchain_cluster.wallet.get_addresses()[0];

    let response = cluster
        .execute_jsonrpc(
            "sui_devInspectTransactionBlock",
            json!({
                "sender_address": sender.to_string(),
                "tx_bytes": encode_transaction_kind(&divide_by_zero_transaction_kind()),
            }),
        )
        .await
        .unwrap();

    let result = &response["result"];
    assert_eq!(result["effects"]["status"]["status"], "failure");
    assert!(!result["error"].is_null(), "Expected an execution error");
    assert!(result["results"].is_null());
}

#[tokio::test]
async fn test_dev_inspect_matches_fullnode_jsonrpc() {
    telemetry_subscribers::init_for_testing();
    let cluster = WriteTestCluster::new().await.unwrap();
    let sender = cluster.onchain_cluster.wallet.get_addresses()[0];

    // Successful execution: the responses must match in full, including the raw transaction
    // bytes (proving the synthesized TransactionData is identical) and effects.
    let params = json!({
        "sender_address": sender.to_string(),
        "tx_bytes": encode_transaction_kind(&option_none_transaction_kind()),
        "additional_args": { "showRawTxnDataAndEffects": true },
    });

    let indexer = cluster
        .execute_jsonrpc("sui_devInspectTransactionBlock", params.clone())
        .await
        .unwrap();
    let fullnode = cluster
        .execute_fullnode_jsonrpc("sui_devInspectTransactionBlock", params)
        .await
        .unwrap();

    assert!(
        indexer["error"].is_null(),
        "indexer RPC error: {}",
        indexer["error"]
    );
    assert!(
        fullnode["error"].is_null(),
        "fullnode RPC error: {}",
        fullnode["error"]
    );
    assert_eq!(indexer["result"], fullnode["result"]);

    // Failed execution: the responses must match except for the `error` message -- the fullnode
    // stringifies the executor's error, which is not part of the gRPC simulate response that the
    // indexer's implementation is built on, so the indexer recovers a differently-formatted
    // message from the effects' execution status.
    let params = json!({
        "sender_address": sender.to_string(),
        "tx_bytes": encode_transaction_kind(&divide_by_zero_transaction_kind()),
    });

    let indexer = cluster
        .execute_jsonrpc("sui_devInspectTransactionBlock", params.clone())
        .await
        .unwrap();
    let fullnode = cluster
        .execute_fullnode_jsonrpc("sui_devInspectTransactionBlock", params)
        .await
        .unwrap();

    let mut indexer_result = indexer["result"].clone();
    let mut fullnode_result = fullnode["result"].clone();

    let indexer_error = indexer_result.as_object_mut().unwrap().remove("error");
    let fullnode_error = fullnode_result.as_object_mut().unwrap().remove("error");
    assert!(indexer_error.is_some_and(|e| !e.is_null()));
    assert!(fullnode_error.is_some_and(|e| !e.is_null()));

    assert_eq!(indexer_result, fullnode_result);
}

#[tokio::test]
async fn test_dev_inspect_invalid_tx_bytes() {
    let cluster = WriteTestCluster::new().await.unwrap();
    let sender = cluster.onchain_cluster.wallet.get_addresses()[0];

    let response = cluster
        .execute_jsonrpc(
            "sui_devInspectTransactionBlock",
            json!({
                "sender_address": sender.to_string(),
                "tx_bytes": "invalid_tx_bytes",
            }),
        )
        .await
        .unwrap();

    assert_eq!(response["error"]["code"], -32602);
}
