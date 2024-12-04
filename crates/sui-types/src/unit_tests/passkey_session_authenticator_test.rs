// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::committee::EpochId;
use crate::crypto::get_key_pair;
use crate::passkey_authenticator::passkey_authenticator_test::MyUserValidationMethod;
use crate::passkey_session_authenticator::{
    PasskeySessionAuthenticator, RawPasskeySessionAuthenticator,
};
use crate::{
    base_types::{dbg_addr, ObjectID, SuiAddress},
    crypto::{PublicKey, Signature, SignatureScheme},
    error::SuiError,
    object::Object,
    signature::GenericSignature,
    transaction::{TransactionData, TEST_ONLY_GAS_UNIT_FOR_TRANSFER},
};
use fastcrypto::ed25519::Ed25519KeyPair;
use passkey_types::webauthn::{CredentialRequestOptions, PublicKeyCredentialRequestOptions};

use fastcrypto::traits::KeyPair;
use fastcrypto::traits::ToFromBytes;
use p256::pkcs8::DecodePublicKey;
use passkey_types::webauthn::CredentialCreationOptions;

use crate::passkey_authenticator::passkey_authenticator_test::make_credential_creation_option;
use passkey_authenticator::Authenticator;
use passkey_client::Client;
use passkey_types::{ctap2::Aaguid, webauthn::UserVerificationRequirement, Bytes, Passkey};
use shared_crypto::intent::{Intent, IntentMessage};
use url::Url;
/// Response with fields from passkey authentication.
#[derive(Debug)]
pub struct PasskeySessionResponse<T> {
    user_sig_bytes: Vec<u8>,
    authenticator_data: Vec<u8>,
    client_data_json: String,
    kp: Ed25519KeyPair,
    intent_msg: IntentMessage<T>,
}

/// Create a new passkey credential, derives its address
/// and request a signature from passkey for a test transaction.
async fn create_credential_and_commit_ephemeral_pk(
    origin: &Url,
    request: CredentialCreationOptions,
    max_epoch: EpochId,
) -> PasskeySessionResponse<TransactionData> {
    // Set up authenticator and client.
    let my_aaguid = Aaguid::new_empty();
    let user_validation_method = MyUserValidationMethod {};
    let store: Option<Passkey> = None;
    let my_authenticator = Authenticator::new(my_aaguid, store, user_validation_method);
    let mut my_client = Client::new(my_authenticator);

    // Create credential with the request option.
    let my_webauthn_credential = my_client.register(origin, request, None).await.unwrap();
    let verifying_key = p256::ecdsa::VerifyingKey::from_public_key_der(
        my_webauthn_credential
            .response
            .public_key
            .unwrap()
            .as_slice(),
    )
    .unwrap();

    // Derive its compact pubkey from DER format.
    let encoded_point = verifying_key.to_encoded_point(false);
    let x = encoded_point.x();
    let y = encoded_point.y();
    let prefix = if y.unwrap()[31] % 2 == 0 { 0x02 } else { 0x03 };
    let mut pk_bytes = vec![prefix];
    pk_bytes.extend_from_slice(x.unwrap());
    let pk =
        PublicKey::try_from_bytes(SignatureScheme::PasskeySessionAuthenticator, &pk_bytes).unwrap();

    // Derives its sui address and make a test transaction with it as sender.
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

    // Compute the challenge as eph_flag || eph_pk || max_epoch. This is the challenge for the passkey to sign.
    let kp: Ed25519KeyPair = get_key_pair().1;
    let mut register_msg = vec![SignatureScheme::ED25519.flag()];
    register_msg.extend_from_slice(kp.public().as_bytes());
    register_msg.extend_from_slice(&max_epoch.to_be_bytes());

    // Send the challenge to the passkey to sign with the rp_id.
    let credential_request = CredentialRequestOptions {
        public_key: PublicKeyCredentialRequestOptions {
            challenge: Bytes::from(register_msg),
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
        .authenticate(origin, credential_request, None)
        .await
        .unwrap();

    // Parse the response, gets the signature from der format and normalize it to lower s.
    let sig_bytes_der = authenticated_cred.response.signature.as_slice();
    let sig = p256::ecdsa::Signature::from_der(sig_bytes_der).unwrap();
    let sig_bytes = sig.normalize_s().unwrap_or(sig).to_bytes();

    // Parse authenticator_data and client_data_json from response.
    let authenticator_data = authenticated_cred.response.authenticator_data.as_slice();
    let client_data_json = authenticated_cred.response.client_data_json.as_slice();

    // Prepare flag || sig || pk.
    let mut user_sig_bytes = vec![SignatureScheme::Secp256r1.flag()];
    user_sig_bytes.extend_from_slice(&sig_bytes);
    user_sig_bytes.extend_from_slice(&pk_bytes);

    PasskeySessionResponse::<TransactionData> {
        user_sig_bytes,
        authenticator_data: authenticator_data.to_vec(),
        client_data_json: String::from_utf8_lossy(client_data_json).to_string(),
        kp,
        intent_msg,
    }
}

#[tokio::test]
async fn test_passkey_session_sig_serde() {
    let origin = Url::parse("https://www.sui.io").unwrap();
    let request = make_credential_creation_option(&origin);
    let max_epoch = 2;
    let response = create_credential_and_commit_ephemeral_pk(&origin, request, max_epoch).await;

    let raw = RawPasskeySessionAuthenticator {
        passkey_signature: Signature::from_bytes(&response.user_sig_bytes).unwrap(),
        max_epoch,
        ephemeral_signature: Signature::new_secure(&response.intent_msg, &response.kp),
        authenticator_data: response.authenticator_data,
        client_data_json: response.client_data_json,
    };
    let passkey: PasskeySessionAuthenticator = raw.try_into().unwrap();
    let serialized = bcs::to_bytes(&passkey).unwrap();

    // deser back to passkey authenticator is the same
    let deserialized: PasskeySessionAuthenticator = bcs::from_bytes(&serialized).unwrap();
    assert_eq!(passkey, deserialized);

    // serde round trip for generic signature is the same
    let signature = GenericSignature::PasskeySessionAuthenticator(passkey);

    let serialized_str = serde_json::to_string(&signature).unwrap();
    let deserialized: GenericSignature = serde_json::from_str(&serialized_str).unwrap();
    assert_eq!(deserialized.as_ref(), signature.as_ref());
}

#[tokio::test]
async fn test_passkey_fails_invalid_json() {
    let origin = Url::parse("https://www.sui.io").unwrap();
    let request = make_credential_creation_option(&origin);
    let response = create_credential_and_commit_ephemeral_pk(&origin, request, 10).await;
    let client_data_json_missing_type = r#"{"challenge":"9-fH7nX8Nb1JvUynz77mv1kXOkGkg1msZb2qhvZssGI","origin":"http://localhost:5173","crossOrigin":false}"#;
    let raw = RawPasskeySessionAuthenticator {
        authenticator_data: response.authenticator_data.clone(),
        client_data_json: client_data_json_missing_type.to_string(),
        passkey_signature: Signature::from_bytes(&response.user_sig_bytes).unwrap(),
        max_epoch: 10,
        ephemeral_signature: Signature::new_secure(&response.intent_msg, &response.kp),
    };
    let res: Result<PasskeySessionAuthenticator, SuiError> = raw.try_into();
    let err = res.unwrap_err();
    assert_eq!(
        err,
        SuiError::InvalidSignature {
            error: "Invalid client data json".to_string()
        }
    );
}

#[tokio::test]
async fn test_passkey_fails_invalid_challenge() {
    let origin = Url::parse("https://www.sui.io").unwrap();
    let request = make_credential_creation_option(&origin);
    let response = create_credential_and_commit_ephemeral_pk(&origin, request, 10).await;
    let fake_client_data_json = r#"{"type":"webauthn.get","challenge":"wrong_base64_encoding","origin":"http://localhost:5173","crossOrigin":false}"#;
    let raw = RawPasskeySessionAuthenticator {
        authenticator_data: response.authenticator_data,
        client_data_json: fake_client_data_json.to_string(),
        passkey_signature: Signature::from_bytes(&response.user_sig_bytes).unwrap(),
        max_epoch: 10,
        ephemeral_signature: Signature::new_secure(&response.intent_msg, &response.kp),
    };
    let res: Result<PasskeySessionAuthenticator, SuiError> = raw.try_into();
    let err = res.unwrap_err();
    assert_eq!(
        err,
        SuiError::InvalidSignature {
            error: "Invalid encoded challenge".to_string()
        }
    );
}

#[tokio::test]
async fn test_passkey_fails_wrong_client_data_type() {
    let origin = Url::parse("https://www.sui.io").unwrap();
    let request = make_credential_creation_option(&origin);
    let response = create_credential_and_commit_ephemeral_pk(&origin, request, 10).await;
    let fake_client_data_json = r#"{"type":"webauthn.create","challenge":"9-fH7nX8Nb1JvUynz77mv1kXOkGkg1msZb2qhvZssGI","origin":"http://localhost:5173","crossOrigin":false}"#;
    let raw = RawPasskeySessionAuthenticator {
        authenticator_data: response.authenticator_data.clone(),
        client_data_json: fake_client_data_json.to_string(),
        passkey_signature: Signature::from_bytes(&response.user_sig_bytes).unwrap(),
        max_epoch: 10,
        ephemeral_signature: Signature::new_secure(&response.intent_msg, &response.kp),
    };
    let res: Result<PasskeySessionAuthenticator, SuiError> = raw.try_into();
    let err = res.unwrap_err();
    assert_eq!(
        err,
        SuiError::InvalidSignature {
            error: "Invalid client data type".to_string()
        }
    );
}
