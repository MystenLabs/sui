// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    bls12377::{
        BLS12377AggregateSignature, BLS12377KeyPair, BLS12377PrivateKey, BLS12377PublicKey,
        BLS12377PublicKeyBytes, BLS12377Signature,
    },
    traits::{AggregateAuthenticator, EncodeDecodeBase64, ToFromBytes, VerifyingKey},
};
use rand::{rngs::StdRng, SeedableRng as _};
use signature::{Signer, Verifier};

pub fn keys() -> Vec<BLS12377KeyPair> {
    let mut rng = StdRng::from_seed([0; 32]);
    (0..4)
        .map(|_| BLS12377KeyPair::generate(&mut rng))
        .collect()
}

#[test]
fn import_export_public_key() {
    let kpref = keys().pop().unwrap();
    let public_key = kpref.public();
    let export = public_key.encode_base64();
    let import = BLS12377PublicKey::decode_base64(&export);
    assert!(import.is_ok());
    assert_eq!(&import.unwrap(), public_key);
}

#[test]
fn import_export_secret_key() {
    let kpref = keys().pop().unwrap();
    let secret_key = kpref.private();
    let export = secret_key.encode_base64();
    let import = BLS12377PrivateKey::decode_base64(&export);
    assert!(import.is_ok());
    assert_eq!(import.unwrap().as_ref(), secret_key.as_ref());
}

#[test]
fn to_from_bytes_signature() {
    let kpref = keys().pop().unwrap();
    let signature = kpref.sign(b"Hello, world");
    let sig_bytes = signature.as_ref();
    let rebuilt_sig = <BLS12377Signature as ToFromBytes>::from_bytes(sig_bytes).unwrap();
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
    let (pubkeys, signatures): (Vec<BLS12377PublicKey>, Vec<BLS12377Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    // Verify the batch.
    let res = BLS12377PublicKey::verify_batch(&digest.0, &pubkeys, &signatures);
    assert!(res.is_ok(), "{:?}", res);
}

#[test]
fn verify_invalid_batch() {
    // Make signatures.
    let message: &[u8] = b"Hello, world!";
    let digest = message.digest();
    let (pubkeys, mut signatures): (Vec<BLS12377PublicKey>, Vec<BLS12377Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    // mangle one signature
    signatures[0] = BLS12377Signature::default();

    // Verify the batch.
    let res = BLS12377PublicKey::verify_batch(&digest.0, &pubkeys, &signatures);
    assert!(res.is_err(), "{:?}", res);
}

#[test]
fn verify_valid_aggregate_signature() {
    // Make signatures.
    let message: &[u8] = b"Hello, world!";
    let digest = message.digest();
    let (pubkeys, signatures): (Vec<BLS12377PublicKey>, Vec<BLS12377Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    let aggregated_signature = BLS12377AggregateSignature::aggregate(signatures).unwrap();

    // // Verify the batch.
    let res = aggregated_signature.verify(&pubkeys[..], &digest.0);
    assert!(res.is_ok(), "{:?}", res);
}

#[test]
fn verify_invalid_aggregate_signature_length_mismatch() {
    // Make signatures.
    let message: &[u8] = b"Hello, world!";
    let digest = message.digest();
    let (pubkeys, signatures): (Vec<BLS12377PublicKey>, Vec<BLS12377Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    let aggregated_signature = BLS12377AggregateSignature::aggregate(signatures).unwrap();

    // // Verify the batch.
    let res = aggregated_signature.verify(&pubkeys[..2], &digest.0);
    assert!(res.is_err(), "{:?}", res);
}

#[test]
fn verify_invalid_aggregate_signature_public_key_switch() {
    // Make signatures.
    let message: &[u8] = b"Hello, world!";
    let digest = message.digest();
    let (mut pubkeys, signatures): (Vec<BLS12377PublicKey>, Vec<BLS12377Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    let aggregated_signature = BLS12377AggregateSignature::aggregate(signatures).unwrap();

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
    let (pubkeys1, signatures1): (Vec<BLS12377PublicKey>, Vec<BLS12377Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest1.0);
            (kp.public().clone(), sig)
        })
        .unzip();
    let aggregated_signature1 = BLS12377AggregateSignature::aggregate(signatures1).unwrap();

    // Make signatures.
    let message2: &[u8] = b"Hello, world!";
    let digest2 = message2.digest();
    let (pubkeys2, signatures2): (Vec<BLS12377PublicKey>, Vec<BLS12377Signature>) = keys()
        .into_iter()
        .take(2)
        .map(|kp| {
            let sig = kp.sign(&digest2.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    let aggregated_signature2 = BLS12377AggregateSignature::aggregate(signatures2).unwrap();

    assert!(BLS12377AggregateSignature::batch_verify(
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
    let (pubkeys1, signatures1): (Vec<BLS12377PublicKey>, Vec<BLS12377Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(&digest1.0);
            (kp.public().clone(), sig)
        })
        .unzip();
    let aggregated_signature1 = BLS12377AggregateSignature::aggregate(signatures1).unwrap();

    // Make signatures.
    let message2: &[u8] = b"Hello, world!";
    let digest2 = message2.digest();
    let (pubkeys2, signatures2): (Vec<BLS12377PublicKey>, Vec<BLS12377Signature>) = keys()
        .into_iter()
        .take(2)
        .map(|kp| {
            let sig = kp.sign(&digest2.0);
            (kp.public().clone(), sig)
        })
        .unzip();

    let aggregated_signature2 = BLS12377AggregateSignature::aggregate(signatures2).unwrap();

    assert!(BLS12377AggregateSignature::batch_verify(
        &[aggregated_signature1.clone(), aggregated_signature2.clone()],
        &[&pubkeys1[..]],
        &[&digest1.0[..], &digest2.0[..]]
    )
    .is_err());

    assert!(BLS12377AggregateSignature::batch_verify(
        &[aggregated_signature1.clone(), aggregated_signature2.clone()],
        &[&pubkeys1[..], &pubkeys2[1..]],
        &[&digest1.0[..], &digest2.0[..]]
    )
    .is_err());

    assert!(BLS12377AggregateSignature::batch_verify(
        &[aggregated_signature1, aggregated_signature2],
        &[&pubkeys1[..], &pubkeys2[..]],
        &[&digest2.0[..]]
    )
    .is_err());
}

#[test]
fn test_serialize_deserialize_aggregate_signatures() {
    // Test empty aggregate signature
    let sig = BLS12377AggregateSignature::default();
    let serialized = bincode::serialize(&sig).unwrap();
    let deserialized: BLS12377AggregateSignature = bincode::deserialize(&serialized).unwrap();
    assert_eq!(deserialized.as_ref(), sig.as_ref());

    let message = b"hello, narwhal";
    // Test populated aggregate signature
    let (_, signatures): (Vec<BLS12377PublicKey>, Vec<BLS12377Signature>) = keys()
        .into_iter()
        .take(3)
        .map(|kp| {
            let sig = kp.sign(message);
            (kp.public().clone(), sig)
        })
        .unzip();

    let sig = BLS12377AggregateSignature::aggregate(signatures).unwrap();
    let serialized = bincode::serialize(&sig).unwrap();
    let deserialized: BLS12377AggregateSignature = bincode::deserialize(&serialized).unwrap();
    assert_eq!(deserialized.as_ref(), sig.as_ref());
}

#[test]
fn test_add_signatures_to_aggregate() {
    let pks: Vec<BLS12377PublicKey> = keys()
        .into_iter()
        .take(3)
        .map(|kp| kp.public().clone())
        .collect();
    let message = b"hello, narwhal";

    // Test 'add signature'
    let mut sig1 = BLS12377AggregateSignature::default();
    // Test populated aggregate signature
    keys().into_iter().take(3).for_each(|kp| {
        let sig = kp.sign(message);
        sig1.add_signature(sig).unwrap();
    });

    assert!(sig1.verify(&pks, message).is_ok());

    // Test 'add aggregate signature'
    let mut sig2 = BLS12377AggregateSignature::default();

    let kp = &keys()[0];
    let sig = BLS12377AggregateSignature::aggregate(vec![kp.sign(message)]).unwrap();
    sig2.add_aggregate(sig).unwrap();

    assert!(sig2.verify(&pks[0..1], message).is_ok());

    let aggregated_signature = BLS12377AggregateSignature::aggregate(
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
fn test_public_key_bytes_conversion() {
    let kp = keys().pop().unwrap();
    let pk_bytes: BLS12377PublicKeyBytes = kp.public().into();
    let rebuilded_pk: BLS12377PublicKey = pk_bytes.try_into().unwrap();
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
