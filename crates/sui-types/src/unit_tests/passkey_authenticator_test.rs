// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use super::to_signing_message;
use crate::crypto::DefaultHash;
use crate::passkey_authenticator::{PasskeyAuthenticator, RawPasskeyAuthenticator};
use crate::{
    base_types::{dbg_addr, ObjectID, SuiAddress},
    crypto::{PublicKey, Signature, SignatureScheme},
    error::SuiError,
    object::Object,
    signature::GenericSignature,
    signature_verification::VerifiedDigestCache,
    transaction::{TransactionData, TEST_ONLY_GAS_UNIT_FOR_TRANSFER},
};
use fastcrypto::hash::HashFunction;
use fastcrypto::rsa::{Base64UrlUnpadded, Encoding as _};
use fastcrypto::traits::ToFromBytes;
use p256::pkcs8::DecodePublicKey;
use passkey_authenticator::{Authenticator, UserValidationMethod};
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
use shared_crypto::intent::{Intent, IntentMessage};
use url::Url;

/// Helper struct to initialize passkey client.
pub struct MyUserValidationMethod {}
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

/// Response with fields from passkey authentication.
#[derive(Debug)]
pub struct PasskeyResponse<T> {
    user_sig_bytes: Vec<u8>,
    authenticator_data: Vec<u8>,
    client_data_json: String,
    intent_msg: IntentMessage<T>,
    sender: SuiAddress,
}

/// Create a new passkey credential, derives its address
/// and request a signature from passkey for a test transaction.
async fn create_credential_and_sign_test_tx(
    origin: &Url,
    request: CredentialCreationOptions,
) -> PasskeyResponse<TransactionData> {
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
    let pk = PublicKey::try_from_bytes(SignatureScheme::PasskeyAuthenticator, &pk_bytes).unwrap();

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

    // Compute the challenge as blake2b_hash(intent_msg(tx)). This is the challenge for the passkey to sign.
    let passkey_digest = to_signing_message(&intent_msg);

    // Send the challenge to the passkey to sign with the rp_id.
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

    PasskeyResponse {
        user_sig_bytes,
        authenticator_data: authenticator_data.to_vec(),
        client_data_json: String::from_utf8_lossy(client_data_json).to_string(),
        intent_msg,
        sender,
    }
}

fn make_credential_creation_option(origin: &Url) -> CredentialCreationOptions {
    let challenge_bytes_from_rp: Bytes = random_vec(32).into();
    let user_entity = PublicKeyCredentialUserEntity {
        id: random_vec(32).into(),
        display_name: "Johnny Passkey".into(),
        name: "jpasskey@example.org".into(),
    };
    CredentialCreationOptions {
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
    }
}

#[tokio::test]
async fn test_passkey_serde() {
    let origin = Url::parse("https://www.sui.io").unwrap();
    let request = make_credential_creation_option(&origin);
    let response = create_credential_and_sign_test_tx(&origin, request).await;

    let raw = RawPasskeyAuthenticator {
        user_signature: Signature::from_bytes(&response.user_sig_bytes).unwrap(),
        authenticator_data: response.authenticator_data,
        client_data_json: response.client_data_json,
    };
    let passkey: PasskeyAuthenticator = raw.try_into().unwrap();
    let serialized = bcs::to_bytes(&passkey).unwrap();

    // deser back to passkey authenticator is the same
    let deserialized: PasskeyAuthenticator = bcs::from_bytes(&serialized).unwrap();
    assert_eq!(passkey, deserialized);

    // serde round trip for generic signature is the same
    let signature = GenericSignature::PasskeyAuthenticator(passkey);
    let serialized_str = serde_json::to_string(&signature).unwrap();
    let deserialized: GenericSignature = serde_json::from_str(&serialized_str).unwrap();
    assert_eq!(deserialized.as_ref(), signature.as_ref());
}

#[tokio::test]
async fn test_passkey_authenticator() {
    let origin = Url::parse("https://www.sui.io").unwrap();
    let request = make_credential_creation_option(&origin);
    let response = create_credential_and_sign_test_tx(&origin, request).await;

    let sig = GenericSignature::PasskeyAuthenticator(
        PasskeyAuthenticator::new_for_testing(
            response.authenticator_data,
            response.client_data_json,
            Signature::from_bytes(&response.user_sig_bytes).unwrap(),
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
async fn test_passkey_fails_invalid_json() {
    let origin = Url::parse("https://www.sui.io").unwrap();
    let request = make_credential_creation_option(&origin);
    let response = create_credential_and_sign_test_tx(&origin, request).await;
    let client_data_json_missing_type = r#"{"challenge":"9-fH7nX8Nb1JvUynz77mv1kXOkGkg1msZb2qhvZssGI","origin":"http://localhost:5173","crossOrigin":false}"#;
    let raw = RawPasskeyAuthenticator {
        authenticator_data: response.authenticator_data.clone(),
        client_data_json: client_data_json_missing_type.to_string(),
        user_signature: Signature::from_bytes(&response.user_sig_bytes).unwrap(),
    };
    let res: Result<PasskeyAuthenticator, SuiError> = raw.try_into();
    let err = res.unwrap_err();
    assert_eq!(
        err,
        SuiError::InvalidSignature {
            error: "Invalid client data json".to_string()
        }
    );
    const CORRECT_LEN: usize = DefaultHash::OUTPUT_SIZE;
    let client_data_json_too_short = format!(
        r#"{{"type":"webauthn.get", "challenge":"{}","origin":"http://localhost:5173","crossOrigin":false, "unknown": "unknown"}}"#,
        Base64UrlUnpadded::encode_string(&[0; CORRECT_LEN - 1])
    );
    let raw = RawPasskeyAuthenticator {
        authenticator_data: response.authenticator_data.clone(),
        client_data_json: client_data_json_too_short,
        user_signature: Signature::from_bytes(&response.user_sig_bytes).unwrap(),
    };
    let res: Result<PasskeyAuthenticator, SuiError> = raw.try_into();
    assert!(res.is_err());

    let client_data_json_too_long = format!(
        r#"{{"type":"webauthn.get", "challenge":"{}","origin":"http://localhost:5173","crossOrigin":false, "unknown": "unknown"}}"#,
        Base64UrlUnpadded::encode_string(&[0; CORRECT_LEN + 1])
    );
    let raw_2 = RawPasskeyAuthenticator {
        authenticator_data: response.authenticator_data.clone(),
        client_data_json: client_data_json_too_long,
        user_signature: Signature::from_bytes(&response.user_sig_bytes).unwrap(),
    };
    let res_2: Result<PasskeyAuthenticator, SuiError> = raw_2.try_into();
    assert!(res_2.is_err());

    let client_data_json_correct = format!(
        r#"{{"type":"webauthn.get", "challenge":"{}","origin":"http://localhost:5173","crossOrigin":false, "unknown": "unknown"}}"#,
        Base64UrlUnpadded::encode_string(&[0; CORRECT_LEN])
    );
    let raw_3 = RawPasskeyAuthenticator {
        authenticator_data: response.authenticator_data,
        client_data_json: client_data_json_correct,
        user_signature: Signature::from_bytes(&response.user_sig_bytes).unwrap(),
    };
    let res_3: Result<PasskeyAuthenticator, SuiError> = raw_3.try_into();
    assert!(res_3.is_ok());
}

#[tokio::test]
async fn test_passkey_fails_invalid_challenge() {
    let origin = Url::parse("https://www.sui.io").unwrap();
    let request = make_credential_creation_option(&origin);
    let response = create_credential_and_sign_test_tx(&origin, request).await;
    let fake_client_data_json = r#"{"type":"webauthn.get","challenge":"wrong_base64_encoding","origin":"http://localhost:5173","crossOrigin":false}"#;
    let raw = RawPasskeyAuthenticator {
        authenticator_data: response.authenticator_data,
        client_data_json: fake_client_data_json.to_string(),
        user_signature: Signature::from_bytes(&response.user_sig_bytes).unwrap(),
    };
    let res: Result<PasskeyAuthenticator, SuiError> = raw.try_into();
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
    let response = create_credential_and_sign_test_tx(&origin, request).await;
    let fake_client_data_json = r#"{"type":"webauthn.create","challenge":"9-fH7nX8Nb1JvUynz77mv1kXOkGkg1msZb2qhvZssGI","origin":"http://localhost:5173","crossOrigin":false}"#;
    let raw = RawPasskeyAuthenticator {
        authenticator_data: response.authenticator_data,
        client_data_json: fake_client_data_json.to_string(),
        user_signature: Signature::from_bytes(&response.user_sig_bytes).unwrap(),
    };
    let res: Result<PasskeyAuthenticator, SuiError> = raw.try_into();
    let err = res.unwrap_err();
    assert_eq!(
        err,
        SuiError::InvalidSignature {
            error: "Invalid client data type".to_string()
        }
    );
}

// #[tokio::test]
// async fn test_passkey_fails_not_normalized_signature() {
//     // crafts a particular not normalized signature, fails to verify. this is produced from typescript client https://github.com/joyqvq/sui-webauthn-poc/tree/joy/tx-example
//     let tx_data: TransactionData = bcs::from_bytes(&Base64::decode("AAAAAHaTZLc0GGZ6RNYAqPC8LWZV7xHO+54zf71arV1MwFUtAcDum6pkbPZZN/iYq0zJpOxiV2wrZAnVU0bnNpOjombGAgAAAAAAAAAgAIiQFrz1abd2rNdo76dQS026yMAS1noA7FiGsggyt9V2k2S3NBhmekTWAKjwvC1mVe8RzvueM3+9Wq1dTMBVLegDAAAAAAAAgIQeAAAAAAAA").unwrap()).unwrap();
//     let response = PasskeyResponse::<TransactionData> {
//         user_sig_bytes: Hex::decode("02bbd02ace0bad3b32eb3a891dc5c85e56274f52695d24db41b247ec694d1531d6fe1a5bec11a8063d1eb0512e7971bfd23395c2cb8862f73049d0f78fd204c6d602276d5f3a22f3e698cdd2272a63da8bfdd9344de73312c7f7f9eca21bfc304f2e").unwrap(),
//         authenticator_data: Hex::decode("49960de5880e8c687434170f6476605b8fe4aeb9a28632c7995cf3ba831d97631d00000000").unwrap(),
//         client_data_json: r#"{"type":"webauthn.get","challenge":"AAAAZgUD1inhS1l9qUfZePaivu6IbIo_SxCGmYcfTwrmcFU","origin":"http://localhost:5173","crossOrigin":false}"#.to_string(),
//         intent_msg: IntentMessage::new(Intent::sui_transaction(), tx_data),
//         sender: SuiAddress::from_str("0x769364b73418667a44d600a8f0bc2d6655ef11cefb9e337fbd5aad5d4cc0552d").unwrap()
//     };
//     let sig = GenericSignature::PasskeyAuthenticator(
//         PasskeyAuthenticator::new_for_testing(
//             response.authenticator_data,
//             response.client_data_json,
//             Signature::from_bytes(&response.user_sig_bytes).unwrap(),
//         )
//         .unwrap(),
//     );

//     let res = sig.verify_authenticator(
//         &response.intent_msg,
//         response.sender,
//         0,
//         &Default::default(),
//         Arc::new(VerifiedDigestCache::new_empty()),
//     );
//     let err = res.unwrap_err();
//     assert_eq!(
//         err,
//         SuiError::InvalidSignature {
//             error: "Fails to verify".to_string()
//         }
//     );
// }

// #[tokio::test]
// async fn test_real_passkey_output() {
//     // response from a real passkey authenticator created in iCloud, from typescript client: https://github.com/joyqvq/sui-webauthn-poc/tree/joy/tx-example
//     let address =
//         SuiAddress::from_str("0xac8564f638fbf673fc92eb85b5abe5f7c29bdaa60a4a10329868fbe6c551dda2")
//             .unwrap();
//     let sig = GenericSignature::from_bytes(&Base64::decode("BiVJlg3liA6MaHQ0Fw9kdmBbj+SuuaKGMseZXPO6gx2XYx0AAAAAigF7InR5cGUiOiJ3ZWJhdXRobi5nZXQiLCJjaGFsbGVuZ2UiOiJBQUFBdF9taklCMXZiVnBZTTZXVjZZX29peDZKOGFOXzlzYjhTS0ZidWtCZmlRdyIsIm9yaWdpbiI6Imh0dHA6Ly9sb2NhbGhvc3Q6NTE3MyIsImNyb3NzT3JpZ2luIjpmYWxzZX1iApjskL9Xyfopyg9Av7MSrcchSpfWqAYoJ+qfSId4gNmoQ1YNgj2alDpRIbq9kthmyGY25+k24FrW114PEoy5C+8DPRcOCTtACi3ZywtZ4UILhwV+Suh79rWtbKqDqhBQwxM=").unwrap()).unwrap();
//     let tx_data: TransactionData = bcs::from_bytes(&Base64::decode("AAAAAKyFZPY4+/Zz/JLrhbWr5ffCm9qmCkoQMpho++bFUd2iAUwOMmeNHuxq2hS4PvO1uivs9exQGefW2wNQAt7tRkkdAgAAAAAAAAAgCsJHAaWbb8oUlZsGdsyW3Atf3d51wBEr9HLkrBF0/UushWT2OPv2c/yS64W1q+X3wpvapgpKEDKYaPvmxVHdougDAAAAAAAAgIQeAAAAAAAA").unwrap()).unwrap();
//     let res = sig.verify_authenticator(
//         &IntentMessage::new(Intent::sui_transaction(), tx_data),
//         address,
//         0,
//         &Default::default(),
//         Arc::new(VerifiedDigestCache::new_empty()),
//     );
//     assert!(res.is_ok());
// }
