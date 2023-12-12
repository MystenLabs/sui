// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::error::{SuiError, SuiResult};
use sui_types::utils::{
    get_legacy_zklogin_user_address, get_zklogin_user_address, make_zklogin_tx,
    sign_zklogin_tx_with_default_proof,
};
use sui_types::SUI_AUTHENTICATOR_STATE_OBJECT_ID;
use test_cluster::{TestCluster, TestClusterBuilder};

use sui_core::authority_client::AuthorityAPI;
use sui_macros::sim_test;

async fn do_zklogin_test(address: SuiAddress, legacy: bool) -> SuiResult {
    let test_cluster = TestClusterBuilder::new().build().await;
    let (_, tx, _) = make_zklogin_tx(address, legacy);

    test_cluster
        .authority_aggregator()
        .authority_clients
        .values()
        .next()
        .unwrap()
        .authority_client()
        .handle_transaction(tx)
        .await
        .map(|_| ())
}

#[sim_test]
async fn test_zklogin_feature_deny() {
    use sui_protocol_config::ProtocolConfig;

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_zklogin_auth_for_testing(false);
        config
    });

    let err = do_zklogin_test(get_zklogin_user_address(), false)
        .await
        .unwrap_err();

    assert!(matches!(err, SuiError::UnsupportedFeatureError { .. }));
}

#[sim_test]
async fn test_zklogin_feature_legacy_address_deny() {
    use sui_protocol_config::ProtocolConfig;

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_verify_legacy_zklogin_address(false);
        config
    });

    let err = do_zklogin_test(get_legacy_zklogin_user_address(), true)
        .await
        .unwrap_err();
    assert!(matches!(err, SuiError::SignerSignatureAbsent { .. }));
}

#[sim_test]
async fn test_legacy_zklogin_address_accept() {
    use sui_protocol_config::ProtocolConfig;
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_verify_legacy_zklogin_address(true);
        config
    });
    let err = do_zklogin_test(get_legacy_zklogin_user_address(), true)
        .await
        .unwrap_err();

    // it does not hit the signer absent error.
    assert!(matches!(err, SuiError::InvalidSignature { .. }));
}

#[sim_test]
async fn zklogin_end_to_end_test() {
    run_zklogin_end_to_end_test(TestClusterBuilder::new().with_default_jwks().build().await).await;
}

#[sim_test]
async fn zklogin_end_to_end_test_with_auth_state_creation() {
    // Create test cluster without auth state object in genesis
    let test_cluster = TestClusterBuilder::new()
        .with_protocol_version(23.into())
        .with_epoch_duration_ms(10000)
        .with_default_jwks()
        .build()
        .await;

    // Wait until we are in an epoch that has zklogin enabled, but the auth state object is not
    // created yet.
    test_cluster.wait_for_protocol_version(24.into()).await;

    // Now wait until the next epoch, when the auth state object is created.
    test_cluster.wait_for_epoch(None).await;

    // run zklogin end to end test
    run_zklogin_end_to_end_test(test_cluster).await;
}

async fn run_zklogin_end_to_end_test(mut test_cluster: TestCluster) {
    // wait for JWKs to be fetched and sequenced.
    test_cluster.wait_for_authenticator_state_update().await;

    let rgp = test_cluster.get_reference_gas_price().await;
    let sender = test_cluster.get_address_0();

    let context = &mut test_cluster.wallet;
    let accounts_and_objs = context.get_all_accounts_and_gas_objects().await.unwrap();
    let gas_object = accounts_and_objs[0].1[0];
    let object_to_send = accounts_and_objs[0].1[1];

    let zklogin_addr = get_zklogin_user_address();

    // first send an object to the zklogin address.
    let txn = context.sign_transaction(
        &TestTransactionBuilder::new(sender, gas_object, rgp)
            .transfer(object_to_send, zklogin_addr)
            .build(),
    );

    context.execute_transaction_must_succeed(txn).await;

    // now send it back
    let gas_object = context
        .get_gas_objects_owned_by_address(zklogin_addr, None)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    let txn = TestTransactionBuilder::new(zklogin_addr, gas_object, rgp)
        .transfer_sui(None, sender)
        .build();

    let (_, signed_txn, _) = sign_zklogin_tx_with_default_proof(txn, false);

    context.execute_transaction_must_succeed(signed_txn).await;

    assert!(context
        .get_gas_objects_owned_by_address(zklogin_addr, None)
        .await
        .unwrap()
        .is_empty());
}

#[sim_test]
async fn test_create_authenticator_state_object() {
    let test_cluster = TestClusterBuilder::new()
        .with_protocol_version(23.into())
        .with_epoch_duration_ms(10000)
        .build()
        .await;

    let handles = test_cluster.all_node_handles();

    // no node has the authenticator state object yet
    for h in &handles {
        h.with(|node| {
            assert!(node
                .state()
                .database
                .get_latest_object_ref_or_tombstone(SUI_AUTHENTICATOR_STATE_OBJECT_ID)
                .unwrap()
                .is_none());
        });
    }

    // wait until feature is enabled
    test_cluster.wait_for_protocol_version(24.into()).await;
    // wait until next epoch - authenticator state object is created at the end of the first epoch
    // in which it is supported.
    test_cluster.wait_for_epoch_all_nodes(2).await; // protocol upgrade completes in epoch 1

    for h in &handles {
        h.with(|node| {
            node.state()
                .database
                .get_latest_object_ref_or_tombstone(SUI_AUTHENTICATOR_STATE_OBJECT_ID)
                .unwrap()
                .expect("auth state object should exist");
        });
    }
}

// This test is intended to look for forks caused by conflicting / repeated JWK votes from
// validators.
#[cfg(msim)]
#[sim_test]
async fn test_conflicting_jwks() {
    use futures::StreamExt;
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};
    use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
    use sui_json_rpc_types::TransactionFilter;
    use sui_types::base_types::ObjectID;
    use sui_types::transaction::{TransactionDataAPI, TransactionKind};
    use tokio::time::Duration;

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(45000)
        .with_jwk_fetch_interval(Duration::from_secs(5))
        .build()
        .await;

    let jwks = Arc::new(Mutex::new(Vec::new()));
    let jwks_clone = jwks.clone();

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let mut txns = node.state().subscription_handler.subscribe_transactions(
            TransactionFilter::ChangedObject(ObjectID::from_hex_literal("0x7").unwrap()),
        );
        let state = node.state();

        tokio::spawn(async move {
            while let Some(tx) = txns.next().await {
                let digest = *tx.transaction_digest();
                let tx = state
                    .database
                    .get_transaction_block(&digest)
                    .unwrap()
                    .unwrap();
                match &tx.data().intent_message().value.kind() {
                    TransactionKind::EndOfEpochTransaction(_) => (),
                    TransactionKind::AuthenticatorStateUpdate(update) => {
                        let jwks = &mut *jwks_clone.lock().unwrap();
                        for jwk in &update.new_active_jwks {
                            jwks.push(jwk.clone());
                        }
                    }
                    _ => panic!("{:?}", tx),
                }
            }
        });
    });

    for _ in 0..5 {
        test_cluster.wait_for_epoch(None).await;
    }

    let mut seen_jwks = HashSet::new();

    // ensure no jwk is repeated.
    for jwk in jwks.lock().unwrap().iter() {
        assert!(seen_jwks.insert((jwk.jwk_id.clone(), jwk.jwk.clone(), jwk.epoch)));
    }
}
