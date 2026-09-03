// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! NB: Most tests in this module expect real network connections and interactions, thus
//! they should nearly all be tokio::test rather than simtest.

use core::panic;
use move_core_types::identifier::Identifier;
use std::fs::File;
use std::num::NonZeroUsize;
use std::time::Duration;
use sui_core::authority_client::AuthorityAPI;
use sui_core::authority_client::make_network_authority_clients_with_network_config;
use sui_core::traffic_controller::{
    TrafficController, TrafficSim, nodefw_test_server::NodeFwTestServer,
};
use sui_macros::sim_test;
use sui_network::default_mysten_network_config;
use sui_protocol_config::ProtocolConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_test_transaction_builder::batch_make_transfer_transactions;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas_coin::GAS;
use sui_types::traffic_control::TrafficControlReconfigParams;
use sui_types::{
    SUI_FRAMEWORK_PACKAGE_ID,
    crypto::Ed25519SuiSignature,
    messages_grpc::SubmitTxRequest,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    signature::GenericSignature,
    traffic_control::{
        FreqThresholdConfig, PolicyConfig, PolicyType, RemoteFirewallConfig, Weight,
    },
    transaction::{
        FundsWithdrawalArg, GasData, TransactionData, TransactionDataV1, TransactionExpiration,
        TransactionKind, add_gasless_token_for_testing,
    },
};
use test_cluster::{TestCluster, TestClusterBuilder};

#[tokio::test]
async fn test_validator_traffic_control_noop() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
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
async fn test_validator_traffic_control_ok() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
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
async fn test_validator_traffic_control_dry_run() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
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
async fn test_validator_traffic_control_error_blocked() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
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
        let response = auth_client
            .submit_transaction(SubmitTxRequest::new_transaction(tx.clone()), None)
            .await;
        if let Err(err) = response
            && err.to_string().contains("Too many requests")
        {
            return Ok(());
        }
    }
    panic!("Expected error policy to trigger within {n} requests");
}

#[tokio::test]
async fn test_validator_traffic_control_error_blocked_with_policy_reconfig()
-> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let n = 5;
    let policy_config = PolicyConfig {
        connection_blocklist_ttl_sec: 100,
        error_policy_type: PolicyType::TestNConnIP(n - 1),
        dry_run: true,
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

    // Before reconfiguring the policy, we should not block any requests due to dry run mode,
    // even after far exceeding the threshold. However the blocklist should be updated.
    for _ in 0..(2 * n) {
        let response = auth_client
            .submit_transaction(SubmitTxRequest::new_transaction(tx.clone()), None)
            .await;
        if let Err(err) = response {
            assert!(
                !err.to_string().contains("Too many requests"),
                "Expected no blocked requests due to dry run mode"
            );
        }
    }
    // Reconfigure traffic control to disable dry run mode
    for node in test_cluster.all_validator_handles() {
        node.state()
            .reconfigure_traffic_control(TrafficControlReconfigParams {
                error_threshold: None,
                spam_threshold: None,
                dry_run: Some(false),
            })
            .await
            .unwrap();
    }
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    // If Node and TrafficController has not crashed, blocklist and policy freq state should still
    // be intact. A single additional erroneous request from the client should trigger enforcement.
    let response = auth_client
        .submit_transaction(SubmitTxRequest::new_transaction(tx.clone()), None)
        .await;
    if let Err(err) = response
        && err.to_string().contains("Too many requests")
    {
        return Ok(());
    }
    panic!("Expected error policy to trigger on next requests after reconfiguration");
}

#[tokio::test]
async fn test_validator_traffic_control_error_delegated() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
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
        drain_path: tempfile::tempdir().unwrap().keep().join("drain"),
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
        let response = auth_client
            .submit_transaction(SubmitTxRequest::new_transaction(tx.clone()), None)
            .await;
        if let Err(err) = response
            && err.to_string().contains("Too many requests")
        {
            return Ok(());
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
async fn test_traffic_control_dead_mans_switch() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let policy_config = PolicyConfig {
        connection_blocklist_ttl_sec: 3,
        spam_policy_type: PolicyType::TestNConnIP(10),
        spam_sample_rate: Weight::one(),
        dry_run: false,
        ..Default::default()
    };

    // sink all traffic to trigger dead mans switch
    let drain_path = tempfile::tempdir().unwrap().keep().join("drain");
    assert!(!drain_path.exists(), "Expected drain file to not yet exist",);

    let firewall_config = RemoteFirewallConfig {
        remote_fw_url: String::from("http://127.0.0.1:65000"),
        delegate_spam_blocking: true,
        delegate_error_blocking: false,
        destination_port: 9000,
        drain_path: drain_path.clone(),
        drain_timeout_secs: 6,
    };

    let tc = TrafficController::init_for_test(policy_config.clone(), Some(firewall_config.clone()))
        .await;
    assert!(
        !drain_path.exists(),
        "Expected drain file to not exist after startup unless previously set",
    );

    // after n seconds with no traffic, the dead mans switch should be engaged
    let mut drain_enabled = false;
    for _ in 0..4 {
        if drain_path.exists() {
            drain_enabled = true;
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    }
    assert!(drain_enabled, "Expected drain file to be enabled");

    // if we drop traffic controller and re-instantiate, drain file should remain set
    drop(tc);
    let _tc = TrafficController::init_for_test(policy_config, Some(firewall_config)).await;
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
    telemetry_subscribers::init_for_testing();
    let drain_path = tempfile::tempdir().unwrap().keep().join("drain");
    assert!(!drain_path.exists(), "Expected drain file to not yet exist",);
    File::create(&drain_path).expect("Failed to touch nodefw drain file");
    assert!(drain_path.exists(), "Expected drain file to exist",);

    std::fs::remove_file(&drain_path).unwrap();
    Ok(())
}

#[sim_test]
async fn test_traffic_sketch_no_blocks() {
    telemetry_subscribers::init_for_testing();
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
    telemetry_subscribers::init_for_testing();
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
    telemetry_subscribers::init_for_testing();
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
    telemetry_subscribers::init_for_testing();
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

/// The effects returned by `execute_transaction` come from the validators; the
/// rpc fullnode only serves the transaction itself once it has executed the
/// checkpoint that includes it, so a read issued straight after execution can
/// still miss.
async fn wait_for_transaction_readable(test_cluster: &TestCluster, digest: &TransactionDigest) {
    test_cluster.wait_for_tx_settlement(&[*digest]).await;
}

async fn assert_traffic_control_ok(mut test_cluster: TestCluster) -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let context = &mut test_cluster.wallet;
    let mut grpc_client = test_cluster.fullnode_handle.grpc_client.clone();

    let txn_count = 4;
    let mut txns = batch_make_transfer_transactions(context, txn_count).await;
    assert!(
        txns.len() >= txn_count,
        "Expect at least {} txns. Do we generate enough gas objects during genesis?",
        txn_count,
    );

    let txn = txns.swap_remove(0);
    let tx_digest = txn.digest();

    let executed = grpc_client.execute_transaction(&txn).await.unwrap();
    assert_eq!(executed.effects.transaction_digest(), tx_digest);

    wait_for_transaction_readable(&test_cluster, tx_digest).await;
    let fetched = grpc_client.get_transaction(tx_digest).await.unwrap();
    assert_eq!(fetched.effects.transaction_digest(), tx_digest);

    // Executing the same transaction again returns its finalized effects.
    let executed = grpc_client.execute_transaction(&txn).await.unwrap();
    assert_eq!(executed.effects.transaction_digest(), tx_digest);

    // Use a different txn to avoid the case where the txn effects are already cached locally.
    let txn = txns.swap_remove(0);
    let tx_digest = txn.digest();

    let executed = grpc_client.execute_transaction(&txn).await.unwrap();
    assert_eq!(executed.effects.transaction_digest(), tx_digest);

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
    let mut grpc_client = test_cluster.fullnode_handle.grpc_client.clone();
    let mut txns = batch_make_transfer_transactions(context, txn_count).await;
    assert!(
        txns.len() >= txn_count,
        "Expect at least {} txns. Do we generate enough gas objects during genesis?",
        txn_count,
    );

    let txn = txns.swap_remove(0);
    let tx_digest = txn.digest();

    let executed = grpc_client.execute_transaction(&txn).await.unwrap();
    assert_eq!(executed.effects.transaction_digest(), tx_digest);

    wait_for_transaction_readable(&test_cluster, tx_digest).await;

    // it should take no more than 4 requests to be added to the blocklist
    for _ in 0..txn_count {
        let response = grpc_client.get_transaction(tx_digest).await;
        assert!(
            response.is_ok(),
            "Expected request to succeed in dry-run mode"
        );
    }
    Ok(())
}

#[tokio::test]
async fn test_validator_traffic_control_gasless_spam_blocked() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let n = 5;
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_gasless_for_testing();
        cfg
    });
    let policy_config = PolicyConfig {
        connection_blocklist_ttl_sec: 1,
        proxy_blocklist_ttl_sec: 5,
        spam_policy_type: PolicyType::TestNConnIP(n - 1),
        spam_sample_rate: Weight::one(),
        error_policy_type: PolicyType::TestPanicOnInvocation,
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

    let chain_id = test_cluster.get_chain_identifier();
    let sender = test_cluster.wallet.get_addresses()[0];
    let recipient = test_cluster.wallet.get_addresses()[1];

    // Register SUI as an allowed gasless token type for this test.
    add_gasless_token_for_testing(GAS::type_tag().to_canonical_string(true), 0);

    // Build a gasless transaction with a valid balance::send_funds MoveCall.
    let mut builder = ProgrammableTransactionBuilder::new();
    let withdraw_arg = FundsWithdrawalArg::balance_from_sender(1000, GAS::type_tag());
    let withdraw_arg = builder.funds_withdrawal(withdraw_arg).unwrap();
    let balance = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("redeem_funds").unwrap(),
        vec![GAS::type_tag()],
        vec![withdraw_arg],
    );
    let recipient_arg = builder.pure(recipient).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![GAS::type_tag()],
        vec![balance, recipient_arg],
    );
    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());
    let tx_data = TransactionData::V1(TransactionDataV1 {
        kind: tx_kind,
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: sender,
            price: 0,
            budget: 0,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: chain_id,
            nonce: 0,
        },
    });
    let tx = test_cluster.wallet.sign_transaction(&tx_data).await;

    for _ in 0..n {
        let response = auth_client
            .submit_transaction(SubmitTxRequest::new_transaction(tx.clone()), None)
            .await;
        if let Err(err) = response
            && err.to_string().contains("Too many requests")
        {
            return Ok(());
        }
    }
    panic!("Expected spam policy to trigger within {n} requests for gasless transactions");
}
