// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::crypto::bcs_signable_test::Foo;
use proptest::collection;
use proptest::prelude::*;

#[test]
fn public_key_equality() {
    let (_, ed_kp1) = random_key_pair_by_type(SignatureScheme::ED25519).unwrap();
    let (_, ed_kp2) = random_key_pair_by_type(SignatureScheme::ED25519).unwrap();
    let (_, k1_kp1) = random_key_pair_by_type(SignatureScheme::Secp256k1).unwrap();
    let (_, k1_kp2) = random_key_pair_by_type(SignatureScheme::Secp256k1).unwrap();

    let ed_pk1 = ed_kp1.public();
    let ed_pk2 = ed_kp2.public();
    let k1_pk1 = k1_kp1.public();
    let k1_pk2 = k1_kp2.public();

    // reflexivity
    assert_eq!(ed_pk1, ed_pk1);
    assert_eq!(ed_pk2, ed_pk2);
    assert_eq!(k1_pk1, k1_pk1);
    assert_eq!(k1_pk2, k1_pk2);

    // different scheme
    assert_ne!(ed_pk1, k1_pk1);
    assert_ne!(ed_pk1, k1_pk2);
    assert_ne!(ed_pk2, k1_pk1);
    assert_ne!(ed_pk2, k1_pk2);

    // different key
    assert_ne!(ed_pk1, ed_pk2);
    assert_ne!(k1_pk1, k1_pk2);
}

proptest! {
    // Check those functions do not panic
    #[test]
    fn test_get_key_pair_from_bytes(
        bytes in collection::vec(any::<u8>(), 0..1024)
    ){
        let _key_pair = get_key_pair_from_bytes::<AuthorityKeyPair>(&bytes);
        let _key_pair = get_key_pair_from_bytes::<NetworkKeyPair>(&bytes);
        let _key_pair = get_key_pair_from_bytes::<AccountKeyPair>(&bytes);
    }

    #[test]
    fn test_from_signable_bytes(
        bytes in collection::vec(any::<u8>(), 0..1024)
    ){
        let _foo = Foo::from_signable_bytes(&bytes);
    }

    #[test]
    fn test_authority_pk_bytes(
        bytes in collection::vec(any::<u8>(), 0..1024)
    ){
        let _apkb = AuthorityPublicKeyBytes::from_bytes(&bytes);
        let _suisig = Ed25519SuiSignature::from_bytes(&bytes);
        let _suisig = Secp256k1SuiSignature::from_bytes(&bytes);
        let _pk = PublicKey::try_from_bytes(SignatureScheme::BLS12381, &bytes);
        let _pk = PublicKey::try_from_bytes(SignatureScheme::ED25519, &bytes);
        let _pk = PublicKey::try_from_bytes(SignatureScheme::Secp256k1, &bytes);
        let _sig = Signature::from_bytes(&bytes);
    }

    #[test]
    fn test_deserialize_keypair(
        bytes in collection::vec(any::<u8>(), 0..1024)
    ){
        let _skp: Result<SuiKeyPair, _> = bincode::deserialize(&bytes);
        let _pk: Result<PublicKey, _> = bincode::deserialize(&bytes);
    }


}
