// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::join_all;
use prometheus::Registry;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sui_core::authority::AuthorityState;
use sui_core::authority_aggregator::{AuthAggMetrics, AuthorityAggregator};
use sui_core::consensus_adapter::position_submit_certificate;
use sui_core::safe_client::SafeClientMetricsBase;
use sui_core::test_utils::make_transfer_sui_transaction;
use sui_macros::sim_test;
use sui_node::SuiNodeHandle;
use sui_types::crypto::get_account_key_pair;
use sui_types::gas::GasCostSummary;
use sui_types::message_envelope::Message;
use sui_types::object::Object;
use test_utils::{
    authority::{spawn_test_authorities, test_authority_configs},
    network::TestClusterBuilder,
};
use tokio::time::{sleep, timeout};
use tracing::info;

#[sim_test]
async fn advance_epoch_tx_test() {
    // Same as local_advance_epoch_tx_test, but uses network clients.
    let authorities = spawn_test_authorities([].into_iter(), &test_authority_configs()).await;
    let states: Vec<_> = authorities
        .iter()
        .map(|authority| authority.with(|node| node.state()))
        .collect();
    advance_epoch_tx_test_impl(states).await;
}

async fn advance_epoch_tx_test_impl(states: Vec<Arc<AuthorityState>>) {
    let tasks: Vec<_> = states
        .iter()
        .map(|state| async {
            let (_system_state, effects) = state
                .create_and_execute_advance_epoch_tx(
                    &state.epoch_store_for_testing(),
                    &GasCostSummary::new(0, 0, 0),
                    0, // checkpoint
                    0, // epoch_start_timestamp_ms
                )
                .await
                .unwrap();
            // Check that the validator didn't commit the transaction yet.
            assert!(state
                .get_signed_effects_and_maybe_resign(
                    &effects.transaction_digest,
                    &state.epoch_store_for_testing()
                )
                .unwrap()
                .is_none());
            effects
        })
        .collect();
    let results: HashSet<_> = futures::future::join_all(tasks)
        .await
        .into_iter()
        .map(|result| result.digest())
        .collect();
    // Check that all validators have the same result.
    assert_eq!(results.len(), 1);
}

#[sim_test]
async fn basic_reconfig_end_to_end_test() {
    // TODO remove this sleep when this test passes consistently
    sleep(Duration::from_secs(1)).await;
    let authorities = spawn_test_authorities([].into_iter(), &test_authority_configs()).await;
    trigger_reconfiguration(&authorities).await;
}

#[sim_test]
async fn reconfig_with_revert_end_to_end_test() {
    let (sender, keypair) = get_account_key_pair();
    let gas1 = Object::with_owner_for_testing(sender); // committed
    let gas2 = Object::with_owner_for_testing(sender); // (most likely) reverted
    let authorities = spawn_test_authorities(
        [gas1.clone(), gas2.clone()].into_iter(),
        &test_authority_configs(),
    )
    .await;
    let registry = Registry::new();

    // gas1 transaction is committed
    let tx = make_transfer_sui_transaction(
        gas1.compute_object_reference(),
        sender,
        None,
        sender,
        &keypair,
        None,
    );
    let net = AuthorityAggregator::new_from_local_system_state(
        &authorities[0].with(|node| node.state().db()),
        &authorities[0].with(|node| node.state().committee_store().clone()),
        SafeClientMetricsBase::new(&registry),
        AuthAggMetrics::new(&registry),
    )
    .unwrap();
    let cert = net
        .process_transaction(tx.clone())
        .await
        .unwrap()
        .into_cert_for_testing();
    let effects1 = net
        .process_certificate(cert.clone().into_inner())
        .await
        .unwrap();
    assert_eq!(0, effects1.epoch());

    // gas2 transaction is (most likely) reverted
    let tx = make_transfer_sui_transaction(
        gas2.compute_object_reference(),
        sender,
        None,
        sender,
        &keypair,
        None,
    );
    let cert = net
        .process_transaction(tx.clone())
        .await
        .unwrap()
        .into_cert_for_testing();

    // Close epoch on 3 (2f+1) validators.
    let mut reverting_authority_idx = None;
    for (i, handle) in authorities.iter().enumerate() {
        handle
            .with_async(|node| async {
                if position_submit_certificate(&net.committee, &node.state().name, tx.digest())
                    < (authorities.len() - 1)
                {
                    node.close_epoch_for_testing().await.unwrap();
                } else {
                    // remember the authority that wouild submit it to consensus last.
                    reverting_authority_idx = Some(i);
                }
            })
            .await;
    }

    let reverting_authority_idx = reverting_authority_idx.unwrap();
    let client = net
        .get_client(&authorities[reverting_authority_idx].with(|node| node.state().name))
        .unwrap();
    client
        .handle_certificate(cert.clone().into_inner())
        .await
        .unwrap();

    authorities[reverting_authority_idx]
        .with_async(|node| async {
            let object = node
                .state()
                .get_objects(&[gas2.id()])
                .await
                .unwrap()
                .into_iter()
                .next()
                .unwrap()
                .unwrap();
            // verify that authority 0 advanced object version
            assert_eq!(2, object.version().value());
        })
        .await;

    // Wait for all nodes to reach the next epoch.
    let handles: Vec<_> = authorities
        .iter()
        .map(|handle| {
            handle.with_async(|node| async {
                loop {
                    if node.state().current_epoch_for_testing() == 1 {
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            })
        })
        .collect();
    join_all(handles).await;

    let mut epoch = None;
    for handle in authorities.iter() {
        handle
            .with_async(|node| async {
                let object = node
                    .state()
                    .get_objects(&[gas1.id()])
                    .await
                    .unwrap()
                    .into_iter()
                    .next()
                    .unwrap()
                    .unwrap();
                assert_eq!(2, object.version().value());
                // Due to race conditions, it's possible that tx2 went in
                // before 2f+1 validators sent EndOfPublish messages and close
                // the curtain of epoch 0. So, we are asserting that
                // the object version is either 1 or 2, but needs to be
                // consistent in all validators.
                // Note that previously test checked that object version == 2 on authority 0
                let object = node
                    .state()
                    .get_objects(&[gas2.id()])
                    .await
                    .unwrap()
                    .into_iter()
                    .next()
                    .unwrap()
                    .unwrap();
                let object_version = object.version().value();
                if epoch.is_none() {
                    assert!(object_version == 1 || object_version == 2);
                    epoch.replace(object_version);
                } else {
                    assert_eq!(epoch, Some(object_version));
                }
            })
            .await;
    }
}

// This test just starts up a cluster that reconfigures itself under 0 load.
#[sim_test]
async fn test_passive_reconfig() {
    telemetry_subscribers::init_for_testing();
    sui_protocol_config::ProtocolConfig::poison_get_for_min_version();

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(1000)
        .build()
        .await
        .unwrap();

    let mut epoch_rx = test_cluster
        .fullnode_handle
        .sui_node
        .subscribe_to_epoch_change();

    let target_epoch: u64 = std::env::var("RECONFIG_TARGET_EPOCH")
        .ok()
        .map(|v| v.parse().unwrap())
        .unwrap_or(4);

    timeout(Duration::from_secs(60), async move {
        while let Ok(committee) = epoch_rx.recv().await {
            info!("received epoch {}", committee.epoch);
            if committee.epoch >= target_epoch {
                break;
            }
        }
    })
    .await
    .expect("Timed out waiting for cluster to target epoch");
}

#[sim_test]
async fn test_validator_resign_effects() {
    // This test checks that validators are able to re-sign transaction effects that were finalized
    // in previous epochs. This allows authority aggregator to form a new effects certificate
    // in the new epoch.
    let (sender, keypair) = get_account_key_pair();
    let gas = Object::with_owner_for_testing(sender);
    let configs = test_authority_configs();
    let authorities = spawn_test_authorities([gas.clone()].into_iter(), &configs).await;
    let tx = make_transfer_sui_transaction(
        gas.compute_object_reference(),
        sender,
        None,
        sender,
        &keypair,
        None,
    );
    let registry = Registry::new();
    let mut net = AuthorityAggregator::new_from_local_system_state(
        &authorities[0].with(|node| node.state().db()),
        &authorities[0].with(|node| node.state().committee_store().clone()),
        SafeClientMetricsBase::new(&registry),
        AuthAggMetrics::new(&registry),
    )
    .unwrap();
    let cert = net
        .process_transaction(tx.clone())
        .await
        .unwrap()
        .into_cert_for_testing();
    let effects0 = net
        .process_certificate(cert.clone().into_inner())
        .await
        .unwrap();
    assert_eq!(effects0.epoch(), 0);
    // Give it enough time for the transaction to be checkpointed and hence finalized.
    sleep(Duration::from_secs(10)).await;
    trigger_reconfiguration(&authorities).await;
    // Manually reconfigure the aggregator.
    net.committee.epoch = 1;
    let effects1 = net.process_certificate(cert.into_inner()).await.unwrap();
    // Ensure that we are able to form a new effects cert in the new epoch.
    assert_eq!(effects1.epoch(), 1);
    assert_eq!(effects0.into_message(), effects1.into_message());
}

async fn trigger_reconfiguration(authorities: &[SuiNodeHandle]) {
    info!("Starting reconfiguration");
    let start = Instant::now();

    // Close epoch on 3 (2f+1) validators.
    for handle in authorities.iter().skip(1) {
        handle
            .with_async(|node| async { node.close_epoch_for_testing().await.unwrap() })
            .await;
    }
    info!("close_epoch complete after {:?}", start.elapsed());

    // Wait for all nodes to reach the next epoch.
    let handles: Vec<_> = authorities
        .iter()
        .map(|handle| {
            handle.with_async(|node| async {
                loop {
                    if node.state().epoch_store_for_testing().epoch() == 1 {
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            })
        })
        .collect();

    timeout(Duration::from_secs(40), join_all(handles))
        .await
        .expect("timed out waiting for reconfiguration to complete");

    info!("reconfiguration complete after {:?}", start.elapsed());
}
