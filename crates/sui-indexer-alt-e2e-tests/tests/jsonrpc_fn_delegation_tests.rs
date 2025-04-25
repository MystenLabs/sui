// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::Context;
use prometheus::Registry;
use reqwest::Client;
use serde_json::{json, Value};
use sui_indexer_alt_jsonrpc::{
    args::SystemPackageTaskArgs, config::RpcConfig, start_rpc, NodeArgs, RpcArgs,
};
use sui_indexer_alt_reader::bigtable_reader::BigtableArgs;
use sui_macros::sim_test;
use sui_pg_db::{temp::get_available_port, DbArgs};
use sui_swarm_config::genesis_config::AccountConfig;
use test_cluster::{TestCluster, TestClusterBuilder};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use url::Url;

struct FnDelegationTestCluster {
    onchain_cluster: TestCluster,
    rpc_url: Url,
    rpc_handle: JoinHandle<()>,
    client: Client,
    cancel: CancellationToken,
}

impl FnDelegationTestCluster {
    /// Creates a new test cluster with an RPC with transaction execution enabled.
    async fn new() -> anyhow::Result<Self> {
        let onchain_cluster = TestClusterBuilder::new()
            .with_num_validators(1)
            .with_epoch_duration_ms(2000)
            .with_accounts(vec![
                AccountConfig {
                    address: None,
                    gas_amounts: vec![1_000_000_000_000; 1],
                };
                4
            ])
            .build()
            .await;

        // Unwrap since we know the URL should be valid.
        let fullnode_rpc_url = Url::parse(onchain_cluster.rpc_url())?;

        let cancel = CancellationToken::new();

        let rpc_listen_address =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), get_available_port());
        let rpc_url = Url::parse(&format!("http://{}/", rpc_listen_address))
            .expect("Failed to parse RPC URL");

        // We don't expose metrics in these tests, but we create a registry to collect them anyway.
        let registry = Registry::new();

        let rpc_args = RpcArgs {
            rpc_listen_address,
            ..Default::default()
        };

        let rpc_handle = start_rpc(
            None,
            None,
            DbArgs::default(),
            BigtableArgs::default(),
            rpc_args,
            NodeArgs {
                fullnode_rpc_url: Some(fullnode_rpc_url),
            },
            SystemPackageTaskArgs::default(),
            RpcConfig::default(),
            &registry,
            cancel.child_token(),
        )
        .await
        .expect("Failed to start JSON-RPC server");

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
        let sigs: Vec<_> = sigs.iter().map(|sig| sig.encoded()).collect();

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
        let sigs: Vec<_> = sigs.iter().map(|sig| sig.encoded()).collect();

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

#[sim_test]
async fn test_execution() {
    telemetry_subscribers::init_for_testing();
    let test_cluster = FnDelegationTestCluster::new()
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

    tracing::info!("execution rpc response is {:?}", response);

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

#[sim_test]
async fn test_execution_with_deprecated_mode() {
    telemetry_subscribers::init_for_testing();

    let test_cluster = FnDelegationTestCluster::new()
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

    tracing::info!("execution rpc response is {:?}", response);

    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(
        response["error"]["message"],
        "Invalid Params: WaitForLocalExecution mode is deprecated"
    );

    test_cluster.stopped().await;
}

#[sim_test]
async fn test_execution_with_no_sigs() {
    telemetry_subscribers::init_for_testing();

    let test_cluster = FnDelegationTestCluster::new()
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

    tracing::info!("execution rpc response is {:?}", response);

    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(response["error"]["message"], "Invalid params");
    assert!(response["error"]["data"]
        .as_str()
        .unwrap()
        .starts_with("missing field `signatures`"));

    test_cluster.stopped().await;
}

#[sim_test]
async fn test_execution_with_empty_sigs() {
    telemetry_subscribers::init_for_testing();

    let test_cluster = FnDelegationTestCluster::new()
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

    tracing::info!("execution rpc response is {:?}", response);

    assert_eq!(response["error"]["code"], -32002);
    assert_eq!(
        response["error"]["message"],
        "Invalid user signature: Expect 1 signer signatures but got 0"
    );

    test_cluster.stopped().await;
}

#[sim_test]
async fn test_execution_with_aborted_tx() {
    telemetry_subscribers::init_for_testing();

    let test_cluster = FnDelegationTestCluster::new()
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

    tracing::info!("execution rpc response is {:?}", response);

    assert_eq!(response["result"]["effects"]["status"]["status"], "failure");

    test_cluster.stopped().await;
}

#[sim_test]
async fn test_dry_run() {
    let test_cluster = FnDelegationTestCluster::new()
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

#[sim_test]
async fn test_dry_run_with_invalid_tx() {
    let test_cluster = FnDelegationTestCluster::new()
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
    assert!(response["error"]["data"]
        .as_str()
        .unwrap()
        .starts_with("Invalid value was given to the function"));
    test_cluster.stopped().await;
}

#[sim_test]
async fn test_get_all_balances() {
    let test_cluster = FnDelegationTestCluster::new()
        .await
        .expect("Failed to create test cluster");

    let address = test_cluster.onchain_cluster.wallet.get_addresses()[1];
    let response = test_cluster
        .execute_jsonrpc(
            "suix_getAllBalances".to_string(),
            json!({ "owner": address.to_string().as_str()}),
        )
        .await
        .unwrap();
    // Only check that FN can return a valid response and not check the contents;
    // the contents is FN logic and thus should be tested on the FN side.
    assert_eq!(response["result"][0]["coinType"], "0x2::sui::SUI");
    test_cluster.stopped().await;
}

#[sim_test]
async fn test_get_all_balances_with_invalid_address() {
    let test_cluster = FnDelegationTestCluster::new()
        .await
        .expect("Failed to create test cluster");
    let invalid_address = "23333";

    let response = test_cluster
        .execute_jsonrpc(
            "suix_getAllBalances".to_string(),
            json!({ "owner": invalid_address }),
        )
        .await
        .unwrap();

    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(response["error"]["message"], "Invalid params");
    assert!(response["error"]["data"]
        .as_str()
        .unwrap()
        .contains("Deserialization failed"));

    test_cluster.stopped().await;
}
