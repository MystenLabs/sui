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
use sui_indexer_alt_jsonrpc::NodeArgs;
use sui_indexer_alt_jsonrpc::RpcArgs;
use sui_indexer_alt_jsonrpc::args::SystemPackageTaskArgs;
use sui_indexer_alt_jsonrpc::config::RpcConfig;
use sui_indexer_alt_jsonrpc::start_rpc;
use sui_indexer_alt_reader::bigtable_reader::BigtableArgs;
use sui_indexer_alt_reader::consistent_reader::ConsistentReaderArgs;
use sui_macros::sim_test;
use sui_pg_db::DbArgs;
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

        let rpc_listen_address =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), get_available_port());
        let rpc_url = Url::parse(&format!("http://{}/", rpc_listen_address))
            .expect("Failed to parse RPC URL");

        let registry = Registry::new();

        let service = start_rpc(
            None,
            None,
            DbArgs::default(),
            BigtableArgs::default(),
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
        .expect("Failed to start JSON-RPC server");

        Ok(Self {
            onchain_cluster,
            rpc_url,
            service,
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

#[sim_test]
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

    // Digest is correct.
    assert_eq!(result["digest"], tx_digest);

    // Input: sender is correct and signatures are present.
    assert_eq!(result["transaction"]["data"]["sender"], sender.to_string());
    assert!(
        !result["transaction"]["txSignatures"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // Raw input is a non-empty base64 string.
    assert!(!result["rawTransaction"].as_str().unwrap().is_empty());

    // Effects: successful, gas costs present, digest matches.
    let effects = &result["effects"];
    assert_eq!(effects["status"]["status"], "success");
    assert_eq!(effects["transactionDigest"], tx_digest);
    assert!(
        effects["gasUsed"]["computationCost"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            > 0
    );

    // Raw effects is a non-empty byte array.
    assert!(!result["rawEffects"].as_array().unwrap().is_empty());

    // Balance changes: sender loses, recipient gains.
    let balance_changes = result["balanceChanges"].as_array().unwrap();
    let sender_str = sender.to_string();
    let recipient_str = recipient.to_string();

    let sender_amount: i128 = balance_changes
        .iter()
        .find(|bc| {
            bc["owner"]["AddressOwner"]
                .as_str()
                .is_some_and(|a| a == sender_str)
        })
        .expect("sender should have a balance change")["amount"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(sender_amount < 0);

    let recipient_amount: i128 = balance_changes
        .iter()
        .find(|bc| {
            bc["owner"]["AddressOwner"]
                .as_str()
                .is_some_and(|a| a == recipient_str)
        })
        .expect("recipient should have a balance change")["amount"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(recipient_amount, 1_000);

    // Object changes: at least one mutated (gas coin).
    let object_changes = result["objectChanges"].as_array().unwrap();
    assert!(
        object_changes
            .iter()
            .any(|c| c["type"].as_str() == Some("mutated"))
    );
}

#[sim_test]
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

#[sim_test]
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

#[sim_test]
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

#[sim_test]
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

    assert!(response["error"]["code"].is_number());
}

#[sim_test]
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

    // Effects are successful.
    assert_eq!(result["effects"]["status"]["status"], "success");
    assert!(
        result["effects"]["gasUsed"]["computationCost"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            > 0
    );

    // Input sender is correct.
    assert_eq!(result["input"]["sender"], sender.to_string());

    // Balance changes: recipient gets 1000 MIST.
    let recipient_str = recipient.to_string();
    let recipient_amount: i128 = result["balanceChanges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|bc| {
            bc["owner"]["AddressOwner"]
                .as_str()
                .is_some_and(|a| a == recipient_str)
        })
        .expect("recipient should have a balance change")["amount"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(recipient_amount, 1_000);

    // Object changes should be present.
    assert!(!result["objectChanges"].as_array().unwrap().is_empty());
}

#[sim_test]
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

    // The transaction either fails at execution (effects with failure status)
    // or is rejected by the validator during simulation (error response).
    let has_failure_effects = response["result"]["effects"]["status"]["status"] == "failure";
    let has_error = response["error"].is_object();
    assert!(
        has_failure_effects || has_error,
        "expected failure effects or error response, got: {response}"
    );
}

#[sim_test]
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

#[sim_test]
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
