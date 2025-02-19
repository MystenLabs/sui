// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use tempfile::TempDir;

use fastcrypto::ed25519::Ed25519KeyPair;
use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_macros::sim_test;
use sui_sdk::verify_personal_message_signature::verify_personal_message_signature;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{Ed25519SuiSignature, SuiKeyPair};
use sui_types::crypto::{SignatureScheme, SuiSignatureInner};
use sui_types::multisig::{MultiSig, MultiSigPublicKey};
use sui_types::{
    crypto::{get_key_pair, Signature},
    signature::GenericSignature,
    utils::sign_zklogin_personal_msg,
};

#[test]
fn mnemonic_test() {
    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
    let (address, phrase, scheme) = keystore
        .generate_and_add_new_key(SignatureScheme::ED25519, None, None, None)
        .unwrap();

    let keystore_path_2 = temp_dir.path().join("sui2.keystore");
    let mut keystore2 = Keystore::from(FileBasedKeystore::new(&keystore_path_2).unwrap());
    let imported_address = keystore2
        .import_from_mnemonic(&phrase, SignatureScheme::ED25519, None, None)
        .unwrap();
    assert_eq!(scheme.flag(), Ed25519SuiSignature::SCHEME.flag());
    assert_eq!(address, imported_address);
}

#[test]
fn keystore_display_test() -> Result<(), anyhow::Error> {
    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
    assert!(keystore.to_string().contains("sui.keystore"));
    assert!(!keystore.to_string().contains("keys:"));
    Ok(())
}

#[tokio::test]
async fn test_verify_personal_message_signature() {
    let (address, sec1): (_, Ed25519KeyPair) = get_key_pair();
    let message = b"hello";
    let intent_message = IntentMessage::new(
        Intent::personal_message(),
        PersonalMessage {
            message: message.to_vec(),
        },
    );

    let s = Signature::new_secure(&intent_message, &sec1);
    let signature: GenericSignature = GenericSignature::Signature(s);
    let res = verify_personal_message_signature(signature.clone(), message, address, None).await;
    assert!(res.is_ok());

    let res =
        verify_personal_message_signature(signature, "wrong msg".as_bytes(), address, None).await;
    assert!(res.is_err());
}

#[sim_test]
async fn test_verify_signature_zklogin() {
    use test_cluster::TestClusterBuilder;

    let message = b"hello";
    let personal_message = PersonalMessage {
        message: message.to_vec(),
    };
    let (user_address, signature) = sign_zklogin_personal_msg(personal_message.clone());

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(15000)
        .with_default_jwks()
        .build()
        .await;
    test_cluster.wait_for_epoch(Some(1)).await;
    test_cluster.wait_for_authenticator_state_update().await;
    let client = test_cluster.sui_client();
    let res = verify_personal_message_signature(
        signature.clone(),
        message,
        user_address,
        Some(client.clone()),
    )
    .await;
    assert!(res.is_ok());

    let res = verify_personal_message_signature(
        signature,
        "wrong msg".as_bytes(),
        user_address,
        Some(client.clone()),
    )
    .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn test_verify_signature_multisig() {
    let kp1: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair().1);
    let kp2: SuiKeyPair = SuiKeyPair::Secp256k1(get_key_pair().1);

    let message = b"hello";
    let intent_message = IntentMessage::new(
        Intent::personal_message(),
        PersonalMessage {
            message: message.to_vec(),
        },
    );
    let sig1: GenericSignature = Signature::new_secure(&intent_message, &kp1).into();
    let sig2: GenericSignature = Signature::new_secure(&intent_message, &kp2).into();
    let multisig_pk =
        MultiSigPublicKey::new(vec![kp1.public(), kp2.public()], vec![1, 1], 2).unwrap();
    let address: SuiAddress = (&multisig_pk).into();
    let multisig = MultiSig::combine(vec![sig1, sig2], multisig_pk).unwrap();
    let generic_sig = GenericSignature::MultiSig(multisig);

    let res = verify_personal_message_signature(generic_sig.clone(), message, address, None).await;
    assert!(res.is_ok());

    let res =
        verify_personal_message_signature(generic_sig, "wrong msg".as_bytes(), address, None).await;
    assert!(res.is_err());
}
