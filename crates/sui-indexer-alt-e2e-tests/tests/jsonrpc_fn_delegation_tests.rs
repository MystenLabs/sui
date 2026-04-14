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
use sui_test_transaction_builder::make_staking_transaction;
use sui_types::base_types::SuiAddress;
use sui_types::transaction::TransactionDataAPI;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
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
    /// Creates a new test cluster with an RPC with delegation governance enabled.
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
            ConsistentReaderArgs::default(),
            rpc_args,
            NodeArgs {
                fullnode_rpc_url: Some(fullnode_rpc_url),
                fullnode_grpc_url: None,
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

    async fn get_validator_address(&self) -> SuiAddress {
        self.onchain_cluster
            .grpc_client()
            .get_system_state_summary(None)
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
