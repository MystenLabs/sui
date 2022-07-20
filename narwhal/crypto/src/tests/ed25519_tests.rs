// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{
    ed25519::{
        Ed25519AggregateSignature, Ed25519KeyPair, Ed25519PrivateKey, Ed25519PublicKey,
        Ed25519PublicKeyBytes, Ed25519Signature,
    },
    hkdf::hkdf_generate_from_ikm,
    traits::{AggregateAuthenticator, EncodeDecodeBase64, ToFromBytes, VerifyingKey},
};

use blake2::digest::Update;
use rand::{rngs::StdRng, SeedableRng as _};
use sha3::Sha3_256;
use signature::{Signer, Verifier};

impl Hash for &[u8] {
    type TypedDigest = Digest;

    fn digest(&self) -> Digest {
        Digest(blake2b_256(|hasher| hasher.update(self)))
    }
}

pub fn keys() -> Vec<Ed25519KeyPair> {
    let mut rng = StdRng::from_seed([0; 32]);
    (0..4).map(|_| Ed25519KeyPair::generate(&mut rng)).collect()
}

#[test]
fn serialize_deserialize() {
    let kpref = keys().pop().unwrap();
    let public_key = kpref.public();

    let bytes = bincode::serialize(&public_key).unwrap();
    let pk2 = bincode::deserialize::<Ed25519PublicKey>(&bytes).unwrap();
    assert_eq!(*public_key, pk2);

    let private_key = kpref.private();
    let bytes = bincode::serialize(&private_key).unwrap();
    let privkey = bincode::deserialize::<Ed25519PublicKey>(&bytes).unwrap();
    let bytes2 = bincode::serialize(&privkey).unwrap();
    assert_eq!(bytes, bytes2);
}

#[test]
fn test_serde_signatures_non_human_readable() {
    let message = b"hello, narwhal";
    // Test populated aggregate signature
    let sig = keys().pop().unwrap().sign(message);
    let serialized = bincode::serialize(&sig).unwrap();
    let deserialized: Ed25519Signature = bincode::deserialize(&serialized).unwrap();
    assert_eq!(deserialized.0, sig.0);
}

#[test]
fn test_serde_signatures_human_readable() {
    let kp = keys().pop().unwrap();
    let message: &[u8] = b"Hello, world!";
    let signature = kp.sign(message);

    let serialized = serde_json::to_string(&signature).unwrap();
    println!("{:?}", serialized);
    assert_eq!(
        format!(
            "\"{}\"",
            base64ct::Base64::encode_string(&signature.0.to_bytes())
        ),
        serialized
    );
    let deserialized: Ed25519Signature = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, signature);
}

#[test]
fn import_export_public_key() {
    let kpref = keys().pop().unwrap();
    let public_key = kpref.public();
    let export = public_key.encode_base64();
    let import = Ed25519PublicKey::decode_base64(&export);
    assert!(import.is_ok());
    assert_eq!(&import.unwrap(), public_key);
}

#[test]
fn import_export_secret_key() {
    let kpref = keys().pop().unwrap();
    let secret_key = kpref.private();
    let export = secret_key.encode_base64();
    let import = Ed25519PrivateKey::decode_base64(&export);
    assert!(import.is_ok());
    assert_eq!(import.unwrap().as_ref(), secret_key.as_ref());
}
#[test]
fn to_from_bytes_signature() {
    let kpref = keys().pop().unwrap();
    let signature = kpref.sign(b"Hello, world");
    let sig_bytes = signature.as_ref();
    let rebuilt_sig = <Ed25519Signature as ToFromBytes>::from_bytes(sig_bytes).unwrap();
    assert_eq!(rebuilt_sig, signature);
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

#[test]
fn verify_valid_batch() {
    // Make signatures.
    let message: &[u8] = b"Hello, world!";
    let digest = message.digest();
    let (pubkeys, signatures): (Vec<Ed25519PublicKey>, Vec<Ed25519Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    // Verify the batch.
    let res = Ed25519PublicKey::verify_batch(&digest.0, &pubkeys, &signatures);
    assert!(res.is_ok(), "{:?}", res);
}

#[test]
fn verify_invalid_batch() {
    // Make signatures.
    let message: &[u8] = b"Hello, world!";
    let digest = message.digest();
    let (pubkeys, mut signatures): (Vec<Ed25519PublicKey>, Vec<Ed25519Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    // mangle one signature
    signatures[0] = <Ed25519Signature as ToFromBytes>::from_bytes(&[0u8; 64]).unwrap();

    // Verify the batch.
    let res = Ed25519PublicKey::verify_batch(&digest.0, &pubkeys, &signatures);
    assert!(res.is_err(), "{:?}", res);
}

#[test]
fn verify_valid_aggregate_signature() {
    // Make signatures.
    let message: &[u8] = b"Hello, world!";
    let digest = message.digest();
    let (pubkeys, signatures): (Vec<Ed25519PublicKey>, Vec<Ed25519Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    let aggregated_signature = Ed25519AggregateSignature::aggregate(signatures).unwrap();

    // // Verify the batch.
    let res = aggregated_signature.verify(&pubkeys[..], &digest.0);
    assert!(res.is_ok(), "{:?}", res);
}

#[test]
fn verify_invalid_aggregate_signature_length_mismatch() {
    // Make signatures.
    let message: &[u8] = b"Hello, world!";
    let digest = message.digest();
    let (pubkeys, signatures): (Vec<Ed25519PublicKey>, Vec<Ed25519Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    let aggregated_signature = Ed25519AggregateSignature::aggregate(signatures).unwrap();

    // // Verify the batch.
    let res = aggregated_signature.verify(&pubkeys[..2], &digest.0);
    assert!(res.is_err(), "{:?}", res);
}

#[test]
fn verify_invalid_aggregate_signature_public_key_switch() {
    // Make signatures.
    let message: &[u8] = b"Hello, world!";
    let digest = message.digest();
    let (mut pubkeys, signatures): (Vec<Ed25519PublicKey>, Vec<Ed25519Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    let aggregated_signature = Ed25519AggregateSignature::aggregate(signatures).unwrap();

    pubkeys[0] = keys()[3].public().clone();

    // // Verify the batch.
    let res = aggregated_signature.verify(&pubkeys[..], &digest.0);
    assert!(res.is_err(), "{:?}", res);
}

#[test]
fn verify_batch_aggregate_signature() {
    // Make signatures.
    let message1: &[u8] = b"Hello, world!";
    let digest1 = message1.digest();
    let (pubkeys1, signatures1): (Vec<Ed25519PublicKey>, Vec<Ed25519Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest1.0);
            (kp.public().clone(), sig)
        })
        .unzip();
    let aggregated_signature1 = Ed25519AggregateSignature::aggregate(signatures1).unwrap();

    // Make signatures.
    let message2: &[u8] = b"Hello, world!";
    let digest2 = message2.digest();
    let (pubkeys2, signatures2): (Vec<Ed25519PublicKey>, Vec<Ed25519Signature>) = keys()
        .into_iter()
        .take(2)
        .map(|kp| {
            let sig = kp.sign(&digest2.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    let aggregated_signature2 = Ed25519AggregateSignature::aggregate(signatures2).unwrap();

    assert!(Ed25519AggregateSignature::batch_verify(
        &[aggregated_signature1, aggregated_signature2],
        &[&pubkeys1[..], &pubkeys2[..]],
        &[&digest1.0[..], &digest2.0[..]]
    )
    .is_ok());
}

#[test]
fn verify_batch_aggregate_signature_length_mismatch() {
    // Make signatures.
    let message1: &[u8] = b"Hello, world!";
    let digest1 = message1.digest();
    let (pubkeys1, signatures1): (Vec<Ed25519PublicKey>, Vec<Ed25519Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest1.0);
            (kp.public().clone(), sig)
        })
        .unzip();
    let aggregated_signature1 = Ed25519AggregateSignature::aggregate(signatures1).unwrap();

    // Make signatures.
    let message2: &[u8] = b"Hello, world!";
    let digest2 = message2.digest();
    let (pubkeys2, signatures2): (Vec<Ed25519PublicKey>, Vec<Ed25519Signature>) = keys()
        .into_iter()
        .take(2)
        .map(|kp| {
            let sig = kp.sign(&digest2.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    let aggregated_signature2 = Ed25519AggregateSignature::aggregate(signatures2).unwrap();

    assert!(Ed25519AggregateSignature::batch_verify(
        &[aggregated_signature1.clone(), aggregated_signature2.clone()],
        &[&pubkeys1[..]],
        &[&digest1.0[..], &digest2.0[..]]
    )
    .is_err());

    assert!(Ed25519AggregateSignature::batch_verify(
        &[aggregated_signature1.clone(), aggregated_signature2.clone()],
        &[&pubkeys1[..], &pubkeys2[1..]],
        &[&digest1.0[..], &digest2.0[..]]
    )
    .is_err());

    assert!(Ed25519AggregateSignature::batch_verify(
        &[aggregated_signature1, aggregated_signature2],
        &[&pubkeys1[..], &pubkeys2[..]],
        &[&digest2.0[..]]
    )
    .is_err());
}

#[test]
fn test_serialize_deserialize_aggregate_signatures() {
    // Test empty aggregate signature
    let sig = Ed25519AggregateSignature::default();
    let serialized = bincode::serialize(&sig).unwrap();
    let deserialized: Ed25519AggregateSignature = bincode::deserialize(&serialized).unwrap();
    assert_eq!(deserialized.0, sig.0);

    let message = b"hello, narwhal";
    // Test populated aggregate signature
    let (_, signatures): (Vec<Ed25519PublicKey>, Vec<Ed25519Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(message);
            (kp.public().clone(), sig)
        })
        .unzip();

    let sig = Ed25519AggregateSignature::aggregate(signatures).unwrap();
    let serialized = bincode::serialize(&sig).unwrap();
    let deserialized: Ed25519AggregateSignature = bincode::deserialize(&serialized).unwrap();
    assert_eq!(deserialized.0, sig.0);
}

#[test]
fn test_add_signatures_to_aggregate() {
    let pks: Vec<Ed25519PublicKey> = keys()
        .into_iter()
        .take(3)
        .map(|kp| kp.public().clone())
        .collect();
    let message = b"hello, narwhal";

    // Test 'add signature'
    let mut sig1 = Ed25519AggregateSignature::default();
    // Test populated aggregate signature
    keys().into_iter().take(3).for_each(|kp| {
        let sig = kp.sign(message);
        sig1.add_signature(sig).unwrap();
    });

    assert!(sig1.verify(&pks, message).is_ok());

    // Test 'add aggregate signature'
    let mut sig2 = Ed25519AggregateSignature::default();

    let kp = &keys()[0];
    let sig = Ed25519AggregateSignature::aggregate(vec![kp.sign(message)]).unwrap();
    sig2.add_aggregate(sig).unwrap();

    assert!(sig2.verify(&pks[0..1], message).is_ok());

    let aggregated_signature = Ed25519AggregateSignature::aggregate(
        keys()
            .into_iter()
            .take(3)
            .skip(1)
            .map(|kp| kp.sign(message))
            .collect(),
    )
    .unwrap();

    sig2.add_aggregate(aggregated_signature).unwrap();

    assert!(sig2.verify(&pks, message).is_ok());
}

#[test]
fn test_hkdf_generate_from_ikm() {
    let seed = &[
        0, 0, 1, 1, 2, 2, 4, 4, 8, 2, 0, 9, 3, 2, 4, 1, 1, 1, 2, 0, 1, 1, 3, 4, 1, 2, 9, 8, 7, 6,
        5, 4,
    ];
    let salt = &[3, 2, 1];
    let kp = hkdf_generate_from_ikm::<Sha3_256, Ed25519KeyPair>(seed, salt, None).unwrap();
    let kp2 = hkdf_generate_from_ikm::<Sha3_256, Ed25519KeyPair>(seed, salt, None).unwrap();
    assert_eq!(kp.private().as_bytes(), kp2.private().as_bytes());
}

#[test]
fn test_public_key_bytes_conversion() {
    let kp = keys().pop().unwrap();
    let pk_bytes: Ed25519PublicKeyBytes = kp.public().into();
    let rebuilded_pk: Ed25519PublicKey = pk_bytes.try_into().unwrap();
    assert_eq!(kp.public().as_bytes(), rebuilded_pk.as_bytes());
}

#[test]
fn test_copy_key_pair() {
    let kp = keys().pop().unwrap();
    let kp_copied = kp.copy();

    assert_eq!(kp.public().0.as_bytes(), kp_copied.public().0.as_bytes());
    assert_eq!(kp.private().0.as_bytes(), kp_copied.private().0.as_bytes());
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
