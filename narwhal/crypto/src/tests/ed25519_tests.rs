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
    traits::{AggregateAuthenticator, EncodeDecodeBase64, KeyPair, ToFromBytes, VerifyingKey},
};
use blake2::digest::Update;
use ed25519_consensus::VerificationKey;
use rand::{rngs::StdRng, SeedableRng as _};
use serde_reflection::{Samples, Tracer, TracerConfig};
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

    // serialize with Ed25519PrivateKey successes
    let privkey = bincode::deserialize::<Ed25519PrivateKey>(&bytes).unwrap();
    let bytes2 = bincode::serialize(&privkey).unwrap();
    assert_eq!(bytes, bytes2);

    // serialize with Ed25519PublicKey fails
    assert!(bincode::deserialize::<Ed25519PublicKey>(&bytes).is_err());
}

#[test]
fn custom_serde_reflection() {
    let config = TracerConfig::default()
        .record_samples_for_newtype_structs(true)
        .record_samples_for_structs(true)
        .record_samples_for_tuple_structs(true);
    let mut tracer = Tracer::new(config);
    let mut samples = Samples::new();

    let message = b"hello, narwhal";
    let sig = keys().pop().unwrap().sign(message);
    tracer
        .trace_value(&mut samples, &sig)
        .expect("trace value Ed25519Signature");
    assert!(samples.value("Ed25519Signature").is_some());
    tracer
        .trace_type::<Ed25519Signature>(&samples)
        .expect("trace type Ed25519PublicKey");

    let kpref = keys().pop().unwrap();
    let public_key = kpref.public();
    tracer
        .trace_value(&mut samples, public_key)
        .expect("trace value Ed25519PublicKey");
    assert!(samples.value("Ed25519PublicKey").is_some());
    // The Ed25519PublicKey struct and its ser/de implementation treats itself as a "newtype struct".
    // But `trace_type()` only supports the base type.
    tracer
        .trace_type::<VerificationKey>(&samples)
        .expect("trace type VerificationKey");
}

#[test]
fn test_serde_signatures_non_human_readable() {
    let message = b"hello, narwhal";
    // Test populated aggregate signature
    let sig = keys().pop().unwrap().sign(message);
    let serialized = bincode::serialize(&sig).unwrap();
    let deserialized: Ed25519Signature = bincode::deserialize(&serialized).unwrap();
    assert_eq!(deserialized.sig, sig.sig);
}

#[test]
fn test_serde_signatures_human_readable() {
    let kp = keys().pop().unwrap();
    let message: &[u8] = b"Hello, world!";
    let signature = kp.sign(message);

    let serialized = serde_json::to_string(&signature).unwrap();
    assert_eq!(
        format!(
            r#"{{"base64":"{}"}}"#,
            base64ct::Base64::encode_string(&signature.sig.to_bytes())
        ),
        serialized
    );
    let deserialized: Ed25519Signature = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.as_ref(), signature.as_ref());
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
    assert_eq!(rebuilt_sig.as_ref(), signature.as_ref());
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

fn signature_test_inputs() -> (Vec<u8>, Vec<Ed25519PublicKey>, Vec<Ed25519Signature>) {
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

    (digest.to_vec(), pubkeys, signatures)
}

#[test]
fn verify_valid_batch() {
    let (digest, pubkeys, signatures) = signature_test_inputs();

    let res = Ed25519PublicKey::verify_batch_empty_fail(&digest[..], &pubkeys, &signatures);
    assert!(res.is_ok(), "{:?}", res);
}

#[test]
fn verify_invalid_batch() {
    let (digest, pubkeys, mut signatures) = signature_test_inputs();
    // mangle one signature
    signatures[0] = <Ed25519Signature as ToFromBytes>::from_bytes(&[0u8; 64]).unwrap();

    let res = Ed25519PublicKey::verify_batch_empty_fail(&digest, &pubkeys, &signatures);
    assert!(res.is_err(), "{:?}", res);
}

#[test]
fn verify_empty_batch() {
    let (digest, _, _) = signature_test_inputs();

    let res = Ed25519PublicKey::verify_batch_empty_fail(&digest[..], &[], &[]);
    assert!(res.is_err(), "{:?}", res);
}

#[test]
fn verify_batch_missing_public_keys() {
    let (digest, pubkeys, signatures) = signature_test_inputs();

    // missing leading public keys
    let res = Ed25519PublicKey::verify_batch_empty_fail(&digest, &pubkeys[1..], &signatures);
    assert!(res.is_err(), "{:?}", res);

    // missing trailing public keys
    let res = Ed25519PublicKey::verify_batch_empty_fail(
        &digest,
        &pubkeys[..pubkeys.len() - 1],
        &signatures,
    );
    assert!(res.is_err(), "{:?}", res);
}

#[test]
fn verify_valid_aggregate_signaature() {
    let (digest, pubkeys, signatures) = signature_test_inputs();
    let aggregated_signature = Ed25519AggregateSignature::aggregate(signatures).unwrap();

    let res = aggregated_signature.verify(&pubkeys[..], &digest);
    assert!(res.is_ok(), "{:?}", res);
}

#[test]
fn verify_invalid_aggregate_signature_length_mismatch() {
    let (digest, pubkeys, signatures) = signature_test_inputs();
    let aggregated_signature = Ed25519AggregateSignature::aggregate(signatures).unwrap();

    let res = aggregated_signature.verify(&pubkeys[..2], &digest);
    assert!(res.is_err(), "{:?}", res);
}

#[test]
fn verify_invalid_aggregate_signature_public_key_switch() {
    let (digest, mut pubkeys, signatures) = signature_test_inputs();
    let aggregated_signature = Ed25519AggregateSignature::aggregate(signatures).unwrap();

    pubkeys[0] = keys()[3].public().clone();

    let res = aggregated_signature.verify(&pubkeys[..], &digest);
    assert!(res.is_err(), "{:?}", res);
}

fn verify_batch_aggregate_signature_inputs() -> (
    Vec<u8>,
    Vec<u8>,
    Vec<Ed25519PublicKey>,
    Vec<Ed25519PublicKey>,
    Ed25519AggregateSignature,
    Ed25519AggregateSignature,
) {
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
    let message2: &[u8] = b"Hello, worl!";
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
    (
        digest1.to_vec(),
        digest2.to_vec(),
        pubkeys1,
        pubkeys2,
        aggregated_signature1,
        aggregated_signature2,
    )
}

#[test]
fn verify_batch_aggregate_signature() {
    let (digest1, digest2, pubkeys1, pubkeys2, aggregated_signature1, aggregated_signature2) =
        verify_batch_aggregate_signature_inputs();

    assert!(Ed25519AggregateSignature::batch_verify(
        &[aggregated_signature1, aggregated_signature2],
        &[&pubkeys1[..], &pubkeys2[..]],
        &[&digest1[..], &digest2[..]]
    )
    .is_ok());
}

#[test]
fn verify_batch_missing_parameters_length_mismatch() {
    let (digest1, digest2, pubkeys1, pubkeys2, aggregated_signature1, aggregated_signature2) =
        verify_batch_aggregate_signature_inputs();

    // Fewer pubkeys than signatures
    assert!(Ed25519AggregateSignature::batch_verify(
        &[aggregated_signature1.clone(), aggregated_signature2.clone()],
        &[&pubkeys1[..]],
        &[&digest1[..], &digest2[..]]
    )
    .is_err());
    assert!(Ed25519AggregateSignature::batch_verify(
        &[aggregated_signature1.clone(), aggregated_signature2.clone()],
        &[&pubkeys1[..]],
        &[&digest1[..]]
    )
    .is_err());

    // Fewer messages than signatures
    assert!(Ed25519AggregateSignature::batch_verify(
        &[aggregated_signature1.clone(), aggregated_signature2.clone()],
        &[&pubkeys1[..], &pubkeys2[..]],
        &[&digest1[..]]
    )
    .is_err());
    assert!(Ed25519AggregateSignature::batch_verify(
        &[aggregated_signature1, aggregated_signature2],
        &[&pubkeys1[..]],
        &[&digest1[..]]
    )
    .is_err());
}

#[test]
fn verify_batch_missing_keys_in_batch() {
    let (digest1, digest2, pubkeys1, pubkeys2, aggregated_signature1, aggregated_signature2) =
        verify_batch_aggregate_signature_inputs();

    // Pubkeys missing at the end
    assert!(Ed25519AggregateSignature::batch_verify(
        &[aggregated_signature1.clone(), aggregated_signature2.clone()],
        &[&pubkeys1[..], &pubkeys2[1..]],
        &[&digest1[..], &digest2[..]]
    )
    .is_err());

    // Pubkeys missing at the start
    assert!(Ed25519AggregateSignature::batch_verify(
        &[aggregated_signature1.clone(), aggregated_signature2.clone()],
        &[&pubkeys1[..], &pubkeys2[..pubkeys2.len() - 1]],
        &[&digest1[..], &digest2[..]]
    )
    .is_err());

    // add an extra signature to both aggregated_signature that batch_verify takes in
    let mut signatures1_with_extra = aggregated_signature1;
    let kp = &keys()[0];
    let sig = kp.sign(&digest1);
    let res = signatures1_with_extra.add_signature(sig);
    assert!(res.is_ok());

    let mut signatures2_with_extra = aggregated_signature2;
    let kp = &keys()[0];
    let sig2 = kp.sign(&digest1);
    let res = signatures2_with_extra.add_signature(sig2);
    assert!(res.is_ok());

    assert!(Ed25519AggregateSignature::batch_verify(
        &[
            signatures1_with_extra.clone(),
            signatures2_with_extra.clone()
        ],
        &[&pubkeys1[..]],
        &[&digest1[..], &digest2[..]]
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
