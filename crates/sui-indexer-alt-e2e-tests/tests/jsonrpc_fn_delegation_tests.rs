// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::Context;
use reqwest::Client;
use serde_json::Value;
use serde_json::json;
use sui_indexer_alt_e2e_tests::OffchainCluster;
use sui_indexer_alt_e2e_tests::OffchainClusterConfig;
use sui_indexer_alt_e2e_tests::local_ingestion_client_args;
use sui_indexer_alt_jsonrpc::NodeArgs as JsonRpcNodeArgs;
use sui_swarm_config::genesis_config::AccountConfig;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_test_transaction_builder::make_staking_transaction;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Owner;
use sui_types::transaction::TransactionDataAPI;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use url::Url;

struct FnDelegationTestCluster {
    onchain_cluster: TestCluster,
    offchain: OffchainCluster,
    client: Client,
    /// Checkpoint ingestion directory shared between TestCluster and OffchainCluster, held to keep
    /// the temp dir alive for the lifetime of the cluster.
    _ingestion_dir: tempfile::TempDir,
}

impl FnDelegationTestCluster {
    async fn new() -> anyhow::Result<Self> {
        let (client_args, ingestion_dir) = local_ingestion_client_args();

        let onchain_cluster = TestClusterBuilder::new()
            .with_num_validators(2)
            .with_epoch_duration_ms(300_000)
            .with_accounts(vec![
                AccountConfig {
                    address: None,
                    gas_amounts: vec![1_000_000_000_000; 5],
                };
                4
            ])
            .with_data_ingestion_dir(ingestion_dir.path().to_owned())
            .build()
            .await;

        let fullnode_rpc_url = Url::parse(onchain_cluster.rpc_url())?;

        // Pass `client_args` to link the OffchainCluster to the checkpoint ingestion directory
        // written to by the TestCluster
        let offchain = OffchainCluster::new(
            client_args,
            OffchainClusterConfig {
                jsonrpc_node_args: JsonRpcNodeArgs {
                    fullnode_rpc_url: Some(fullnode_rpc_url.clone()),
                    fullnode_grpc_url: Some(fullnode_rpc_url.to_string()),
                },
                ..Default::default()
            },
            &prometheus::Registry::new(),
        )
        .await
        .context("Failed to create off-chain cluster")?;

        Ok(Self {
            onchain_cluster,
            offchain,
            client: Client::new(),
            _ingestion_dir: ingestion_dir,
        })
    }

    async fn get_validator_address(&self) -> SuiAddress {
        self.get_validator_addresses().await[0]
    }

    async fn get_validator_addresses(&self) -> Vec<SuiAddress> {
        self.onchain_cluster
            .grpc_client()
            .get_system_state_summary(None)
            .await
            .unwrap()
            .active_validators
            .iter()
            .map(|v| v.sui_address)
            .collect()
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
            .post(self.offchain.jsonrpc_url())
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

    /// Wait for the indexer, consistent store, and BigTable to all catch up to the fullnode's
    /// latest checkpoint.
    async fn wait_for_indexing(&self) {
        let cp = self
            .onchain_cluster
            .fullnode_handle
            .sui_node
            .state()
            .get_latest_checkpoint_sequence_number()
            .unwrap();

        let timeout = Duration::from_secs(60);
        self.offchain
            .wait_for_indexer(cp, timeout)
            .await
            .expect("Timed out waiting for indexer");
        self.offchain
            .wait_for_consistent_store(cp, timeout)
            .await
            .expect("Timed out waiting for consistent store");
        self.offchain
            .wait_for_bigtable(cp, timeout)
            .await
            .expect("Timed out waiting for bigtable");
    }
}

/// Pulls the single sender-owned object created by a staking tx — the newly minted StakedSui.
fn staked_sui_id_from_effects(effects: &TransactionEffects, owner: SuiAddress) -> ObjectID {
    let mut created = effects.created().into_iter().filter_map(|((id, _, _), o)| {
        matches!(o, Owner::AddressOwner(addr) if addr == owner).then_some(id)
    });
    let id = created
        .next()
        .expect("staking tx should create a StakedSui owned by the sender");
    assert!(
        created.next().is_none(),
        "unexpected additional sender-owned objects created by staking tx"
    );
    id
}

/// Projects a `getStakes`/`getStakesByIds` response to `(validatorAddress, [stakedSuiId])` so
/// comparisons are insensitive to reward values (which can shift between back-to-back dry runs).
fn stake_id_projection(result: &Value) -> Vec<(String, Vec<String>)> {
    result
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let validator = entry["validatorAddress"].as_str().unwrap().to_string();
            let ids = entry["stakes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s["stakedSuiId"].as_str().unwrap().to_string())
                .collect();
            (validator, ids)
        })
        .collect()
}

#[tokio::test]
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

    test_cluster.wait_for_indexing().await;

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

#[tokio::test]
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

#[tokio::test]
async fn test_stakes_correct_ordering() {
    let test_cluster = FnDelegationTestCluster::new()
        .await
        .expect("Failed to create test cluster");

    let wallet = &test_cluster.onchain_cluster.wallet;
    let validators = test_cluster.get_validator_addresses().await;
    assert!(validators.len() >= 2, "need at least 2 validators");
    let validator_a = validators[0];
    let validator_b = validators[1];

    // Stake once to validator A.
    let tx_a = make_staking_transaction(wallet, validator_a).await;
    let stake_owner_address = tx_a.data().transaction_data().sender();
    let effects_a = wallet.execute_transaction_must_succeed(tx_a).await.effects;
    let stake_a = staked_sui_id_from_effects(&effects_a, stake_owner_address);

    // Stake twice to validator B (from the same owner).
    let tx_b1 = make_staking_transaction(wallet, validator_b).await;
    let effects_b1 = wallet.execute_transaction_must_succeed(tx_b1).await.effects;
    let stake_b1 = staked_sui_id_from_effects(&effects_b1, stake_owner_address);
    let tx_b2 = make_staking_transaction(wallet, validator_b).await;
    let effects_b2 = wallet.execute_transaction_must_succeed(tx_b2).await.effects;
    let stake_b2 = staked_sui_id_from_effects(&effects_b2, stake_owner_address);

    test_cluster.wait_for_indexing().await;

    let response = test_cluster
        .execute_jsonrpc(
            "suix_getStakes".to_string(),
            json!({ "owner": stake_owner_address }),
        )
        .await
        .unwrap();

    let result = &response["result"];

    // Should have 2 DelegatedStake entries (one per validator).
    assert_eq!(result.as_array().unwrap().len(), 2);

    // Validator A: exactly the stake from tx_a.
    let entry_a = result
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["validatorAddress"] == validator_a.to_string().as_str())
        .expect("missing DelegatedStake for validator A");
    let entry_a_ids: Vec<&str> = entry_a["stakes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["stakedSuiId"].as_str().unwrap())
        .collect();
    assert_eq!(entry_a_ids, vec![stake_a.to_string().as_str()]);

    // Validator B: the two stakes from tx_b1 and tx_b2 (order is `list_owned_objects`-driven,
    // so compare as a set).
    let entry_b = result
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["validatorAddress"] == validator_b.to_string().as_str())
        .expect("missing DelegatedStake for validator B");
    let entry_b_ids: std::collections::HashSet<String> = entry_b["stakes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["stakedSuiId"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        entry_b_ids,
        std::collections::HashSet::from([stake_b1.to_string(), stake_b2.to_string()])
    );

    // All stakes should be Pending (epoch 0, no rewards yet).
    for entry in result.as_array().unwrap() {
        for stake in entry["stakes"].as_array().unwrap() {
            assert_eq!(stake["status"], "Pending");
        }
    }

    // Advance one epoch so the stakes reach their activation epoch. Stakes requested in epoch 0
    // have activation_epoch = 1, so after one reconfiguration current_epoch >= activation_epoch.
    test_cluster.onchain_cluster.trigger_reconfiguration().await;

    test_cluster.wait_for_indexing().await;

    let response = test_cluster
        .execute_jsonrpc(
            "suix_getStakes".to_string(),
            json!({ "owner": stake_owner_address }),
        )
        .await
        .unwrap();
    let result = &response["result"];

    // All stakes should now be Active
    for entry in result.as_array().unwrap() {
        for stake in entry["stakes"].as_array().unwrap() {
            assert_eq!(stake["status"], "Active", "stake not active: {stake}");
            let _reward: u64 = stake["estimatedReward"]
                .as_str()
                .expect("estimatedReward should be a BigInt string")
                .parse()
                .expect("estimatedReward should parse as u64");
        }
    }

    // Query just validator B's two stakes by ID. Inner stake ordering follows caller-provided
    // ID order, so feeding [b1, b2] should return them in that order.
    let forward_response = test_cluster
        .execute_jsonrpc(
            "suix_getStakesByIds".to_string(),
            json!({ "staked_sui_ids": [stake_b1, stake_b2] }),
        )
        .await
        .unwrap();

    let forward_b_ids: Vec<String> = stake_id_projection(&forward_response["result"])
        .into_iter()
        .flat_map(|(_, ids)| ids)
        .collect();
    assert_eq!(
        forward_b_ids,
        vec![stake_b1.to_string(), stake_b2.to_string()]
    );

    // Reversing the input IDs flips the inner ordering.
    let reversed_response = test_cluster
        .execute_jsonrpc(
            "suix_getStakesByIds".to_string(),
            json!({ "staked_sui_ids": [stake_b2, stake_b1] }),
        )
        .await
        .unwrap();

    let reversed_b_ids: Vec<String> = stake_id_projection(&reversed_response["result"])
        .into_iter()
        .flat_map(|(_, ids)| ids)
        .collect();
    assert_eq!(
        reversed_b_ids,
        vec![stake_b2.to_string(), stake_b1.to_string()]
    );
}

#[tokio::test]
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

/// Withdrawing a stake (calling `sui_system::request_withdraw_stake`) deletes the StakedSui
/// object. With two stakes in play, withdrawing one of them should drop only that one from a
/// `getStakesByIds` response covering both. The remaining stake must still come back. This is
/// our documented divergence from legacy `sui-json-rpc`, which returned Unstaked for the
/// withdrawn one.
#[tokio::test]
async fn test_get_stakes_by_ids_omits_withdrawn() {
    let test_cluster = FnDelegationTestCluster::new()
        .await
        .expect("Failed to create test cluster");

    let wallet = &test_cluster.onchain_cluster.wallet;
    let validator_address = test_cluster.get_validator_address().await;

    // Create two stakes.
    let staking_tx_a = make_staking_transaction(wallet, validator_address).await;
    let stake_owner = staking_tx_a.data().transaction_data().sender();
    wallet.execute_transaction_must_succeed(staking_tx_a).await;
    let staking_tx_b = make_staking_transaction(wallet, validator_address).await;
    wallet.execute_transaction_must_succeed(staking_tx_b).await;

    test_cluster.wait_for_indexing().await;

    // Pull both stake IDs out of the get_stakes response.
    let get_stakes_response = test_cluster
        .execute_jsonrpc(
            "suix_getStakes".to_string(),
            json!({ "owner": stake_owner }),
        )
        .await
        .unwrap();
    let stake_ids: Vec<ObjectID> = get_stakes_response["result"][0]["stakes"]
        .as_array()
        .expect("missing stakes array")
        .iter()
        .map(|s| {
            s["stakedSuiId"]
                .as_str()
                .expect("missing stakedSuiId")
                .parse()
                .expect("malformed stakedSuiId")
        })
        .collect();
    assert_eq!(stake_ids.len(), 2, "expected two stakes, got {stake_ids:?}");
    let withdrawn_stake_id = stake_ids[0];
    let kept_stake_id = stake_ids[1];

    // Withdraw the first stake.
    let stake_ref = test_cluster
        .onchain_cluster
        .get_latest_object_ref(&withdrawn_stake_id)
        .await;
    let gas_price = wallet.get_reference_gas_price().await.unwrap();
    let accounts_and_objs = wallet.get_all_accounts_and_gas_objects().await.unwrap();
    let sender = accounts_and_objs[0].0;
    let gas_object = accounts_and_objs[0].1[0];
    let unstake_tx = wallet
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, gas_price)
                .call_unstaking(stake_ref)
                .build(),
        )
        .await;
    wallet.execute_transaction_must_succeed(unstake_tx).await;

    test_cluster.wait_for_indexing().await;

    // Query both IDs. The withdrawn stake should be omitted; the kept one should still come
    // back.
    let get_by_id_response = test_cluster
        .execute_jsonrpc(
            "suix_getStakesByIds".to_string(),
            json!({ "staked_sui_ids": [withdrawn_stake_id, kept_stake_id] }),
        )
        .await
        .unwrap();

    let returned_ids: Vec<&str> = get_by_id_response["result"]
        .as_array()
        .expect("result should be an array")
        .iter()
        .flat_map(|delegated| {
            delegated["stakes"]
                .as_array()
                .expect("stakes should be an array")
                .iter()
                .map(|s| s["stakedSuiId"].as_str().expect("missing stakedSuiId"))
        })
        .collect();

    assert_eq!(
        returned_ids,
        vec![kept_stake_id.to_string().as_str()],
        "expected only the kept stake to be returned, got {get_by_id_response}",
    );

    // getStakes(owner) should also reflect the withdrawal — only the kept stake remains
    // listed against this owner.
    let get_stakes_response = test_cluster
        .execute_jsonrpc(
            "suix_getStakes".to_string(),
            json!({ "owner": stake_owner }),
        )
        .await
        .unwrap();
    let owner_returned_ids: Vec<&str> = get_stakes_response["result"]
        .as_array()
        .expect("result should be an array")
        .iter()
        .flat_map(|delegated| {
            delegated["stakes"]
                .as_array()
                .expect("stakes should be an array")
                .iter()
                .map(|s| s["stakedSuiId"].as_str().expect("missing stakedSuiId"))
        })
        .collect();
    assert_eq!(
        owner_returned_ids,
        vec![kept_stake_id.to_string().as_str()],
        "expected only the kept stake from getStakes(owner), got {get_stakes_response}",
    );
}
