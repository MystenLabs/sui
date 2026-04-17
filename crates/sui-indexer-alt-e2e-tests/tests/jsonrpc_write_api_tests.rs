// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;

use anyhow::Context;
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
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use url::Url;

struct WriteTestCluster {
    onchain_cluster: TestCluster,
    rpc_url: Url,
    #[allow(unused)]
    service: Service,
    #[allow(unused)]
    database: TempDb,
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
                fullnode_rpc_url: None,
                fullnode_grpc_url: Some(fullnode_grpc_url),
            },
            SystemPackageTaskArgs::default(),
            RpcConfig::default(),
            &registry,
        )
        .await
        .context("Failed to start JSON-RPC server")?;

        Ok(Self {
            onchain_cluster,
            rpc_url,
            service: rpc_service.merge(indexer_service),
            database,
            client: Client::new(),
        })
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
        let query = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        let response = self
            .client
            .post(self.rpc_url.clone())
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
    assert!(!result["rawTransaction"].as_str().unwrap().is_empty());

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
