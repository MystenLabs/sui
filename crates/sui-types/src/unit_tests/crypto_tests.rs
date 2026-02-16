// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::crypto::bcs_signable_test::Foo;
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

// ===========================================================================
// SignatureScheme flag round-trip tests
// ===========================================================================

#[test]
fn signature_scheme_flag_round_trip() {
    let schemes = [
        (SignatureScheme::ED25519, 0x00),
        (SignatureScheme::Secp256k1, 0x01),
        (SignatureScheme::Secp256r1, 0x02),
        (SignatureScheme::MultiSig, 0x03),
        (SignatureScheme::BLS12381, 0x04),
        (SignatureScheme::ZkLoginAuthenticator, 0x05),
        (SignatureScheme::PasskeyAuthenticator, 0x06),
    ];
    for (scheme, expected_flag) in &schemes {
        assert_eq!(scheme.flag(), *expected_flag);
        let recovered = SignatureScheme::from_flag_byte(expected_flag).unwrap();
        assert_eq!(*scheme, recovered);
    }
}

#[test]
fn signature_scheme_from_flag_invalid() {
    assert!(SignatureScheme::from_flag_byte(&0x07).is_err());
    assert!(SignatureScheme::from_flag_byte(&0xFF).is_err());
    assert!(SignatureScheme::from_flag("999").is_err());
    assert!(SignatureScheme::from_flag("abc").is_err());
}

#[test]
fn signature_scheme_from_flag_string() {
    assert_eq!(
        SignatureScheme::from_flag("0").unwrap(),
        SignatureScheme::ED25519
    );
    assert_eq!(
        SignatureScheme::from_flag("1").unwrap(),
        SignatureScheme::Secp256k1
    );
    assert_eq!(
        SignatureScheme::from_flag("2").unwrap(),
        SignatureScheme::Secp256r1
    );
}

// ===========================================================================
// SuiKeyPair encode/decode tests for all schemes
// ===========================================================================

#[test]
fn keypair_encode_decode_secp256k1() {
    let kp = SuiKeyPair::Secp256k1(Secp256k1KeyPair::generate(&mut StdRng::from_seed([1; 32])));
    let encoded = kp.encode().unwrap();
    assert!(encoded.starts_with("suiprivkey"));
    let decoded = SuiKeyPair::decode(&encoded).unwrap();
    assert_eq!(kp, decoded);
}

#[test]
fn keypair_encode_decode_secp256r1() {
    let kp = SuiKeyPair::Secp256r1(Secp256r1KeyPair::generate(&mut StdRng::from_seed([2; 32])));
    let encoded = kp.encode().unwrap();
    assert!(encoded.starts_with("suiprivkey"));
    let decoded = SuiKeyPair::decode(&encoded).unwrap();
    assert_eq!(kp, decoded);
}

#[test]
fn keypair_from_bytes_empty_fails() {
    assert!(SuiKeyPair::from_bytes(&[]).is_err());
}

#[test]
fn keypair_from_bytes_invalid_flag_fails() {
    // Flag 0xFF is not a valid signature scheme
    let mut bytes = vec![0xFF];
    bytes.extend_from_slice(&[0u8; 64]);
    assert!(SuiKeyPair::from_bytes(&bytes).is_err());
}

#[test]
fn keypair_to_bytes_from_bytes_round_trip_all_schemes() {
    let seeds: [(u8, fn(StdRng) -> SuiKeyPair); 3] = [
        (0, |mut rng| {
            SuiKeyPair::Ed25519(Ed25519KeyPair::generate(&mut rng))
        }),
        (1, |mut rng| {
            SuiKeyPair::Secp256k1(Secp256k1KeyPair::generate(&mut rng))
        }),
        (2, |mut rng| {
            SuiKeyPair::Secp256r1(Secp256r1KeyPair::generate(&mut rng))
        }),
    ];
    for (seed, make_kp) in &seeds {
        let kp = make_kp(StdRng::from_seed([*seed; 32]));
        let bytes = kp.to_bytes();
        let recovered = SuiKeyPair::from_bytes(&bytes).unwrap();
        assert_eq!(kp, recovered);
    }
}

#[test]
fn keypair_base64_round_trip() {
    let kp = SuiKeyPair::Ed25519(Ed25519KeyPair::generate(&mut StdRng::from_seed([5; 32])));
    let b64 = kp.encode_base64();
    let recovered = SuiKeyPair::decode_base64(&b64).unwrap();
    assert_eq!(kp, recovered);
}

#[test]
fn keypair_copy_preserves_equality() {
    let kp = SuiKeyPair::Ed25519(Ed25519KeyPair::generate(&mut StdRng::from_seed([3; 32])));
    let kp_copy = kp.copy();
    assert_eq!(kp, kp_copy);
    assert_eq!(kp.public(), kp_copy.public());
}

#[test]
fn keypair_serde_json_round_trip() {
    let kp = SuiKeyPair::Ed25519(Ed25519KeyPair::generate(&mut StdRng::from_seed([4; 32])));
    let json = serde_json::to_string(&kp).unwrap();
    let recovered: SuiKeyPair = serde_json::from_str(&json).unwrap();
    assert_eq!(kp, recovered);
}

// ===========================================================================
// PublicKey tests
// ===========================================================================

#[test]
fn public_key_flag_matches_scheme() {
    let ed_kp = SuiKeyPair::Ed25519(get_key_pair().1);
    let k1_kp = SuiKeyPair::Secp256k1(get_key_pair().1);
    let r1_kp = SuiKeyPair::Secp256r1(get_key_pair().1);

    assert_eq!(ed_kp.public().flag(), SignatureScheme::ED25519.flag());
    assert_eq!(k1_kp.public().flag(), SignatureScheme::Secp256k1.flag());
    assert_eq!(r1_kp.public().flag(), SignatureScheme::Secp256r1.flag());

    assert_eq!(ed_kp.public().scheme(), SignatureScheme::ED25519);
    assert_eq!(k1_kp.public().scheme(), SignatureScheme::Secp256k1);
    assert_eq!(r1_kp.public().scheme(), SignatureScheme::Secp256r1);
}

#[test]
fn public_key_base64_encode_decode_round_trip() {
    let kp = SuiKeyPair::Ed25519(get_key_pair().1);
    let pk = kp.public();
    let encoded = pk.encode_base64();
    let decoded = PublicKey::decode_base64(&encoded).unwrap();
    assert_eq!(pk, decoded);
}

#[test]
fn public_key_base64_decode_invalid_flag() {
    // Construct a base64 string with an invalid flag byte (0xFF)
    let mut bytes = vec![0xFF];
    bytes.extend_from_slice(&[0u8; 32]);
    let encoded = Base64::encode(&bytes);
    assert!(PublicKey::decode_base64(&encoded).is_err());
}

#[test]
fn public_key_base64_decode_empty() {
    let encoded = Base64::encode(&[]);
    assert!(PublicKey::decode_base64(&encoded).is_err());
}

#[test]
fn public_key_try_from_bytes_unsupported_scheme() {
    assert!(PublicKey::try_from_bytes(SignatureScheme::MultiSig, &[0u8; 32]).is_err());
    assert!(PublicKey::try_from_bytes(SignatureScheme::ZkLoginAuthenticator, &[0u8; 32]).is_err());
    assert!(PublicKey::try_from_bytes(SignatureScheme::BLS12381, &[0u8; 32]).is_err());
}

#[test]
fn public_key_try_from_bytes_passkey() {
    // Passkey uses secp256r1 public key internally
    let kp = Secp256r1KeyPair::generate(&mut StdRng::from_seed([7; 32]));
    let pk_bytes = kp.public().as_bytes().to_vec();
    let pk = PublicKey::try_from_bytes(SignatureScheme::PasskeyAuthenticator, &pk_bytes).unwrap();
    assert_eq!(pk.scheme(), SignatureScheme::PasskeyAuthenticator);
}

#[test]
fn public_key_from_str_round_trip() {
    let kp = SuiKeyPair::Secp256k1(get_key_pair().1);
    let pk = kp.public();
    let s = pk.encode_base64();
    let recovered = PublicKey::from_str(&s).unwrap();
    assert_eq!(pk, recovered);
}

// ===========================================================================
// ZkLoginPublicIdentifier tests
// ===========================================================================

#[test]
fn zklogin_public_identifier_new_and_validate() {
    let iss = "https://accounts.google.com";
    let address_seed =
        Bn254FrElement::from_str("1234567890123456789012345678901234567890").unwrap();
    let zk_id = ZkLoginPublicIdentifier::new(iss, &address_seed).unwrap();
    assert!(zk_id.validate().is_ok());

    // First byte is iss length
    assert_eq!(zk_id.0[0] as usize, iss.len());
    // Next bytes are iss
    assert_eq!(&zk_id.0[1..1 + iss.len()], iss.as_bytes());
}

#[test]
fn zklogin_public_identifier_validate_empty_bytes() {
    let zk_id = ZkLoginPublicIdentifier(vec![]);
    assert!(zk_id.validate().is_err());
}

#[test]
fn zklogin_public_identifier_validate_truncated_iss() {
    // iss_len says 50 bytes but we only provide 5
    let mut bytes = vec![50u8];
    bytes.extend_from_slice(b"short");
    let zk_id = ZkLoginPublicIdentifier(bytes);
    assert!(zk_id.validate().is_err());
}

#[test]
fn zklogin_public_identifier_validate_invalid_utf8_iss() {
    // iss_len = 4, then 4 bytes of invalid UTF-8
    let bytes = vec![4, 0xFF, 0xFE, 0xFD, 0xFC, 0, 0, 0, 0];
    let zk_id = ZkLoginPublicIdentifier(bytes);
    assert!(zk_id.validate().is_err());
}

#[test]
fn zklogin_public_identifier_validate_oversized_address_seed() {
    // Valid iss but address seed > 32 bytes
    let iss = "test";
    let mut bytes = vec![iss.len() as u8];
    bytes.extend_from_slice(iss.as_bytes());
    bytes.extend_from_slice(&[0u8; 33]); // 33 bytes > 32
    let zk_id = ZkLoginPublicIdentifier(bytes);
    assert!(zk_id.validate().is_err());
}

#[test]
fn zklogin_public_identifier_validate_no_address_seed() {
    // Valid iss of length 3, but no address seed bytes at all (still valid: empty seed <= 32)
    let iss = "abc";
    let mut bytes = vec![iss.len() as u8];
    bytes.extend_from_slice(iss.as_bytes());
    let zk_id = ZkLoginPublicIdentifier(bytes);
    // Empty address seed has length 0 which is <= 32, should be OK
    assert!(zk_id.validate().is_ok());
}

// ===========================================================================
// ZkLoginPublicIdentifier accessor tests
// ===========================================================================

#[test]
fn zklogin_public_identifier_iss_accessor() {
    let iss = "https://accounts.google.com";
    let address_seed =
        Bn254FrElement::from_str("1234567890123456789012345678901234567890").unwrap();
    let zk_id = ZkLoginPublicIdentifier::new(iss, &address_seed).unwrap();
    assert_eq!(zk_id.iss().unwrap(), iss);
}

#[test]
fn zklogin_public_identifier_address_seed_bytes_accessor() {
    let iss = "https://accounts.google.com";
    let address_seed =
        Bn254FrElement::from_str("1234567890123456789012345678901234567890").unwrap();
    let zk_id = ZkLoginPublicIdentifier::new(iss, &address_seed).unwrap();
    let seed_bytes = zk_id.address_seed_bytes().unwrap();
    assert_eq!(seed_bytes.len(), 32); // padded to 32 bytes
    assert_eq!(seed_bytes, &address_seed.padded());
}

#[test]
fn zklogin_public_identifier_iss_on_empty_fails() {
    let zk_id = ZkLoginPublicIdentifier(vec![]);
    assert!(zk_id.iss().is_err());
}

// ===========================================================================
// SuiKeyPair::scheme() tests
// ===========================================================================

#[test]
fn keypair_scheme() {
    let ed = SuiKeyPair::Ed25519(Ed25519KeyPair::generate(&mut StdRng::from_seed([0; 32])));
    let k1 = SuiKeyPair::Secp256k1(Secp256k1KeyPair::generate(&mut StdRng::from_seed([1; 32])));
    let r1 = SuiKeyPair::Secp256r1(Secp256r1KeyPair::generate(&mut StdRng::from_seed([2; 32])));

    assert_eq!(ed.scheme(), SignatureScheme::ED25519);
    assert_eq!(k1.scheme(), SignatureScheme::Secp256k1);
    assert_eq!(r1.scheme(), SignatureScheme::Secp256r1);

    // scheme should match public key's scheme
    assert_eq!(ed.scheme(), ed.public().scheme());
    assert_eq!(k1.scheme(), k1.public().scheme());
    assert_eq!(r1.scheme(), r1.public().scheme());
}

// ===========================================================================
// Signature creation and verification tests
// ===========================================================================

#[test]
fn ed25519_sign_and_verify_secure() {
    let (addr, kp): (SuiAddress, AccountKeyPair) = get_key_pair();
    let sui_kp = SuiKeyPair::Ed25519(kp);
    let msg = b"test message for signing";
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), msg.to_vec());
    let sig = Signature::new_secure(&intent_msg, &sui_kp);
    assert!(sig
        .verify_secure(&intent_msg, addr, SignatureScheme::ED25519)
        .is_ok());
}

#[test]
fn secp256k1_sign_and_verify_secure() {
    let (addr, kp): (SuiAddress, Secp256k1KeyPair) = get_key_pair();
    let sui_kp = SuiKeyPair::Secp256k1(kp);
    let msg = b"secp256k1 test message";
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), msg.to_vec());
    let sig = Signature::new_secure(&intent_msg, &sui_kp);
    assert!(sig
        .verify_secure(&intent_msg, addr, SignatureScheme::Secp256k1)
        .is_ok());
}

#[test]
fn secp256r1_sign_and_verify_secure() {
    let (addr, kp): (SuiAddress, Secp256r1KeyPair) = get_key_pair();
    let sui_kp = SuiKeyPair::Secp256r1(kp);
    let msg = b"secp256r1 test message";
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), msg.to_vec());
    let sig = Signature::new_secure(&intent_msg, &sui_kp);
    assert!(sig
        .verify_secure(&intent_msg, addr, SignatureScheme::Secp256r1)
        .is_ok());
}

#[test]
fn verify_secure_wrong_address_fails() {
    let (_addr, kp): (SuiAddress, AccountKeyPair) = get_key_pair();
    let sui_kp = SuiKeyPair::Ed25519(kp);
    let (wrong_addr, _): (SuiAddress, AccountKeyPair) = get_key_pair();
    let msg = b"wrong address test";
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), msg.to_vec());
    let sig = Signature::new_secure(&intent_msg, &sui_kp);
    assert!(sig
        .verify_secure(&intent_msg, wrong_addr, SignatureScheme::ED25519)
        .is_err());
}

#[test]
fn verify_secure_wrong_message_fails() {
    let (addr, kp): (SuiAddress, AccountKeyPair) = get_key_pair();
    let sui_kp = SuiKeyPair::Ed25519(kp);
    let msg1 = b"original message";
    let msg2 = b"tampered message";
    let intent_msg1 = IntentMessage::new(Intent::sui_transaction(), msg1.to_vec());
    let intent_msg2 = IntentMessage::new(Intent::sui_transaction(), msg2.to_vec());
    let sig = Signature::new_secure(&intent_msg1, &sui_kp);
    assert!(sig
        .verify_secure(&intent_msg2, addr, SignatureScheme::ED25519)
        .is_err());
}

// ===========================================================================
// Signature parsing tests
// ===========================================================================

#[test]
fn signature_from_bytes_empty_fails() {
    assert!(Signature::from_bytes(&[]).is_err());
}

#[test]
fn signature_from_bytes_invalid_flag_fails() {
    let mut bytes = vec![0xFF];
    bytes.extend_from_slice(&[0u8; 100]);
    assert!(Signature::from_bytes(&bytes).is_err());
}

#[test]
fn signature_from_str_round_trip() {
    let (_, kp): (SuiAddress, AccountKeyPair) = get_key_pair();
    let sui_kp = SuiKeyPair::Ed25519(kp);
    let sig = Signer::sign(&sui_kp, b"hello");
    let b64 = Base64::encode(sig.as_ref());
    let recovered = Signature::from_str(&b64).unwrap();
    assert_eq!(sig, recovered);
}

#[test]
fn signature_serde_json_round_trip() {
    let (_, kp): (SuiAddress, AccountKeyPair) = get_key_pair();
    let sui_kp = SuiKeyPair::Ed25519(kp);
    let sig: Signature = Signer::sign(&sui_kp, b"test serde");
    let json = serde_json::to_string(&sig).unwrap();
    let recovered: Signature = serde_json::from_str(&json).unwrap();
    assert_eq!(sig, recovered);
}

#[test]
fn signature_bcs_round_trip() {
    let (_, kp): (SuiAddress, AccountKeyPair) = get_key_pair();
    let sui_kp = SuiKeyPair::Ed25519(kp);
    let sig: Signature = Signer::sign(&sui_kp, b"test bcs");
    let bytes = bcs::to_bytes(&sig).unwrap();
    let recovered: Signature = bcs::from_bytes(&bytes).unwrap();
    assert_eq!(sig, recovered);
}

// ===========================================================================
// SuiSignature trait tests (signature_bytes / public_key_bytes / scheme)
// ===========================================================================

#[test]
fn sui_signature_inner_components_ed25519() {
    let (_, kp): (SuiAddress, AccountKeyPair) = get_key_pair();
    let sui_kp = SuiKeyPair::Ed25519(kp);
    let sig: Signature = Signer::sign(&sui_kp, b"component test");
    assert_eq!(sig.scheme(), SignatureScheme::ED25519);
    // signature_bytes length should be Ed25519Signature::LENGTH
    assert_eq!(sig.signature_bytes().len(), Ed25519Signature::LENGTH);
    // public_key_bytes length should be Ed25519PublicKey::LENGTH
    assert_eq!(sig.public_key_bytes().len(), Ed25519PublicKey::LENGTH);
    // public key extracted from sig should match original
    assert_eq!(sig.public_key_bytes(), sui_kp.public().as_ref());
}

#[test]
fn sui_signature_inner_components_secp256k1() {
    let (_, kp): (SuiAddress, Secp256k1KeyPair) = get_key_pair();
    let sui_kp = SuiKeyPair::Secp256k1(kp);
    let sig: Signature = Signer::sign(&sui_kp, b"k1 component test");
    assert_eq!(sig.scheme(), SignatureScheme::Secp256k1);
    assert_eq!(sig.signature_bytes().len(), Secp256k1Signature::LENGTH);
    assert_eq!(sig.public_key_bytes().len(), Secp256k1PublicKey::LENGTH);
    assert_eq!(sig.public_key_bytes(), sui_kp.public().as_ref());
}

// ===========================================================================
// RandomnessRound tests
// ===========================================================================

#[test]
fn randomness_round_arithmetic() {
    let r1 = RandomnessRound(10);
    let r2 = RandomnessRound(3);
    assert_eq!(r1 + r2, RandomnessRound(13));
    assert_eq!(r1 - r2, RandomnessRound(7));
    assert_eq!(r1 + 5u64, RandomnessRound(15));
    assert_eq!(r1 - 5u64, RandomnessRound(5));
}

#[test]
fn randomness_round_checked_sub() {
    let r = RandomnessRound(10);
    assert_eq!(r.checked_sub(5), Some(RandomnessRound(5)));
    assert_eq!(r.checked_sub(10), Some(RandomnessRound(0)));
    assert_eq!(r.checked_sub(11), None);

    let r0 = RandomnessRound(0);
    assert_eq!(r0.checked_sub(0), Some(RandomnessRound(0)));
    assert_eq!(r0.checked_sub(1), None);
}

#[test]
fn randomness_round_is_zero() {
    assert!(RandomnessRound(0).is_zero());
    assert!(!RandomnessRound(1).is_zero());
    assert!(!RandomnessRound(u64::MAX).is_zero());
}

#[test]
fn randomness_round_checked_add() {
    let r = RandomnessRound(u64::MAX - 1);
    assert_eq!(r.checked_add(1), Some(RandomnessRound(u64::MAX)));
    assert_eq!(r.checked_add(2), None);

    let r0 = RandomnessRound(0);
    assert_eq!(r0.checked_add(0), Some(RandomnessRound(0)));
    assert_eq!(r0.checked_add(u64::MAX), Some(RandomnessRound(u64::MAX)));
}

#[test]
fn randomness_round_display() {
    let r = RandomnessRound(42);
    assert_eq!(format!("{}", r), "42");
}

#[test]
fn randomness_round_signature_message_deterministic() {
    let r1 = RandomnessRound(100);
    let r2 = RandomnessRound(100);
    assert_eq!(r1.signature_message(), r2.signature_message());

    let r3 = RandomnessRound(101);
    assert_ne!(r1.signature_message(), r3.signature_message());
}

#[test]
fn randomness_round_signature_message_prefix() {
    let r = RandomnessRound(0);
    let msg = r.signature_message();
    assert!(msg.starts_with(b"random_beacon round "));
}

#[test]
fn randomness_round_ordering() {
    let r1 = RandomnessRound(1);
    let r2 = RandomnessRound(2);
    assert!(r1 < r2);
    assert!(r2 > r1);
    assert_eq!(r1, RandomnessRound(1));
}

#[test]
fn randomness_round_serde_round_trip() {
    let r = RandomnessRound(12345);
    let bytes = bcs::to_bytes(&r).unwrap();
    let recovered: RandomnessRound = bcs::from_bytes(&bytes).unwrap();
    assert_eq!(r, recovered);
}

// ===========================================================================
// AuthorityPublicKeyBytes tests
// ===========================================================================

#[test]
fn authority_public_key_bytes_from_bytes_wrong_length() {
    assert!(AuthorityPublicKeyBytes::from_bytes(&[0u8; 10]).is_err());
    assert!(AuthorityPublicKeyBytes::from_bytes(&[]).is_err());
}

#[test]
fn authority_public_key_bytes_display_and_debug() {
    let bytes = [0u8; AuthorityPublicKey::LENGTH];
    let apkb = AuthorityPublicKeyBytes(bytes);
    let display = format!("{}", apkb);
    let debug = format!("{:?}", apkb);
    assert!(!display.is_empty());
    assert!(!debug.is_empty());
}

#[test]
fn authority_public_key_bytes_from_str() {
    let kp: AuthorityKeyPair = get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
    let apkb = AuthorityPublicKeyBytes::from(kp.public());
    let hex_str = Hex::encode(apkb.0);
    let recovered = AuthorityPublicKeyBytes::from_str(&hex_str).unwrap();
    assert_eq!(apkb, recovered);
}

#[test]
fn authority_public_key_bytes_default_is_zero() {
    let default = AuthorityPublicKeyBytes::default();
    assert_eq!(default.0, [0u8; AuthorityPublicKey::LENGTH]);
}

// ===========================================================================
// Ed25519SuiSignature default test
// ===========================================================================

#[test]
fn ed25519_sui_signature_default_is_zero() {
    let default = Ed25519SuiSignature::default();
    let expected_len = Ed25519PublicKey::LENGTH + Ed25519Signature::LENGTH + 1;
    assert_eq!(default.as_ref().len(), expected_len);
    assert!(default.as_ref().iter().all(|&b| b == 0));
}

// ===========================================================================
// get_key_pair_from_bytes validation tests
// ===========================================================================

#[test]
fn get_key_pair_from_bytes_wrong_length_fails() {
    let result = get_key_pair_from_bytes::<AccountKeyPair>(&[0u8; 10]);
    assert!(result.is_err());
}

#[test]
fn deterministic_random_account_key_is_deterministic() {
    let (addr1, _) = deterministic_random_account_key();
    let (addr2, _) = deterministic_random_account_key();
    assert_eq!(addr1, addr2);
}

// ===========================================================================
// CompressedSignature AsRef tests
// ===========================================================================

#[test]
fn compressed_signature_as_ref() {
    let sig_bytes = Ed25519SignatureAsBytes([0u8; Ed25519Signature::LENGTH]);
    let compressed = CompressedSignature::Ed25519(sig_bytes);
    assert_eq!(compressed.as_ref().len(), Ed25519Signature::LENGTH);
}

// ===========================================================================
// Proof of possession with wrong address fails
// ===========================================================================

#[test]
fn proof_of_possession_wrong_address_fails() {
    let address1 =
        SuiAddress::from_str("0x1a4623343cd42be47d67314fce0ad042f3c82685544bc91d8c11d24e74ba7357")
            .unwrap();
    let address2 =
        SuiAddress::from_str("0x0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();
    let kp: AuthorityKeyPair = get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
    let pop = generate_proof_of_possession(&kp, address1);
    // Verifying with a different address should fail
    assert!(verify_proof_of_possession(&pop, kp.public(), address2).is_err());
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
