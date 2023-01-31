// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::crypto::bcs_signable_test::Foo;
use proptest::collection;
use proptest::prelude::*;

#[test]
fn public_key_equality() {
    let ed_kp1: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair().1);
    let ed_kp2: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair().1);
    let k1_kp1: SuiKeyPair = SuiKeyPair::Secp256k1(get_key_pair().1);
    let k1_kp2: SuiKeyPair = SuiKeyPair::Secp256k1(get_key_pair().1);

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

#[test]
fn test_serde_suikeypair_roundtrip() {
    let kp: Ed25519KeyPair = get_key_pair().1;
    let skp: SuiKeyPair = SuiKeyPair::Ed25519(kp);
    let serialized_skp = bincode::serialize(&skp).unwrap();
    let deserialized_skp: SuiKeyPair = bincode::deserialize(&serialized_skp).unwrap();
    assert_eq!(skp, deserialized_skp);
}

#[test]
fn test_serde_signature() {
    let sig = Ed25519SuiSignature::from_bytes(&[0; Ed25519SuiSignature::LENGTH]).unwrap();
    let sui_sig: Signature = sig.clone().into();
    let serialized_sig = bincode::serialize(&sig).unwrap();
    let serialized_sui_sig = bincode::serialize(&sui_sig).unwrap();
    let deserialized_sui_sig: Signature = bincode::deserialize(&serialized_sui_sig).unwrap();
    assert_eq!(sui_sig, deserialized_sui_sig);

    // the serde of [enum Signature] should be the same as all its members.
    assert_eq!(serialized_sui_sig.len(), serialized_sig.len());
}

#[test]
fn test_serde_public_key() {
    let skp: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair().1);
    let sui_pk: PublicKey = skp.public();
    let serialized_sui_pk = bincode::serialize(&sui_pk).unwrap();
    let deserialized_sui_pk: PublicKey = bincode::deserialize(&serialized_sui_pk).unwrap();
    assert_eq!(sui_pk, deserialized_sui_pk);
}

#[test]
fn test_serde_authority_public_key_bytes() {
    let pk = AuthorityPublicKeyBytes::from_bytes(&[0; AuthorityPublicKey::LENGTH]).unwrap();
    let serialized_pk = bincode::serialize(&pk).unwrap();
    let deserialized_pk: AuthorityPublicKeyBytes = bincode::deserialize(&serialized_pk).unwrap();
    assert_eq!(pk, deserialized_pk);
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
    fn test_from_bytes(
        bytes in collection::vec(any::<u8>(), 0..1024)
    ){
        let _apkb = AuthorityPublicKeyBytes::from_bytes(&bytes);
        let _suisig = Ed25519SuiSignature::from_bytes(&bytes);
        let _suisig = Secp256k1SuiSignature::from_bytes(&bytes);
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
