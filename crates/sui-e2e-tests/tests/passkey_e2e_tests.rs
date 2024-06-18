// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use fastcrypto::traits::ToFromBytes;
use p256::pkcs8::DecodePublicKey;
use passkey::{
    authenticator::{Authenticator, UserValidationMethod},
    client::Client,
    types::{
        ctap2::Aaguid,
        rand::random_vec,
        webauthn::{
            AttestationConveyancePreference, CredentialCreationOptions, CredentialRequestOptions,
            PublicKeyCredentialCreationOptions, PublicKeyCredentialParameters,
            PublicKeyCredentialRequestOptions, PublicKeyCredentialRpEntity,
            PublicKeyCredentialType, PublicKeyCredentialUserEntity, UserVerificationRequirement,
        },
        Bytes, Passkey,
    },
};
use shared_crypto::intent::{Intent, IntentMessage};
use std::net::SocketAddr;
use sui_core::authority_client::AuthorityAPI;
use sui_macros::sim_test;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::crypto::Signature;
use sui_types::error::{SuiError, SuiResult};
use sui_types::signature::GenericSignature;
use sui_types::transaction::Transaction;
use sui_types::{
    base_types::SuiAddress,
    crypto::{PublicKey, SignatureScheme},
    passkey_authenticator::{to_signing_digest, PasskeyAuthenticator},
    transaction::TransactionData,
};
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use url::Url;
struct MyUserValidationMethod {}
#[async_trait::async_trait]
impl UserValidationMethod for MyUserValidationMethod {
    async fn check_user_presence(&self) -> bool {
        true
    }

    async fn check_user_verification(&self) -> bool {
        true
    }

    fn is_verification_enabled(&self) -> Option<bool> {
        Some(true)
    }

    fn is_presence_enabled(&self) -> bool {
        true
    }
}

/// A helper struct for passkey response and transaction construction.
pub struct PasskeyResponse<T> {
    user_sig_bytes: Vec<u8>,
    authenticator_data: Vec<u8>,
    client_data_json: Vec<u8>,
    intent_msg: IntentMessage<T>,
}

/// Submits a transaction to the test cluster and returns the result.
async fn execute_tx(tx: Transaction, test_cluster: &TestCluster) -> SuiResult {
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

/// Register a new passkey, derive its address, fund it with gas and create a test
/// transaction, then get a response from the passkey from signing.
async fn create_credential_and_sign_test_tx(
    test_cluster: &TestCluster,
    sender: Option<SuiAddress>,
) -> PasskeyResponse<TransactionData> {
    // set up authenticator and client
    let my_aaguid = Aaguid::new_empty();
    let user_validation_method = MyUserValidationMethod {};
    let store: Option<Passkey> = None;
    let my_authenticator = Authenticator::new(my_aaguid, store, user_validation_method);
    let mut my_client = Client::new(my_authenticator);
    let origin = Url::parse("https://www.sui.io").unwrap();

    // create credential
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

    // derive compact pubkey from DER format
    let encoded_point = verifying_key.to_encoded_point(false);
    let x = encoded_point.x();
    let y = encoded_point.y();
    let prefix = if y.unwrap()[31] % 2 == 0 { 0x02 } else { 0x03 };
    let mut pk_bytes = vec![prefix];
    pk_bytes.extend_from_slice(x.unwrap());
    let pk = PublicKey::try_from_bytes(SignatureScheme::PasskeyAuthenticator, &pk_bytes).unwrap();

    // compute sui address and make a test transaction
    let sender = match sender {
        Some(s) => s,
        None => SuiAddress::from(&pk),
    };
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), sender)
        .await;
    let tx_data = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data);

    // compute the challenge = blake2b_hash(intent_msg(tx)) for passkey credential request
    let passkey_digest = to_signing_digest(&intent_msg);

    // request a signature from passkey with challenge set to passkey_digest.
    let credential_request = CredentialRequestOptions {
        public_key: PublicKeyCredentialRequestOptions {
            challenge: Bytes::from(passkey_digest.to_vec()),
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

    // parse signature from der format in response and normalize it to lower s.
    let sig_bytes_der = authenticated_cred.response.signature.as_slice();
    let sig = p256::ecdsa::Signature::from_der(sig_bytes_der).unwrap();
    let sig_bytes = sig.normalize_s().unwrap_or(sig).to_bytes();

    let mut user_sig_bytes = vec![SignatureScheme::Secp256r1.flag()];
    user_sig_bytes.extend_from_slice(&sig_bytes);
    user_sig_bytes.extend_from_slice(&pk_bytes);

    // parse authenticator_data and client_data_json from response.
    let authenticator_data = authenticated_cred.response.authenticator_data.as_slice();
    let client_data_json = authenticated_cred.response.client_data_json.as_slice();

    PasskeyResponse {
        user_sig_bytes,
        authenticator_data: authenticator_data.to_vec(),
        client_data_json: client_data_json.to_vec(),
        intent_msg,
    }
}

fn make_passkey_tx(response: PasskeyResponse<TransactionData>) -> Transaction {
    let sig = GenericSignature::PasskeyAuthenticator(
        PasskeyAuthenticator::new_for_testing(
            response.authenticator_data,
            response.client_data_json,
            Signature::from_bytes(&response.user_sig_bytes).unwrap(),
        )
        .unwrap(),
    );
    Transaction::from_generic_sig_data(response.intent_msg.value, vec![sig])
}

#[sim_test]
async fn test_passkey_feature_deny() {
    use sui_protocol_config::ProtocolConfig;
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_passkey_auth_for_testing(false);
        config
    });
    let test_cluster = TestClusterBuilder::new().build().await;
    let response = create_credential_and_sign_test_tx(&test_cluster, None).await;
    let tx = make_passkey_tx(response);
    let err = execute_tx(tx, &test_cluster).await.unwrap_err();
    assert!(matches!(err, SuiError::UnsupportedFeatureError { .. }));
}

#[sim_test]
async fn test_passkey_authenticator_verifies() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let response = create_credential_and_sign_test_tx(&test_cluster, None).await;
    let tx = make_passkey_tx(response);
    let res = execute_tx(tx, &test_cluster).await;
    assert!(res.is_ok());
}

#[sim_test]
async fn test_passkey_fails_mismatched_challenge() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let response = create_credential_and_sign_test_tx(&test_cluster, None).await;
    let client_data_json_with_wrong_challenge = r#"{"type":"webauthn.get","challenge":"9-fH7nX8Nb1JvUynz77mv1kXOkGkg1msZb2qhvZssGI","origin":"http://localhost:5173","crossOrigin":false}"#.as_bytes();
    let sig = GenericSignature::PasskeyAuthenticator(
        PasskeyAuthenticator::new_for_testing(
            response.authenticator_data,
            client_data_json_with_wrong_challenge.to_vec(),
            Signature::from_bytes(&response.user_sig_bytes).unwrap(),
        )
        .unwrap(),
    );
    let tx = Transaction::from_generic_sig_data(response.intent_msg.value, vec![sig]);
    let res = execute_tx(tx, &test_cluster).await;
    let err = res.unwrap_err();
    assert_eq!(
        err,
        SuiError::InvalidSignature {
            error: "Invalid challenge".to_string()
        }
    );
}

#[sim_test]
async fn test_passkey_fails_to_verify_sig() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let response = create_credential_and_sign_test_tx(&test_cluster, None).await;
    let mut modified_sig = response.user_sig_bytes.clone();
    modified_sig[1] = 0x00;
    let sig = GenericSignature::PasskeyAuthenticator(
        PasskeyAuthenticator::new_for_testing(
            response.authenticator_data,
            response.client_data_json.to_vec(),
            Signature::from_bytes(&modified_sig).unwrap(),
        )
        .unwrap(),
    );
    let tx = Transaction::from_generic_sig_data(response.intent_msg.value, vec![sig]);
    let res = execute_tx(tx, &test_cluster).await;
    let err = res.unwrap_err();
    assert_eq!(
        err,
        SuiError::InvalidSignature {
            error: "Fails to verify".to_string()
        }
    );
}

#[sim_test]
async fn test_passkey_fails_wrong_author() {
    let test_cluster = TestClusterBuilder::new().build().await;
    // modify sender that receives gas and construct test txn.
    let response = create_credential_and_sign_test_tx(&test_cluster, Some(SuiAddress::ZERO)).await;
    let sig = GenericSignature::PasskeyAuthenticator(
        PasskeyAuthenticator::new_for_testing(
            response.authenticator_data,
            response.client_data_json.to_vec(),
            Signature::from_bytes(&response.user_sig_bytes).unwrap(),
        )
        .unwrap(),
    );
    let tx = Transaction::from_generic_sig_data(response.intent_msg.value, vec![sig]);
    let res = execute_tx(tx, &test_cluster).await;
    let err = res.unwrap_err();
    assert!(matches!(err, SuiError::SignerSignatureAbsent { .. }));
}
