// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::ed25519::Ed25519KeyPair;
use fastcrypto::traits::KeyPair as _;
use fastcrypto_zkp::bn254::zk_login::ZkLoginInputs;
use rand::{SeedableRng, rngs::StdRng};
use shared_crypto::intent::Intent;
use shared_crypto::intent::IntentMessage;
use std::net::SocketAddr;
use sui_core::authority_client::AuthorityAPI;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::committee::EpochId;
use sui_types::crypto::{PublicKey, Signature, SuiKeyPair};
use sui_types::error::{SuiErrorKind, SuiResult, UserInputError};
use sui_types::messages_grpc::SubmitTxRequest;
use sui_types::signature::GenericSignature;
use sui_types::transaction::Transaction;
use sui_types::utils::load_test_vectors;
use sui_types::utils::{
    get_legacy_zklogin_user_address, get_zklogin_user_address, make_zklogin_tx, sign_zklogin_tx,
};
use sui_types::zk_login_authenticator::ZkLoginAuthenticator;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;

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
        .submit_transaction(
            SubmitTxRequest::new_transaction(tx),
            Some(SocketAddr::new([127, 0, 0, 1].into(), 0)),
        )
        .await
        .map(|_| ())
}

async fn build_zklogin_tx(test_cluster: &TestCluster, max_epoch: EpochId) -> Transaction {
    // load test vectors
    let (kp, pk_zklogin, inputs) =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];
    let zklogin_addr = (pk_zklogin).into();

    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), zklogin_addr)
        .await;
    let tx_data = TestTransactionBuilder::new(zklogin_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();

    let msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let eph_sig = Signature::new_secure(&msg, kp);

    // combine ephemeral sig with zklogin inputs.
    let generic_sig = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
        inputs.clone(),
        max_epoch,
        eph_sig,
    ));
    Transaction::from_generic_sig_data(tx_data, vec![generic_sig])
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

    assert!(matches!(
        err.as_inner(),
        SuiErrorKind::UserInputError {
            error: UserInputError::Unsupported(..)
        }
    ));
}

#[sim_test]
async fn test_zklogin_feature_legacy_address_deny() {
    use sui_protocol_config::ProtocolConfig;

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_verify_legacy_zklogin_address_for_testing(false);
        config.set_zklogin_max_epoch_upper_bound_delta_for_testing(None);
        config
    });

    let err = do_zklogin_test(get_legacy_zklogin_user_address(), true)
        .await
        .unwrap_err();
    assert!(matches!(
        err.as_inner(),
        SuiErrorKind::SignerSignatureAbsent { .. }
    ));
}

#[sim_test]
async fn test_legacy_zklogin_address_accept() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_verify_legacy_zklogin_address_for_testing(true);
        config
    });
    let err = do_zklogin_test(get_legacy_zklogin_user_address(), true)
        .await
        .unwrap_err();

    // it does not hit the signer absent error.
    assert!(matches!(
        err.as_inner(),
        SuiErrorKind::InvalidSignature { .. }
    ));
}

#[sim_test]
async fn zklogin_end_to_end_test() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    // V1 proofs should work even when zklogin_auth_v2 is disabled.
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_zklogin_auth_v2_for_testing(false);
        config
    });

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(15000)
        .with_default_jwks()
        .build()
        .await;

    test_cluster.wait_for_authenticator_state_update().await;
    let signed_txn = build_zklogin_tx(&test_cluster, 2).await;
    let context = &test_cluster.wallet;
    let res = context.execute_transaction_may_fail(signed_txn).await;
    assert!(res.is_ok());

    // a txn with max_epoch mismatch with proof, fails to execute.
    let signed_txn_with_wrong_max_epoch = build_zklogin_tx(&test_cluster, 1).await;
    assert!(
        context
            .execute_transaction_may_fail(signed_txn_with_wrong_max_epoch)
            .await
            .is_err()
    );
}

#[sim_test]
async fn test_max_epoch_too_large_fail_tx() {
    use sui_protocol_config::ProtocolConfig;
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_zklogin_max_epoch_upper_bound_delta_for_testing(Some(1));
        config
    });

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(15000)
        .with_default_jwks()
        .build()
        .await;
    test_cluster.wait_for_authenticator_state_update().await;
    let context = &test_cluster.wallet;
    // current epoch is 1, upper bound is 1 + 1, so max_epoch as 3 in zklogin signature should fail.
    let signed_txn = build_zklogin_tx(&test_cluster, 2).await;
    let res = context.execute_transaction_may_fail(signed_txn).await;
    assert!(
        res.unwrap_err()
            .to_string()
            .contains("ZKLogin max epoch too large")
    );
}

#[sim_test]
async fn test_expired_zklogin_sig() {
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(15000)
        .with_default_jwks()
        .build()
        .await;

    // trigger reconfiguration that advanced epoch to 1.
    test_cluster.trigger_reconfiguration().await;
    // trigger reconfiguration that advanced epoch to 2.
    test_cluster.trigger_reconfiguration().await;
    // trigger reconfiguration that advanced epoch to 3.
    test_cluster.trigger_reconfiguration().await;

    // load one test vector, the zklogin inputs corresponds to max_epoch = 1
    let (kp, pk_zklogin, inputs) =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];
    let zklogin_addr = (pk_zklogin).into();

    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), zklogin_addr)
        .await;
    let context = &test_cluster.wallet;

    let tx_data = TestTransactionBuilder::new(zklogin_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();

    let msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let eph_sig = Signature::new_secure(&msg, kp);

    // combine ephemeral sig with zklogin inputs.
    let generic_sig = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
        inputs.clone(),
        2,
        eph_sig.clone(),
    ));
    let signed_txn_expired = Transaction::from_generic_sig_data(tx_data.clone(), vec![generic_sig]);

    let res = context
        .execute_transaction_may_fail(signed_txn_expired)
        .await;
    assert!(
        res.unwrap_err()
            .to_string()
            .contains("ZKLogin expired at epoch 2")
    );
}

async fn build_zklogin_v2_tx(test_cluster: &TestCluster) -> Transaction {
    let address_seed =
        "1930628255822123795956154519923524356793387287437090556144422698180443693114";
    let zklogin_inputs = ZkLoginInputs::from_json(
        r#"{"proofPoints":{"a":["4913491815640002925508764814861178584881454035317776104347888483537912573177","17464247119089096977765585378460061328465709176842125201639874369409917083365","1"],"b":[["13623903508208593385147109129252793918112295419570003309520868038720322470557","21609423682403605552756457705069928412495291852654002331866073641632927420027"],["21392198638402084688930318789933313022805249822640479452861513428525783839707","1188996632803951473949030842369314644349566079256879538309939741515182911983"],["1","0"]],"c":["8847019028968200963788057481027139711885570926967685201543612972187276716667","14579483098715294861159755601821797996287919909580326110060065627124968449243","1"]},"issBase64Details":{"value":"wiaXNzIjoiaHR0cHM6Ly9qd3QtdGVzdGVyLm15c3RlbmxhYnMuY29tIiw","indexMod4":2},"headerBase64":"eyJraWQiOiJzdWkta2V5LWlkLTgxOTIiLCJ0eXAiOiJKV1QiLCJhbGciOiJSUzI1NiJ9","addressSeed":"1930628255822123795956154519923524356793387287437090556144422698180443693114"}"#,
        address_seed,
    ).unwrap();

    let kp = SuiKeyPair::Ed25519(Ed25519KeyPair::generate(&mut StdRng::from_seed([0; 32])));
    let zklogin_addr = SuiAddress::from(&PublicKey::from_zklogin_inputs(&zklogin_inputs).unwrap());
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), zklogin_addr)
        .await;
    let tx_data = TestTransactionBuilder::new(zklogin_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let (_, tx, _) = sign_zklogin_tx(&kp, zklogin_inputs, tx_data);
    tx
}

#[sim_test]
async fn test_zklogin_v2_end_to_end() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_zklogin_auth_v2_for_testing(true);
        config
    });

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(15000)
        .with_default_jwks()
        .build()
        .await;

    test_cluster.wait_for_authenticator_state_update().await;

    let signed_txn = build_zklogin_v2_tx(&test_cluster).await;
    let res = test_cluster
        .wallet
        .execute_transaction_may_fail(signed_txn)
        .await;
    assert!(
        res.is_ok(),
        "V2 zklogin transaction should succeed: {:?}",
        res.err()
    );
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
        .with_epoch_duration_ms(15000)
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
                    .get_transaction_cache_reader()
                    .get_transaction_block(&digest)
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
