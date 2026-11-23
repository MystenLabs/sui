// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::crypto::{AuthorityKeyPair, KeypairTraits, RandomnessRound, get_key_pair_from_rng};
use crate::utils::create_fake_transaction;
use fastcrypto::groups::bls12381::Scalar;
use fastcrypto_threshold::polynomial::{Eval, Poly};
use fastcrypto_threshold::types::ShareIndex;
use rand::SeedableRng;
use rand::rngs::StdRng;

const THRESHOLD: u16 = 3;
const NUM_SHARES: u16 = 5;

struct TestBeacon {
    public_key: BeaconPublicKey,
    shares: Vec<Eval<Scalar>>,
}

impl TestBeacon {
    fn new(rng: &mut StdRng) -> Self {
        let private_poly = Poly::<Scalar>::rand(THRESHOLD - 1, rng);
        let public_key = private_poly.commit::<G2Element>().c0();
        let shares = (1..=NUM_SHARES)
            .map(|i| private_poly.eval(ShareIndex::new(i).unwrap()))
            .collect();
        Self { public_key, shares }
    }

    /// Derives the bundle key from `THRESHOLD` partial signatures, as the validators do.
    fn derive_key(&self, id_message: &[u8]) -> BundleKey {
        let partials =
            ThresholdBls12381MinSig::partial_sign_batch(self.shares.iter().skip(1), id_message);
        ThresholdBls12381MinSig::aggregate(THRESHOLD, partials.iter()).unwrap()
    }
}

fn author(rng: &mut StdRng) -> AuthorityName {
    let (_, key): (_, AuthorityKeyPair) = get_key_pair_from_rng(rng);
    key.public().into()
}

fn plaintext() -> BundlePlaintext {
    BundlePlaintext {
        transactions: vec![create_fake_transaction(), create_fake_transaction()],
    }
}

#[test]
fn seal_and_open() {
    let mut rng = StdRng::from_seed([1; 32]);
    let beacon = TestBeacon::new(&mut rng);
    let author = author(&mut rng);
    let plaintext = plaintext();

    let bundle = EncryptedBundle::seal(&mut rng, &beacon.public_key, 7, author, &plaintext);
    bundle.verify_signature().unwrap();

    let key = beacon.derive_key(&bundle.id_message());
    bundle.verify_key(&beacon.public_key, &key).unwrap();
    assert_eq!(bundle.open(&key).unwrap(), plaintext);

    let decoded: EncryptedBundle = bcs::from_bytes(&bcs::to_bytes(&bundle).unwrap()).unwrap();
    assert_eq!(decoded.open(&key).unwrap(), plaintext);
}

#[test]
fn key_for_other_identity_does_not_open() {
    let mut rng = StdRng::from_seed([2; 32]);
    let beacon = TestBeacon::new(&mut rng);
    let author = author(&mut rng);
    let plaintext = plaintext();

    let bundle = EncryptedBundle::seal(&mut rng, &beacon.public_key, 7, author, &plaintext);
    let other = EncryptedBundle::seal(&mut rng, &beacon.public_key, 7, author, &plaintext);
    assert_ne!(bundle.id(), other.id());

    let other_key = beacon.derive_key(&other.id_message());
    assert_eq!(
        bundle.verify_key(&beacon.public_key, &other_key),
        Err(EncryptedBundleError::InvalidKey)
    );
    assert_eq!(
        bundle.open(&other_key),
        Err(EncryptedBundleError::DecryptionFailed)
    );
}

#[test]
fn tampered_bundle_fails_signature_check() {
    let mut rng = StdRng::from_seed([3; 32]);
    let beacon = TestBeacon::new(&mut rng);
    let author_a = author(&mut rng);
    let author_b = author(&mut rng);
    let bundle = EncryptedBundle::seal(&mut rng, &beacon.public_key, 7, author_a, &plaintext());

    let mut changed_ciphertext = bundle.clone();
    changed_ciphertext.ciphertext[0] ^= 1;
    assert_eq!(
        changed_ciphertext.verify_signature(),
        Err(EncryptedBundleError::InvalidSignature)
    );

    let mut changed_epoch = bundle.clone();
    changed_epoch.epoch += 1;
    assert_eq!(
        changed_epoch.verify_signature(),
        Err(EncryptedBundleError::InvalidSignature)
    );

    let mut changed_author = bundle.clone();
    changed_author.author = author_b;
    assert_eq!(
        changed_author.verify_signature(),
        Err(EncryptedBundleError::InvalidSignature)
    );

    // An operator that replaces the one-time key with its own can sign the bundle, but the
    // identity changes, so the released key does not open the enclave's ciphertext.
    let (_, operator_key): (_, Ed25519KeyPair) = get_key_pair_from_rng(&mut rng);
    let mut replaced_vk = bundle.clone();
    replaced_vk.vk = operator_key.public().as_bytes().to_vec();
    let signature: Ed25519Signature = operator_key.sign(&replaced_vk.signing_message());
    replaced_vk.signature = signature.as_bytes().to_vec();
    replaced_vk.verify_signature().unwrap();
    let released_key = beacon.derive_key(&replaced_vk.id_message());
    assert_eq!(
        replaced_vk.open(&released_key),
        Err(EncryptedBundleError::DecryptionFailed)
    );
}

#[test]
fn malformed_fields_are_rejected() {
    let mut rng = StdRng::from_seed([4; 32]);
    let beacon = TestBeacon::new(&mut rng);
    let author = author(&mut rng);
    let bundle = EncryptedBundle::seal(&mut rng, &beacon.public_key, 7, author, &plaintext());

    let mut short_nonce = bundle.clone();
    short_nonce.nonce.pop();
    assert_eq!(
        short_nonce.verify_signature(),
        Err(EncryptedBundleError::Malformed("nonce"))
    );

    let mut zero_nonce = bundle.clone();
    zero_nonce.nonce = G2Element::zero().to_byte_array().to_vec();
    assert_eq!(
        zero_nonce.verify_signature(),
        Err(EncryptedBundleError::Malformed("nonce"))
    );

    let mut short_vk = bundle;
    short_vk.vk.pop();
    assert_eq!(
        short_vk.verify_signature(),
        Err(EncryptedBundleError::Malformed("vk"))
    );
}

#[test]
fn identity_messages_are_separated_from_beacon_rounds() {
    let mut rng = StdRng::from_seed([5; 32]);
    let beacon = TestBeacon::new(&mut rng);
    let author = author(&mut rng);
    let bundle = EncryptedBundle::seal(&mut rng, &beacon.public_key, 0, author, &plaintext());

    let round_prefix = RandomnessRound::new(0).signature_message();
    let round_prefix = &round_prefix[..round_prefix.len() - 8];
    assert!(!ID_TAG.starts_with(round_prefix));
    assert!(!round_prefix.starts_with(ID_TAG));
    assert_ne!(ID_TAG[0], round_prefix[0]);
    assert!(bundle.id_message().starts_with(ID_TAG));
}
