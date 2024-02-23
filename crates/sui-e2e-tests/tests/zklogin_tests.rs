// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use shared_crypto::intent::Intent;
use shared_crypto::intent::IntentMessage;
use sui_core::authority_client::AuthorityAPI;
use sui_macros::sim_test;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::Signature;
use sui_types::error::{SuiError, SuiResult};
use sui_types::signature::GenericSignature;
use sui_types::transaction::Transaction;
use sui_types::utils::load_test_vectors;
use sui_types::utils::{
    get_legacy_zklogin_user_address, get_zklogin_user_address, make_zklogin_tx,
};
use sui_types::zk_login_authenticator::ZkLoginAuthenticator;
use sui_types::SUI_AUTHENTICATOR_STATE_OBJECT_ID;
use test_cluster::{TestCluster, TestClusterBuilder};

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

    // wait for current epoch to 11
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(1000)
        .build()
        .await;
    test_cluster
        .wait_for_epoch_with_timeout(Some(11), Duration::from_secs(300))
        .await;
    let rgp = test_cluster.get_reference_gas_price().await;

    // zklogin sig tx fails to execute bc it has max_epoch set to 10.
    let context = &test_cluster.wallet;

    let (eph_kp, pk_zklogin, zklogin_inputs) =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];
    let zklogin_addr = (pk_zklogin).into();
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), zklogin_addr)
        .await;
    let tx_data = TestTransactionBuilder::new(zklogin_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());

    let sig: GenericSignature = ZkLoginAuthenticator::new(
        zklogin_inputs.clone(),
        10,
        Signature::new_secure(&intent_msg, eph_kp),
    )
    .into();
    let tx = Transaction::from_generic_sig_data(tx_data.clone(), vec![sig]);

    let res = context.execute_transaction_may_fail(tx).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("ZKLogin expired at epoch 10"));
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

async fn run_zklogin_end_to_end_test(test_cluster: TestCluster) {
    // wait for JWKs to be fetched and sequenced.
    test_cluster.wait_for_authenticator_state_update().await;
    let test_vectors =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1..];
    for (kp, pk_zklogin, inputs) in test_vectors {
        let zklogin_addr = (pk_zklogin).into();
        let (sender, gas) = test_cluster
            .wallet
            .get_one_gas_object()
            .await
            .unwrap()
            .unwrap();

        let rgp = test_cluster.get_reference_gas_price().await;
        let context = &test_cluster.wallet;

        // first send some gas to the zklogin address.
        let transfer_to_zklogin = context.sign_transaction(
            &TestTransactionBuilder::new(sender, gas, rgp)
                .transfer_sui(Some(20000000000), zklogin_addr)
                .build(),
        );
        let _ = context
            .execute_transaction_must_succeed(transfer_to_zklogin)
            .await;

        let gas_obj = context
            .get_one_gas_object_owned_by_address(zklogin_addr)
            .await
            .unwrap()
            .unwrap();

        // create txn to send from the zklogin address.
        let tx_data = TestTransactionBuilder::new(zklogin_addr, gas_obj, rgp)
            .transfer_sui(None, SuiAddress::ZERO)
            .build();

        let msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
        let eph_sig = Signature::new_secure(&msg, kp);

        // combine ephemeral sig with zklogin inputs.
        let generic_sig = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
            inputs.clone(),
            10,
            eph_sig.clone(),
        ));
        let signed_txn = Transaction::from_generic_sig_data(tx_data.clone(), vec![generic_sig]);

        // a valid txn executes.
        context.execute_transaction_must_succeed(signed_txn).await;

        // a txn with max_epoch mismatch with proof, fails to execute.
        let generic_sig = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
            inputs.clone(),
            0,
            eph_sig,
        ));
        let signed_txn_expired = Transaction::from_generic_sig_data(tx_data, vec![generic_sig]);
        let result = context
            .execute_transaction_may_fail(signed_txn_expired)
            .await;
        assert!(result.is_err());
    }
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
                .get_cache_reader()
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
                .get_cache_reader()
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
                    .get_cache_reader()
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
