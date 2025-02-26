// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::Context;
use prometheus::Registry;
use reqwest::Client;
use serde_json::{json, Value};
use sui_indexer_alt_jsonrpc::{
    args::WriteArgs, config::RpcConfig, data::system_package_task::SystemPackageTaskArgs,
    start_rpc, RpcArgs,
};
use sui_pg_db::{
    temp::{get_available_port, TempDb},
    DbArgs,
};
use sui_swarm_config::genesis_config::AccountConfig;
use test_cluster::{TestCluster, TestClusterBuilder};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use url::Url;

const EPOCH_DURATION_MS: u64 = 2000;
const GAS_OBJECT_COUNT: usize = 1;
const DEFAULT_GAS_AMOUNT: u64 = 1_000_000_000_000;

struct WriteTestCluster {
    onchain_cluster: TestCluster,
    rpc_url: String,
    rpc_handle: JoinHandle<()>,
    client: Client,
    cancel: CancellationToken,
}

impl WriteTestCluster {
    /// Creates a new test cluster with an RPC with transaction execution enabled.
    async fn new() -> anyhow::Result<Self> {
        let onchain_cluster = TestClusterBuilder::new()
            .with_num_validators(1)
            .with_epoch_duration_ms(EPOCH_DURATION_MS)
            .with_accounts(vec![
                AccountConfig {
                    address: None,
                    gas_amounts: vec![DEFAULT_GAS_AMOUNT; GAS_OBJECT_COUNT],
                };
                4
            ])
            .build()
            .await;

        // Unwrap since we know the URL should be valid.
        let fullnode_rpc_url = Url::parse(onchain_cluster.rpc_url())?;

        let cancel = CancellationToken::new();

        let (rpc_handle, rpc_url) = set_up_rpc_server(fullnode_rpc_url, cancel.clone()).await;

        Ok(Self {
            onchain_cluster,
            rpc_url,
            rpc_handle,
            client: Client::new(),
            cancel,
        })
    }

    /// Builds a simple transaction and returns the digest, tx bytes, and sigs to be used for testing.
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
        let signed_tx = self.onchain_cluster.wallet.sign_transaction(&tx);
        let (tx_bytes, sigs) = signed_tx.to_tx_bytes_and_signatures();
        let tx_bytes = tx_bytes.encoded();
        let sigs = sigs.iter().map(|sig| sig.encoded()).collect::<Vec<_>>();

        Ok((tx_digest, tx_bytes, sigs))
    }

    /// Builds a transaction that would abort if called by a normal user.
    async fn privileged_transaction(&self) -> anyhow::Result<(String, String, Vec<String>)> {
        let tx: sui_types::transaction::TransactionData = self
            .onchain_cluster
            .test_transaction_builder()
            .await
            .call_request_remove_validator()
            .build();
        let tx_digest = tx.digest().to_string();
        let signed_tx = self.onchain_cluster.wallet.sign_transaction(&tx);
        let (tx_bytes, sigs) = signed_tx.to_tx_bytes_and_signatures();
        let tx_bytes = tx_bytes.encoded();
        let sigs = sigs.iter().map(|sig| sig.encoded()).collect::<Vec<_>>();

        Ok((tx_digest, tx_bytes, sigs))
    }

    async fn execute_jsonrpc(&self, method: String, params: Value) -> anyhow::Result<Value> {
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

        let body: Value = response
            .json()
            .await
            .context("Failed to parse JSON-RPC response")?;

        Ok(body)
    }

    async fn stopped(self) {
        self.cancel.cancel();
        let _ = self.rpc_handle.await;
    }
}

#[tokio::test]
async fn test_execution() {
    let test_cluster = WriteTestCluster::new()
        .await
        .expect("Failed to create test cluster");

    let (tx_digest, tx_bytes, sigs) = test_cluster.transfer_transaction().await.unwrap();

    // Call the executeTransactionBlock method and check that the response is valid.
    let response = test_cluster
        .execute_jsonrpc(
            "sui_executeTransactionBlock".to_string(),
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

    // Checking that all the requested fields are present in the response.
    assert_eq!(response["result"]["digest"], tx_digest);
    assert!(response["result"]["transaction"].is_object());
    assert!(response["result"]["rawTransaction"].is_string());
    assert!(response["result"]["effects"].is_object());
    assert!(response["result"]["rawEffects"].is_array());
    assert!(response["result"]["events"].is_array());
    assert!(response["result"]["objectChanges"].is_array());
    assert!(response["result"]["balanceChanges"].is_array());

    test_cluster.stopped().await;
}

#[tokio::test]
async fn test_execution_with_deprecated_mode() {
    let test_cluster = WriteTestCluster::new()
        .await
        .expect("Failed to create test cluster");

    let (_, tx_bytes, sigs) = test_cluster.transfer_transaction().await.unwrap();

    // Call the executeTransactionBlock method and check that the response is valid.
    let response = test_cluster
        .execute_jsonrpc(
            "sui_executeTransactionBlock".to_string(),
            json!({
                "tx_bytes": tx_bytes,
                "signatures": sigs,
                "request_type": "WaitForLocalExecution",
            }),
        )
        .await
        .unwrap();

    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(
        response["error"]["message"],
        "Invalid Params: WaitForLocalExecution mode is deprecated"
    );

    test_cluster.stopped().await;
}

#[tokio::test]
async fn test_execution_with_no_sigs() {
    let test_cluster = WriteTestCluster::new()
        .await
        .expect("Failed to create test cluster");

    let (_, tx_bytes, _) = test_cluster.transfer_transaction().await.unwrap();

    // Call the executeTransactionBlock method and check that the response is valid.
    let response = test_cluster
        .execute_jsonrpc(
            "sui_executeTransactionBlock".to_string(),
            json!({
                "tx_bytes": tx_bytes,
            }),
        )
        .await
        .unwrap();

    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(response["error"]["message"], "Invalid params");

    test_cluster.stopped().await;
}

#[tokio::test]
async fn test_execution_with_empty_sigs() {
    let test_cluster = WriteTestCluster::new()
        .await
        .expect("Failed to create test cluster");

    let (_, tx_bytes, _) = test_cluster.transfer_transaction().await.unwrap();

    // Call the executeTransactionBlock method and check that the response is valid.
    let response = test_cluster
        .execute_jsonrpc(
            "sui_executeTransactionBlock".to_string(),
            json!({
                "tx_bytes": tx_bytes,
                "signatures": [],
            }),
        )
        .await
        .unwrap();

    assert_eq!(response["error"]["code"], -32002);
    assert_eq!(
        response["error"]["message"],
        "Invalid user signature: Expect 1 signer signatures but got 0"
    );

    test_cluster.stopped().await;
}

#[tokio::test]
async fn test_execution_with_aborted_tx() {
    let test_cluster = WriteTestCluster::new()
        .await
        .expect("Failed to create test cluster");

    let (_, tx_bytes, sigs) = test_cluster.privileged_transaction().await.unwrap();

    // Call the executeTransactionBlock method and check that the response is valid.
    let response = test_cluster
        .execute_jsonrpc(
            "sui_executeTransactionBlock".to_string(),
            json!({
                "tx_bytes": tx_bytes,
                "signatures": sigs,
                "options": {
                    "showEffects": true,
                },
            }),
        )
        .await
        .unwrap();

    assert_eq!(response["result"]["effects"]["status"]["status"], "failure");

    test_cluster.stopped().await;
}

#[tokio::test]
async fn test_dry_run() {
    let test_cluster = WriteTestCluster::new()
        .await
        .expect("Failed to create test cluster");

    let (_, tx_bytes, _) = test_cluster.transfer_transaction().await.unwrap();

    let response = test_cluster
        .execute_jsonrpc(
            "sui_dryRunTransactionBlock".to_string(),
            json!({
                "tx_bytes": tx_bytes,
            }),
        )
        .await
        .unwrap();

    assert_eq!(response["result"]["effects"]["status"]["status"], "success");

    test_cluster.stopped().await;
}

#[tokio::test]
async fn test_dry_run_with_invalid_tx() {
    let test_cluster = WriteTestCluster::new()
        .await
        .expect("Failed to create test cluster");

    let response = test_cluster
        .execute_jsonrpc(
            "sui_dryRunTransactionBlock".to_string(),
            json!({
                "tx_bytes": "invalid_tx_bytes",
            }),
        )
        .await
        .unwrap();

    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(response["error"]["message"], "Invalid params");

    test_cluster.stopped().await;
}

async fn set_up_rpc_server(
    fullnode_rpc_url: Url,
    cancel: CancellationToken,
) -> (JoinHandle<()>, String) {
    let rpc_port = get_available_port();
    let rpc_listen_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), rpc_port);
    let rpc_url = Url::parse(&format!("http://{}/", rpc_listen_address))
        .expect("Failed to parse RPC URL")
        .to_string();

    // We don't expose metrics in these tests, but we create a registry to collect them anyway.
    let registry = Registry::new();

    let database = TempDb::new().expect("Failed to create temporary database");

    let db_args = DbArgs {
        database_url: database.database().url().clone(),
        ..Default::default()
    };

    let rpc_args = RpcArgs {
        rpc_listen_address,
        ..Default::default()
    };

    let write_args = WriteArgs { fullnode_rpc_url };

    let rpc_handle = start_rpc(
        db_args,
        rpc_args,
        Some(write_args),
        SystemPackageTaskArgs::default(),
        RpcConfig::example(),
        &registry,
        cancel.child_token(),
    )
    .await
    .expect("Failed to start JSON-RPC server");

    (rpc_handle, rpc_url)
}
