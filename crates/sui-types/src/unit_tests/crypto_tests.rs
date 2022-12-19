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
fn test_pop() {
    // The result from this deterministic test is used in crates/sui-framework/tests/validator_tests.move
    // sender address: 0x21b60aa9a8cb189ccbe20461dbfad2202fdef55b
    // pop: 89f311605323ce0151b24adba390692e49faa19c9fedad72a04af228796acb3e147cf4b7a85794265bbf6dda15897090
    // pk: 99f25ef61f8032b914636460982c5cc6f134ef1ddae76657f2cbfec1ebfc8d097374080df6fcf0dcb8bc4b0d8e0af5d80ebbff2b4c599f54f42d6312dfc314276078c1cc347ebbbec5198be258513f386b930d02c2749a803e2330955ebd1a10

    let keypair: AuthorityKeyPair = get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
    let (address, _): (SuiAddress, AccountKeyPair) =
        get_key_pair_from_rng(&mut StdRng::from_seed([0; 32]));
    let mut domain_with_pk: Vec<u8> = Vec::new();
    domain_with_pk.extend_from_slice(PROOF_OF_POSSESSION_DOMAIN);
    domain_with_pk.extend_from_slice(keypair.public().as_bytes());
    domain_with_pk.extend_from_slice(address.as_ref());
    let pop = generate_proof_of_possession(&keypair, address);
    println!("pop= {:?}", Hex::encode(pop.as_bytes()));
    println!("pk= {:?}", Hex::encode(keypair.public().as_bytes()));
    println!("add= {:?}", address);
    let intent_msg = IntentMessage::new(
        Intent::default().with_scope(IntentScope::ProofOfPossession),
        domain_with_pk,
    );
    assert!(pop
        .verify_secure(&intent_msg, None, keypair.public().into())
        .is_ok());
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
