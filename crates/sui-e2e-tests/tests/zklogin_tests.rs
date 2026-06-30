// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::ed25519::Ed25519KeyPair;
use fastcrypto::jwt_utils::parse_and_validate_jwt;
use fastcrypto::traits::KeyPair;
use fastcrypto_zkp::bn254::utils::{gen_address_seed, get_nonce, get_proof};
use fastcrypto_zkp::bn254::zk_login::{JWK, JwkId, OIDCProvider, ZkLoginInputs, fetch_jwks};
use fastcrypto_zkp::bn254::zk_login_api::ZkLoginEnv;
use im::hashmap::HashMap as ImHashMap;
use num_bigint::BigUint;
use rand::SeedableRng;
use rand::rngs::StdRng;
use shared_crypto::intent::Intent;
use shared_crypto::intent::IntentMessage;
use shared_crypto::intent::PersonalMessage;
use std::net::SocketAddr;
use std::sync::Arc;
use sui_core::authority_client::AuthorityAPI;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::committee::EpochId;
use sui_types::crypto::{PublicKey, Signature, SuiKeyPair};
use sui_types::error::{SuiErrorKind, SuiResult, UserInputError};
use sui_types::messages_grpc::SubmitTxRequest;
use sui_types::signature::{GenericSignature, VerifyParams};
use sui_types::signature_verification::VerifiedDigestCache;
use sui_types::transaction::Transaction;
use sui_types::utils::load_test_vectors;
use sui_types::utils::{
    get_legacy_zklogin_user_address, get_zklogin_user_address, make_zklogin_tx,
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
        eph_sig.clone(),
    ));
    Transaction::from_generic_sig_data(tx_data.clone(), vec![generic_sig])
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

/// End-to-end coverage for v2 migration, tested prover-v1 and prover-v2 against
// with sui's v2 flag enabled and disabled.
#[ignore = "depends on live external prover services; run manually with --ignored"]
#[tokio::test]
async fn zklogin_v1_v2_migration_scenario_test() {
    const V1_PROVER_URL: &str = "https://prover-dev.mystenlabs.com/v1";
    const V2_PROVER_URL: &str = "https://prover-dev-v2.mystenlabs.com/v1";
    const JWT_TESTER: &str = "https://jwt-tester.mystenlabs.com";

    // Fixed inputs (mirroring fastcrypto's live e2e tests) so the derived nonce is
    // deterministic and reproducible.
    const MAX_EPOCH: u64 = 10;
    const JWT_RANDOMNESS: &str = "100681567828351849884072155819400689117";
    const USER_SALT: &str = "129390038577185583942388216820280642146";
    const V2_FALLBACK_LOG: &str = "falling back to v1";

    /// Captured log output, used to assert whether the v2->v1 fallback path ran.
    #[derive(Clone, Default)]
    struct Logs(Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Logs {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Logs {
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
        }

        fn assert_fallback(&self) {
            let logs = self.text();
            assert!(
                logs.contains(V2_FALLBACK_LOG),
                "expected v2->v1 fallback, captured logs:\n{logs}"
            );
        }

        fn assert_no_fallback(&self) {
            let logs = self.text();
            assert!(
                !logs.contains(V2_FALLBACK_LOG),
                "unexpected v2->v1 fallback, captured logs:\n{logs}"
            );
        }
    }

    /// Create an ephemeral key and nonce, then calls `jwt-tester` for JWT, and
    // calls prover to construct `ZkLoginInputs`. Also returns the JWK and keypair.
    async fn prove(
        provider: OIDCProvider,
        jwt_path: &str,
        prover_url: &str,
    ) -> (ZkLoginInputs, SuiKeyPair, SuiAddress, ImHashMap<JwkId, JWK>) {
        let client = reqwest::Client::new();

        // Fixed ephemeral key pair.
        let kp = Ed25519KeyPair::generate(&mut StdRng::from_seed([0; 32]));
        let mut eph_pk_bytes = vec![0x00];
        eph_pk_bytes.extend(kp.public().as_ref());
        let kp_bigint = BigUint::from_bytes_be(&eph_pk_bytes).to_string();

        // Compute the nonce and fetch a JWT from the test issuer.
        let nonce = get_nonce(&eph_pk_bytes, MAX_EPOCH, JWT_RANDOMNESS).unwrap();
        let iss = provider.get_config().iss;
        let jwt_resp: serde_json::Value = client
            .post(format!(
                "{JWT_TESTER}/{jwt_path}?nonce={nonce}&iss={iss}&sub=test"
            ))
            .header("Content-Type", "application/json")
            .header("Content-Length", "0")
            .send()
            .await
            .expect("issuer request failed")
            .json()
            .await
            .expect("issuer response not json");
        let jwt = jwt_resp["jwt"]
            .as_str()
            .expect("issuer did not return a jwt")
            .to_string();

        // Ask the prover for the zkLogin proof.
        let reader = get_proof(
            &jwt,
            MAX_EPOCH,
            JWT_RANDOMNESS,
            &kp_bigint,
            USER_SALT,
            prover_url,
        )
        .await
        .expect("get_proof failed");

        let (sub, aud, _) = parse_and_validate_jwt(&jwt).unwrap();
        let address_seed = gen_address_seed(USER_SALT, "sub", &sub, &aud).unwrap();
        let inputs = ZkLoginInputs::from_reader(reader, &address_seed).expect("from_reader failed");
        let author = SuiAddress::from(&PublicKey::from_zklogin_inputs(&inputs).unwrap());

        // Fetch the issuer's JWK set live and index it by (iss, kid).
        let jwks = fetch_jwks(&provider, &client, false)
            .await
            .expect("fetch_jwks failed")
            .into_iter()
            .collect();

        (inputs, SuiKeyPair::Ed25519(kp), author, jwks)
    }

    /// Verify a zklogin txn signature and assert logs.
    fn verify(
        inputs: &ZkLoginInputs,
        eph_kp: &SuiKeyPair,
        author: SuiAddress,
        jwks: &ImHashMap<JwkId, JWK>,
        zklogin_auth_v2: bool,
    ) -> (SuiResult, Logs) {
        let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
        config.set_zklogin_auth_v2_for_testing(zklogin_auth_v2);

        let msg = PersonalMessage {
            message: b"zklogin v2 live test".to_vec(),
        };
        let intent_msg = IntentMessage::new(Intent::personal_message(), msg);
        let eph_sig = Signature::new_secure(&intent_msg, eph_kp);
        let authenticator = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
            inputs.clone(),
            MAX_EPOCH,
            eph_sig,
        ));

        let params = VerifyParams::new(
            jwks.clone(),
            vec![],
            ZkLoginEnv::Test,
            config.zklogin_auth_v2(),
            config.verify_legacy_zklogin_address(),
            config.accept_zklogin_in_multisig(),
            config.accept_passkey_in_multisig(),
            config.zklogin_max_epoch_upper_bound_delta(),
            config.additional_multisig_checks(),
            config.validate_zklogin_public_identifier(),
        );

        // Capture fastcrypto's debug logs so we can tell which verifying key
        // accepted the proof (v2 directly vs. v2->v1 fallback).
        let logs = Logs::default();
        let writer = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .finish();
        let res = tracing::subscriber::with_default(subscriber, || {
            authenticator.verify_authenticator(
                &intent_msg,
                author,
                0,
                &params,
                Arc::new(VerifiedDigestCache::new_empty()),
            )
        });
        (res, logs)
    }

    // Call v2 endpoint for proof.
    let (inputs, eph_kp, author, jwks) =
        prove(OIDCProvider::TestIssuerKey8192, "8192/jwt", V2_PROVER_URL).await;

    // Flag enabled: verifies on the v2 key directly, no fallback.
    let (res, logs) = verify(&inputs, &eph_kp, author, &jwks, true);
    assert!(res.is_ok());
    logs.assert_no_fallback();

    // Flag disabled: only the v1 key is tried, which rejects it; the v2 key is
    // never attempted, so there is nothing to fall back from.
    let (res, logs) = verify(&inputs, &eph_kp, author, &jwks, false);
    assert!(res.is_err());
    logs.assert_no_fallback();

    // Call v1 endpoint for proof.
    let (inputs, eph_kp, author, jwks) =
        prove(OIDCProvider::TestIssuer, "jwt", V1_PROVER_URL).await;

    // Flag disabled: verifies on the v1 key directly, no v2 attempt.
    let (res, logs) = verify(&inputs, &eph_kp, author, &jwks, false);
    assert!(res.is_ok());
    logs.assert_no_fallback();

    // Flag enabled: v2 key is attempted first and fails, then falls back to v1 (the
    // 2x-cost path), so enabling v2 stays backward compatible.
    let (res, logs) = verify(&inputs, &eph_kp, author, &jwks, true);
    assert!(res.is_ok());
    logs.assert_fallback();
}
