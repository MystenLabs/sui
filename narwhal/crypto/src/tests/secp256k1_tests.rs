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
    assert_eq!(*public_key, pk2);

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
    assert_eq!(deserialized.sig, signature.sig);
}

#[test]
fn import_export_public_key() {
    let kpref = keys().pop().unwrap();
    let public_key = kpref.public();
    let export = public_key.encode_base64();
    let import = Secp256k1PublicKey::decode_base64(&export);
    assert!(import.is_ok());
    assert_eq!(&import.unwrap(), public_key);
}

#[test]
fn test_public_key_bytes_conversion() {
    let kp = keys().pop().unwrap();
    let pk_bytes: Secp256k1PublicKeyBytes = kp.public().clone().into();
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
    let signature = kpref.sign(b"Hello, world");
    let sig_bytes = signature.as_ref();
    let rebuilt_sig = <Secp256k1Signature as signature::Signature>::from_bytes(sig_bytes).unwrap();
    assert_eq!(rebuilt_sig.sig, signature.sig);
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
