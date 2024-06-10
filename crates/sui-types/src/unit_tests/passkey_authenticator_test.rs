// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use super::PasskeyAuthenticator;
use crate::{
    base_types::{dbg_addr, ObjectID, SuiAddress},
    crypto::{DefaultHash, PublicKey, Signature, SignatureScheme},
    object::Object,
    signature::GenericSignature,
    signature_verification::VerifiedDigestCache,
    transaction::{TransactionData, TEST_ONLY_GAS_UNIT_FOR_TRANSFER},
};
use fastcrypto::hash::HashFunction;
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

/// A helper struct for pk and sig bytes.
pub struct PasskeyResponse<T> {
    pk_bytes: Vec<u8>,
    sig_bytes: Vec<u8>,
    authenticator_data: Vec<u8>,
    client_data_json: Vec<u8>,
    intent_msg: IntentMessage<T>,
    sender: SuiAddress,
}

/// Register a new passkey and return the public key in bytes.
async fn register() -> PasskeyResponse<TransactionData> {
    // set up authenticator and client
    let user_entity = PublicKeyCredentialUserEntity {
        id: random_vec(32).into(),
        display_name: "Johnny Passkey".into(),
        name: "jpasskey@example.org".into(),
    };
    let origin = Url::parse("https://www.sui.io").unwrap();
    let my_aaguid = Aaguid::new_empty();
    let user_validation_method = MyUserValidationMethod {};
    let store: Option<Passkey> = None;
    let my_authenticator = Authenticator::new(my_aaguid, store, user_validation_method);
    let mut my_client = Client::new(my_authenticator);

    // make credential creation request
    let challenge_bytes_from_rp: Bytes = random_vec(32).into();
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

    // create credential
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
    let pk = PublicKey::try_from_bytes(SignatureScheme::Secp256r1, &pk_bytes).unwrap();

    // compute sui address and make a test transaction
    let sender = SuiAddress::from(&pk);
    let recipient = dbg_addr(2);
    let object_id = ObjectID::ZERO;
    let object = Object::immutable_with_id_for_testing(object_id);
    let gas_price = 1000;
    let tx_data = TransactionData::new_transfer_sui(
        recipient,
        sender,
        None,
        object.compute_object_reference(),
        gas_price * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        gas_price,
    );
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data);

    // compute the challenge = blake2b_hash(intent_msg(tx)) for passkey credential request
    let mut hasher = DefaultHash::default();
    hasher.update(&bcs::to_bytes(&intent_msg).expect("Message serialization should not fail"));
    let passkey_digest = hasher.finalize().digest;

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

    // parse authenticator_data and client_data_json from response.
    let authenticator_data = authenticated_cred.response.authenticator_data.as_slice();
    let client_data_json = authenticated_cred.response.client_data_json.as_slice();

    PasskeyResponse {
        pk_bytes: pk_bytes.to_vec(),
        sig_bytes: sig_bytes.to_vec(),
        authenticator_data: authenticator_data.to_vec(),
        client_data_json: client_data_json.to_vec(),
        intent_msg,
        sender,
    }
}

#[tokio::test]
async fn test_passkey_authenticator_verifies() {
    let response = register().await;
    let mut user_sig_bytes = vec![SignatureScheme::Secp256r1.flag()];
    user_sig_bytes.extend_from_slice(&response.sig_bytes);
    user_sig_bytes.extend_from_slice(&response.pk_bytes);

    let sig = GenericSignature::PasskeyAuthenticator(
        PasskeyAuthenticator::new_for_testing(
            response.authenticator_data,
            response.client_data_json,
            Signature::from_bytes(&user_sig_bytes).unwrap(),
        )
        .unwrap(),
    );

    let res = sig.verify_authenticator(
        &response.intent_msg,
        response.sender,
        0,
        &Default::default(),
        Arc::new(VerifiedDigestCache::new_empty()),
    );
    assert!(res.is_ok());
}

#[tokio::test]
async fn test_passkey_fails_incorrect_sgianture_scheme() {}

#[tokio::test]
async fn test_passkey_fails_invalid_challenge() {}

#[tokio::test]
async fn test_passkey_fails_mismatched_challenge() {}

#[tokio::test]
async fn test_passkey_fails_wrong_client_data_type() {}

#[tokio::test]
async fn test_passkey_fails_to_verify_sig() {}

#[tokio::test]
async fn test_passkey_fails_wrong_author() {}

#[tokio::test]
async fn test_passkey_fails_not_normalized_signature() {}
