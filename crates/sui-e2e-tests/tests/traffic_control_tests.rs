// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! NB: Most tests in this module expect real network connections and interactions, thus
//! they should nearly all be tokio::test rather than simtest.

use core::panic;
use fastcrypto::encoding::Base64;
use jsonrpsee::{core::client::ClientT, rpc_params};
use std::fs::File;
use std::num::NonZeroUsize;
use std::time::Duration;
use sui_core::authority_client::make_network_authority_clients_with_network_config;
use sui_core::authority_client::AuthorityAPI;
use sui_core::traffic_controller::{
    nodefw_test_server::NodeFwTestServer, TrafficController, TrafficSim,
};
use sui_json_rpc_types::{
    SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_macros::sim_test;
use sui_network::default_mysten_network_config;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_test_transaction_builder::batch_make_transfer_transactions;
use sui_types::{
    crypto::Ed25519SuiSignature,
    quorum_driver_types::ExecuteTransactionRequestType,
    signature::GenericSignature,
    traffic_control::{
        FreqThresholdConfig, PolicyConfig, PolicyType, RemoteFirewallConfig, Weight,
    },
};
use test_cluster::{TestCluster, TestClusterBuilder};

#[tokio::test]
async fn test_validator_traffic_control_noop() -> Result<(), anyhow::Error> {
    let policy_config = PolicyConfig {
        connection_blocklist_ttl_sec: 1,
        proxy_blocklist_ttl_sec: 5,
        // This should never be invoked when set as an error policy
        // as we are not sending requests that error
        error_policy_type: PolicyType::TestPanicOnInvocation,
        dry_run: false,
        spam_sample_rate: Weight::one(),
        ..Default::default()
    };
    let network_config = ConfigBuilder::new_with_temp_dir()
        .committee_size(NonZeroUsize::new(4).unwrap())
        .with_policy_config(Some(policy_config))
        .build();
    let test_cluster = TestClusterBuilder::new()
        .set_network_config(network_config)
        .build()
        .await;

    assert_traffic_control_ok(test_cluster).await
}

#[tokio::test]
async fn test_fullnode_traffic_control_noop() -> Result<(), anyhow::Error> {
    let policy_config = PolicyConfig {
        connection_blocklist_ttl_sec: 1,
        proxy_blocklist_ttl_sec: 5,
        // This should never be invoked when set as an error policy
        // as we are not sending requests that error
        error_policy_type: PolicyType::TestPanicOnInvocation,
        spam_sample_rate: Weight::one(),
        dry_run: false,
        ..Default::default()
    };
    let test_cluster = TestClusterBuilder::new()
        .with_fullnode_policy_config(Some(policy_config))
        .build()
        .await;
    assert_traffic_control_ok(test_cluster).await
}

#[tokio::test]
async fn test_validator_traffic_control_ok() -> Result<(), anyhow::Error> {
    let policy_config = PolicyConfig {
        connection_blocklist_ttl_sec: 1,
        proxy_blocklist_ttl_sec: 5,
        spam_policy_type: PolicyType::TestNConnIP(5),
        // This should never be invoked when set as an error policy
        // as we are not sending requests that error
        error_policy_type: PolicyType::TestPanicOnInvocation,
        dry_run: false,
        spam_sample_rate: Weight::one(),
        ..Default::default()
    };
    let network_config = ConfigBuilder::new_with_temp_dir()
        .committee_size(NonZeroUsize::new(4).unwrap())
        .with_policy_config(Some(policy_config))
        .build();
    let test_cluster = TestClusterBuilder::new()
        .set_network_config(network_config)
        .build()
        .await;

    assert_traffic_control_ok(test_cluster).await
}

#[tokio::test]
async fn test_fullnode_traffic_control_ok() -> Result<(), anyhow::Error> {
    let policy_config = PolicyConfig {
        connection_blocklist_ttl_sec: 1,
        proxy_blocklist_ttl_sec: 5,
        spam_policy_type: PolicyType::TestNConnIP(10),
        // This should never be invoked when set as an error policy
        // as we are not sending requests that error
        error_policy_type: PolicyType::TestPanicOnInvocation,
        spam_sample_rate: Weight::one(),
        dry_run: false,
        ..Default::default()
    };
    let test_cluster = TestClusterBuilder::new()
        .with_fullnode_policy_config(Some(policy_config))
        .build()
        .await;
    assert_traffic_control_ok(test_cluster).await
}

#[tokio::test]
async fn test_validator_traffic_control_dry_run() -> Result<(), anyhow::Error> {
    let n = 5;
    let policy_config = PolicyConfig {
        connection_blocklist_ttl_sec: 1,
        proxy_blocklist_ttl_sec: 5,
        spam_policy_type: PolicyType::TestNConnIP(n - 1),
        spam_sample_rate: Weight::one(),
        // This should never be invoked when set as an error policy
        // as we are not sending requests that error
        error_policy_type: PolicyType::TestPanicOnInvocation,
        dry_run: true,
        ..Default::default()
    };
    let network_config = ConfigBuilder::new_with_temp_dir()
        .committee_size(NonZeroUsize::new(4).unwrap())
        .with_policy_config(Some(policy_config))
        .build();
    let test_cluster = TestClusterBuilder::new()
        .set_network_config(network_config)
        .build()
        .await;

    assert_validator_traffic_control_dry_run(test_cluster, n as usize).await
}

#[tokio::test]
async fn test_fullnode_traffic_control_dry_run() -> Result<(), anyhow::Error> {
    let txn_count = 15;
    let policy_config = PolicyConfig {
        connection_blocklist_ttl_sec: 1,
        proxy_blocklist_ttl_sec: 5,
        spam_policy_type: PolicyType::TestNConnIP(txn_count - 1),
        spam_sample_rate: Weight::one(),
        // This should never be invoked when set as an error policy
        // as we are not sending requests that error
        error_policy_type: PolicyType::TestPanicOnInvocation,
        dry_run: true,
        ..Default::default()
    };
    let test_cluster = TestClusterBuilder::new()
        .with_fullnode_policy_config(Some(policy_config))
        .build()
        .await;

    let context = test_cluster.wallet;
    let jsonrpc_client = &test_cluster.fullnode_handle.rpc_client;
    let mut txns = batch_make_transfer_transactions(&context, txn_count as usize).await;
    assert!(
        txns.len() >= txn_count as usize,
        "Expect at least {} txns. Do we generate enough gas objects during genesis?",
        txn_count,
    );

    let txn = txns.swap_remove(0);
    let tx_digest = txn.digest();
    let (tx_bytes, signatures) = txn.to_tx_bytes_and_signatures();
    let params = rpc_params![
        tx_bytes,
        signatures,
        SuiTransactionBlockResponseOptions::new(),
        ExecuteTransactionRequestType::WaitForLocalExecution
    ];

    let response: SuiTransactionBlockResponse = jsonrpc_client
        .request("sui_executeTransactionBlock", params.clone())
        .await
        .unwrap();
    let SuiTransactionBlockResponse {
        digest,
        confirmed_local_execution,
        ..
    } = response;
    assert_eq!(&digest, tx_digest);
    assert!(confirmed_local_execution.unwrap());

    // it should take no more than 4 requests to be added to the blocklist
    for _ in 0..txn_count {
        let response: Result<SuiTransactionBlockResponse, _> = jsonrpc_client
            .request("sui_getTransactionBlock", rpc_params![*tx_digest])
            .await;
        assert!(
            response.is_ok(),
            "Expected request to succeed in dry-run mode"
        );
    }
    Ok(())
}

#[tokio::test]
async fn test_validator_traffic_control_error_blocked() -> Result<(), anyhow::Error> {
    let n = 5;
    let policy_config = PolicyConfig {
        connection_blocklist_ttl_sec: 1,
        // Test that any N requests will cause an IP to be added to the blocklist.
        error_policy_type: PolicyType::TestNConnIP(n - 1),
        dry_run: false,
        ..Default::default()
    };
    let network_config = ConfigBuilder::new_with_temp_dir()
        .committee_size(NonZeroUsize::new(4).unwrap())
        .with_policy_config(Some(policy_config))
        .build();
    let committee = network_config.committee_with_network();
    let test_cluster = TestClusterBuilder::new()
        .set_network_config(network_config)
        .build()
        .await;
    let local_clients = make_network_authority_clients_with_network_config(
        &committee,
        &default_mysten_network_config(),
    );
    let (_, auth_client) = local_clients.first_key_value().unwrap();

    let mut txns = batch_make_transfer_transactions(&test_cluster.wallet, n as usize).await;
    let mut tx = txns.swap_remove(0);
    let signatures = tx.tx_signatures_mut_for_testing();
    signatures.pop();
    signatures.push(GenericSignature::Signature(
        sui_types::crypto::Signature::Ed25519SuiSignature(Ed25519SuiSignature::default()),
    ));

    // it should take no more than 4 requests to be added to the blocklist
    for _ in 0..n {
        let response = auth_client.handle_transaction(tx.clone(), None).await;
        if let Err(err) = response {
            if err.to_string().contains("Too many requests") {
                return Ok(());
            }
        }
    }
    panic!("Expected error policy to trigger within {n} requests");
}

#[tokio::test]
async fn test_fullnode_traffic_control_spam_blocked() -> Result<(), anyhow::Error> {
    let txn_count = 15;
    let policy_config = PolicyConfig {
        connection_blocklist_ttl_sec: 3,
        // Test that any N requests will cause an IP to be added to the blocklist.
        spam_policy_type: PolicyType::TestNConnIP(txn_count - 1),
        spam_sample_rate: Weight::one(),
        dry_run: false,
        ..Default::default()
    };
    let test_cluster = TestClusterBuilder::new()
        .with_fullnode_policy_config(Some(policy_config))
        .build()
        .await;

    let context = test_cluster.wallet;
    let jsonrpc_client = &test_cluster.fullnode_handle.rpc_client;

    let mut txns = batch_make_transfer_transactions(&context, txn_count as usize).await;
    assert!(
        txns.len() >= txn_count as usize,
        "Expect at least {} txns. Do we generate enough gas objects during genesis?",
        txn_count,
    );

    let txn = txns.swap_remove(0);
    let tx_digest = txn.digest();
    let (tx_bytes, signatures) = txn.to_tx_bytes_and_signatures();
    let params = rpc_params![
        tx_bytes,
        signatures,
        SuiTransactionBlockResponseOptions::new(),
        ExecuteTransactionRequestType::WaitForLocalExecution
    ];

    let response: SuiTransactionBlockResponse = jsonrpc_client
        .request("sui_executeTransactionBlock", params.clone())
        .await
        .unwrap();
    let SuiTransactionBlockResponse {
        digest,
        confirmed_local_execution,
        ..
    } = response;
    assert_eq!(&digest, tx_digest);
    assert!(confirmed_local_execution.unwrap());

    // it should take no more than 4 requests to be added to the blocklist
    for _ in 0..txn_count {
        let response: Result<SuiTransactionBlockResponse, _> = jsonrpc_client
            .request("sui_getTransactionBlock", rpc_params![*tx_digest])
            .await;
        if let Err(err) = response {
            // TODO: fix validator blocking error handling such that the error message
            // is not misleading. The full error message currently is the following:
            //  Transaction execution failed due to issues with transaction inputs, please
            //  review the errors and try again: Too many requests.
            assert!(
                err.to_string().contains("Too many requests"),
                "Error not due to spam policy"
            );
            return Ok(());
        }
    }
    panic!("Expected spam policy to trigger within {txn_count} requests");
}

#[tokio::test]
async fn test_fullnode_traffic_control_error_blocked() -> Result<(), anyhow::Error> {
    let txn_count = 5;
    let policy_config = PolicyConfig {
        connection_blocklist_ttl_sec: 3,
        error_policy_type: PolicyType::TestNConnIP(txn_count - 1),
        dry_run: false,
        ..Default::default()
    };
    let test_cluster = TestClusterBuilder::new()
        .with_fullnode_policy_config(Some(policy_config))
        .build()
        .await;

    let jsonrpc_client = &test_cluster.fullnode_handle.rpc_client;
    let context = test_cluster.wallet;

    let mut txns = batch_make_transfer_transactions(&context, txn_count as usize).await;
    assert!(
        txns.len() >= txn_count as usize,
        "Expect at least {} txns. Do we generate enough gas objects during genesis?",
        txn_count,
    );

    // it should take no more than 4 requests to be added to the blocklist
    for _ in 0..txn_count {
        let txn = txns.swap_remove(0);
        let tx_digest = txn.digest();
        let (tx_bytes, _signatures) = txn.to_tx_bytes_and_signatures();
        // create invalid (empty) client signature
        let signatures: Vec<Base64> = vec![];
        let params = rpc_params![
            tx_bytes,
            signatures,
            SuiTransactionBlockResponseOptions::new(),
            ExecuteTransactionRequestType::WaitForLocalExecution
        ];
        let response: Result<SuiTransactionBlockResponse, _> = jsonrpc_client
            .request("sui_executeTransactionBlock", params.clone())
            .await;
        if let Err(err) = response {
            if err.to_string().contains("Too many requests") {
                return Ok(());
            }
        } else {
            let SuiTransactionBlockResponse {
                digest,
                confirmed_local_execution,
                ..
            } = response.unwrap();
            assert_eq!(&digest, tx_digest);
            assert!(confirmed_local_execution.unwrap());
        }
    }
    panic!("Expected spam policy to trigger within {txn_count} requests");
}

#[tokio::test]
async fn test_validator_traffic_control_error_delegated() -> Result<(), anyhow::Error> {
    let n = 5;
    let port = 65000;
    let policy_config = PolicyConfig {
        connection_blocklist_ttl_sec: 120,
        proxy_blocklist_ttl_sec: 120,
        // Test that any N - 1 requests will cause an IP to be added to the blocklist.
        error_policy_type: PolicyType::TestNConnIP(n - 1),
        dry_run: false,
        ..Default::default()
    };
    // enable remote firewall delegation
    let firewall_config = RemoteFirewallConfig {
        remote_fw_url: format!("http://127.0.0.1:{}", port),
        delegate_spam_blocking: true,
        delegate_error_blocking: false,
        destination_port: 8080,
        drain_path: tempfile::tempdir().unwrap().into_path().join("drain"),
        drain_timeout_secs: 10,
    };
    let network_config = ConfigBuilder::new_with_temp_dir()
        .committee_size(NonZeroUsize::new(4).unwrap())
        .with_policy_config(Some(policy_config))
        .with_firewall_config(Some(firewall_config))
        .build();
    let committee = network_config.committee_with_network();
    let test_cluster = TestClusterBuilder::new()
        .set_network_config(network_config)
        .build()
        .await;
    let local_clients = make_network_authority_clients_with_network_config(
        &committee,
        &default_mysten_network_config(),
    );
    let (_, auth_client) = local_clients.first_key_value().unwrap();

    let mut txns = batch_make_transfer_transactions(&test_cluster.wallet, n as usize).await;
    let mut tx = txns.swap_remove(0);
    let signatures = tx.tx_signatures_mut_for_testing();
    signatures.pop();
    signatures.push(GenericSignature::Signature(
        sui_types::crypto::Signature::Ed25519SuiSignature(Ed25519SuiSignature::default()),
    ));

    // start test firewall server
    let mut server = NodeFwTestServer::new();
    server.start(port).await;
    // await for the server to start
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // it should take no more than 4 requests to be added to the blocklist
    for _ in 0..n {
        let response = auth_client.handle_transaction(tx.clone(), None).await;
        if let Err(err) = response {
            if err.to_string().contains("Too many requests") {
                return Ok(());
            }
        }
    }
    let fw_blocklist = server.list_addresses_rpc().await;
    assert!(
        !fw_blocklist.is_empty(),
        "Expected blocklist to be non-empty"
    );
    server.stop().await;
    Ok(())
}

#[tokio::test]
async fn test_fullnode_traffic_control_spam_delegated() -> Result<(), anyhow::Error> {
    let txn_count = 10;
    let port = 65001;
    let policy_config = PolicyConfig {
        connection_blocklist_ttl_sec: 120,
        proxy_blocklist_ttl_sec: 120,
        // Test that any N - 1 requests will cause an IP to be added to the blocklist.
        spam_policy_type: PolicyType::TestNConnIP(txn_count - 1),
        spam_sample_rate: Weight::one(),
        dry_run: false,
        ..Default::default()
    };
    // enable remote firewall delegation
    let firewall_config = RemoteFirewallConfig {
        remote_fw_url: format!("http://127.0.0.1:{}", port),
        delegate_spam_blocking: true,
        delegate_error_blocking: false,
        destination_port: 9000,
        drain_path: tempfile::tempdir().unwrap().into_path().join("drain"),
        drain_timeout_secs: 10,
    };
    let test_cluster = TestClusterBuilder::new()
        .with_fullnode_policy_config(Some(policy_config))
        .with_fullnode_fw_config(Some(firewall_config.clone()))
        .build()
        .await;

    // start test firewall server
    let mut server = NodeFwTestServer::new();
    server.start(port).await;
    // await for the server to start
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    let context = test_cluster.wallet;
    let jsonrpc_client = &test_cluster.fullnode_handle.rpc_client;
    let mut txns = batch_make_transfer_transactions(&context, txn_count as usize).await;
    assert!(
        txns.len() >= txn_count as usize,
        "Expect at least {} txns. Do we generate enough gas objects during genesis?",
        txn_count,
    );

    let txn = txns.swap_remove(0);
    let tx_digest = txn.digest();
    let (tx_bytes, signatures) = txn.to_tx_bytes_and_signatures();
    let params = rpc_params![
        tx_bytes,
        signatures,
        SuiTransactionBlockResponseOptions::new(),
        ExecuteTransactionRequestType::WaitForLocalExecution
    ];

    // it should take no more than 4 requests to be added to the blocklist
    let response: SuiTransactionBlockResponse = jsonrpc_client
        .request("sui_executeTransactionBlock", params.clone())
        .await
        .unwrap();
    let SuiTransactionBlockResponse {
        digest,
        confirmed_local_execution,
        ..
    } = response;
    assert_eq!(&digest, tx_digest);
    assert!(confirmed_local_execution.unwrap());

    for _ in 0..txn_count {
        let response: Result<SuiTransactionBlockResponse, _> = jsonrpc_client
            .request("sui_getTransactionBlock", rpc_params![*tx_digest])
            .await;
        assert!(response.is_ok(), "Expected request to succeed");
    }
    let fw_blocklist = server.list_addresses_rpc().await;
    assert!(
        !fw_blocklist.is_empty(),
        "Expected blocklist to be non-empty"
    );
    server.stop().await;
    Ok(())
}

#[tokio::test]
async fn test_traffic_control_dead_mans_switch() -> Result<(), anyhow::Error> {
    let policy_config = PolicyConfig {
        connection_blocklist_ttl_sec: 3,
        spam_policy_type: PolicyType::TestNConnIP(10),
        spam_sample_rate: Weight::one(),
        dry_run: false,
        ..Default::default()
    };

    // sink all traffic to trigger dead mans switch
    let drain_path = tempfile::tempdir().unwrap().into_path().join("drain");
    assert!(!drain_path.exists(), "Expected drain file to not yet exist",);

    let firewall_config = RemoteFirewallConfig {
        remote_fw_url: String::from("http://127.0.0.1:65000"),
        delegate_spam_blocking: true,
        delegate_error_blocking: false,
        destination_port: 9000,
        drain_path: drain_path.clone(),
        drain_timeout_secs: 10,
    };

    // NOTE: we need to hold onto this tc handle to ensure we don't inadvertently close
    // the receive channel (this would cause traffic controller to exit the loop and thus
    // we will never engage the dead mans switch)
    let _tc = TrafficController::init_for_test(policy_config, Some(firewall_config));
    assert!(
        !drain_path.exists(),
        "Expected drain file to not exist after startup unless previously set",
    );

    // after n seconds with no traffic, the dead mans switch should be engaged
    let mut drain_enabled = false;
    for _ in 0..10 {
        if drain_path.exists() {
            drain_enabled = true;
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    }
    assert!(drain_enabled, "Expected drain file to be enabled");

    // if we drop traffic controller and re-instantiate, drain file should remain set
    for _ in 0..3 {
        assert!(
            drain_path.exists(),
            "Expected drain file to be disabled at startup unless previously enabled",
        );
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    }

    std::fs::remove_file(&drain_path).unwrap();
    Ok(())
}

#[tokio::test]
async fn test_traffic_control_manual_set_dead_mans_switch() -> Result<(), anyhow::Error> {
    let drain_path = tempfile::tempdir().unwrap().into_path().join("drain");
    assert!(!drain_path.exists(), "Expected drain file to not yet exist",);
    File::create(&drain_path).expect("Failed to touch nodefw drain file");
    assert!(drain_path.exists(), "Expected drain file to exist",);

    std::fs::remove_file(&drain_path).unwrap();
    Ok(())
}

#[sim_test]
async fn test_traffic_sketch_no_blocks() {
    let sketch_config = FreqThresholdConfig {
        client_threshold: 10_100,
        proxied_client_threshold: 10_100,
        window_size_secs: 4,
        update_interval_secs: 1,
        ..Default::default()
    };
    let policy = PolicyConfig {
        connection_blocklist_ttl_sec: 1,
        proxy_blocklist_ttl_sec: 1,
        spam_policy_type: PolicyType::NoOp,
        error_policy_type: PolicyType::FreqThreshold(sketch_config),
        channel_capacity: 100,
        dry_run: false,
        ..Default::default()
    };
    let metrics = TrafficSim::run(
        policy,
        10,     // num_clients
        10_000, // per_client_tps
        Duration::from_secs(20),
        true, // report
    )
    .await;

    let expected_requests = 10_000 * 10 * 20;
    assert!(metrics.num_blocked < 10_010);
    assert!(metrics.num_requests > expected_requests - 1_000);
    assert!(metrics.num_requests < expected_requests + 200);
    assert!(metrics.num_blocklist_adds <= 1);
    if let Some(first_block) = metrics.abs_time_to_first_block {
        assert!(first_block > Duration::from_secs(2));
    }
    assert!(metrics.num_blocklist_adds < 10);
    assert!(metrics.total_time_blocked < Duration::from_secs(10));
}

#[ignore]
#[sim_test]
async fn test_traffic_sketch_with_slow_blocks() {
    let sketch_config = FreqThresholdConfig {
        client_threshold: 9_900,
        proxied_client_threshold: 9_900,
        window_size_secs: 4,
        update_interval_secs: 1,
        ..Default::default()
    };
    let policy = PolicyConfig {
        connection_blocklist_ttl_sec: 1,
        proxy_blocklist_ttl_sec: 1,
        spam_policy_type: PolicyType::NoOp,
        error_policy_type: PolicyType::FreqThreshold(sketch_config),
        channel_capacity: 100,
        dry_run: false,
        ..Default::default()
    };
    let metrics = TrafficSim::run(
        policy,
        10,     // num_clients
        10_000, // per_client_tps
        Duration::from_secs(20),
        true, // report
    )
    .await;

    let expected_requests = 10_000 * 10 * 20;
    assert!(metrics.num_requests > expected_requests - 1_000);
    assert!(metrics.num_requests < expected_requests + 200);
    // due to averaging, we will take 4 seconds to start blocking, then
    // will be in blocklist for 1 second (roughly)
    assert!(metrics.num_blocked as f64 > (expected_requests as f64 / 4.0) * 0.90);
    // 10 clients, blocked at least every 5 seconds, over 20 seconds
    assert!(metrics.num_blocklist_adds >= 40);
    assert!(metrics.abs_time_to_first_block.unwrap() < Duration::from_secs(5));
    assert!(metrics.total_time_blocked > Duration::from_millis(3500));
}

#[sim_test]
async fn test_traffic_sketch_with_sampled_spam() {
    let sketch_config = FreqThresholdConfig {
        client_threshold: 4_500,
        proxied_client_threshold: 4_500,
        window_size_secs: 4,
        update_interval_secs: 1,
        ..Default::default()
    };
    let policy = PolicyConfig {
        connection_blocklist_ttl_sec: 1,
        proxy_blocklist_ttl_sec: 1,
        spam_policy_type: PolicyType::FreqThreshold(sketch_config),
        spam_sample_rate: Weight::new(0.5).unwrap(),
        dry_run: false,
        ..Default::default()
    };
    let metrics = TrafficSim::run(
        policy,
        1,      // num_clients
        10_000, // per_client_tps
        Duration::from_secs(20),
        true, // report
    )
    .await;

    let expected_requests = 10_000 * 20;
    assert!(metrics.num_requests > expected_requests - 1_000);
    assert!(metrics.num_requests < expected_requests + 200);
    // number of blocked requests should be nearly the same
    // as before, as we have half the single client TPS,
    // but the threshould is also halved. However, divide by
    // 5 instead of 4 as a buffer due in case we're unlucky with
    // the sampling
    assert!(metrics.num_blocked > (expected_requests / 5) - 1000);
}

#[sim_test]
async fn test_traffic_sketch_allowlist_mode() {
    let policy_config = PolicyConfig {
        connection_blocklist_ttl_sec: 1,
        proxy_blocklist_ttl_sec: 1,
        // first two clients allowlisted, rest blocked
        allow_list: Some(vec![String::from("127.0.0.0"), String::from("127.0.0.1")]),
        dry_run: false,
        ..Default::default()
    };
    let metrics = TrafficSim::run(
        policy_config,
        4,      // num_clients
        10_000, // per_client_tps
        Duration::from_secs(10),
        true, // report
    )
    .await;

    let expected_requests = 10_000 * 10 * 4;
    // ~half of all requests blocked
    assert!(metrics.num_blocked >= expected_requests / 2 - 1000);
    assert!(metrics.num_requests > expected_requests - 1_000);
    assert!(metrics.num_requests < expected_requests + 200);
}

async fn assert_traffic_control_ok(mut test_cluster: TestCluster) -> Result<(), anyhow::Error> {
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

/// Test that in dry-run mode, actions that would otherwise
/// lead to request blocking (in this case, a spammy client)
/// are allowed to proceed.
async fn assert_validator_traffic_control_dry_run(
    mut test_cluster: TestCluster,
    txn_count: usize,
) -> Result<(), anyhow::Error> {
    let context = &mut test_cluster.wallet;
    let jsonrpc_client = &test_cluster.fullnode_handle.rpc_client;
    let mut txns = batch_make_transfer_transactions(context, txn_count).await;
    assert!(
        txns.len() >= txn_count,
        "Expect at least {} txns. Do we generate enough gas objects during genesis?",
        txn_count,
    );

    let txn = txns.swap_remove(0);
    let tx_digest = txn.digest();
    let (tx_bytes, signatures) = txn.to_tx_bytes_and_signatures();
    let params = rpc_params![
        tx_bytes,
        signatures,
        SuiTransactionBlockResponseOptions::new(),
        ExecuteTransactionRequestType::WaitForLocalExecution
    ];

    let response: SuiTransactionBlockResponse = jsonrpc_client
        .request("sui_executeTransactionBlock", params.clone())
        .await
        .unwrap();
    let SuiTransactionBlockResponse {
        digest,
        confirmed_local_execution,
        ..
    } = response;
    assert_eq!(&digest, tx_digest);
    assert!(confirmed_local_execution.unwrap());

    // it should take no more than 4 requests to be added to the blocklist
    for _ in 0..txn_count {
        let response: Result<SuiTransactionBlockResponse, _> = jsonrpc_client
            .request("sui_getTransactionBlock", rpc_params![*tx_digest])
            .await;
        assert!(
            response.is_ok(),
            "Expected request to succeed in dry-run mode"
        );
    }
    Ok(())
}
