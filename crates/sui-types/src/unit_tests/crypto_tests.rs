// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::crypto::bcs_signable_test::Foo;
use crate::signature::GenericSignature;
use proptest::collection;
use proptest::prelude::*;

#[test]
fn serde_keypair() {
    let skp = SuiKeyPair::Ed25519(Ed25519KeyPair::generate(&mut StdRng::from_seed([0; 32])));
    let encoded = skp.encode().unwrap();
    assert_eq!(
        encoded,
        "suiprivkey1qzdlfxn2qa2lj5uprl8pyhexs02sg2wrhdy7qaq50cqgnffw4c2477kg9h3"
    );
    let decoded = SuiKeyPair::decode(&encoded).unwrap();
    assert_eq!(skp, decoded);
}

#[test]
fn serde_pubkey() {
    let skp = SuiKeyPair::Ed25519(get_key_pair().1);
    let ser = serde_json::to_string(&skp.public()).unwrap();
    assert_eq!(
        ser,
        format!(
            "{{\"Ed25519\":\"{}\"}}",
            Base64::encode(skp.public().as_ref())
        )
    );
}

#[test]
fn serde_round_trip_authority_quorum_sign_info() {
    let info = AuthorityQuorumSignInfo::<true> {
        epoch: 0,
        signature: Default::default(),
        signers_map: RoaringBitmap::new(),
    };
    let ser = serde_json::to_string(&info).unwrap();
    println!("{}", ser);
    let schema = schemars::schema_for!(AuthorityQuorumSignInfo<true>);
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());

    let bytes = bcs::to_bytes(&info).unwrap();
    let info2: AuthorityQuorumSignInfo<true> = bcs::from_bytes(&bytes).unwrap();
    assert_eq!(info.signature.sig, info2.signature.sig);
}

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
fn test_proof_of_possession() {
    let address =
        SuiAddress::from_str("0x1a4623343cd42be47d67314fce0ad042f3c82685544bc91d8c11d24e74ba7357")
            .unwrap();
    let kp: AuthorityKeyPair = get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
    let pop = generate_proof_of_possession(&kp, address);
    let mut msg = vec![];
    msg.extend_from_slice(kp.public().as_bytes());
    msg.extend_from_slice(address.as_ref());
    println!("Address: {:?}", address);
    println!("Pubkey: {:?}", Hex::encode(kp.public().as_bytes()));
    println!("Proof of possession: {:?}", Hex::encode(&pop));
    assert!(verify_proof_of_possession(&pop, kp.public(), address).is_ok());

    // Result from: target/debug/sui validator serialize-payload-pop --account-address 0x1a4623343cd42be47d67314fce0ad042f3c82685544bc91d8c11d24e74ba7357 --protocol-public-key 99f25ef61f8032b914636460982c5cc6f134ef1ddae76657f2cbfec1ebfc8d097374080df6fcf0dcb8bc4b0d8e0af5d80ebbff2b4c599f54f42d6312dfc314276078c1cc347ebbbec5198be258513f386b930d02c2749a803e2330955ebd1a10
    let msg = Base64::decode("BQAAgAGZ8l72H4AyuRRjZGCYLFzG8TTvHdrnZlfyy/7B6/yNCXN0CA32/PDcuLxLDY4K9dgOu/8rTFmfVPQtYxLfwxQnYHjBzDR+u77FGYviWFE/OGuTDQLCdJqAPiMwlV69GhAaRiM0PNQr5H1nMU/OCtBC88gmhVRLyR2MEdJOdLpzVwAAAAAAAAAA").unwrap();
    let sig = kp.sign(&msg);
    assert!(verify_proof_of_possession(&sig, kp.public(), address).is_ok());
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
        let _skp: Result<SuiKeyPair, _> = bcs::from_bytes(&bytes);
        let _pk: Result<PublicKey, _> = bcs::from_bytes(&bytes);
    }


}

// Same seed as the pq-sigs-ts "digest-msg" interop vector.
const MLDSA65_TEST_SEED: [u8; 32] = [2; 32];
const MLDSA65_TEST_ADDRESS: &str =
    "0xa44576e02f83a9e1bddac6fd742a77931d1689d9a61122eb3125dee425f6dd36";

fn mldsa65_test_keypair() -> SuiKeyPair {
    SuiKeyPair::MLDSA65(MLDSA65KeyPair::from_bytes(&MLDSA65_TEST_SEED).unwrap())
}

#[test]
fn mldsa65_address_golden() {
    // Pins blake2b256(flag || pk) end to end.
    let kp = mldsa65_test_keypair();
    let address: SuiAddress = (&kp.public()).into();
    assert_eq!(address.to_string(), MLDSA65_TEST_ADDRESS);
}

#[test]
fn mldsa65_keystore_blob_roundtrip() {
    let kp = mldsa65_test_keypair();
    let blob = kp.to_bytes();
    // flag || 32-byte seed
    assert_eq!(blob.len(), 33);
    assert_eq!(blob[0], SignatureScheme::MLDSA65.flag());
    let decoded = SuiKeyPair::from_bytes(&blob).unwrap();
    assert_eq!(kp.public(), decoded.public());
    assert_eq!(blob, decoded.to_bytes());

    let encoded = kp.encode().unwrap();
    let decoded = SuiKeyPair::decode(&encoded).unwrap();
    assert_eq!(kp.public(), decoded.public());
}

#[test]
fn mldsa65_blob_wrong_lengths_rejected() {
    // The 4,032-byte expanded key must never be accepted as a keystore blob.
    for len in [0usize, 31, 33, 4032] {
        let mut blob = vec![SignatureScheme::MLDSA65.flag()];
        blob.extend(std::iter::repeat_n(0u8, len));
        assert!(
            SuiKeyPair::from_bytes(&blob).is_err(),
            "flag || {len} bytes must be rejected"
        );
    }
    assert!(SuiKeyPair::from_bytes(&[]).is_err());
    let mut blob = mldsa65_test_keypair().to_bytes();
    blob.push(0);
    assert!(SuiKeyPair::from_bytes(&blob).is_err());
}

#[test]
fn mldsa65_envelope_roundtrip() {
    let kp = mldsa65_test_keypair();
    // Signing is hedged, so only the roundtrip can be pinned.
    let sig = kp.sign(b"mldsa envelope roundtrip");
    assert_eq!(sig.scheme().flag(), SignatureScheme::MLDSA65.flag());
    // flag || sig(3,309) || pk(1,952)
    let bytes = sig.as_ref().to_vec();
    assert_eq!(bytes.len(), 5262);
    let parsed = Signature::from_bytes(&bytes).unwrap();
    assert_eq!(parsed, sig);
    assert_eq!(parsed.as_ref(), &bytes[..]);
}

#[test]
fn mldsa65_verify_secure_intent_message() {
    let kp = mldsa65_test_keypair();
    let address: SuiAddress = (&kp.public()).into();
    let msg = IntentMessage::new(Intent::sui_transaction(), Foo("mldsa65".to_string()));
    let sig = Signature::new_secure(&msg, &kp);
    assert!(
        sig.verify_secure(&msg, address, SignatureScheme::MLDSA65)
            .is_ok()
    );
    assert!(
        sig.verify_secure(&msg, SuiAddress::ZERO, SignatureScheme::MLDSA65)
            .is_err()
    );
}

#[test]
fn mldsa65_not_parsed_as_generic_signature() {
    // The transaction-level parser rejects 0x07 until verification is wired up.
    let kp = mldsa65_test_keypair();
    let sig = kp.sign(b"message");
    assert!(GenericSignature::from_bytes(sig.as_ref()).is_err());
}

#[test]
fn mldsa65_public_key_decode() {
    let kp = mldsa65_test_keypair();
    let pk = kp.public();
    let encoded = pk.encode_base64();
    let decoded = PublicKey::decode_base64(&encoded).unwrap();
    assert_eq!(pk, decoded);
    let mut bytes = vec![SignatureScheme::MLDSA65.flag()];
    bytes.extend_from_slice(&pk.as_ref()[..1951]);
    assert!(PublicKey::decode_base64(&Base64::encode(&bytes)).is_err());
    assert!(PublicKey::try_from_bytes(SignatureScheme::MLDSA65, &pk.as_ref()[..1951]).is_err());
}

#[test]
fn mldsa65_zklogin_ephemeral_rejected() {
    // verify_claims must reject an ML-DSA ephemeral key explicitly.
    use crate::signature::{AuthenticatorTrait, VerifyParams};
    use crate::signature_verification::VerifiedDigestCache;
    use crate::zk_login_authenticator::ZkLoginAuthenticator;
    use crate::zk_login_util::get_zklogin_inputs;
    use std::sync::Arc;

    let kp = mldsa65_test_keypair();
    let msg = IntentMessage::new(Intent::sui_transaction(), Foo("zklogin".to_string()));
    let eph_sig = Signature::new_secure(&msg, &kp);
    let inputs = get_zklogin_inputs();
    let author: SuiAddress = SuiAddress::try_from_unpadded(&inputs).unwrap();
    let authenticator = ZkLoginAuthenticator::new(inputs, 10, eph_sig);
    let generic = GenericSignature::ZkLoginAuthenticator(authenticator);

    let parsed = GenericSignature::from_bytes(generic.as_ref()).unwrap();
    let GenericSignature::ZkLoginAuthenticator(parsed_auth) = parsed else {
        panic!("expected zklogin authenticator");
    };
    let res = parsed_auth
        .verify_claims(
            &msg,
            author,
            &VerifyParams::default(),
            Arc::new(VerifiedDigestCache::new_empty()),
        )
        .map_err(|e| e.into_inner());
    assert!(
        matches!(res, Err(crate::error::SuiErrorKind::InvalidSignature { error })
            if error.contains("zkLogin ephemeral signature scheme not supported"))
    );
}

#[test]
fn mldsa65_bcs_variant_index_pinned() {
    // The BCS tag is consensus-critical (serialized inside MultiSig committees).
    let kp = mldsa65_test_keypair();
    assert_eq!(bcs::to_bytes(&kp.public()).unwrap()[0], 5);
}
