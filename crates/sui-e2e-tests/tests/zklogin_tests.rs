// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::traits::EncodeDecodeBase64;
use fastcrypto_zkp::bn254::zk_login::ZkLoginInputs;
use shared_crypto::intent::{Intent, IntentMessage};
use sui_core::authority_client::AuthorityAPI;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_macros::sim_test;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{PublicKey, Signature, SuiKeyPair};
use sui_types::error::{SuiError, SuiResult};
use sui_types::signature::GenericSignature;
use sui_types::transaction::Transaction;
use sui_types::utils::{
    get_legacy_zklogin_user_address, get_zklogin_user_address, make_zklogin_tx, TestData,
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
    let context = &mut test_cluster.wallet;

    // load deterministic keypairs where the zk proof corresponds to the keypair.
    let file = std::fs::File::open("../sui-types/src/unit_tests/zklogin_test_vectors.json")
        .expect("Unable to open file");
    let test_datum: Vec<TestData> = serde_json::from_reader(file).unwrap();

    for test in test_datum {
        let kp = SuiKeyPair::decode_base64(&test.kp).unwrap();
        let inputs = ZkLoginInputs::from_json(&test.zklogin_inputs, &test.address_seed).unwrap();
        let pk_zklogin = PublicKey::from_zklogin_inputs(&inputs).unwrap();
        let zklogin_addr = (&pk_zklogin).into();

        let (sender, gas) = context.get_one_gas_object().await.unwrap().unwrap();

        // first fund the zklogin address.
        let transfer_to_zklogin_addr = context.sign_transaction(
            &TestTransactionBuilder::new(sender, gas, rgp)
                .transfer_sui(Some(20000000000), zklogin_addr)
                .build(),
        );
        let resp = context
            .execute_transaction_must_succeed(transfer_to_zklogin_addr)
            .await;

        // send it from the zklogin address.
        let new_obj = resp
            .effects
            .unwrap()
            .created()
            .first()
            .unwrap()
            .reference
            .to_object_ref();

        let tx_data = TestTransactionBuilder::new(zklogin_addr, new_obj, rgp)
            .transfer_sui(Some(1000000), sender)
            .build();
        let msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
        let eph_sig = Signature::new_secure(&msg, &kp);

        let generic_sig =
            GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(inputs, 10, eph_sig));
        let signed_txn = Transaction::from_generic_sig_data(tx_data, vec![generic_sig]);
        context.execute_transaction_must_succeed(signed_txn).await;
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

#[sim_test]
async fn zklogin_tx_fails_proof_max_epoch_passed() {}

#[sim_test]
async fn zklogin_test_fails_kid_not_found() {}
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
