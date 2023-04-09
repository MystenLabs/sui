// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use fastcrypto::{traits::{ToFromBytes, EncodeDecodeBase64}, encoding::{Base64, Encoding}, ed25519::Ed25519Signature};
use once_cell::sync::OnceCell;
use rand::{rngs::StdRng, SeedableRng};
use roaring::RoaringBitmap;

use super::{MultiSigPublicKey, ThresholdUnit, WeightUnit};
use crate::{
    base_types::SuiAddress,
    crypto::{
        get_key_pair, get_key_pair_from_rng, Ed25519SuiSignature, Signature, SuiKeyPair,
        SuiSignatureInner, PublicKey, SuiSignature,
    },
    multisig::{MultiSig, MAX_SIGNER_IN_MULTISIG},
    signature::{AuthenticatorTrait, GenericSignature}, messages::TransactionData,
};
use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};

pub fn keys() -> Vec<SuiKeyPair> {
    let mut seed = StdRng::from_seed([0; 32]);
    let kp1: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair_from_rng(&mut seed).1);
    let kp2: SuiKeyPair = SuiKeyPair::Secp256k1(get_key_pair_from_rng(&mut seed).1);
    let kp3: SuiKeyPair = SuiKeyPair::Secp256r1(get_key_pair_from_rng(&mut seed).1);
    vec![kp1, kp2, kp3]
}

#[test]
fn multisig_scenarios() {
    let keys = keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();
    let pk3 = keys[2].public();

    let multisig_pk = MultiSigPublicKey::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone()],
        vec![1, 1, 1],
        2,
    )
    .unwrap();
    let addr = SuiAddress::from(multisig_pk.clone());
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig1 = Signature::new_secure(&msg, &keys[0]);
    let sig2 = Signature::new_secure(&msg, &keys[1]);
    let sig3 = Signature::new_secure(&msg, &keys[2]);

    // Any 2 of 3 signatures verifies ok.
    let multi_sig1 =
        MultiSig::combine(vec![sig1.clone(), sig2.clone()], multisig_pk.clone()).unwrap();
    assert!(multi_sig1.verify_secure_generic(&msg, addr).is_ok());

    let multi_sig2 =
        MultiSig::combine(vec![sig1.clone(), sig3.clone()], multisig_pk.clone()).unwrap();
    assert!(multi_sig2.verify_secure_generic(&msg, addr).is_ok());

    let multi_sig3 =
        MultiSig::combine(vec![sig2.clone(), sig3.clone()], multisig_pk.clone()).unwrap();
    assert!(multi_sig3.verify_secure_generic(&msg, addr).is_ok());

    // 1 of 3 signature verify fails.
    let multi_sig4 = MultiSig::combine(vec![sig2.clone()], multisig_pk).unwrap();
    assert!(multi_sig4.verify_secure_generic(&msg, addr).is_err());

    // Incorrect address fails.
    let kp4: SuiKeyPair = SuiKeyPair::Secp256r1(get_key_pair().1);
    let pk4 = kp4.public();
    let multisig_pk_1 = MultiSigPublicKey::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone(), pk4],
        vec![1, 1, 1, 1],
        1,
    )
    .unwrap();
    let multisig5 = MultiSig::combine(vec![sig1.clone(), sig2.clone()], multisig_pk_1).unwrap();
    assert!(multisig5.verify_secure_generic(&msg, addr).is_err());

    // Create a MultiSig pubkey of pk1 (weight = 1), pk2 (weight = 2), pk3 (weight = 3), threshold 3.
    let multisig_pk_2 = MultiSigPublicKey::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone()],
        vec![1, 2, 3],
        3,
    )
    .unwrap();
    let addr_2 = SuiAddress::from(multisig_pk_2.clone());

    // sig1 and sig2 (3 of 6) verifies ok.
    let multi_sig_6 =
        MultiSig::combine(vec![sig1.clone(), sig2.clone()], multisig_pk_2.clone()).unwrap();
    assert!(multi_sig_6.verify_secure_generic(&msg, addr_2).is_ok());

    // providing the same sig twice fails.
    let multi_sig_6 =
        MultiSig::combine(vec![sig1.clone(), sig1.clone()], multisig_pk_2.clone()).unwrap();
    assert!(multi_sig_6.verify_secure_generic(&msg, addr_2).is_err());

    // Change position for sig2 and sig1 fails.
    let multi_sig_7 =
        MultiSig::combine(vec![sig2.clone(), sig1.clone()], multisig_pk_2.clone()).unwrap();
    assert!(multi_sig_7.verify_secure_generic(&msg, addr_2).is_err());

    // sig3 itself (3 of 6) verifies ok.
    let multi_sig_8 = MultiSig::combine(vec![sig3.clone()], multisig_pk_2.clone()).unwrap();
    assert!(multi_sig_8.verify_secure_generic(&msg, addr_2).is_ok());

    // sig2 itself (2 of 6) verifies fail.
    let multi_sig_9 = MultiSig::combine(vec![sig2.clone()], multisig_pk_2.clone()).unwrap();
    assert!(multi_sig_9.verify_secure_generic(&msg, addr_2).is_err());

    // A bad sig in the multisig fails, even though sig2 and sig3 verifies and weights meets threshold.
    let bad_sig = Signature::new_secure(
        &IntentMessage::new(
            Intent::sui_transaction(),
            PersonalMessage {
                message: "Bad message".as_bytes().to_vec(),
            },
        ),
        &keys[0],
    );
    let multi_sig_9 = MultiSig::combine(vec![bad_sig, sig2, sig3], multisig_pk_2).unwrap();
    assert!(multi_sig_9.verify_secure_generic(&msg, addr_2).is_err());

    // Wrong bitmap verifies fail.
    let mut bitmap = RoaringBitmap::new();
    bitmap.insert(1);
    let multi_sig_10 = MultiSig {
        sigs: vec![sig1.to_compressed().unwrap()], // sig1 has index 0
        bitmap,
        multisig_pk: MultiSigPublicKey::new(vec![pk1, pk2, pk3], vec![1, 2, 3], 3).unwrap(),
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

    let multisig_pk = MultiSigPublicKey::new(vec![pk1, pk2], vec![1, 1], 2).unwrap();

    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig1 = Signature::new_secure(&msg, &kp1);
    let sig2 = Signature::new_secure(&msg, &kp2);
    let sig3 = Signature::new_secure(&msg, &kp3);

    // MultiSigPublicKey contains only 2 public key but 3 signatures are passed, fails to combine.
    assert!(MultiSig::combine(vec![sig1, sig2, sig3], multisig_pk.clone()).is_err());

    // Cannot create malformed MultiSig.
    assert!(MultiSig::combine(vec![], multisig_pk).is_err());
}
#[test]
fn test_serde_roundtrip() {
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );

    for kp in keys() {
        let pk = kp.public();
        let multisig_pk = MultiSigPublicKey::new(vec![pk], vec![1], 1).unwrap();
        let sig = Signature::new_secure(&msg, &kp);
        let multisig = MultiSig::combine(vec![sig], multisig_pk).unwrap();
        let plain_bytes = bcs::to_bytes(&multisig).unwrap();

        let generic_sig = GenericSignature::MultiSig(multisig);
        let generic_sig_bytes = generic_sig.as_bytes();
        let generic_sig_roundtrip = GenericSignature::from_bytes(generic_sig_bytes).unwrap();
        assert_eq!(generic_sig, generic_sig_roundtrip);

        // A MultiSig flag 0x03 is appended before the bcs serialized bytes.
        assert_eq!(plain_bytes.len() + 1, generic_sig_bytes.len());
        assert_eq!(generic_sig_bytes.first().unwrap(), &0x03);
    }

    // Malformed multisig cannot be deserialized
    let multisig_pk = MultiSigPublicKey {
        pk_map: vec![(keys()[0].public(), 1)],
        threshold: 1,
    };
    let multisig = MultiSig {
        sigs: vec![], // No sigs
        bitmap: RoaringBitmap::new(),
        multisig_pk,
        bytes: OnceCell::new(),
    };

    let generic_sig = GenericSignature::MultiSig(multisig);
    let generic_sig_bytes = generic_sig.as_bytes();
    assert!(GenericSignature::from_bytes(generic_sig_bytes).is_err());

    // Malformed multisig_pk cannot be deserialized
    let multisig_pk_1 = MultiSigPublicKey {
        pk_map: vec![],
        threshold: 0,
    };

    let multisig_1 = MultiSig {
        sigs: vec![],
        bitmap: RoaringBitmap::new(),
        multisig_pk: multisig_pk_1,
        bytes: OnceCell::new(),
    };

    let generic_sig_1 = GenericSignature::MultiSig(multisig_1);
    let generic_sig_bytes = generic_sig_1.as_bytes();
    assert!(GenericSignature::from_bytes(generic_sig_bytes).is_err());

    // Single sig serialization unchanged.
    let sig = Ed25519SuiSignature::default();
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
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig = Signature::new_secure(&msg, &kp);
    assert!(sig.verify_secure_generic(&msg, addr).is_ok());
}

#[test]
fn test_multisig_pk_failure() {
    let keys = keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();
    let pk3 = keys[2].public();

    // Fails on weight 0.
    assert!(MultiSigPublicKey::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone()],
        vec![0, 1, 1],
        2
    )
    .is_err());

    // Fails on threshold 0.
    assert!(MultiSigPublicKey::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone()],
        vec![0, 1, 1],
        0
    )
    .is_err());

    // Fails on incorrect array length.
    assert!(
        MultiSigPublicKey::new(vec![pk1.clone(), pk2.clone(), pk3.clone()], vec![1], 2).is_err()
    );

    // Fails on empty array length.
    assert!(MultiSigPublicKey::new(vec![pk1, pk2, pk3], vec![], 2).is_err());
}

#[test]
fn test_multisig_address() {
    // Pin an hardcoded multisig address generation here. If this fails, the address
    // generation logic may have changed. If this is intended, update the hardcoded value below.
    let keys = keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();
    let pk3 = keys[2].public();

    let threshold: ThresholdUnit = 2;
    let w1: WeightUnit = 1;
    let w2: WeightUnit = 2;
    let w3: WeightUnit = 3;

    let multisig_pk =
        MultiSigPublicKey::new(vec![pk1, pk2, pk3], vec![w1, w2, w3], threshold).unwrap();
    let address: SuiAddress = multisig_pk.into();
    assert_eq!(
        SuiAddress::from_str("0xe35c69eb504de34afdbd9f307fb3ca152646c92d549fea00065d26fc422109ea")
            .unwrap(),
        address
    );
}

#[test]
fn test_max_sig() {
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let mut seed = StdRng::from_seed([0; 32]);
    let mut keys: Vec<SuiKeyPair> = Vec::new();
    for _ in 0..10 {
        keys.push(SuiKeyPair::Ed25519(get_key_pair_from_rng(&mut seed).1));
    }

    // multisig_pk with larger that max number of pks fails.
    assert!(MultiSigPublicKey::new(
        vec![keys[0].public(); MAX_SIGNER_IN_MULTISIG + 1],
        vec![WeightUnit::MAX; MAX_SIGNER_IN_MULTISIG + 1],
        ThresholdUnit::MAX
    )
    .is_err());

    // multisig_pk with unreachable threshold fails.
    assert!(MultiSigPublicKey::new(
        vec![keys[0].public(); 5],
        vec![3; MAX_SIGNER_IN_MULTISIG],
        16
    )
    .is_err());

    // multisig_pk with max weights for each pk and max reachable threshold is ok.
    let high_threshold_pk = MultiSigPublicKey::new(
        vec![keys[0].public(); MAX_SIGNER_IN_MULTISIG],
        vec![WeightUnit::MAX; MAX_SIGNER_IN_MULTISIG],
        (WeightUnit::MAX as ThresholdUnit) * (MAX_SIGNER_IN_MULTISIG as ThresholdUnit),
    )
    .unwrap();
    let address: SuiAddress = high_threshold_pk.clone().into();
    let sig = Signature::new_secure(&msg, &keys[0]);

    // But max threshold cannot be met, fails to verify.
    let multisig = MultiSig::combine(vec![sig; MAX_SIGNER_IN_MULTISIG], high_threshold_pk).unwrap();
    assert!(multisig.verify_secure_generic(&msg, address).is_err());

    // multisig_pk with max weights for each pk with threshold is 1x max weight verifies ok.
    let low_threshold_pk = MultiSigPublicKey::new(
        vec![keys[0].public(); MAX_SIGNER_IN_MULTISIG],
        vec![WeightUnit::MAX; 10],
        WeightUnit::MAX.into(),
    )
    .unwrap();
    let address: SuiAddress = low_threshold_pk.clone().into();
    let sig = Signature::new_secure(&msg, &keys[0]);
    let multisig = MultiSig::combine(vec![sig; 1], low_threshold_pk).unwrap();
    assert!(multisig.verify_secure_generic(&msg, address).is_ok());
}

#[test]
fn te() {
    let s1 = Signature::from_bytes(&Base64::decode("AFtiPovFXDRVnX7PLEjZzlzcIyN4lSszamrBq/u4l2mS6x6a3uW9C6Fj9gBRA9OxPAH5M5Qt1+sHGHX3rmzsrgVMoxeoEQG32SXVoammIG4k0ULXlFGvrrIY/RnZHq93kQ==").unwrap()).unwrap();
    let s2 = Signature::from_bytes(&Base64::decode("AFQxTiu9Dei908iP5K7dL0ILX3YzRUoY4gLfaZV2Agkd7nc5c/ZfcLwJxXZhlSfme/PnpXsZcTm2jpcieQNBBwCxc5ih4C4eomkKJp7hhnK4OXoh6F0OoQ8KLM2J4fEeCQ==").unwrap()).unwrap();
    let s3 = Signature::from_bytes(&Base64::decode("APQiqdE+Ta+PGKT3GasosAlTqrmOT7WvSnQ1nfSkRBlhknS9eXwOxirswr0rJoKntNl/C+mPgQUrqFQxYu+bJQVDk2wWqyyE40E+u5oUFCZm2iTsGuXt56nGt5YH54bSQg==").unwrap()).unwrap();
    println!("comp1: {:?}", s1.to_compressed().unwrap());
    println!("comp2: {:?}", s2.to_compressed().unwrap());
    println!("comp3: {:?}", s3.to_compressed().unwrap());

    match s3.clone() {
        Signature::Ed25519SuiSignature(c) => {
            let s = Ed25519Signature::from_bytes(c.signature_bytes()).unwrap();
            println!("ss: {:?}", s);

        },
        _ => {},
    };
    let a1: SuiAddress = (&s1.to_public_key().unwrap()).into();
    let a2: SuiAddress = (&s2.to_public_key().unwrap()).into();
    let a3: SuiAddress = (&s3.to_public_key().unwrap()).into();
    println!("a1: {:?}", a1);
    println!("a2: {:?}", a2);
    println!("a3: {:?}", a3);

    let pk1 = s1.to_public_key().unwrap();
    let pk2 = s2.to_public_key().unwrap();
    let pk3 = s3.to_public_key().unwrap();
    println!("pk1: {:?}", pk1);
    println!("pk2: {:?}", pk2);
    println!("pk3: {:?}", pk3);

    
    let value: IntentMessage<TransactionData> = IntentMessage::new(
        Intent::sui_transaction(),
        bcs::from_bytes(&Base64::decode("AAABACBP/9AAVSK+S8Apckx/D27XCTpr86CbkOYvYdwVGB4aPgEBAQABAABP/9AAVSK+S8Apckx/D27XCTpr86CbkOYvYdwVGB4aPgEICob+LuQD2JMpE5yRYAS2nkvDNttTFey+iEbz9QQLZ9YwAAAAAAAAICb4vXjGp/Ix2wQuBIlk1TBJk9nCOleTvq/fV4HQOnMvT//QAFUivkvAKXJMfw9u1wk6a/Ogm5DmL2HcFRgeGj4BAAAAAAAAAIDDyQEAAAAAAA==").unwrap()).unwrap(),
    );
    let r1 = s1.verify_secure_generic(&value, a1);
    println!("r1: {:?}", r1);
    let r2 = s2.verify_secure_generic(&value, a2);
    println!("r2: {:?}", r2);
    let r3 = s3.verify_secure_generic(&value, a3);
    println!("r3: {:?}", r3);

    let multisig_pk = MultiSigPublicKey::new(vec![
        PublicKey::decode_base64("AEOTbBarLITjQT67mhQUJmbaJOwa5e3nqca3lgfnhtJC").unwrap(),
        PublicKey::decode_base64("AEyjF6gRAbfZJdWhqaYgbiTRQteUUa+ushj9Gdker3eR").unwrap(),
        PublicKey::decode_base64("AMdynPV1TMQ1FfSEkOFaxXuKwYfgmoSTmqobzDFG/xHY").unwrap(),
        PublicKey::decode_base64("ALFzmKHgLh6iaQomnuGGcrg5eiHoXQ6hDwoszYnh8R4J").unwrap(),
        PublicKey::decode_base64("ANYn9Xm0UCX+YttTK5uB9CR+nRTruLpTEnCHEej2f4C0").unwrap(),
        PublicKey::decode_base64("ANIeZLtOzH0fwcImIOwvb7f6MR9EcO63vZmHYn5uuoV0").unwrap(),
        ], vec![1, 1, 1, 1, 1, 1], 3).unwrap();
    let add = SuiAddress::from(multisig_pk.clone());
    println!("add: {:?}", add);
    let multisig = MultiSig::combine(vec![s3, s1, s2], multisig_pk).unwrap();
    println!("multisig: {:?}", multisig);
    let binding = GenericSignature::MultiSig(multisig.clone());
    let ser_multisig = binding.as_bytes();
    println!("ser: {:?}", Base64::encode(ser_multisig));
    let res = multisig.verify_secure_generic(&value, add);
    println!("res: {:?}", res);
}