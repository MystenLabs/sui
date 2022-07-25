// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{
    secp256k1::{
        Secp256k1KeyPair, Secp256k1PrivateKey, Secp256k1PublicKey, Secp256k1PublicKeyBytes,
        Secp256k1Signature,
    },
    traits::{EncodeDecodeBase64, ToFromBytes},
};

use rand::{rngs::StdRng, SeedableRng as _};
use signature::{Signer, Verifier};

pub fn keys() -> Vec<Secp256k1KeyPair> {
    let mut rng = StdRng::from_seed([0; 32]);

    (0..4)
        .map(|_| Secp256k1KeyPair::generate(&mut rng))
        .collect()
}

#[test]
fn serialize_deserialize() {
    let kpref = keys().pop().unwrap();
    let public_key = kpref.public();

    let bytes = bincode::serialize(&public_key).unwrap();
    let pk2 = bincode::deserialize::<Secp256k1PublicKey>(&bytes).unwrap();
    assert_eq!(public_key.as_ref(), pk2.as_ref());

    let private_key = kpref.private();
    let bytes = bincode::serialize(&private_key).unwrap();
    let privkey = bincode::deserialize::<Secp256k1PrivateKey>(&bytes).unwrap();
    let bytes2 = bincode::serialize(&privkey).unwrap();
    assert_eq!(bytes, bytes2);

    let signature = Secp256k1Signature::default();
    let bytes = bincode::serialize(&signature).unwrap();
    let sig = bincode::deserialize::<Secp256k1Signature>(&bytes).unwrap();
    let bytes2 = bincode::serialize(&sig).unwrap();
    assert_eq!(bytes, bytes2);

    // test serde_json serialization
    let serialized = serde_json::to_string(&signature).unwrap();
    println!("{:?}", serialized);
    let deserialized: Secp256k1Signature = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.as_ref(), signature.as_ref());
}

#[test]
fn import_export_public_key() {
    let kpref = keys().pop().unwrap();
    let public_key = kpref.public();
    let export = public_key.encode_base64();
    let import = Secp256k1PublicKey::decode_base64(&export);
    assert!(import.is_ok());
    assert_eq!(import.unwrap().as_ref(), public_key.as_ref());
}

#[test]
fn test_public_key_bytes_conversion() {
    let kp = keys().pop().unwrap();
    let pk_bytes: Secp256k1PublicKeyBytes = kp.public().into();
    let rebuilded_pk: Secp256k1PublicKey = pk_bytes.try_into().unwrap();
    assert_eq!(kp.public().as_bytes(), rebuilded_pk.as_bytes());
}

#[test]
fn import_export_secret_key() {
    let kpref = keys().pop().unwrap();
    let secret_key = kpref.private();
    let export = secret_key.encode_base64();
    let import = Secp256k1PrivateKey::decode_base64(&export);
    assert!(import.is_ok());
    assert_eq!(import.unwrap().as_ref(), secret_key.as_ref());
}

#[test]
fn test_copy_key_pair() {
    let kp = keys().pop().unwrap();
    let kp_copied = kp.copy();

    assert_eq!(kp.public().as_bytes(), kp_copied.public().as_bytes());
    assert_eq!(kp.private().as_bytes(), kp_copied.private().as_bytes());
}

#[test]
fn to_from_bytes_signature() {
    let kpref = keys().pop().unwrap();
    let signature = kpref.sign(b"Hello, world!");
    let sig_bytes = signature.as_ref();
    let rebuilt_sig = <Secp256k1Signature as ToFromBytes>::from_bytes(sig_bytes).unwrap();
    assert_eq!(rebuilt_sig.as_ref(), signature.as_ref())
}

#[test]
fn verify_valid_signature() {
    // Get a keypair.
    let kp = keys().pop().unwrap();

    // Make signature.
    let message: &[u8] = b"Hello, world!";
    let digest = message.digest();

    let signature = kp.sign(&digest.0);

    // Verify the signature.
    assert!(kp.public().verify(&digest.0, &signature).is_ok());
}

#[test]
fn verify_invalid_signature() {
    // Get a keypair.
    let kp = keys().pop().unwrap();

    // Make signature.
    let message: &[u8] = b"Hello, world!";
    let digest = message.digest();

    let signature = kp.sign(&digest.0);

    // Verify the signature.
    let bad_message: &[u8] = b"Bad message!";
    let digest = bad_message.digest();

    assert!(kp.public().verify(&digest.0, &signature).is_err());
}

#[tokio::test]
async fn signature_service() {
    // Get a keypair.
    let kp = keys().pop().unwrap();
    let pk = kp.public().clone();

    // Spawn the signature service.
    let mut service = SignatureService::new(kp);

    // Request signature from the service.
    let message: &[u8] = b"Hello, world!";
    let digest = message.digest();
    let signature = service.request_signature(digest).await;

    // Verify the signature we received.
    assert!(pk.verify(digest.as_ref(), &signature).is_ok());
}

use proptest::arbitrary::Arbitrary;

proptest::proptest! {
    #[test]
    fn test_k256_against_secp256k1_lib_with_recovery(
        r in <[u8; 32]>::arbitrary()
) {
        let message: &[u8] = b"hello world!";
        let hashed_msg = rust_secp256k1::Message::from_slice(<sha3::Keccak256 as sha3::digest::Digest>::digest(message).as_slice()).unwrap();

        // contruct private key with bytes and signs message
        let priv_key = <Secp256k1PrivateKey as ToFromBytes>::from_bytes(&r).unwrap();
        let key_pair = Secp256k1KeyPair::from(priv_key);
        let key_pair_copied = key_pair.copy();
        let key_pair_copied_2 = key_pair.copy();
        let signature: Secp256k1Signature = key_pair.sign(message);
        assert!(key_pair.public().verify(message, &signature).is_ok());

        // use k256 to construct private key with the same bytes and signs the same message
        let priv_key_1 = k256::ecdsa::SigningKey::from_bytes(&r).unwrap();
        let pub_key_1 = priv_key_1.verifying_key();
        let signature_1: k256::ecdsa::recoverable::Signature = priv_key_1.sign(message);
        assert!(pub_key_1.verify(message, &signature_1).is_ok());

        // two private keys are serialized the same
        assert_eq!(key_pair_copied.private().as_bytes(), priv_key_1.to_bytes().as_slice());

        // two pubkeys are the same
        assert_eq!(
            key_pair.public().as_bytes(),
            pub_key_1.to_bytes().as_slice()
        );

        // same recovered pubkey are recovered
        let recovered_key = signature.sig.recover(&hashed_msg).unwrap();
        let recovered_key_1 = signature_1.recover_verifying_key(message).expect("couldn't recover pubkey");
        assert_eq!(recovered_key.serialize(),recovered_key_1.to_bytes().as_slice());

        // same signatures produced from both implementations
        assert_eq!(signature.as_ref(), ToFromBytes::as_bytes(&signature_1));

        // use ffi-implemented keypair to verify sig constructed by k256
        let sig_bytes_1 = bincode::serialize(&signature_1.as_ref()).unwrap();
        let secp_sig1 = bincode::deserialize::<Secp256k1Signature>(&sig_bytes_1).unwrap();
        assert!(key_pair_copied_2.public().verify(message, &secp_sig1).is_ok());

        // use k256 keypair to verify sig constructed by ffi-implementation
        let typed_sig = k256::ecdsa::recoverable::Signature::try_from(signature.as_ref()).unwrap();
        assert!(pub_key_1.verify(message, &typed_sig).is_ok());
    }
}
