// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use fastcrypto::traits::ToFromBytes;
use once_cell::sync::OnceCell;
use rand::{rngs::StdRng, SeedableRng};
use roaring::RoaringBitmap;

use crate::{
    base_types::SuiAddress,
    crypto::{
        get_key_pair, get_key_pair_from_rng, Ed25519SuiSignature, Signature, SuiKeyPair,
        SuiSignatureInner,
    },
    intent::{Intent, IntentMessage, PersonalMessage},
    multisig::{AuthenticatorTrait, GenericSignature},
};

use super::{MultiPublicKey, MultiSignature, ThresholdUnit, WeightUnit};

#[test]
fn multisig_scenarios() {
    let kp1: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair().1);
    let kp2: SuiKeyPair = SuiKeyPair::Secp256k1(get_key_pair().1);
    let kp3: SuiKeyPair = SuiKeyPair::Secp256r1(get_key_pair().1);

    let pk1 = kp1.public();
    let pk2 = kp2.public();
    let pk3 = kp3.public();

    let multi_pk = MultiPublicKey::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone()],
        vec![1, 1, 1],
        2,
    )
    .unwrap();
    let addr = SuiAddress::from(multi_pk.clone());
    let msg = IntentMessage::new(
        Intent::default(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig1 = Signature::new_secure(&msg, &kp1);
    let sig2 = Signature::new_secure(&msg, &kp2);
    let sig3 = Signature::new_secure(&msg, &kp3);

    // Any 2 of 3 signatures verifies ok.
    let multisig1 =
        MultiSignature::combine(vec![sig1.clone(), sig2.clone()], multi_pk.clone()).unwrap();
    assert!(multisig1.verify_secure_generic(&msg, addr).is_ok());

    let multisig2 =
        MultiSignature::combine(vec![sig1.clone(), sig3.clone()], multi_pk.clone()).unwrap();
    assert!(multisig2.verify_secure_generic(&msg, addr).is_ok());

    let multisig3 =
        MultiSignature::combine(vec![sig2.clone(), sig3.clone()], multi_pk.clone()).unwrap();
    assert!(multisig3.verify_secure_generic(&msg, addr).is_ok());

    // 1 of 3 signature verify fails.
    let multisig4 = MultiSignature::combine(vec![sig2.clone()], multi_pk).unwrap();
    assert!(multisig4.verify_secure_generic(&msg, addr).is_err());

    // Incorrect address fails.
    let kp4: SuiKeyPair = SuiKeyPair::Secp256r1(get_key_pair().1);
    let pk4 = kp4.public();
    let multi_pk_1 = MultiPublicKey::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone(), pk4],
        vec![1, 1, 1, 1],
        1,
    )
    .unwrap();
    let multisig5 = MultiSignature::combine(vec![sig1.clone(), sig2.clone()], multi_pk_1).unwrap();
    assert!(multisig5.verify_secure_generic(&msg, addr).is_err());

    // Create a multi-pubkey of pk1: 1, pk2: 2, pk3: 3, threshold 3.
    let multi_pk_2 = MultiPublicKey::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone()],
        vec![1, 2, 3],
        3,
    )
    .unwrap();
    let addr_2 = SuiAddress::from(multi_pk_2.clone());

    // sig1 and sig2 (3 of 6) verifies ok.
    let multi_sig_6 =
        MultiSignature::combine(vec![sig1.clone(), sig2.clone()], multi_pk_2.clone()).unwrap();
    assert!(multi_sig_6.verify_secure_generic(&msg, addr_2).is_ok());

    // Change position for sig2 and sig1 fails.
    let multi_sig_7 =
        MultiSignature::combine(vec![sig2.clone(), sig1.clone()], multi_pk_2.clone()).unwrap();
    assert!(multi_sig_7.verify_secure_generic(&msg, addr_2).is_err());

    // sig3 itself (3 of 6) verifies ok.
    let multi_sig_8 = MultiSignature::combine(vec![sig3], multi_pk_2.clone()).unwrap();
    assert!(multi_sig_8.verify_secure_generic(&msg, addr_2).is_ok());

    // sig2 itself (2 of 6) verifies fail.
    let multi_sig_9 = MultiSignature::combine(vec![sig2], multi_pk_2).unwrap();
    assert!(multi_sig_9.verify_secure_generic(&msg, addr_2).is_err());

    // Wrong bitmap verifies fail.
    let mut bitmap = RoaringBitmap::new();
    bitmap.insert(1);
    let multi_sig_10 = MultiSignature {
        sigs: vec![sig1.to_compressed().unwrap()], // sig1 has index 0
        bitmap,
        multi_pk: MultiPublicKey::new(vec![pk1, pk2, pk3], vec![1, 2, 3], 3).unwrap(),
        bytes: OnceCell::new(),
    };
    assert!(multi_sig_10.verify_secure_generic(&msg, addr_2).is_err());
}

#[test]
fn test_combine_sigs() {
    let kp1: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair().1);
    let kp2: SuiKeyPair = SuiKeyPair::Secp256k1(get_key_pair().1);
    let kp3: SuiKeyPair = SuiKeyPair::Secp256r1(get_key_pair().1);

    let pk1 = kp1.public();
    let pk2 = kp2.public();

    let multi_pk = MultiPublicKey::new(vec![pk1, pk2], vec![1, 1], 2).unwrap();

    let msg = IntentMessage::new(
        Intent::default(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig1 = Signature::new_secure(&msg, &kp1);
    let sig2 = Signature::new_secure(&msg, &kp2);
    let sig3 = Signature::new_secure(&msg, &kp3);

    // MultiPublicKey contains only 2 public key but 3 signatures are passed, fails to combine.
    assert!(MultiSignature::combine(vec![sig1, sig2, sig3], multi_pk.clone()).is_err());

    // Cannot create malformed MultiPublicKey.
    assert!(MultiSignature::combine(vec![], multi_pk).is_err());
    // Cannot create malformed MultiSignature.
}
#[test]
fn test_serde_roundtrip() {
    let msg = IntentMessage::new(
        Intent::default(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );

    for kp in vec![
        SuiKeyPair::Secp256k1(get_key_pair().1),
        SuiKeyPair::Secp256r1(get_key_pair().1),
        SuiKeyPair::Ed25519(get_key_pair().1),
    ] {
        let pk = kp.public();
        let multi_pk = MultiPublicKey::new(vec![pk], vec![1], 1).unwrap();
        let sig = Signature::new_secure(&msg, &kp);
        let multisig = MultiSignature::combine(vec![sig], multi_pk).unwrap();
        let plain_bytes = bcs::to_bytes(&multisig).unwrap();

        let generic_sig = GenericSignature::MultiSignature(multisig);
        let generic_sig_bytes = generic_sig.as_bytes();
        let generic_sig_roundtrip = GenericSignature::from_bytes(generic_sig_bytes).unwrap();
        assert_eq!(generic_sig, generic_sig_roundtrip);

        // A multisig flag 0x03 is appended before the bcs serialized bytes.
        assert_eq!(plain_bytes.len() + 1, generic_sig_bytes.len());
        assert_eq!(generic_sig_bytes.first().unwrap(), &0x03);
    }

    // Single sig serialization unchanged.
    let sig = Ed25519SuiSignature::from_bytes(&[0; Ed25519SuiSignature::LENGTH]).unwrap();
    let single_sig = GenericSignature::Signature(sig.clone().into());
    let single_sig_bytes = single_sig.as_bytes();
    let single_sig_roundtrip = GenericSignature::from_bytes(single_sig_bytes).unwrap();
    assert_eq!(single_sig, single_sig_roundtrip);
    assert_eq!(single_sig_bytes.len(), Ed25519SuiSignature::LENGTH);
    assert_eq!(
        single_sig_bytes.first().unwrap(),
        &Ed25519SuiSignature::SCHEME.flag()
    );
    assert_eq!(sig.as_bytes().len(), single_sig_bytes.len());
}

#[test]
fn single_sig_port_works() {
    let kp: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair().1);
    let addr = SuiAddress::from(&kp.public());
    let msg = IntentMessage::new(
        Intent::default(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig = Signature::new_secure(&msg, &kp);
    assert!(sig.verify_secure_generic(&msg, addr).is_ok());
}

#[test]
fn test_multisig_address() {
    // Pin an hardcoded multisig address generation here. If this fails, the address
    // generation logic may have changed. If this is intended, update the hardcoded value below.
    let mut seed = StdRng::from_seed([0; 32]);
    let kp1: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair_from_rng(&mut seed).1);
    let kp2: SuiKeyPair = SuiKeyPair::Secp256k1(get_key_pair_from_rng(&mut seed).1);
    let kp3: SuiKeyPair = SuiKeyPair::Secp256r1(get_key_pair_from_rng(&mut seed).1);

    let pk1 = kp1.public();
    let pk2 = kp2.public();
    let pk3 = kp3.public();

    // let mut bytes = Vec::new();
    let threshold: ThresholdUnit = 2;
    let w1: WeightUnit = 1;
    let w2: WeightUnit = 2;
    let w3: WeightUnit = 3;

    let multi_pk = MultiPublicKey::new(vec![pk1, pk2, pk3], vec![w1, w2, w3], threshold).unwrap();
    let address: SuiAddress = multi_pk.into();
    assert_eq!(
        SuiAddress::from_str("0x0efe135bce10b338b81be899842302e54f6c3043").unwrap(),
        address
    );
}
