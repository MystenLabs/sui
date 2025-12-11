// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use anyhow::Context;
use prometheus::Registry;
use reqwest::Client;
use serde_json::{Value, json};
use sui_futures::service::Service;
use sui_indexer_alt_jsonrpc::{
    NodeArgs, RpcArgs, args::SystemPackageTaskArgs, config::RpcConfig, start_rpc,
};
use sui_indexer_alt_reader::bigtable_reader::BigtableArgs;
use sui_macros::sim_test;
use sui_pg_db::{DbArgs, temp::get_available_port};
use sui_swarm_config::genesis_config::AccountConfig;
use sui_test_transaction_builder::{make_publish_transaction, make_staking_transaction};
use sui_types::{base_types::SuiAddress, transaction::TransactionDataAPI};
use test_cluster::{TestCluster, TestClusterBuilder};
use url::Url;

struct FnDelegationTestCluster {
    onchain_cluster: TestCluster,
    rpc_url: Url,
    /// Hold on to the service so it doesn't get dropped (and therefore aborted) until the cluster
    /// goes out of scope.
    #[allow(unused)]
    service: Service,
    client: Client,
}

impl FnDelegationTestCluster {
    /// Creates a new test cluster with an RPC with transaction execution enabled.
    async fn new() -> anyhow::Result<Self> {
        let onchain_cluster = TestClusterBuilder::new()
            .with_num_validators(1)
            .with_epoch_duration_ms(300_000) // 5 minutes
            .with_accounts(vec![
                AccountConfig {
                    address: None,
                    gas_amounts: vec![1_000_000_000_000; 2],
                };
                4
            ])
            .build()
            .await;

        // Unwrap since we know the URL should be valid.
        let fullnode_rpc_url = Url::parse(onchain_cluster.rpc_url())?;

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

        let service = start_rpc(
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
        let signed_tx = self.onchain_cluster.wallet.sign_transaction(&tx).await;
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
        let signed_tx = self.onchain_cluster.wallet.sign_transaction(&tx).await;
        let (tx_bytes, sigs) = signed_tx.to_tx_bytes_and_signatures();
        let tx_bytes = tx_bytes.encoded();
        let sigs: Vec<_> = sigs.iter().map(|sig| sig.encoded()).collect();

        Ok((tx_digest, tx_bytes, sigs))
    }

    async fn get_validator_address(&self) -> SuiAddress {
        self.onchain_cluster
            .sui_client()
            .governance_api()
            .get_latest_sui_system_state()
            .await
            .unwrap()
            .active_validators
            .first()
            .unwrap()
            .sui_address
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
    assert!(
        response["error"]["data"]
            .as_str()
            .unwrap()
            .starts_with("missing field `signatures`")
    );
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
    assert!(
        response["error"]["data"]
            .as_str()
            .unwrap()
            .starts_with("Invalid value was given to the function")
    );
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
    assert!(
        response["error"]["data"]
            .as_str()
            .unwrap()
            .contains("Deserialization failed")
    );
}

#[sim_test]
async fn test_get_stakes_and_by_ids() {
    let test_cluster = FnDelegationTestCluster::new()
        .await
        .expect("Failed to create test cluster");

    let wallet = &test_cluster.onchain_cluster.wallet;

    // Execute a staking transaction so we have a stake to query.
    let validator_address = test_cluster.get_validator_address().await;
    let staking_transaction = make_staking_transaction(wallet, validator_address).await;
    let stake_owner_address = staking_transaction.data().transaction_data().sender();

    wallet
        .execute_transaction_must_succeed(staking_transaction)
        .await;

    // Get the stake by owner.
    let get_stakes_response = test_cluster
        .execute_jsonrpc(
            "suix_getStakes".to_string(),
            json!({ "owner": stake_owner_address }),
        )
        .await
        .unwrap();

    assert_eq!(
        get_stakes_response["result"][0]["validatorAddress"],
        validator_address.to_string().as_str()
    );
    assert!(get_stakes_response["result"][0]["stakes"][0]["stakedSuiId"].is_string());
    let stake_id = get_stakes_response["result"][0]["stakes"][0]["stakedSuiId"]
        .as_str()
        .unwrap();

    // Now get the stake by id.
    let get_stakes_by_ids_response = test_cluster
        .execute_jsonrpc(
            "suix_getStakesByIds".to_string(),
            json!({ "staked_sui_ids": [stake_id] }),
        )
        .await
        .unwrap();

    // Two responses should match.
    assert_eq!(get_stakes_response, get_stakes_by_ids_response);
}

#[sim_test]
async fn test_get_stakes_invalid_params() {
    let test_cluster = FnDelegationTestCluster::new()
        .await
        .expect("Failed to create test cluster");

    let response = test_cluster
        .execute_jsonrpc(
            "suix_getStakes".to_string(),
            json!({ "owner": "invalid_address" }),
        )
        .await
        .unwrap();

    // Check that we have all the error information in the response.
    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(response["error"]["message"], "Invalid params");
    assert!(
        response["error"]["data"]
            .as_str()
            .unwrap()
            .contains("Deserialization failed")
    );

    let response = test_cluster
        .execute_jsonrpc(
            "suix_getStakesByIds".to_string(),
            json!({ "staked_sui_ids": ["invalid_stake_id"] }),
        )
        .await
        .unwrap();

    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(response["error"]["message"], "Invalid params");
    assert!(
        response["error"]["data"]
            .as_str()
            .unwrap()
            .contains("AccountAddressParseError")
    );
}

#[sim_test]
async fn test_get_validators_apy() {
    let test_cluster = FnDelegationTestCluster::new()
        .await
        .expect("Failed to create test cluster");

    let validator_address = test_cluster.get_validator_address().await;

    let response = test_cluster
        .execute_jsonrpc("suix_getValidatorsApy".to_string(), json!({}))
        .await
        .unwrap();

    assert_eq!(
        response["result"]["apys"][0]["address"],
        validator_address.to_string()
    );
}

#[sim_test]
async fn test_get_balance() {
    let test_cluster = FnDelegationTestCluster::new()
        .await
        .expect("Failed to create test cluster");
    let wallet = &test_cluster.onchain_cluster.wallet;

    // Publish another coin to better test the API.
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["packages", "coin"]);
    let publish_transaction = make_publish_transaction(wallet, path).await;
    let owner_address = publish_transaction.data().transaction_data().sender();

    let execution_result = wallet
        .execute_transaction_must_succeed(publish_transaction)
        .await;

    let package_id = execution_result.get_new_package_obj().unwrap().0;

    // Test out the specified coin type.
    // Parse the coin type so we have the same string representation as the used by fullnode.
    let coin_type = sui_types::parse_sui_struct_tag(&format!("{}::my_coin::MY_COIN", package_id))
        .unwrap()
        .to_string();
    let response = test_cluster
        .execute_jsonrpc(
            "suix_getBalance".to_string(),
            json!({ "owner": owner_address.to_string().as_str(), "coinType": coin_type}),
        )
        .await
        .unwrap();

    assert_eq!(response["result"]["totalBalance"], "1230");
    assert_eq!(response["result"]["coinType"], coin_type);

    // Test out the default coin type.
    let response = test_cluster
        .execute_jsonrpc(
            "suix_getBalance".to_string(),
            json!({ "owner": owner_address.to_string().as_str()}),
        )
        .await
        .unwrap();

    assert_eq!(response["result"]["coinType"], "0x2::sui::SUI");

    // Test out the invalid coin type.
    let response = test_cluster
        .execute_jsonrpc(
            "suix_getBalance".to_string(),
            json!({ "owner": owner_address.to_string().as_str(), "coinType": "invalid_coin_type"}),
        )
        .await
        .unwrap();
    assert_eq!(response["error"]["code"], -32602);
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Invalid struct type: invalid_coin_type")
    );

    // Test out the invalid address.
    let response = test_cluster
        .execute_jsonrpc(
            "suix_getBalance".to_string(),
            json!({ "owner": "invalid_address", "coinType": coin_type}),
        )
        .await
        .unwrap();
    assert_eq!(response["error"]["code"], -32602);
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Invalid params")
    );
}
