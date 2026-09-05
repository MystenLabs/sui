// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto_zkp::bn254::zk_login::ZkLoginInputs;
use fastcrypto_zkp::bn254::zk_login_api::CircuitVersion;
use shared_crypto::intent::Intent;
use shared_crypto::intent::IntentMessage;
use std::net::SocketAddr;
#[cfg(msim)]
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
use sui_types::multisig::{MultiSig, MultiSigPublicKey};
use sui_types::signature::GenericSignature;
use sui_types::transaction::Transaction;
use sui_types::utils::load_test_vectors;
#[cfg(msim)]
use sui_types::utils::pinned_jwks;
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
    let (kp, _, inputs) =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];
    build_zklogin_tx_with_inputs(
        test_cluster,
        kp,
        inputs,
        max_epoch,
        CircuitVersion::V1,
        false,
    )
    .await
}

/// Fund `zklogin_addr` and build a transfer from it, signed with the ephemeral key
/// and wrapped in a `ZkLoginAuthenticator` with the given inputs.
async fn build_zklogin_tx_with_inputs(
    test_cluster: &TestCluster,
    kp: &SuiKeyPair,
    inputs: &ZkLoginInputs,
    max_epoch: EpochId,
    circuit_version: CircuitVersion,
    multisig: bool,
) -> Transaction {
    let zklogin_pk = match circuit_version {
        CircuitVersion::V1 => PublicKey::from_zklogin_inputs(inputs),
        CircuitVersion::V2 => PublicKey::from_zklogin_v2_inputs(inputs),
    }
    .unwrap();
    let multisig_pk =
        multisig.then(|| MultiSigPublicKey::new(vec![zklogin_pk.clone()], vec![1], 1).unwrap());
    let sender = multisig_pk
        .as_ref()
        .map(SuiAddress::from)
        .unwrap_or_else(|| SuiAddress::from(&zklogin_pk));
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), sender)
        .await;
    let tx_data = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();

    let msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let eph_sig = Signature::new_secure(&msg, kp);

    // combine ephemeral sig with zklogin inputs.
    let authenticator = ZkLoginAuthenticator::new(inputs.clone(), max_epoch, eph_sig);
    let generic_sig = match circuit_version {
        CircuitVersion::V1 => GenericSignature::ZkLoginAuthenticator(authenticator),
        CircuitVersion::V2 => GenericSignature::ZkLoginAuthenticatorV2(authenticator.into()),
    };
    let generic_sig = if let Some(multisig_pk) = multisig_pk {
        GenericSignature::MultiSig(MultiSig::combine(vec![generic_sig], multisig_pk).unwrap())
    } else {
        generic_sig
    };
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

/// Submit a transfer signed with a V2 proof captured from a prover-dev-v2 response.
#[cfg(msim)]
async fn execute_v2_proof_tx(test_cluster: &TestCluster, multisig: bool) -> anyhow::Result<()> {
    let (eph_kp, _, inputs) =
        load_test_vectors("../sui-types/src/unit_tests/zklogin_v2_test_vectors.json")
            .into_iter()
            .next()
            .unwrap();
    let tx = build_zklogin_tx_with_inputs(
        test_cluster,
        &eph_kp,
        &inputs,
        10,
        CircuitVersion::V2,
        multisig,
    )
    .await;
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
        .await?;
    Ok(())
}

#[cfg(msim)]
async fn run_zklogin_v2_protocol_gate_test(v2_enabled: bool, multisig: bool) {
    use fastcrypto_zkp::bn254::zk_login::{JWK, JwkId};

    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    // Serve the pinned test-issuer JWKs from every validator.
    let jwks = pinned_jwks();
    let jwk_ids: Vec<JwkId> = jwks.keys().cloned().collect();
    let injected: Vec<(JwkId, JWK)> = jwks.into_iter().collect();
    sui_node::set_jwk_injector(Arc::new(move |_, _| Ok(injected.clone())));

    let _guard = ProtocolConfig::apply_overrides_for_testing(move |_, mut config| {
        config.set_zklogin_auth_for_testing(true);
        config.set_zklogin_circuit_mode_for_testing(u64::from(v2_enabled));
        config.set_accept_zklogin_in_multisig_for_testing(true);
        config
    });

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(15000)
        .with_default_jwks()
        .build()
        .await;
    test_cluster
        .wait_for_authenticator_state_update_for_providers(&jwk_ids)
        .await;

    let result = execute_v2_proof_tx(&test_cluster, multisig).await;
    if v2_enabled {
        assert!(result.is_ok(), "V2 transaction failed: {result:?}");
    } else {
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("zkLogin V2 authenticator is not enabled")
        );
    }
}

#[cfg(msim)]
#[sim_test]
async fn test_zklogin_v2_enabled() {
    run_zklogin_v2_protocol_gate_test(true, false).await;
}

#[cfg(msim)]
#[sim_test]
async fn test_zklogin_v2_disabled() {
    run_zklogin_v2_protocol_gate_test(false, false).await;
}

#[cfg(msim)]
#[sim_test]
async fn test_zklogin_v2_multisig_enabled() {
    run_zklogin_v2_protocol_gate_test(true, true).await;
}

#[cfg(msim)]
#[sim_test]
async fn test_zklogin_v2_multisig_disabled() {
    run_zklogin_v2_protocol_gate_test(false, true).await;
}
