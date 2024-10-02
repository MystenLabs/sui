// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{MultiSigPublicKey, ThresholdUnit, WeightUnit};
use crate::{
    base_types::SuiAddress,
    crypto::{
        get_key_pair, get_key_pair_from_rng, Ed25519SuiSignature, PublicKey, Signature, SuiKeyPair,
        SuiSignatureInner, ZkLoginPublicIdentifier,
    },
    multisig::{as_indices, MultiSig, MAX_SIGNER_IN_MULTISIG},
    multisig_legacy::bitmap_to_u16,
    signature::{AuthenticatorTrait, GenericSignature, VerifyParams},
    signature_verification::VerifiedDigestCache,
    utils::{
        keys, load_test_vectors, make_transaction_data, make_zklogin_tx, DEFAULT_ADDRESS_SEED,
        SHORT_ADDRESS_SEED,
    },
    zk_login_authenticator::ZkLoginAuthenticator,
    zk_login_util::DEFAULT_JWK_BYTES,
};
use fastcrypto::{
    ed25519::Ed25519KeyPair,
    encoding::{Base64, Encoding},
    traits::ToFromBytes,
};
use fastcrypto_zkp::bn254::zk_login::{parse_jwks, JwkId, OIDCProvider, ZkLoginInputs, JWK};
use fastcrypto_zkp::bn254::zk_login_api::ZkLoginEnv;
use fastcrypto_zkp::zk_login_utils::Bn254FrElement;
use im::hashmap::HashMap as ImHashMap;
use once_cell::sync::OnceCell;
use rand::{rngs::StdRng, SeedableRng};
use roaring::RoaringBitmap;
use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};
use std::{str::FromStr, sync::Arc};
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
    let sig1: GenericSignature = Signature::new_secure(&msg, &kp1).into();
    let sig2 = Signature::new_secure(&msg, &kp2).into();
    let sig3 = Signature::new_secure(&msg, &kp3).into();

    // MultiSigPublicKey contains only 2 public key but 3 signatures are passed, fails to combine.
    assert!(MultiSig::combine(vec![sig1.clone(), sig2, sig3], multisig_pk.clone()).is_err());

    // Cannot create malformed MultiSig.
    assert!(MultiSig::combine(vec![], multisig_pk.clone()).is_err());
    assert!(MultiSig::combine(vec![sig1.clone(), sig1], multisig_pk).is_err());
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
        let sig = Signature::new_secure(&msg, &kp).into();
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
        bitmap: 0,
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
        bitmap: 0,
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
fn test_multisig_pk_new() {
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
        vec![1, 1, 1],
        0
    )
    .is_err());

    // Fails on incorrect array length.
    assert!(
        MultiSigPublicKey::new(vec![pk1.clone(), pk2.clone(), pk3.clone()], vec![1], 2).is_err()
    );

    // Fails on empty array length.
    assert!(MultiSigPublicKey::new(vec![pk1.clone(), pk2, pk3], vec![], 2).is_err());

    // Fails on dup pks.
    assert!(
        MultiSigPublicKey::new(vec![pk1.clone(), pk1.clone(), pk1], vec![1, 2, 3], 4,).is_err()
    );
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
    let address: SuiAddress = (&multisig_pk).into();
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
    let mut keys = Vec::new();
    let mut pks = Vec::new();

    for _ in 0..11 {
        let k = SuiKeyPair::Ed25519(get_key_pair_from_rng(&mut seed).1);
        pks.push(k.public());
        keys.push(k);
    }

    // multisig_pk with larger that max number of pks fails.
    assert!(MultiSigPublicKey::new(
        pks.clone(),
        vec![WeightUnit::MAX; MAX_SIGNER_IN_MULTISIG + 1],
        ThresholdUnit::MAX
    )
    .is_err());

    // multisig_pk with unreachable threshold fails.
    assert!(MultiSigPublicKey::new(pks.clone()[..5].to_vec(), vec![3; 5], 16).is_err());

    // multisig_pk with max weights for each pk and max reachable threshold is ok.
    let res = MultiSigPublicKey::new(
        pks.clone()[..10].to_vec(),
        vec![WeightUnit::MAX; MAX_SIGNER_IN_MULTISIG],
        (WeightUnit::MAX as ThresholdUnit) * (MAX_SIGNER_IN_MULTISIG as ThresholdUnit),
    );
    assert!(res.is_ok());

    // multisig_pk with unreachable threshold fails.
    let res = MultiSigPublicKey::new(
        pks.clone()[..10].to_vec(),
        vec![WeightUnit::MAX; MAX_SIGNER_IN_MULTISIG],
        (WeightUnit::MAX as ThresholdUnit) * (MAX_SIGNER_IN_MULTISIG as ThresholdUnit) + 1,
    );
    assert!(res.is_err());

    // multisig_pk with max weights for each pk with threshold is 1x max weight validates ok.
    let low_threshold_pk = MultiSigPublicKey::new(
        pks.clone()[..10].to_vec(),
        vec![WeightUnit::MAX; 10],
        WeightUnit::MAX.into(),
    )
    .unwrap();
    let sig = Signature::new_secure(&msg, &keys[0]).into();
    assert!(MultiSig::combine(vec![sig; 1], low_threshold_pk)
        .unwrap()
        .init_and_validate()
        .is_ok());
}

#[test]
fn test_to_from_indices() {
    assert!(as_indices(0b11111111110).is_err());
    assert_eq!(as_indices(0b0000010110).unwrap(), vec![1, 2, 4]);
    assert_eq!(
        as_indices(0b1111111111).unwrap(),
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
    );

    let mut bitmap = RoaringBitmap::new();
    bitmap.insert(1);
    bitmap.insert(2);
    bitmap.insert(4);
    assert_eq!(bitmap_to_u16(bitmap.clone()).unwrap(), 0b0000010110);
    bitmap.insert(11);
    assert!(bitmap_to_u16(bitmap).is_err());
}

#[test]
fn multisig_get_pk() {
    let keys = keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();

    let multisig_pk = MultiSigPublicKey::new(vec![pk1, pk2], vec![1, 1], 2).unwrap();
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig1: GenericSignature = Signature::new_secure(&msg, &keys[0]).into();
    let sig2: GenericSignature = Signature::new_secure(&msg, &keys[1]).into();

    let multi_sig =
        MultiSig::combine(vec![sig1.clone(), sig2.clone()], multisig_pk.clone()).unwrap();

    assert!(multi_sig.get_pk().clone() == multisig_pk);
    assert!(
        *multi_sig.get_sigs() == vec![sig1.to_compressed().unwrap(), sig2.to_compressed().unwrap()]
    );
}

#[test]
fn multisig_get_indices() {
    let keys = keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();
    let pk3 = keys[2].public();

    let multisig_pk = MultiSigPublicKey::new(vec![pk1, pk2, pk3], vec![1, 1, 1], 2).unwrap();
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig1: GenericSignature = Signature::new_secure(&msg, &keys[0]).into();
    let sig2: GenericSignature = Signature::new_secure(&msg, &keys[1]).into();
    let sig3: GenericSignature = Signature::new_secure(&msg, &keys[2]).into();

    let multi_sig1 =
        MultiSig::combine(vec![sig2.clone(), sig3.clone()], multisig_pk.clone()).unwrap();

    let multi_sig2 = MultiSig::combine(
        vec![sig1.clone(), sig2.clone(), sig3.clone()],
        multisig_pk.clone(),
    )
    .unwrap();

    let invalid_multisig = MultiSig::combine(vec![sig3, sig2, sig1], multisig_pk).unwrap();

    // Indexes of public keys in multisig public key instance according to the combined sigs.
    assert!(multi_sig1.get_indices().unwrap() == vec![1, 2]);
    assert!(multi_sig2.get_indices().unwrap() == vec![0, 1, 2]);
    assert!(invalid_multisig.get_indices().unwrap() == vec![0, 1, 2]);
}

#[test]
fn multisig_zklogin_scenarios() {
    // consistency test with sui/sdk/typescript/test/unit/cryptography/multisig.test.ts
    let mut seed = StdRng::from_seed([0; 32]);
    let kp: Ed25519KeyPair = get_key_pair_from_rng(&mut seed).1;
    let skp: SuiKeyPair = SuiKeyPair::Ed25519(kp);
    let pk1 = skp.public();

    let (_, _, inputs) = &load_test_vectors("./src/unit_tests/zklogin_test_vectors.json")[0];
    // pk consistent with the one in make_zklogin_tx
    let pk2 = PublicKey::ZkLogin(
        ZkLoginPublicIdentifier::new(
            &OIDCProvider::Twitch.get_config().iss,
            inputs.get_address_seed(),
        )
        .unwrap(),
    );

    // set up 1-out-of-2 multisig with one zklogin public identifier and one traditional public key.
    let multisig_pk = MultiSigPublicKey::new(vec![pk1, pk2], vec![1, 1], 1).unwrap();
    let multisig_addr = SuiAddress::from(&multisig_pk);
    assert_eq!(
        multisig_addr,
        SuiAddress::from_str("0xb9c0780a3943cde13a2409bf1a6f06ae60b0dff2b2f373260cf627aa4f43a588")
            .unwrap()
    );

    let (_, envelop, zklogin_sig) = make_zklogin_tx(multisig_addr, false);
    let binding = envelop.into_data();
    let tx = binding.transaction_data();
    assert_eq!(Base64::encode(bcs::to_bytes(tx).unwrap()), "AAABACACAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgEBAQABAAC5wHgKOUPN4TokCb8abwauYLDf8rLzcyYM9ieqT0OliAGbB4FfBEl+LgXSLKw6oGFBCyCGjMYZFUxCocYb6ZAnFwEAAAAAAAAAIJZw7UpW1XHubORIOaY8d2+WyBNwoJ+FEAxlsa7h7JHrucB4CjlDzeE6JAm/Gm8GrmCw3/Ky83MmDPYnqk9DpYgBAAAAAAAAABAnAAAAAAAAAA==".to_string());

    let intent_msg = &IntentMessage::new(Intent::sui_transaction(), tx.clone());
    assert_eq!(Base64::encode(zklogin_sig.as_ref()), "BQNNMTczMTgwODkxMjU5NTI0MjE3MzYzNDIyNjM3MTc5MzI3MTk0Mzc3MTc4NDQyODI0MTAxODc5NTc5ODQ3NTE5Mzk5NDI4OTgyNTEyNTBNMTEzNzM5NjY2NDU0NjkxMjI1ODIwNzQwODIyOTU5ODUzODgyNTg4NDA2ODE2MTgyNjg1OTM5NzY2OTczMjU4OTIyODA5MTU2ODEyMDcBMQMCTDU5Mzk4NzExNDczNDg4MzQ5OTczNjE3MjAxMjIyMzg5ODAxNzcxNTIzMDMyNzQzMTEwNDcyNDk5MDU5NDIzODQ5MTU3Njg2OTA4OTVMNDUzMzU2ODI3MTEzNDc4NTI3ODczMTIzNDU3MDM2MTQ4MjY1MTk5Njc0MDc5MTg4ODI4NTg2NDk2Njg4NDAzMjcxNzA0OTgxMTcwOAJNMTA1NjQzODcyODUwNzE1NTU0Njk3NTM5OTA2NjE0MTA4NDAxMTg2MzU5MjU0NjY1OTcwMzcwMTgwNTg3NzAwNDEzNDc1MTg0NjEzNjhNMTI1OTczMjM1NDcyNzc1NzkxNDQ2OTg0OTYzNzIyNDI2MTUzNjgwODU4MDEzMTMzNDMxNTU3MzU1MTEzMzAwMDM4ODQ3Njc5NTc4NTQCATEBMANNMTU3OTE1ODk0NzI1NTY4MjYyNjMyMzE2NDQ3Mjg4NzMzMzc2MjkwMTUyNjk5ODQ2OTk0MDQwNzM2MjM2MDMzNTI1Mzc2Nzg4MTMxNzFMNDU0Nzg2NjQ5OTI0ODg4MTQ0OTY3NjE2MTE1ODAyNDc0ODA2MDQ4NTM3MzI1MDAyOTQyMzkwNDExMzAxNzQyMjUzOTAzNzE2MjUyNwExMXdpYVhOeklqb2lhSFIwY0hNNkx5OXBaQzUwZDJsMFkyZ3VkSFl2YjJGMWRHZ3lJaXcCMmV5SmhiR2NpT2lKU1V6STFOaUlzSW5SNWNDSTZJa3BYVkNJc0ltdHBaQ0k2SWpFaWZRTTIwNzk0Nzg4NTU5NjIwNjY5NTk2MjA2NDU3MDIyOTY2MTc2OTg2Njg4NzI3ODc2MTI4MjIzNjI4MTEzOTE2MzgwOTI3NTAyNzM3OTExCgAAAAAAAABhABHpkQ5JvxqbqCKtqh9M0U5c3o3l62B6ALVOxMq6nsc0y3JlY8Gf1ZoPA976dom6y3JGBUTsry6axfqHcVrtRAy5xu4WMO8+cRFEpkjbBruyKE9ydM++5T/87lA8waSSAA==".to_string());

    let single_sig = GenericSignature::Signature(Signature::new_secure(intent_msg, &skp));
    let multisig = GenericSignature::MultiSig(
        MultiSig::combine(vec![single_sig, zklogin_sig], multisig_pk.clone()).unwrap(),
    );
    assert_eq!(Base64::encode(multisig.as_ref()), "AwIAcAEsWrZtlsE3AdGUKJAPag8Tu6HPfMW7gEemeneO9fmNGiJP/rDZu/tL75lr8A22eFDx9K2G1DL4v8XlmuTtCgOaBwUDTTE3MzE4MDg5MTI1OTUyNDIxNzM2MzQyMjYzNzE3OTMyNzE5NDM3NzE3ODQ0MjgyNDEwMTg3OTU3OTg0NzUxOTM5OTQyODk4MjUxMjUwTTExMzczOTY2NjQ1NDY5MTIyNTgyMDc0MDgyMjk1OTg1Mzg4MjU4ODQwNjgxNjE4MjY4NTkzOTc2Njk3MzI1ODkyMjgwOTE1NjgxMjA3ATEDAkw1OTM5ODcxMTQ3MzQ4ODM0OTk3MzYxNzIwMTIyMjM4OTgwMTc3MTUyMzAzMjc0MzExMDQ3MjQ5OTA1OTQyMzg0OTE1NzY4NjkwODk1TDQ1MzM1NjgyNzExMzQ3ODUyNzg3MzEyMzQ1NzAzNjE0ODI2NTE5OTY3NDA3OTE4ODgyODU4NjQ5NjY4ODQwMzI3MTcwNDk4MTE3MDgCTTEwNTY0Mzg3Mjg1MDcxNTU1NDY5NzUzOTkwNjYxNDEwODQwMTE4NjM1OTI1NDY2NTk3MDM3MDE4MDU4NzcwMDQxMzQ3NTE4NDYxMzY4TTEyNTk3MzIzNTQ3Mjc3NTc5MTQ0Njk4NDk2MzcyMjQyNjE1MzY4MDg1ODAxMzEzMzQzMTU1NzM1NTExMzMwMDAzODg0NzY3OTU3ODU0AgExATADTTE1NzkxNTg5NDcyNTU2ODI2MjYzMjMxNjQ0NzI4ODczMzM3NjI5MDE1MjY5OTg0Njk5NDA0MDczNjIzNjAzMzUyNTM3Njc4ODEzMTcxTDQ1NDc4NjY0OTkyNDg4ODE0NDk2NzYxNjExNTgwMjQ3NDgwNjA0ODUzNzMyNTAwMjk0MjM5MDQxMTMwMTc0MjI1MzkwMzcxNjI1MjcBMTF3aWFYTnpJam9pYUhSMGNITTZMeTlwWkM1MGQybDBZMmd1ZEhZdmIyRjFkR2d5SWl3AjJleUpoYkdjaU9pSlNVekkxTmlJc0luUjVjQ0k2SWtwWFZDSXNJbXRwWkNJNklqRWlmUU0yMDc5NDc4ODU1OTYyMDY2OTU5NjIwNjQ1NzAyMjk2NjE3Njk4NjY4ODcyNzg3NjEyODIyMzYyODExMzkxNjM4MDkyNzUwMjczNzkxMQoAAAAAAAAAYQAR6ZEOSb8am6giraofTNFOXN6N5etgegC1TsTKup7HNMtyZWPBn9WaDwPe+naJustyRgVE7K8umsX6h3Fa7UQMucbuFjDvPnERRKZI2wa7sihPcnTPvuU//O5QPMGkkgADAAIADX2rNYyNrapO+gBJp1sHQ2VVsQo2ghm7aA9wVxNJ13UBAzwbaHR0cHM6Ly9pZC50d2l0Y2gudHYvb2F1dGgyLflu6Eag/zG3tLd5CtZRYx9p1t34RovVSn/+uHFiYfcBAQA=".to_string());
}

#[test]
fn zklogin_in_multisig_works_with_both_addresses() {
    let mut seed = StdRng::from_seed([0; 32]);
    let kp: Ed25519KeyPair = get_key_pair_from_rng(&mut seed).1;
    let skp: SuiKeyPair = SuiKeyPair::Ed25519(kp);

    // create a new multisig address based on pk1 and pk2 where pk1 is a zklogin public identifier, with a crafted unpadded bytes.
    let mut bytes = Vec::new();
    let binding = OIDCProvider::Twitch.get_config();
    let iss_bytes = binding.iss.as_bytes();
    bytes.extend([iss_bytes.len() as u8]);
    bytes.extend(iss_bytes);
    // length here is 31 bytes and left unpadded.
    let address_seed = Bn254FrElement::from_str(SHORT_ADDRESS_SEED).unwrap();
    bytes.extend(address_seed.unpadded());

    let pk1 = PublicKey::ZkLogin(ZkLoginPublicIdentifier(bytes));
    let pk2 = skp.public();
    let multisig_pk = MultiSigPublicKey::new(vec![pk1, pk2.clone()], vec![1; 2], 1).unwrap();
    let multisig_address = SuiAddress::from(&multisig_pk);

    let (kp, _pk, input) = &load_test_vectors("./src/unit_tests/zklogin_test_vectors.json")[0];
    let intent_msg = &IntentMessage::new(
        Intent::sui_transaction(),
        make_transaction_data(multisig_address),
    );
    let user_signature = Signature::new_secure(intent_msg, kp);

    let modified_inputs =
        ZkLoginInputs::from_json(&serde_json::to_string(input).unwrap(), SHORT_ADDRESS_SEED)
            .unwrap();
    let zklogin_sig = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
        modified_inputs.clone(),
        10,
        user_signature,
    ));
    let multisig =
        MultiSig::insecure_new(vec![zklogin_sig.to_compressed().unwrap()], 1, multisig_pk);

    let parsed: ImHashMap<JwkId, JWK> = parse_jwks(DEFAULT_JWK_BYTES, &OIDCProvider::Twitch)
        .unwrap()
        .into_iter()
        .collect();

    let aux_verify_data = VerifyParams::new(parsed, vec![], ZkLoginEnv::Test, true, true, Some(30));
    let res = multisig.verify_claims(
        intent_msg,
        multisig_address,
        &aux_verify_data,
        Arc::new(VerifiedDigestCache::new_empty()),
    );
    // since the zklogin inputs is crafted, it is expected that the proof verify failed, but all checks before passes.
    assert!(
        matches!(res, Err(crate::error::SuiError::InvalidSignature { error }) if error.contains("General cryptographic error: Groth16 proof verify failed"))
    );

    // initialize zklogin pk (pk1_padd) with padded address seed
    let pk1_padded = PublicKey::ZkLogin(
        ZkLoginPublicIdentifier::new(
            &OIDCProvider::Twitch.get_config().iss,
            &Bn254FrElement::from_str(SHORT_ADDRESS_SEED).unwrap(),
        )
        .unwrap(),
    );
    let multisig_pk_padded = MultiSigPublicKey::new(vec![pk1_padded, pk2], vec![1; 2], 1).unwrap();
    let multisig_address_padded = SuiAddress::from(&multisig_pk_padded);
    let modified_inputs_padded =
        ZkLoginInputs::from_json(&serde_json::to_string(input).unwrap(), SHORT_ADDRESS_SEED)
            .unwrap();
    let intent_msg_padded = &IntentMessage::new(
        Intent::sui_transaction(),
        make_transaction_data(multisig_address_padded),
    );
    let user_signature_padded = Signature::new_secure(intent_msg_padded, kp);
    let zklogin_sig_padded = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
        modified_inputs_padded.clone(),
        10,
        user_signature_padded,
    ));
    let multisig_padded = MultiSig::insecure_new(
        vec![zklogin_sig_padded.to_compressed().unwrap()],
        1,
        multisig_pk_padded,
    );

    let res = multisig_padded.verify_claims(
        intent_msg_padded,
        multisig_address_padded,
        &aux_verify_data,
        Arc::new(VerifiedDigestCache::new_empty()),
    );
    assert!(
        matches!(res, Err(crate::error::SuiError::InvalidSignature { error }) if error.contains("General cryptographic error: Groth16 proof verify failed"))
    );
}

#[test]
fn test_derive_multisig_address() {
    // consistency test with typescript: /sdk/typescript/test/unit/cryptography/multisig.test.ts
    let pk1 = PublicKey::ZkLogin(
        ZkLoginPublicIdentifier::new(
            &OIDCProvider::Twitch.get_config().iss,
            &Bn254FrElement::from_str(DEFAULT_ADDRESS_SEED).unwrap(),
        )
        .unwrap(),
    );
    // address seed here is padded with leading 0 to 32 bytes.
    let pk2 = PublicKey::ZkLogin(
        ZkLoginPublicIdentifier::new(
            &OIDCProvider::Twitch.get_config().iss,
            &Bn254FrElement::from_str(SHORT_ADDRESS_SEED).unwrap(),
        )
        .unwrap(),
    );
    assert_eq!(pk1.as_ref().len(), pk2.as_ref().len());

    let multisig_pk = MultiSigPublicKey::new(vec![pk1, pk2], vec![1, 1], 1).unwrap();
    let multisig_addr = SuiAddress::from(&multisig_pk);
    assert_eq!(
        multisig_addr,
        SuiAddress::from_str("0x77a9fbf3c695d78dd83449a81a9e70aa79a77dbfd6fb72037bf09201c12052cd")
            .unwrap()
    );
}
