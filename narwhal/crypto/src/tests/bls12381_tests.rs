// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    bls12381::{
        BLS12381AggregateSignature, BLS12381KeyPair, BLS12381PrivateKey, BLS12381PublicKey,
        BLS12381PublicKeyBytes, BLS12381Signature,
    },
    hkdf::hkdf_generate_from_ikm,
    traits::{AggregateAuthenticator, EncodeDecodeBase64, ToFromBytes, VerifyingKey},
};
use rand::{rngs::StdRng, SeedableRng as _};
use sha3::Sha3_256;
use signature::{Signer, Verifier};

pub fn keys() -> Vec<BLS12381KeyPair> {
    let mut rng = StdRng::from_seed([0; 32]);
    (0..4)
        .map(|_| BLS12381KeyPair::generate(&mut rng))
        .collect()
}

#[test]
fn import_export_public_key() {
    let kpref = keys().pop().unwrap();
    let public_key = kpref.public();
    let export = public_key.encode_base64();
    let import = BLS12381PublicKey::decode_base64(&export);
    assert!(import.is_ok());
    assert_eq!(&import.unwrap(), public_key);
}

#[test]
fn import_export_secret_key() {
    let kpref = keys().pop().unwrap();
    let secret_key = kpref.private();
    let export = secret_key.encode_base64();
    let import = BLS12381PrivateKey::decode_base64(&export);
    assert!(import.is_ok());
    assert_eq!(import.unwrap().as_ref(), secret_key.as_ref());
}

#[test]
fn to_from_bytes_signature() {
    let kpref = keys().pop().unwrap();
    let signature = kpref.sign(b"Hello, world");
    let sig_bytes = signature.as_ref();
    let rebuilt_sig = <BLS12381Signature as ToFromBytes>::from_bytes(sig_bytes).unwrap();
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
    let (pubkeys, signatures): (Vec<BLS12381PublicKey>, Vec<BLS12381Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    // Verify the batch.
    let res = BLS12381PublicKey::verify_batch(&digest.0, &pubkeys, &signatures);
    assert!(res.is_ok(), "{:?}", res);
}

#[test]
fn verify_invalid_batch() {
    // Make signatures.
    let message: &[u8] = b"Hello, world!";
    let digest = message.digest();
    let (pubkeys, mut signatures): (Vec<BLS12381PublicKey>, Vec<BLS12381Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    // mangle one signature
    signatures[0] = BLS12381Signature::default();

    // Verify the batch.
    let res = BLS12381PublicKey::verify_batch(&digest.0, &pubkeys, &signatures);
    assert!(res.is_err(), "{:?}", res);
}

#[test]
fn verify_valid_aggregate_signature() {
    // Make signatures.
    let message: &[u8] = b"Hello, world!";
    let digest = message.digest();
    let (pubkeys, signatures): (Vec<BLS12381PublicKey>, Vec<BLS12381Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    let aggregated_signature = BLS12381AggregateSignature::aggregate(signatures).unwrap();

    // // Verify the batch.
    let res = aggregated_signature.verify(&pubkeys[..], &digest.0);
    assert!(res.is_ok(), "{:?}", res);
}

#[test]
fn verify_invalid_aggregate_signature_length_mismatch() {
    // Make signatures.
    let message: &[u8] = b"Hello, world!";
    let digest = message.digest();
    let (pubkeys, signatures): (Vec<BLS12381PublicKey>, Vec<BLS12381Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    let aggregated_signature = BLS12381AggregateSignature::aggregate(signatures).unwrap();

    // // Verify the batch.
    let res = aggregated_signature.verify(&pubkeys[..2], &digest.0);
    assert!(res.is_err(), "{:?}", res);
}

#[test]
fn verify_invalid_aggregate_signature_public_key_switch() {
    // Make signatures.
    let message: &[u8] = b"Hello, world!";
    let digest = message.digest();
    let (mut pubkeys, signatures): (Vec<BLS12381PublicKey>, Vec<BLS12381Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    let aggregated_signature = BLS12381AggregateSignature::aggregate(signatures).unwrap();

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
    let (pubkeys1, signatures1): (Vec<BLS12381PublicKey>, Vec<BLS12381Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest1.0);
            (kp.public().clone(), sig)
        })
        .unzip();
    let aggregated_signature1 = BLS12381AggregateSignature::aggregate(signatures1).unwrap();

    // Make signatures.
    let message2: &[u8] = b"Hello, world!";
    let digest2 = message2.digest();
    let (pubkeys2, signatures2): (Vec<BLS12381PublicKey>, Vec<BLS12381Signature>) = keys()
        .into_iter()
        .take(2)
        .map(|kp| {
            let sig = kp.sign(&digest2.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    let aggregated_signature2 = BLS12381AggregateSignature::aggregate(signatures2).unwrap();

    assert!(BLS12381AggregateSignature::batch_verify(
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
    let (pubkeys1, signatures1): (Vec<BLS12381PublicKey>, Vec<BLS12381Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest1.0);
            (kp.public().clone(), sig)
        })
        .unzip();
    let aggregated_signature1 = BLS12381AggregateSignature::aggregate(signatures1).unwrap();

    // Make signatures.
    let message2: &[u8] = b"Hello, world!";
    let digest2 = message2.digest();
    let (pubkeys2, signatures2): (Vec<BLS12381PublicKey>, Vec<BLS12381Signature>) = keys()
        .into_iter()
        .take(2)
        .map(|kp| {
            let sig = kp.sign(&digest2.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    let aggregated_signature2 = BLS12381AggregateSignature::aggregate(signatures2).unwrap();

    assert!(BLS12381AggregateSignature::batch_verify(
        &[aggregated_signature1.clone(), aggregated_signature2.clone()],
        &[&pubkeys1[..]],
        &[&digest1.0[..], &digest2.0[..]]
    )
    .is_err());

    assert!(BLS12381AggregateSignature::batch_verify(
        &[aggregated_signature1.clone(), aggregated_signature2.clone()],
        &[&pubkeys1[..], &pubkeys2[1..]],
        &[&digest1.0[..], &digest2.0[..]]
    )
    .is_err());

    assert!(BLS12381AggregateSignature::batch_verify(
        &[aggregated_signature1, aggregated_signature2],
        &[&pubkeys1[..], &pubkeys2[..]],
        &[&digest2.0[..]]
    )
    .is_err());
}

#[test]
fn test_serialize_deserialize_aggregate_signatures() {
    // Test empty aggregate signature
    let sig = BLS12381AggregateSignature::default();
    let serialized = bincode::serialize(&sig).unwrap();
    let deserialized: BLS12381AggregateSignature = bincode::deserialize(&serialized).unwrap();
    assert_eq!(deserialized.as_ref(), sig.as_ref());

    let message = b"hello, narwhal";
    // Test populated aggregate signature
    let (_, signatures): (Vec<BLS12381PublicKey>, Vec<BLS12381Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(message);
            (kp.public().clone(), sig)
        })
        .unzip();

    let sig = BLS12381AggregateSignature::aggregate(signatures).unwrap();
    let serialized = bincode::serialize(&sig).unwrap();
    let deserialized: BLS12381AggregateSignature = bincode::deserialize(&serialized).unwrap();
    assert_eq!(deserialized.as_ref(), sig.as_ref());
}

#[test]
fn test_add_signatures_to_aggregate() {
    let pks: Vec<BLS12381PublicKey> = keys()
        .into_iter()
        .take(3)
        .map(|kp| kp.public().clone())
        .collect();
    let message = b"hello, narwhal";

    // Test 'add signature'
    let mut sig1 = BLS12381AggregateSignature::default();
    // Test populated aggregate signature
    keys().into_iter().take(3).for_each(|kp| {
        let sig = kp.sign(message);
        sig1.add_signature(sig).unwrap();
    });

    assert!(sig1.verify(&pks, message).is_ok());

    // Test 'add aggregate signature'
    let mut sig2 = BLS12381AggregateSignature::default();

    let kp = &keys()[0];
    let sig = BLS12381AggregateSignature::aggregate(vec![kp.sign(message)]).unwrap();
    sig2.add_aggregate(sig).unwrap();

    assert!(sig2.verify(&pks[0..1], message).is_ok());

    let aggregated_signature = BLS12381AggregateSignature::aggregate(
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
fn test_human_readable_signatures() {
    let kp = keys().pop().unwrap();
    let message: &[u8] = b"Hello, world!";
    let signature = kp.sign(message);

    let serialized = serde_json::to_string(&signature).unwrap();
    assert_eq!(
        format!(
            "{{\"sig\":\"{}\"}}",
            base64ct::Base64::encode_string(&signature.sig.to_bytes())
        ),
        serialized
    );
    let deserialized: BLS12381Signature = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, signature);
}

#[test]
fn test_hkdf_generate_from_ikm() {
    let seed = &[
        0, 0, 1, 1, 2, 2, 4, 4, 8, 2, 0, 9, 3, 2, 4, 1, 1, 1, 2, 0, 1, 1, 3, 4, 1, 2, 9, 8, 7, 6,
        5, 4,
    ];
    let salt = &[3, 2, 1];
    let kp = hkdf_generate_from_ikm::<Sha3_256, BLS12381KeyPair>(seed, salt, Some(&[1])).unwrap();
    let kp2 = hkdf_generate_from_ikm::<Sha3_256, BLS12381KeyPair>(seed, salt, Some(&[1])).unwrap();

    assert_eq!(kp.private().as_bytes(), kp2.private().as_bytes());
}

#[test]
fn test_public_key_bytes_conversion() {
    let kp = keys().pop().unwrap();
    let pk_bytes: BLS12381PublicKeyBytes = kp.public().into();
    let rebuilded_pk: BLS12381PublicKey = pk_bytes.try_into().unwrap();
    assert_eq!(kp.public().as_bytes(), rebuilded_pk.as_bytes());
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
