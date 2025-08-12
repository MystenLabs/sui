// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use tempfile::TempDir;

use fastcrypto::ed25519::Ed25519KeyPair;
use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};
use sui_keys::key_derive::generate_new_key;
use sui_keys::key_identity::KeyIdentity;
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, InMemKeystore, Keystore};
use sui_macros::sim_test;
use sui_sdk::{
    verify_personal_message_signature::verify_personal_message_signature,
    wallet_context::WalletContext,
};
use sui_types::base_types::{random_object_ref, SuiAddress};
use sui_types::crypto::{Ed25519SuiSignature, SuiKeyPair, SuiSignature};
use sui_types::crypto::{SignatureScheme, SuiSignatureInner};
use sui_types::multisig::{MultiSig, MultiSigPublicKey};
use sui_types::transaction::{ProgrammableTransaction, TransactionData, TransactionKind};
use sui_types::{
    crypto::{get_key_pair, Signature},
    signature::GenericSignature,
    utils::sign_zklogin_personal_msg,
};

#[tokio::test]
async fn mnemonic_test() {
    let temp_dir = TempDir::new().unwrap();
    let (address, _key_pair, scheme, phrase) =
        generate_new_key(SignatureScheme::ED25519, None, None).unwrap();

    let keystore_path_2 = temp_dir.path().join("sui2.keystore");
    let mut keystore2 =
        Keystore::from(FileBasedKeystore::load_or_create(&keystore_path_2).unwrap());
    let imported_address = keystore2
        .import_from_mnemonic(&phrase, SignatureScheme::ED25519, None, None)
        .await
        .unwrap();
    assert_eq!(scheme.flag(), Ed25519SuiSignature::SCHEME.flag());
    assert_eq!(address, imported_address);
}

#[test]
fn keystore_display_test() -> Result<(), anyhow::Error> {
    let temp_dir = TempDir::new().unwrap();
    let keystore_path = temp_dir.path().join("sui.keystore");
    let keystore = Keystore::from(FileBasedKeystore::load_or_create(&keystore_path).unwrap());
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

    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

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

#[tokio::test]
async fn test_get_wallet_key() {
    // Test that we can get and sign with the wallet key from the "filebased" keystore.
    let filebased = Keystore::from(InMemKeystore::new_insecure_for_tests(1));
    let external = Keystore::from(InMemKeystore::new_insecure_for_tests(1));
    let alias = filebased.aliases()[0].alias.clone();
    let address = external.addresses()[0];
    let alias_identity = KeyIdentity::Alias(alias.clone());

    let mut wallet_context = WalletContext::new_for_tests(filebased, Some(external), None);

    // get address for the alias
    wallet_context
        .get_identity_address(Some(alias_identity.clone()))
        .unwrap();

    // try to get a non-existing alias
    wallet_context
        .get_identity_address(Some(KeyIdentity::Alias("".to_string())))
        .unwrap_err();

    // get keystore by alias
    wallet_context
        .get_keystore_by_identity(&alias_identity)
        .unwrap();
    // get mutable keystore by alias
    wallet_context
        .get_keystore_by_identity_mut(&alias_identity)
        .unwrap();

    let transaction_kind = TransactionKind::ProgrammableTransaction(ProgrammableTransaction {
        inputs: vec![],
        commands: vec![],
    });

    let transaction =
        TransactionData::new(transaction_kind, address, random_object_ref(), 1000, 1000);

    let signature = wallet_context
        .sign_secure(&alias_identity, &transaction, Intent::sui_transaction())
        .await
        .unwrap();

    let intent_message = IntentMessage::new(Intent::sui_transaction(), transaction.clone());

    signature
        .verify_secure(&intent_message, address, SignatureScheme::ED25519)
        .unwrap();

    // Test that we can get and sign with the wallet key from the "external" keystore.
    let filebased = Keystore::from(InMemKeystore::new_insecure_for_tests(1));
    let external = Keystore::from(InMemKeystore::new_insecure_for_tests(1));
    let alias = external.aliases()[0].alias.clone();
    let address = external.addresses()[0];
    let alias_identity = KeyIdentity::Alias(alias.clone());

    let mut wallet_context = WalletContext::new_for_tests(filebased, Some(external), None);
    // get address for the alias
    wallet_context
        .get_identity_address(Some(alias_identity.clone()))
        .unwrap();

    // try to get a non-existing alias
    wallet_context
        .get_identity_address(Some(KeyIdentity::Alias("".to_string())))
        .unwrap_err();

    // get keystore by alias
    wallet_context
        .get_keystore_by_identity(&alias_identity)
        .unwrap();
    // get mutable keystore by alias
    wallet_context
        .get_keystore_by_identity_mut(&alias_identity)
        .unwrap();

    let transaction_kind = TransactionKind::ProgrammableTransaction(ProgrammableTransaction {
        inputs: vec![],
        commands: vec![],
    });

    let transaction =
        TransactionData::new(transaction_kind, address, random_object_ref(), 1000, 1000);

    let signature = wallet_context
        .sign_secure(&alias_identity, &transaction, Intent::sui_transaction())
        .await
        .unwrap();

    let intent_message = IntentMessage::new(Intent::sui_transaction(), transaction.clone());

    signature
        .verify_secure(&intent_message, address, SignatureScheme::ED25519)
        .unwrap();
}
