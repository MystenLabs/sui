// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use fastcrypto::{
    ed25519::Ed25519KeyPair,
    traits::{KeyPair, ToFromBytes},
};
use p256::pkcs8::DecodePublicKey;
use passkey_authenticator::Authenticator;
use passkey_client::Client;
use passkey_types::{
    ctap2::Aaguid,
    rand::random_vec,
    webauthn::{
        AttestationConveyancePreference, CredentialCreationOptions, CredentialRequestOptions,
        PublicKeyCredentialCreationOptions, PublicKeyCredentialParameters,
        PublicKeyCredentialRequestOptions, PublicKeyCredentialRpEntity, PublicKeyCredentialType,
        PublicKeyCredentialUserEntity, UserVerificationRequirement,
    },
    Bytes, Passkey,
};
use rand::{rngs::StdRng, SeedableRng};
use shared_crypto::intent::{Intent, IntentMessage};
use sui_macros::sim_test;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::error::SuiError;
use sui_types::error::UserInputError;
use sui_types::signature::GenericSignature;
use sui_types::transaction::Transaction;
use sui_types::{
    base_types::SuiAddress,
    crypto::{PublicKey, SignatureScheme},
    transaction::TransactionData,
};
use sui_types::{
    crypto::{get_key_pair, Signature},
    passkey_session_authenticator::RawPasskeySessionAuthenticator,
};
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use url::Url;

pub mod passkey_util;
use passkey_util::{execute_tx, MyUserValidationMethod, PasskeyResponse};

async fn make_passkey_session_tx(
    test_cluster: &TestCluster,
    eph_kp: Ed25519KeyPair,
    max_epoch: u64,
    register_msg: Option<Vec<u8>>,
    wrong_register_sig: bool,
    wrong_ephemeral_sig: bool,
    wrong_sender: bool,
) -> Transaction {
    let register_msg = match register_msg {
        Some(msg) => msg,
        None => {
            let mut register_msg = vec![SignatureScheme::ED25519.flag()];
            register_msg.extend_from_slice(eph_kp.public().as_bytes());
            register_msg.extend_from_slice(&max_epoch.to_be_bytes());
            register_msg
        }
    };

    let response =
        create_credential_and_commit_ephemeral_pk(test_cluster, wrong_sender, register_msg).await;
    let ephemeral_signature = Signature::new_secure(&response.intent_msg, &eph_kp);

    let passkey_session_authenticator = RawPasskeySessionAuthenticator {
        authenticator_data: response.authenticator_data,
        client_data_json: response.client_data_json,
        passkey_signature: if wrong_register_sig {
            let mut fake_signature = response.user_sig_bytes.clone();
            fake_signature[2] += 1;
            Signature::from_bytes(&fake_signature).unwrap()
        } else {
            Signature::from_bytes(&response.user_sig_bytes).unwrap()
        },
        max_epoch,
        ephemeral_signature: if wrong_ephemeral_sig {
            let mut fake_signature = ephemeral_signature.as_bytes().to_vec().clone();
            fake_signature[2] += 1;
            Signature::from_bytes(&fake_signature).unwrap()
        } else {
            ephemeral_signature
        },
    }
    .try_into()
    .unwrap();
    let sig = GenericSignature::PasskeySessionAuthenticator(passkey_session_authenticator);
    Transaction::from_generic_sig_data(response.intent_msg.value, vec![sig])
}

/// Register a new passkey, derive its address, fund it with gas and create a test
/// transaction, then get a response from the passkey from signing.
async fn create_credential_and_commit_ephemeral_pk(
    test_cluster: &TestCluster,
    wrong_sender: bool,
    passkey_challenge: Vec<u8>,
) -> PasskeyResponse<TransactionData> {
    // set up authenticator and client
    let my_aaguid = Aaguid::new_empty();
    let user_validation_method = MyUserValidationMethod {};
    let store: Option<Passkey> = None;
    let my_authenticator = Authenticator::new(my_aaguid, store, user_validation_method);
    let mut my_client = Client::new(my_authenticator);
    let origin = Url::parse("https://www.sui.io").unwrap();

    // Create credential.
    let challenge_bytes_from_rp: Bytes = random_vec(32).into();
    let user_entity = PublicKeyCredentialUserEntity {
        id: random_vec(32).into(),
        display_name: "Johnny Passkey".into(),
        name: "jpasskey@example.org".into(),
    };
    let request = CredentialCreationOptions {
        public_key: PublicKeyCredentialCreationOptions {
            rp: PublicKeyCredentialRpEntity {
                id: None, // Leaving the ID as None means use the effective domain
                name: origin.domain().unwrap().into(),
            },
            user: user_entity,
            challenge: challenge_bytes_from_rp,
            pub_key_cred_params: vec![PublicKeyCredentialParameters {
                ty: PublicKeyCredentialType::PublicKey,
                alg: coset::iana::Algorithm::ES256,
            }],
            timeout: None,
            exclude_credentials: None,
            authenticator_selection: None,
            hints: None,
            attestation: AttestationConveyancePreference::None,
            attestation_formats: None,
            extensions: None,
        },
    };
    let my_webauthn_credential = my_client.register(&origin, request, None).await.unwrap();
    let verifying_key = p256::ecdsa::VerifyingKey::from_public_key_der(
        my_webauthn_credential
            .response
            .public_key
            .unwrap()
            .as_slice(),
    )
    .unwrap();

    // Derive compact pubkey from DER format.
    let encoded_point = verifying_key.to_encoded_point(false);
    let x = encoded_point.x();
    let y = encoded_point.y();
    let prefix = if y.unwrap()[31] % 2 == 0 { 0x02 } else { 0x03 };
    let mut pk_bytes = vec![prefix];
    pk_bytes.extend_from_slice(x.unwrap());
    let sender = if wrong_sender {
        (&PublicKey::try_from_bytes(SignatureScheme::PasskeyAuthenticator, &pk_bytes).unwrap())
            .into()
    } else {
        (&PublicKey::try_from_bytes(SignatureScheme::PasskeySessionAuthenticator, &pk_bytes)
            .unwrap())
            .into()
    };
    // Request a signature from passkey with challenge set to passkey_digest.
    let credential_request = CredentialRequestOptions {
        public_key: PublicKeyCredentialRequestOptions {
            challenge: Bytes::from(passkey_challenge),
            timeout: None,
            rp_id: Some(String::from(origin.domain().unwrap())),
            allow_credentials: None,
            user_verification: UserVerificationRequirement::default(),
            attestation: Default::default(),
            attestation_formats: None,
            extensions: None,
            hints: None,
        },
    };

    let authenticated_cred = my_client
        .authenticate(&origin, credential_request, None)
        .await
        .unwrap();

    // Parse signature from der format in response and normalize it to lower s.
    let sig_bytes_der = authenticated_cred.response.signature.as_slice();
    let sig = p256::ecdsa::Signature::from_der(sig_bytes_der).unwrap();
    let sig_bytes = sig.normalize_s().unwrap_or(sig).to_bytes();

    let mut user_sig_bytes = vec![SignatureScheme::Secp256r1.flag()];
    user_sig_bytes.extend_from_slice(&sig_bytes);
    user_sig_bytes.extend_from_slice(&pk_bytes);

    // Parse authenticator_data and client_data_json from response.
    let authenticator_data = authenticated_cred.response.authenticator_data.as_slice();
    let client_data_json = authenticated_cred.response.client_data_json.as_slice();

    // fund gas and make a test transaction.
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), sender)
        .await;
    let tx_data = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data);

    PasskeyResponse {
        sender,
        user_sig_bytes,
        authenticator_data: authenticator_data.to_vec(),
        client_data_json: String::from_utf8_lossy(client_data_json).to_string(),
        intent_msg,
    }
}

#[sim_test]
async fn test_passkey_session_feature_deny() {
    use sui_protocol_config::ProtocolConfig;
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_passkey_session_auth_for_testing(false);
        config
    });
    let test_cluster = TestClusterBuilder::new().build().await;

    let kp: Ed25519KeyPair = get_key_pair().1;
    let max_epoch = 2_u64;
    let tx = make_passkey_session_tx(&test_cluster, kp, max_epoch, None, false, false, false).await;
    let err = execute_tx(tx, &test_cluster).await.unwrap_err();
    assert!(matches!(
        err,
        SuiError::UserInputError {
            error: UserInputError::Unsupported(..)
        }
    ));
}

#[sim_test]
async fn test_passkey_authenticator_scenarios() {
    use sui_protocol_config::ProtocolConfig;
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(15000)
        .with_default_jwks()
        .build()
        .await;

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_zklogin_max_epoch_upper_bound_delta_for_testing(Some(1));
        config
    });
    let kp = Ed25519KeyPair::generate(&mut StdRng::from_seed([0u8; 32]));
    let max_epoch = 1_u64;

    // case 1: sign a tx with passkey ephemeral sig and register sig, passes
    let tx = make_passkey_session_tx(
        &test_cluster,
        kp.copy(),
        max_epoch,
        None,
        false,
        false,
        false,
    )
    .await;
    let res = execute_tx(tx, &test_cluster).await;
    assert!(res.is_ok());

    // case 2: use a mismatched ephemeral key with what the register signature commits to, fails to verify.
    let kp2: Ed25519KeyPair = get_key_pair().1;
    let mut register_msg = vec![SignatureScheme::ED25519.flag()];
    register_msg.extend_from_slice(kp2.public().as_bytes());
    register_msg.extend_from_slice(&max_epoch.to_be_bytes());
    let tx = make_passkey_session_tx(
        &test_cluster,
        kp.copy(),
        max_epoch,
        Some(register_msg),
        false,
        false,
        false,
    )
    .await;
    let res = execute_tx(tx, &test_cluster).await;
    assert!(matches!(
        res,
        Err(SuiError::InvalidSignature { error }) if error == "Invalid parsed challenge"
    ));

    // case 3: use a mismatched max_epoch with what the register signature commits to, fails to verify.
    let mut register_msg = vec![SignatureScheme::ED25519.flag()];
    register_msg.extend_from_slice(kp.public().as_bytes());
    register_msg.extend_from_slice(&(max_epoch + 1).to_be_bytes());
    let tx = make_passkey_session_tx(
        &test_cluster,
        kp.copy(),
        max_epoch,
        Some(register_msg),
        false,
        false,
        false,
    )
    .await;
    let res = execute_tx(tx, &test_cluster).await;
    assert!(matches!(
        res,
        Err(SuiError::InvalidSignature { error }) if error == "Invalid parsed challenge"
    ));

    // case 4: invalid register signature fails to verify.
    let tx = make_passkey_session_tx(
        &test_cluster,
        kp.copy(),
        max_epoch,
        None,
        true,
        false,
        false,
    )
    .await;
    let res = execute_tx(tx, &test_cluster).await;
    assert!(matches!(
        res,
        Err(SuiError::InvalidSignature { error }) if error == "Fails to verify register sig"
    ));
    // case 5: invalid ephemeral signature fails to verify.
    let tx = make_passkey_session_tx(
        &test_cluster,
        kp.copy(),
        max_epoch,
        None,
        false,
        true,
        false,
    )
    .await;
    let res = execute_tx(tx, &test_cluster).await;
    assert!(matches!(
        res,
        Err(SuiError::InvalidSignature { error }) if error == "Fails to verify ephemeral sig"
    ));
    // case 6: advance 2 epochs, the ephermal sig expires, fails to verify
    test_cluster.trigger_reconfiguration().await;
    test_cluster.trigger_reconfiguration().await;

    let tx = make_passkey_session_tx(
        &test_cluster,
        kp.copy(),
        max_epoch,
        None,
        false,
        false,
        false,
    )
    .await;
    let res = execute_tx(tx, &test_cluster).await;
    assert!(matches!(
        res,
        Err(SuiError::InvalidSignature { error }) if error == "Passkey session expired at epoch 1, current epoch 2"
    ));
    // case 4: max_epoch bound delta = 1, but max epoch (10) - current epoch (2) > 1, too large, fails
    let tx = make_passkey_session_tx(&test_cluster, kp.copy(), 10, None, false, false, false).await;
    let res = execute_tx(tx, &test_cluster).await;
    assert!(matches!(
        res,
        Err(SuiError::InvalidSignature { error }) if error == "Passkey session max epoch too large 10, current epoch 2, max accepted: 3"
    ));
}

#[sim_test]
async fn test_passkey_fails_wrong_author() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let kp = Ed25519KeyPair::generate(&mut StdRng::from_seed([0u8; 32]));
    let max_epoch = 1_u64;
    let tx = make_passkey_session_tx(
        &test_cluster,
        kp.copy(),
        max_epoch,
        None,
        false,
        false,
        true,
    )
    .await;
    let err = execute_tx(tx, &test_cluster).await.unwrap_err();
    assert!(matches!(err, SuiError::SignerSignatureAbsent { .. }));
}
