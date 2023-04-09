// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;

use std::time::Duration;

use sui_core::authority_aggregator::{AuthAggMetrics, AuthorityAggregator};

use sui_core::safe_client::SafeClientMetricsBase;
use sui_core::test_utils::make_transfer_sui_transaction;
use sui_macros::sim_test;

use sui_types::crypto::get_account_key_pair;

use sui_types::object::Object;
use test_utils::authority::{spawn_test_authorities, test_authority_configs_with_objects};

#[sim_test]
async fn basic_checkpoints_integration_test() {
    let (sender, keypair) = get_account_key_pair();
    let gas1 = Object::with_owner_for_testing(sender);
    let (configs, mut objects) = test_authority_configs_with_objects([gas1]);
    let gas1 = objects.pop().expect("Should contain a single gas object");
    let authorities = spawn_test_authorities(&configs).await;
    let registry = Registry::new();

    let rgp = authorities
        .get(0)
        .unwrap()
        .with(|sui_node| sui_node.state().reference_gas_price_for_testing())
        .unwrap();

    // gas1 transaction is committed
    let tx = make_transfer_sui_transaction(
        gas1.compute_object_reference(),
        sender,
        None,
        sender,
        &keypair,
        rgp,
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
    let _effects = net
        .process_certificate(cert.clone().into_inner())
        .await
        .unwrap();

    for _ in 0..600 {
        let all_included = authorities.iter().all(|handle| {
            handle.with(|node| {
                node.is_transaction_executed_in_checkpoint(tx.digest())
                    .unwrap()
            })
        });
        if all_included {
            // success
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    panic!("Did not include transaction in checkpoint in 60 seconds");
}
