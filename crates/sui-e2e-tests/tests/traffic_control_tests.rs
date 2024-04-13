// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! NB: Tests in this module expect real network connections and interactions, thus they
//! should all be tokio::test rather than simtest. Any deviation from this should be well
//! understood and justified.

use jsonrpsee::{
    core::{client::ClientT, RpcResult},
    rpc_params,
};
use sui_json_rpc_types::{
    SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_test_transaction_builder::batch_make_transfer_transactions;
use sui_types::{
    quorum_driver_types::ExecuteTransactionRequestType,
    traffic_control::{PolicyConfig, PolicyType},
};
use test_cluster::TestClusterBuilder;

#[tokio::test]
async fn test_traffic_control_rpc_ok() -> Result<(), anyhow::Error> {
    let policy_config = PolicyConfig {
        // TODO: Add some error codes
        tallyable_error_codes: vec![],
        connection_blocklist_ttl_sec: 1,
        proxy_blocklist_ttl_sec: 5,
        // Test that IP forwarding works through this policy
        spam_policy_type: PolicyType::TestInspectIp,
        // This should never be invoked when set as an error policy
        // as we are not sending requests that error
        error_policy_type: PolicyType::TestPanicOnInvocation,
        channel_capacity: 100,
    };
    let network_config = ConfigBuilder::new_with_temp_dir()
        .with_traffic_control_config(Some(policy_config))
        .build();
    let mut test_cluster = TestClusterBuilder::new()
        .set_network_config(network_config)
        .build()
        .await;

    let context = &mut test_cluster.wallet;
    let jsonrpc_client = &test_cluster.fullnode_handle.rpc_client;

    let txn_count = 4;
    let mut txns = batch_make_transfer_transactions(context, txn_count).await;
    assert!(
        txns.len() >= txn_count,
        "Expect at least {} txns. Do we generate enough gas objects during genesis?",
        txn_count,
    );

    let txn = txns.swap_remove(0);
    let tx_digest = txn.digest();

    // Test request with ExecuteTransactionRequestType::WaitForLocalExecution
    let (tx_bytes, signatures) = txn.to_tx_bytes_and_signatures();
    let params = rpc_params![
        tx_bytes,
        signatures,
        SuiTransactionBlockResponseOptions::new(),
        ExecuteTransactionRequestType::WaitForLocalExecution
    ];
    let response: SuiTransactionBlockResponse = jsonrpc_client
        .request("sui_executeTransactionBlock", params)
        .await
        .unwrap();

    let SuiTransactionBlockResponse {
        digest,
        confirmed_local_execution,
        ..
    } = response;
    assert_eq!(&digest, tx_digest);
    assert!(confirmed_local_execution.unwrap());

    let _response: SuiTransactionBlockResponse = jsonrpc_client
        .request("sui_getTransactionBlock", rpc_params![*tx_digest])
        .await
        .unwrap();

    // Test request with ExecuteTransactionRequestType::WaitForEffectsCert
    let (tx_bytes, signatures) = txn.to_tx_bytes_and_signatures();
    let params = rpc_params![
        tx_bytes,
        signatures,
        SuiTransactionBlockResponseOptions::new().with_effects(),
        ExecuteTransactionRequestType::WaitForEffectsCert
    ];
    let response: SuiTransactionBlockResponse = jsonrpc_client
        .request("sui_executeTransactionBlock", params)
        .await
        .unwrap();

    let SuiTransactionBlockResponse {
        effects,
        confirmed_local_execution,
        ..
    } = response;
    assert_eq!(effects.unwrap().transaction_digest(), tx_digest);
    assert!(!confirmed_local_execution.unwrap());

    Ok(())
}

#[tokio::test]
async fn test_traffic_control_rpc_spam_blocked() -> Result<(), anyhow::Error> {
    let policy_config = PolicyConfig {
        // TODO: Add some error codes
        tallyable_error_codes: vec![],
        connection_blocklist_ttl_sec: 1,
        proxy_blocklist_ttl_sec: 5,
        // Test that any 3 requests will cause an IP to be added to the blocklist.
        spam_policy_type: PolicyType::Test3ConnIP,
        error_policy_type: PolicyType::NoOp,
        channel_capacity: 100,
    };
    let network_config = ConfigBuilder::new_with_temp_dir()
        .with_traffic_control_config(Some(policy_config))
        .build();
    let mut test_cluster = TestClusterBuilder::new()
        .set_network_config(network_config)
        .build()
        .await;

    let context = &mut test_cluster.wallet;
    let jsonrpc_client = &test_cluster.fullnode_handle.rpc_client;

    let txn_count = 4;
    let mut txns = batch_make_transfer_transactions(context, txn_count).await;
    assert!(
        txns.len() >= txn_count,
        "Expect at least {} txns. Do we generate enough gas objects during genesis?",
        txn_count,
    );

    let txn = txns.swap_remove(0);
    let (tx_bytes, signatures) = txn.to_tx_bytes_and_signatures();
    let params = rpc_params![
        tx_bytes,
        signatures,
        SuiTransactionBlockResponseOptions::new(),
        ExecuteTransactionRequestType::WaitForLocalExecution
    ];

    // it should take no more than 4 requests to be added to the blocklist
    for _ in 0..3 {
        let response: RpcResult<SuiTransactionBlockResponse> = jsonrpc_client
            .request("sui_executeTransactionBlock", params.clone())
            .await;
        if let Err(err) = response {
            assert!(err.to_string().contains("Too many requests"));
            return Ok(());
        } else {
            response.unwrap();
        }
        // TODO: fix error handling such that the error message is not misleading. The
        // full error message currently is the following:
        // Transaction execution failed due to issues with transaction inputs, please
        // review the errors and try again: Too many requests.
    }
    panic!("Expected spam policy to trigger within 3 requests");
}
