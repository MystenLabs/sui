// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::secp256k1::Secp256k1KeyPair;

use crate::crypto::AccountKeyPair;
use crate::crypto::SuiSignature;
use crate::{crypto::bcs_signable_test::Foo, signature_seed::SignatureSeed};

#[cfg(test)]
const TEST_ID: [u8; 16] = [0u8; 16];
#[cfg(test)]
const TEST_DOMAIN: [u8; 16] = [1u8; 16];

#[test]
fn test_deterministic_addresses_by_id() {
    let seed = SignatureSeed::default();

    let id_0 = [0u8; 32];
    let id_1 = [1u8; 32];

    // Create two addresses with the same ID and check they are equal.
    let sui_address_0_0 =
        seed.new_deterministic_address::<AccountKeyPair>(&id_0, Some(&TEST_DOMAIN));
    assert!(sui_address_0_0.is_ok());

    let sui_address_0_1 =
        seed.new_deterministic_address::<AccountKeyPair>(&id_0, Some(&TEST_DOMAIN));
    assert!(sui_address_0_1.is_ok());

    assert_eq!(sui_address_0_0.unwrap(), sui_address_0_1.clone().unwrap());

    // Create an address with a different ID and check that it differs from the previous one.
    let sui_address_1_0 =
        seed.new_deterministic_address::<AccountKeyPair>(&id_1, Some(&TEST_DOMAIN));
    assert!(sui_address_1_0.is_ok());

    assert_ne!(sui_address_0_1.unwrap(), sui_address_1_0.unwrap());
}

#[test]
fn test_deterministic_addresses_by_seed() {
    let seed_0 = SignatureSeed::from_bytes(&[0u8; 32]).unwrap();
    let seed_1 = SignatureSeed::from_bytes(&[1u8; 32]).unwrap();

    // Create two addresses with the same ID but different seed and check that they differ.
    let sui_address_0 =
        seed_0.new_deterministic_address::<AccountKeyPair>(&TEST_ID, Some(&TEST_DOMAIN));
    assert!(sui_address_0.is_ok());

    let sui_address_1 =
        seed_1.new_deterministic_address::<AccountKeyPair>(&TEST_ID, Some(&TEST_DOMAIN));
    assert!(sui_address_1.is_ok());

    assert_ne!(sui_address_0.unwrap(), sui_address_1.unwrap());
}

#[test]
fn test_deterministic_addresses_by_domain() {
    let seed = SignatureSeed::default();

    let domain_0 = [0u8; 16];
    let domain_1 = [1u8; 16];

    // Create two addresses with the same ID but different domain (they should differ)
    let sui_address_0 = seed.new_deterministic_address::<AccountKeyPair>(&TEST_ID, Some(&domain_0));
    assert!(sui_address_0.is_ok());

    let sui_address_1 = seed.new_deterministic_address::<AccountKeyPair>(&TEST_ID, Some(&domain_1));
    assert!(sui_address_1.is_ok());

    assert_ne!(sui_address_0.unwrap(), sui_address_1.unwrap());
}

#[test]
fn test_deterministic_signing() {
    let seed = SignatureSeed::default();

    let id_0 = [0u8; 32];
    let id_1 = [1u8; 32];

    let msg0 = Foo("test0".to_string());
    let msg1 = Foo("test1".to_string());

    // Create two addresses with a different ID.
    let sui_address_0 = seed
        .new_deterministic_address::<AccountKeyPair>(&id_0, Some(&TEST_DOMAIN))
        .unwrap();
    let sui_address_1 = seed
        .new_deterministic_address::<AccountKeyPair>(&id_1, Some(&TEST_DOMAIN))
        .unwrap();

    // Sign with both addresses.
    let sig_0 = seed.sign::<_, AccountKeyPair>(&id_0, Some(&TEST_DOMAIN), &msg0);
    assert!(sig_0.is_ok());
    let sig_0_ok = sig_0.unwrap();

    let sig_1 = seed.sign::<_, AccountKeyPair>(&id_1, Some(&TEST_DOMAIN), &msg0);
    assert!(sig_1.is_ok());

    // Verify signatures.
    let ver_0 = sig_0_ok.verify(&msg0, sui_address_0);
    assert!(ver_0.is_ok());

    let ver_1 = sig_1.unwrap().verify(&msg0, sui_address_1);
    assert!(ver_1.is_ok());

    // Ensure that signatures cannot be verified against another address.
    let ver_0_with_address_1 = sig_0_ok.verify(&msg0, sui_address_1);
    assert!(ver_0_with_address_1.is_err());

    // Ensure that signatures cannot be verified against another message.
    let ver_0_with_msg1 = sig_0_ok.verify(&msg1, sui_address_0);
    assert!(ver_0_with_msg1.is_err());

    // As we use ed25519, ensure that signatures on the same message are deterministic.
    let sig_0_1 = seed
        .sign::<_, AccountKeyPair>(&id_0, Some(&TEST_DOMAIN), &msg0)
        .unwrap();

    assert_eq!(sig_0_ok.as_ref(), sig_0_1.as_ref())
}

#[test]
fn seed_zeroize_on_drop() {
    let secret_ptr: *const u8;

    {
        // scope for the seed to ensure it's been dropped
        let seed = SignatureSeed::from_bytes(&[0x15u8; 32][..]).unwrap();
        secret_ptr = seed.0.as_ptr();
    }

    let memory: &[u8] = unsafe { std::slice::from_raw_parts(secret_ptr, 32) };
    assert!(!memory.contains(&0x15));
}

#[test]
fn test_sign_verify_with_secp256k1() {
    let seed = SignatureSeed::default();
    let id_0 = [0u8; 32];
    let msg0 = Foo("test0".to_string());

    let sui_address_0 = seed
        .new_deterministic_address::<Secp256k1KeyPair>(&id_0, Some(&TEST_DOMAIN))
        .unwrap();

    let sig_0 = seed.sign::<_, Secp256k1KeyPair>(&id_0, Some(&TEST_DOMAIN), &msg0);
    assert!(sig_0.is_ok());
    let sig_0_ok = sig_0.unwrap();

    let ver_0 = sig_0_ok.verify(&msg0, sui_address_0);
    assert!(ver_0.is_ok());
}
