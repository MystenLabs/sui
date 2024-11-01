// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::traits::EncodeDecodeBase64;
use shared_crypto::intent::{Intent, IntentMessage};
use std::net::SocketAddr;
use sui_core::authority_client::AuthorityAPI;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::error::UserInputError;
use sui_types::multisig_legacy::MultiSigLegacy;
use sui_types::{
    base_types::SuiAddress,
    crypto::{
        get_key_pair, CompressedSignature, PublicKey, Signature, SuiKeyPair,
        ZkLoginAuthenticatorAsBytes, ZkLoginPublicIdentifier,
    },
    error::{SuiError, SuiResult},
    multisig::{MultiSig, MultiSigPublicKey},
    multisig_legacy::MultiSigPublicKeyLegacy,
    signature::GenericSignature,
    transaction::Transaction,
    utils::{keys, load_test_vectors, make_upgraded_multisig_tx},
    zk_login_authenticator::ZkLoginAuthenticator,
};
use test_cluster::{TestCluster, TestClusterBuilder};

async fn do_upgraded_multisig_test() -> SuiResult {
    let test_cluster = TestClusterBuilder::new().build().await;
    let tx = make_upgraded_multisig_tx();

    test_cluster
        .authority_aggregator()
        .authority_clients
        .values()
        .next()
        .unwrap()
        .authority_client()
        .handle_transaction(tx, Some(SocketAddr::new([127, 0, 0, 1].into(), 0)))
        .await
        .map(|_| ())
}

#[sim_test]
async fn test_upgraded_multisig_feature_deny() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_upgraded_multisig_for_testing(false);
        config
    });

    let err = do_upgraded_multisig_test().await.unwrap_err();

    assert!(matches!(
        err,
        SuiError::UserInputError {
            error: UserInputError::Unsupported(..)
        }
    ));
}

#[sim_test]
async fn test_upgraded_multisig_feature_allow() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_upgraded_multisig_for_testing(true);
        config
    });

    let res = do_upgraded_multisig_test().await;

    // we didn't make a real transaction with a valid object, but we verify that we pass the
    // feature gate.
    assert!(matches!(res.unwrap_err(), SuiError::UserInputError { .. }));
}

#[sim_test]
async fn test_multisig_e2e() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let context = &test_cluster.wallet;
    let rgp = test_cluster.get_reference_gas_price().await;

    let keys = keys();
    let pk0 = keys[0].public(); // ed25519
    let pk1 = keys[1].public(); // secp256k1
    let pk2 = keys[2].public(); // secp256r1

    let multisig_pk = MultiSigPublicKey::insecure_new(
        vec![(pk0.clone(), 1), (pk1.clone(), 1), (pk2.clone(), 1)],
        2,
    );
    let multisig_addr = SuiAddress::from(&multisig_pk);

    // fund wallet and get a gas object to use later.
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), multisig_addr)
        .await;

    // 1. sign with key 0 and 1 executes successfully.
    let tx1 = TestTransactionBuilder::new(multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build_and_sign_multisig(multisig_pk.clone(), &[&keys[0], &keys[1]], 0b011);
    let res = context.execute_transaction_must_succeed(tx1).await;
    assert!(res.status_ok().unwrap());

    // 2. sign with key 1 and 2 executes successfully.
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), multisig_addr)
        .await;
    let tx2 = TestTransactionBuilder::new(multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build_and_sign_multisig(multisig_pk.clone(), &[&keys[1], &keys[2]], 0b110);
    let res = context.execute_transaction_must_succeed(tx2).await;
    assert!(res.status_ok().unwrap());

    // 3. signature 2 and 1 swapped fails to execute.
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), multisig_addr)
        .await;
    let tx3 = TestTransactionBuilder::new(multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build_and_sign_multisig(multisig_pk.clone(), &[&keys[2], &keys[1]], 0b110);
    let res = context.execute_transaction_may_fail(tx3).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Invalid sig for pk=AQIOF81ZOeRrGWZBlozXWZELold+J/pz/eOHbbm+xbzrKw=="));

    // 4. sign with key 0 only is below threshold, fails to execute.
    let tx4 = TestTransactionBuilder::new(multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build_and_sign_multisig(multisig_pk.clone(), &[&keys[0]], 0b001);
    let res = context.execute_transaction_may_fail(tx4).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Insufficient weight=1 threshold=2"));

    // 5. multisig with no single sig fails to execute.
    let tx5 = TestTransactionBuilder::new(multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build_and_sign_multisig(multisig_pk.clone(), &[], 0b001);
    let res = context.execute_transaction_may_fail(tx5).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Invalid value was given to the function"));

    // 6. multisig two dup sigs fails to execute.
    let tx6 = TestTransactionBuilder::new(multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build_and_sign_multisig(multisig_pk.clone(), &[&keys[0], &keys[0]], 0b011);
    let res = context.execute_transaction_may_fail(tx6).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Invalid ed25519 pk bytes"));

    // 7. mismatch pks in sig with multisig address fails to execute.
    let kp3: SuiKeyPair = SuiKeyPair::Secp256r1(get_key_pair().1);
    let pk3 = kp3.public();
    let wrong_multisig_pk = MultiSigPublicKey::new(
        vec![pk0.clone(), pk1.clone(), pk3.clone()],
        vec![1, 1, 1],
        2,
    )
    .unwrap();
    let wrong_sender = SuiAddress::from(&wrong_multisig_pk);
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), wrong_sender)
        .await;
    let tx7 = TestTransactionBuilder::new(wrong_sender, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build_and_sign_multisig(wrong_multisig_pk.clone(), &[&keys[0], &keys[2]], 0b101);
    let res = context.execute_transaction_may_fail(tx7).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains(format!("Invalid sig for pk={}", pk3.encode_base64()).as_str()));
}

#[sim_test]
async fn test_multisig_with_zklogin_scenerios() {
    let test_cluster = TestClusterBuilder::new()
        // Use a long epoch duration such that it won't change epoch on its own.
        .with_epoch_duration_ms(10000000)
        .with_default_jwks()
        .build()
        .await;

    // Wait a bit for JWKs to be propagated.
    test_cluster.wait_for_authenticator_state_update().await;
    // Manually trigger epoch change to be able to test zklogin with multiple epochs.
    test_cluster.trigger_reconfiguration().await;

    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &test_cluster.wallet;

    let keys = keys();
    let pk0 = keys[0].public(); // ed25519
    let pk1 = keys[1].public(); // secp256k1
    let pk2 = keys[2].public(); // secp256r1

    // construct a multisig address with 4 pks (ed25519, secp256k1, secp256r1, zklogin) with threshold = 1.
    let (eph_kp, _eph_pk, zklogin_inputs) =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];
    let (eph_kp_1, _, _) =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[2];
    let zklogin_pk = PublicKey::ZkLogin(
        ZkLoginPublicIdentifier::new(zklogin_inputs.get_iss(), zklogin_inputs.get_address_seed())
            .unwrap(),
    );
    let multisig_pk = MultiSigPublicKey::new(
        vec![pk0.clone(), pk1.clone(), pk2.clone(), zklogin_pk.clone()],
        vec![1, 1, 1, 1],
        1,
    )
    .unwrap();

    // fund the multisig address.
    let multisig_addr = SuiAddress::from(&multisig_pk);
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), multisig_addr)
        .await;
    let tx_data = TestTransactionBuilder::new(multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let wrong_intent_msg = IntentMessage::new(Intent::personal_message(), tx_data.clone());

    // 1. a multisig with a bad ed25519 sig fails to execute.
    let wrong_sig: GenericSignature = Signature::new_secure(&wrong_intent_msg, &keys[0]).into();
    let multisig = GenericSignature::MultiSig(
        MultiSig::combine(vec![wrong_sig], multisig_pk.clone()).unwrap(),
    );
    let tx_1 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let res = context.execute_transaction_may_fail(tx_1).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains(format!("Invalid sig for pk={}", pk0.encode_base64()).as_str()));

    // 2. a multisig with a bad secp256k1 sig fails to execute.
    let wrong_sig_2: GenericSignature = Signature::new_secure(&wrong_intent_msg, &keys[1]).into();
    let multisig = GenericSignature::MultiSig(
        MultiSig::combine(vec![wrong_sig_2], multisig_pk.clone()).unwrap(),
    );
    let tx_2 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let res = context.execute_transaction_may_fail(tx_2).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains(format!("Invalid sig for pk={}", pk1.encode_base64()).as_str()));

    // 3. a multisig with a bad secp256r1 sig fails to execute.
    let wrong_sig_3: GenericSignature = Signature::new_secure(&wrong_intent_msg, &keys[2]).into();
    let multisig = GenericSignature::MultiSig(
        MultiSig::combine(vec![wrong_sig_3], multisig_pk.clone()).unwrap(),
    );
    let tx_3 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let res = context.execute_transaction_may_fail(tx_3).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains(format!("Invalid sig for pk={}", pk2.encode_base64()).as_str()));

    // 4. a multisig with a bad ephemeral sig inside zklogin sig fails to execute.
    let wrong_eph_sig = Signature::new_secure(&wrong_intent_msg, eph_kp);
    let wrong_zklogin_sig = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
        zklogin_inputs.clone(),
        2,
        wrong_eph_sig,
    ));
    let multisig = GenericSignature::MultiSig(
        MultiSig::combine(vec![wrong_zklogin_sig], multisig_pk.clone()).unwrap(),
    );
    let tx_4 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let res = context.execute_transaction_may_fail(tx_4).await;
    let pk3 = PublicKey::ZkLogin(
        ZkLoginPublicIdentifier::new(zklogin_inputs.get_iss(), zklogin_inputs.get_address_seed())
            .unwrap(),
    );
    assert!(res
        .unwrap_err()
        .to_string()
        .contains(format!("Invalid sig for pk={}", pk3.encode_base64()).as_str()));

    // 5. a multisig with a mismatch ephermeal sig and zklogin inputs fails to execute.
    let eph_sig = Signature::new_secure(&intent_msg, eph_kp_1);
    let zklogin_sig_mismatch = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
        zklogin_inputs.clone(),
        2,
        eph_sig,
    ));
    let multisig = GenericSignature::MultiSig(
        MultiSig::combine(vec![zklogin_sig_mismatch], multisig_pk.clone()).unwrap(),
    );
    let tx_5 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let res = context.execute_transaction_may_fail(tx_5).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains(format!("Invalid sig for pk={}", pk3.encode_base64()).as_str()));

    // 6. a multisig with an inconsistent max_epoch with zk proof itself fails to execute.
    let eph_sig = Signature::new_secure(&intent_msg, eph_kp);
    let zklogin_sig_wrong_zklogin_inputs = GenericSignature::ZkLoginAuthenticator(
        ZkLoginAuthenticator::new(zklogin_inputs.clone(), 1, eph_sig), // max_epoch set to 1 instead of 2
    );
    let multisig = GenericSignature::MultiSig(
        MultiSig::combine(vec![zklogin_sig_wrong_zklogin_inputs], multisig_pk.clone()).unwrap(),
    );
    let tx_7 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let res = context.execute_transaction_may_fail(tx_7).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Groth16 proof verify failed"));

    // 7. a multisig with the wrong sender fails to execute.
    let wrong_multisig_addr = SuiAddress::from(
        &MultiSigPublicKey::new(
            vec![pk0.clone(), pk1.clone(), pk2.clone()],
            vec![1, 1, 1],
            1,
        )
        .unwrap(),
    );
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), wrong_multisig_addr)
        .await;
    let tx_data = TestTransactionBuilder::new(wrong_multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let sig_4: GenericSignature = ZkLoginAuthenticator::new(
        zklogin_inputs.clone(),
        2,
        Signature::new_secure(&intent_msg, eph_kp),
    )
    .into();
    let multisig =
        GenericSignature::MultiSig(MultiSig::combine(vec![sig_4], multisig_pk.clone()).unwrap());
    let tx_8 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let res = context.execute_transaction_may_fail(tx_8).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Required Signature from"));

    // 8. a multisig with zklogin sig of invalid compact signature bytes fails to execute.
    let multisig = GenericSignature::MultiSig(MultiSig::insecure_new(
        vec![CompressedSignature::ZkLogin(ZkLoginAuthenticatorAsBytes(
            vec![0],
        ))],
        0b1000,
        multisig_pk.clone(),
    ));
    let sender = SuiAddress::try_from(&multisig).unwrap();
    let tx_data = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();

    let tx_7 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let res = context.execute_transaction_may_fail(tx_7).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Invalid zklogin authenticator bytes"));

    // assert positive case for all 4 participanting parties.
    // 1a. good ed25519 sig used in multisig executes successfully.
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), multisig_addr)
        .await;
    let tx_data = TestTransactionBuilder::new(multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let sig_0: GenericSignature = Signature::new_secure(&intent_msg, &keys[0]).into();
    let multisig =
        GenericSignature::MultiSig(MultiSig::combine(vec![sig_0], multisig_pk.clone()).unwrap());
    let tx_8 = Transaction::from_generic_sig_data(tx_data, vec![multisig]);
    let _ = context.execute_transaction_must_succeed(tx_8).await;

    // 2a. good secp256k1 sig used in multisig executes successfully.
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), multisig_addr)
        .await;
    let tx_data = TestTransactionBuilder::new(multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let sig_1: GenericSignature = Signature::new_secure(&intent_msg, &keys[1]).into();
    let multisig =
        GenericSignature::MultiSig(MultiSig::combine(vec![sig_1], multisig_pk.clone()).unwrap());
    let tx_9 = Transaction::from_generic_sig_data(tx_data, vec![multisig]);
    let _ = context.execute_transaction_must_succeed(tx_9).await;

    // 3a. good secp256r1 sig used in multisig executes successfully.
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), multisig_addr)
        .await;
    let tx_data = TestTransactionBuilder::new(multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let sig_2: GenericSignature = Signature::new_secure(&intent_msg, &keys[2]).into();
    let multisig =
        GenericSignature::MultiSig(MultiSig::combine(vec![sig_2], multisig_pk.clone()).unwrap());
    let tx_9 = Transaction::from_generic_sig_data(tx_data, vec![multisig]);
    let _ = context.execute_transaction_must_succeed(tx_9).await;

    // 4b. good zklogin sig used in multisig executes successfully.
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), multisig_addr)
        .await;
    let tx_data = TestTransactionBuilder::new(multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let sig_4: GenericSignature = ZkLoginAuthenticator::new(
        zklogin_inputs.clone(),
        2,
        Signature::new_secure(&intent_msg, eph_kp),
    )
    .into();
    let multisig =
        GenericSignature::MultiSig(MultiSig::combine(vec![sig_4], multisig_pk.clone()).unwrap());
    let tx_10 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let _ = context.execute_transaction_must_succeed(tx_10).await;

    // 4c. good zklogin sig AND good ed25519 combined used in multisig executes successfully.
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), multisig_addr)
        .await;
    let tx_data = TestTransactionBuilder::new(multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let sig: GenericSignature = Signature::new_secure(&intent_msg, &keys[0]).into();
    let sig_1: GenericSignature = ZkLoginAuthenticator::new(
        zklogin_inputs.clone(),
        2,
        Signature::new_secure(&intent_msg, eph_kp),
    )
    .into();
    let multisig = GenericSignature::MultiSig(
        MultiSig::combine(vec![sig, sig_1], multisig_pk.clone()).unwrap(),
    );
    let tx_11 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let _ = context.execute_transaction_must_succeed(tx_11).await;

    // 9. wrong bitmap fails to execute.
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), multisig_addr)
        .await;
    let tx_data = TestTransactionBuilder::new(multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let sig: GenericSignature = Signature::new_secure(&intent_msg, &keys[0]).into();
    let multisig = GenericSignature::MultiSig(MultiSig::insecure_new(
        vec![sig.to_compressed().unwrap()],
        0b0010,
        multisig_pk.clone(),
    ));
    let tx_11 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let res = context.execute_transaction_may_fail(tx_11).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Invalid ed25519 pk bytes"));

    // 10. invalid bitmap b10000 when the max bitmap for 4 pks is b1111, fails to execute.
    let multisig = GenericSignature::MultiSig(MultiSig::insecure_new(
        vec![sig.clone().to_compressed().unwrap()],
        1 << 4,
        multisig_pk.clone(),
    ));
    let tx_10 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let res = context.execute_transaction_may_fail(tx_10).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Invalid public keys index"));

    // 11. malformed multisig pk where threshold = 0, fails to execute.
    let bad_multisig_pk = MultiSigPublicKey::insecure_new(
        vec![(pk0.clone(), 1), (pk1.clone(), 1), (pk2.clone(), 1)],
        0,
    );
    let bad_multisig_addr = SuiAddress::from(&bad_multisig_pk);
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), bad_multisig_addr)
        .await;
    let tx_data = TestTransactionBuilder::new(bad_multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let sig: GenericSignature = Signature::new_secure(&intent_msg, &keys[0]).into();
    let multisig = GenericSignature::MultiSig(MultiSig::insecure_new(
        vec![sig.to_compressed().unwrap()],
        0b001,
        bad_multisig_pk.clone(),
    ));
    let tx_11 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let res = context.execute_transaction_may_fail(tx_11).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Invalid value was given to the function"));

    // 12. malformed multisig a pk has weight = 0, fails to execute.
    let bad_multisig_pk_2 = MultiSigPublicKey::insecure_new(
        vec![(pk0.clone(), 1), (pk1.clone(), 1), (pk2.clone(), 0)],
        1,
    );
    let bad_multisig_addr_2 = SuiAddress::from(&bad_multisig_pk_2);
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), bad_multisig_addr_2)
        .await;
    let tx_data = TestTransactionBuilder::new(bad_multisig_addr_2, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let sig: GenericSignature = Signature::new_secure(&intent_msg, &keys[0]).into();
    let multisig = GenericSignature::MultiSig(MultiSig::insecure_new(
        vec![sig.to_compressed().unwrap()],
        2,
        bad_multisig_pk,
    ));
    let tx_14 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let res = context.execute_transaction_may_fail(tx_14).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Invalid value was given to the function"));

    // 13. pass in 2 sigs when only 1 pk in multisig_pk, fails to execute.
    let small_multisig_pk = MultiSigPublicKey::insecure_new(vec![(pk0.clone(), 1)], 1);
    let bad_multisig_addr_3 = SuiAddress::from(&small_multisig_pk);
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), bad_multisig_addr_3)
        .await;
    let tx_data = TestTransactionBuilder::new(bad_multisig_addr_3, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let sig: GenericSignature = Signature::new_secure(&intent_msg, &keys[0]).into();
    let multisig = GenericSignature::MultiSig(MultiSig::insecure_new(
        vec![sig.to_compressed().unwrap(), sig.to_compressed().unwrap()],
        0b1,
        small_multisig_pk,
    ));
    let tx_13 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let res = context.execute_transaction_may_fail(tx_13).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Invalid value was given to the function"));

    // 14. pass a multisig where there is dup pk in multisig_pk, fails to execute.
    let multisig_pk_with_dup =
        MultiSigPublicKey::insecure_new(vec![(pk0.clone(), 1), (pk0.clone(), 1)], 1);
    let bad_multisig_addr_4 = SuiAddress::from(&multisig_pk_with_dup);
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), bad_multisig_addr_4)
        .await;
    let tx_data = TestTransactionBuilder::new(bad_multisig_addr_4, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let sig: GenericSignature = Signature::new_secure(&intent_msg, &keys[0]).into();
    let multisig = GenericSignature::MultiSig(MultiSig::insecure_new(
        vec![sig.to_compressed().unwrap()],
        0b01,
        multisig_pk_with_dup,
    ));
    let tx_14 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let res = context.execute_transaction_may_fail(tx_14).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Invalid value was given to the function"));

    // 15. a sig with 11 pks fails to execute.
    let multisig_pk_11 = MultiSigPublicKey::insecure_new(vec![(pk0.clone(), 1); 11], 1);
    let bad_multisig_addr_11 = SuiAddress::from(&multisig_pk_11);
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), bad_multisig_addr_11)
        .await;
    let tx_data = TestTransactionBuilder::new(bad_multisig_addr_11, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let sig: GenericSignature = Signature::new_secure(&intent_msg, &keys[0]).into();
    let multisig = GenericSignature::MultiSig(MultiSig::insecure_new(
        vec![sig.to_compressed().unwrap()],
        0b00000000001,
        multisig_pk_11,
    ));
    let tx_15 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let res = context.execute_transaction_may_fail(tx_15).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Invalid value was given to the function"));

    // 16. total weight of all pks < threshold fails to execute.
    let multisig_pk_12 =
        MultiSigPublicKey::insecure_new(vec![(pk0.clone(), 1), (pk0.clone(), 1)], 3);
    let bad_multisig_addr = SuiAddress::from(&multisig_pk_12);
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), bad_multisig_addr)
        .await;
    let tx_data = TestTransactionBuilder::new(bad_multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let sig: GenericSignature = Signature::new_secure(&intent_msg, &keys[0]).into();
    let multisig = GenericSignature::MultiSig(MultiSig::insecure_new(
        vec![sig.to_compressed().unwrap()],
        0b00000000001,
        multisig_pk_12,
    ));
    let tx_16 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let res = context.execute_transaction_may_fail(tx_16).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Invalid value was given to the function"));

    // 17. multisig with empty pk map fails to execute.
    let bad_multisig_empty_pk = MultiSigPublicKey::insecure_new(vec![], 1);
    let bad_multisig_addr = SuiAddress::from(&bad_multisig_empty_pk);
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), bad_multisig_addr)
        .await;
    let tx_data = TestTransactionBuilder::new(bad_multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let sig: GenericSignature = Signature::new_secure(&intent_msg, &keys[0]).into();
    let multisig = GenericSignature::MultiSig(MultiSig::insecure_new(
        vec![sig.to_compressed().unwrap()],
        0b01,
        bad_multisig_empty_pk,
    ));
    let tx_17 = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let res = context.execute_transaction_may_fail(tx_17).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Invalid value was given to the function"));
}

#[sim_test]
async fn test_expired_epoch_zklogin_in_multisig() {
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(15000)
        .with_default_jwks()
        .build()
        .await;
    test_cluster.wait_for_epoch(Some(3)).await;
    // construct tx with max_epoch set to 2.
    let (tx, legacy_tx) = construct_simple_zklogin_multisig_tx(&test_cluster).await;

    // latest multisig fails for expired epoch.
    let res = test_cluster.wallet.execute_transaction_may_fail(tx).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("ZKLogin expired at epoch 2"));

    // legacy multisig also faiils for expired epoch.
    let res = test_cluster
        .wallet
        .execute_transaction_may_fail(legacy_tx)
        .await;
    assert!(res.is_err());
}

#[sim_test]
async fn test_max_epoch_too_large_fail_zklogin_in_multisig() {
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
    // both tx with max_epoch set to 2.
    let (tx, legacy_tx) = construct_simple_zklogin_multisig_tx(&test_cluster).await;

    // max epoch at 2 is larger than current epoch (0) + upper bound (1), tx fails.
    let res = test_cluster.wallet.execute_transaction_may_fail(tx).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("ZKLogin max epoch too large"));

    // legacy tx fails for the same reason
    let res = test_cluster
        .wallet
        .execute_transaction_may_fail(legacy_tx)
        .await;
    assert!(res.is_err());
}

#[sim_test]
async fn test_random_zklogin_in_multisig() {
    let test_vectors =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1..11];
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(15000)
        .with_default_jwks()
        .build()
        .await;
    test_cluster.wait_for_authenticator_state_update().await;

    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &test_cluster.wallet;

    // create a multisig with 10 zklogin pks.
    let pks = test_vectors.iter().map(|(_, pk, _)| pk.clone()).collect();
    let multisig_pk = MultiSigPublicKey::new(pks, vec![1; 10], 10).unwrap();
    let multisig_addr = SuiAddress::from(&multisig_pk);
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), multisig_addr)
        .await;
    let tx_data = TestTransactionBuilder::new(multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let mut zklogin_sigs = vec![];
    for (kp, _pk, inputs) in test_vectors {
        let eph_sig = Signature::new_secure(&intent_msg, kp);
        let zklogin_sig = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
            inputs.clone(),
            2,
            eph_sig,
        ));
        zklogin_sigs.push(zklogin_sig);
    }
    let short_multisig = GenericSignature::MultiSig(
        MultiSig::combine(zklogin_sigs[..9].to_vec(), multisig_pk.clone()).unwrap(),
    );
    let bad_tx = Transaction::from_generic_sig_data(tx_data.clone(), vec![short_multisig]);
    let res = context.execute_transaction_may_fail(bad_tx).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Insufficient weight=9 threshold=10"));

    let multisig = GenericSignature::MultiSig(
        MultiSig::combine(zklogin_sigs.clone(), multisig_pk.clone()).unwrap(),
    );
    let tx = Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]);
    let _ = context.execute_transaction_must_succeed(tx).await;
}
#[sim_test]
async fn test_multisig_legacy_works() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;

    let keys = keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();
    let pk3 = keys[2].public();

    let multisig_pk_legacy = MultiSigPublicKeyLegacy::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone()],
        vec![1, 1, 1],
        2,
    )
    .unwrap();
    let multisig_pk = MultiSigPublicKey::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone()],
        vec![1, 1, 1],
        2,
    )
    .unwrap();
    let multisig_addr = SuiAddress::from(&multisig_pk);
    let context = &test_cluster.wallet;
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), multisig_addr)
        .await;
    let transfer_from_multisig = TestTransactionBuilder::new(multisig_addr, gas, rgp)
        .transfer_sui(Some(1000000), SuiAddress::ZERO)
        .build_and_sign_multisig_legacy(multisig_pk_legacy, &[&keys[0], &keys[1]]);

    context
        .execute_transaction_must_succeed(transfer_from_multisig)
        .await;
}

#[sim_test]
async fn test_zklogin_inside_multisig_feature_deny() {
    // if feature disabled, fails to execute.
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_accept_zklogin_in_multisig_for_testing(false);
        config
    });
    let test_cluster = TestClusterBuilder::new()
        .with_default_jwks()
        .with_epoch_duration_ms(15000)
        .build()
        .await;
    test_cluster.wait_for_authenticator_state_update().await;
    let (tx, legacy_tx) = construct_simple_zklogin_multisig_tx(&test_cluster).await;
    // feature flag disabled fails latest multisig tx.
    let res = test_cluster.wallet.execute_transaction_may_fail(tx).await;
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("zkLogin sig not supported inside multisig"));

    // legacy multisig fails for the same reason.
    let res = test_cluster
        .wallet
        .execute_transaction_may_fail(legacy_tx)
        .await;
    assert!(res.is_err());
}

async fn construct_simple_zklogin_multisig_tx(
    test_cluster: &TestCluster,
) -> (Transaction, Transaction) {
    // construct a multisig address with 1 zklogin pk with threshold = 1.
    let (eph_kp, _eph_pk, zklogin_inputs) =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];
    let zklogin_pk = PublicKey::ZkLogin(
        ZkLoginPublicIdentifier::new(zklogin_inputs.get_iss(), zklogin_inputs.get_address_seed())
            .unwrap(),
    );
    let multisig_pk = MultiSigPublicKey::insecure_new(vec![(zklogin_pk.clone(), 1)], 1);
    let multisig_pk_legacy =
        MultiSigPublicKeyLegacy::new(vec![zklogin_pk.clone()], vec![1], 1).unwrap();
    let rgp = test_cluster.get_reference_gas_price().await;

    let multisig_addr = SuiAddress::from(&multisig_pk);
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), multisig_addr)
        .await;
    let tx_data = TestTransactionBuilder::new(multisig_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let sig_4: GenericSignature = ZkLoginAuthenticator::new(
        zklogin_inputs.clone(),
        2,
        Signature::new_secure(&intent_msg, eph_kp),
    )
    .into();
    let multisig = GenericSignature::MultiSig(
        MultiSig::combine(vec![sig_4.clone()], multisig_pk.clone()).unwrap(),
    );
    let multisig_legacy = GenericSignature::MultiSigLegacy(
        MultiSigLegacy::combine(vec![sig_4.clone()], multisig_pk_legacy.clone()).unwrap(),
    );
    (
        Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig]),
        Transaction::from_generic_sig_data(tx_data.clone(), vec![multisig_legacy]),
    )
}
