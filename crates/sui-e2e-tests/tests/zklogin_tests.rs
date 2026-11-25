// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

use fastcrypto_zkp::bn254::zk_login::ZkLoginInputs;
use shared_crypto::intent::Intent;
use shared_crypto::intent::IntentMessage;
use sui_core::authority_client::AuthorityAPI;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::committee::EpochId;
use sui_types::crypto::{Signature, SuiKeyPair};
use sui_types::error::{SuiErrorKind, SuiResult, UserInputError};
use sui_types::messages_grpc::SubmitTxRequest;
use sui_types::signature::GenericSignature;
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
    build_zklogin_tx_with_inputs(test_cluster, kp, inputs, (pk_zklogin).into(), max_epoch).await
}

/// Fund `zklogin_addr` and build a transfer from it, signed with the ephemeral key
/// and wrapped in a `ZkLoginAuthenticator` with the given inputs.
async fn build_zklogin_tx_with_inputs(
    test_cluster: &TestCluster,
    kp: &SuiKeyPair,
    inputs: &ZkLoginInputs,
    zklogin_addr: SuiAddress,
    max_epoch: EpochId,
) -> Transaction {
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
#[cfg(msim)]
async fn build_zklogin_v2_transfer() -> anyhow::Result<(TestCluster, Transaction)> {
    use fastcrypto_zkp::bn254::zk_login::{JwkId, OIDCProvider};
    use std::sync::Arc;
    use sui_protocol_config::Chain;
    use sui_types::utils::{PINNED_V2_JWK_ID_JSON, load_zklogin_v2_test_vector};
    use sui_types::zk_login_authenticator::ZkLoginAuthenticatorV2;

    let (signer, public_key, inputs, jwk, max_epoch) = load_zklogin_v2_test_vector();
    let jwk_id: JwkId = serde_json::from_str(PINNED_V2_JWK_ID_JSON)?;
    let injected_id = jwk_id.clone();
    sui_node::set_jwk_injector(Arc::new(move |_, provider| {
        if *provider == OIDCProvider::TestIssuerKey8192 {
            Ok(vec![(injected_id.clone(), jwk.clone())])
        } else {
            Ok(vec![])
        }
    }));

    // Use the test verifying key for the pinned proof.
    let cluster = TestClusterBuilder::new()
        .with_chain_override(Chain::Unknown)
        .with_default_jwks()
        .with_num_validators(1)
        .build()
        .await;
    cluster
        .wait_for_authenticator_state_update_for_providers(&[jwk_id])
        .await;

    let author = SuiAddress::from(&public_key);
    let rgp = cluster.get_reference_gas_price().await;
    let gas = cluster
        .fund_address_and_return_gas(rgp, Some(20_000_000_000), author)
        .await;
    let tx = TestTransactionBuilder::new(author, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent = IntentMessage::new(Intent::sui_transaction(), &tx);
    let authenticator =
        ZkLoginAuthenticatorV2::new(inputs, max_epoch, Signature::new_secure(&intent, &signer))?;

    let tx = Transaction::from_generic_sig_data(tx, vec![authenticator.into()]);
    Ok((cluster, tx))
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

#[cfg(msim)]
#[sim_test]
async fn zklogin_v2_accepted_flag_mode_1() -> anyhow::Result<()> {
    use sui_types::effects::TransactionEffectsAPI;

    if sui_simulator::has_mainnet_protocol_config_override() {
        return Ok(());
    }

    let _protocol_override = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_zklogin_circuit_mode_for_testing(1);
        config
    });

    let (cluster, tx) = build_zklogin_v2_transfer().await?;
    let (digest, effects) = cluster.execute_transaction_directly(&tx).await?;
    assert!(effects.status().is_ok(), "{effects:?}");
    cluster.wait_for_tx_settlement(&[digest]).await;
    Ok(())
}

#[cfg(msim)]
#[sim_test]
async fn zklogin_v2_rejected_flag_mode_0() -> anyhow::Result<()> {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return Ok(());
    }

    let _protocol_override = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_zklogin_circuit_mode_for_testing(0);
        config
    });

    let (cluster, tx) = build_zklogin_v2_transfer().await?;
    let error = cluster
        .execute_transaction_directly(&tx)
        .await
        .expect_err("mode 0 should reject a zkLogin v2 transfer");
    assert!(
        error
            .to_string()
            .contains("zkLogin v2 is not enabled on this network"),
        "{error}"
    );
    Ok(())
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
    use std::collections::HashSet;
    use sui_types::SUI_AUTHENTICATOR_STATE_OBJECT_ID;
    use sui_types::effects::TransactionEffectsAPI;
    use sui_types::transaction::{TransactionDataAPI, TransactionKind};
    use tokio::time::Duration;

    // The scan below reads the contents of every checkpoint the fullnode has
    // executed, so pruning must not remove the early epochs first.
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(4)
        .with_epoch_duration_ms(15000)
        .with_jwk_fetch_interval(Duration::from_secs(5))
        .disable_fullnode_pruning()
        .build()
        .await;

    for _ in 0..5 {
        test_cluster.wait_for_epoch(None).await;
    }

    // Collect the JWKs activated by every transaction that mutated the
    // authenticator state object by walking the checkpoints the fullnode has
    // executed so far.
    let jwks = test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let cache = state.get_transaction_cache_reader();
        let highest_executed = state
            .get_checkpoint_store()
            .get_highest_executed_checkpoint_seq_number()
            .unwrap()
            .expect("fullnode has executed at least one checkpoint");

        let mut jwks = Vec::new();
        for seq in 0..=highest_executed {
            let contents = state
                .get_checkpoint_contents_by_sequence_number(seq)
                .unwrap();
            for digests in contents.iter() {
                let effects = cache
                    .get_executed_effects(&digests.transaction)
                    .expect("executed transaction has effects");
                let mutates_authenticator_state = effects
                    .mutated()
                    .iter()
                    .any(|(oref, _)| oref.0 == SUI_AUTHENTICATOR_STATE_OBJECT_ID);
                if !mutates_authenticator_state {
                    continue;
                }

                let tx = cache
                    .get_transaction_block(&digests.transaction)
                    .expect("executed transaction is in the cache");
                match tx.data().intent_message().value.kind() {
                    TransactionKind::EndOfEpochTransaction(_) => (),
                    TransactionKind::AuthenticatorStateUpdate(update) => {
                        jwks.extend(update.new_active_jwks.iter().cloned());
                    }
                    _ => panic!("{:?}", tx),
                }
            }
        }
        jwks
    });

    let mut seen_jwks = HashSet::new();

    // ensure no jwk is repeated.
    for jwk in &jwks {
        assert!(seen_jwks.insert((jwk.jwk_id.clone(), jwk.jwk.clone(), jwk.epoch)));
    }
}
