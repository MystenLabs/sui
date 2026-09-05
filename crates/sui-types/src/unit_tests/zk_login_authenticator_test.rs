// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use std::sync::Arc;

use crate::crypto::{PublicKey, Signature, SignatureScheme, SuiKeyPair, ZkLoginPublicIdentifier};

use crate::signature::{AuthenticatorTrait, VerifyParams};
use crate::signature_verification::VerifiedDigestCache;
use crate::utils::{
    PINNED_PROOF_ADDRESS_SEED, PINNED_V1_PROOF_JSON, SHORT_ADDRESS_SEED, load_test_vectors,
    pinned_jwks,
};
use crate::utils::{get_zklogin_user_address, make_zklogin_tx, sign_zklogin_personal_msg};
use crate::{
    base_types::SuiAddress,
    error::SuiErrorKind,
    signature::GenericSignature,
    zk_login_authenticator::{ZkLoginAuthenticator, ZkLoginAuthenticatorV2},
    zk_login_util::DEFAULT_JWK_BYTES,
};
use fastcrypto::encoding::Base64;
use fastcrypto::traits::{KeyPair, ToFromBytes};

use fastcrypto_zkp::bn254::zk_login::{JWK, JwkId, OIDCProvider, ZkLoginInputs, parse_jwks};
use fastcrypto_zkp::bn254::zk_login_api::{CircuitVersion, ZkLoginEnv};
use fastcrypto_zkp::zk_login_utils::Bn254FrElement;
use imbl::hashmap::HashMap as ImHashMap;
use rand::SeedableRng;
use rand::rngs::StdRng;
use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};

#[test]
fn test_serde_zk_login_signature() {
    // consistency test with typescript: sdk/typescript/test/unit/zklogin/signature.test.ts
    use fastcrypto::encoding::Encoding;
    let (user_address, _tx, authenticator) = make_zklogin_tx(get_zklogin_user_address(), false);
    let serialized = authenticator.as_ref();
    assert_eq!(serialized, &Base64::decode("BQNNMTczMTgwODkxMjU5NTI0MjE3MzYzNDIyNjM3MTc5MzI3MTk0Mzc3MTc4NDQyODI0MTAxODc5NTc5ODQ3NTE5Mzk5NDI4OTgyNTEyNTBNMTEzNzM5NjY2NDU0NjkxMjI1ODIwNzQwODIyOTU5ODUzODgyNTg4NDA2ODE2MTgyNjg1OTM5NzY2OTczMjU4OTIyODA5MTU2ODEyMDcBMQMCTDU5Mzk4NzExNDczNDg4MzQ5OTczNjE3MjAxMjIyMzg5ODAxNzcxNTIzMDMyNzQzMTEwNDcyNDk5MDU5NDIzODQ5MTU3Njg2OTA4OTVMNDUzMzU2ODI3MTEzNDc4NTI3ODczMTIzNDU3MDM2MTQ4MjY1MTk5Njc0MDc5MTg4ODI4NTg2NDk2Njg4NDAzMjcxNzA0OTgxMTcwOAJNMTA1NjQzODcyODUwNzE1NTU0Njk3NTM5OTA2NjE0MTA4NDAxMTg2MzU5MjU0NjY1OTcwMzcwMTgwNTg3NzAwNDEzNDc1MTg0NjEzNjhNMTI1OTczMjM1NDcyNzc1NzkxNDQ2OTg0OTYzNzIyNDI2MTUzNjgwODU4MDEzMTMzNDMxNTU3MzU1MTEzMzAwMDM4ODQ3Njc5NTc4NTQCATEBMANNMTU3OTE1ODk0NzI1NTY4MjYyNjMyMzE2NDQ3Mjg4NzMzMzc2MjkwMTUyNjk5ODQ2OTk0MDQwNzM2MjM2MDMzNTI1Mzc2Nzg4MTMxNzFMNDU0Nzg2NjQ5OTI0ODg4MTQ0OTY3NjE2MTE1ODAyNDc0ODA2MDQ4NTM3MzI1MDAyOTQyMzkwNDExMzAxNzQyMjUzOTAzNzE2MjUyNwExMXdpYVhOeklqb2lhSFIwY0hNNkx5OXBaQzUwZDJsMFkyZ3VkSFl2YjJGMWRHZ3lJaXcCMmV5SmhiR2NpT2lKU1V6STFOaUlzSW5SNWNDSTZJa3BYVkNJc0ltdHBaQ0k2SWpFaWZRTTIwNzk0Nzg4NTU5NjIwNjY5NTk2MjA2NDU3MDIyOTY2MTc2OTg2Njg4NzI3ODc2MTI4MjIzNjI4MTEzOTE2MzgwOTI3NTAyNzM3OTExCgAAAAAAAABhAG6Bf8BLuaIEgvF8Lx2jVoRWKKRIlaLlEJxgvqwq5nDX+rvzJxYAUFd7KeQBd9upNx+CHpmINkfgj26jcHbbqAy5xu4WMO8+cRFEpkjbBruyKE9ydM++5T/87lA8waSSAA==").unwrap());
    let deserialized = GenericSignature::from_bytes(serialized).unwrap();
    assert_eq!(deserialized, authenticator);

    let addr: SuiAddress = (&authenticator).try_into().unwrap();
    assert_eq!(addr, user_address);
}

#[test]
fn test_serde_zk_login_v2_signature() {
    let (keypair, v1_public_key, inputs) =
        load_test_vectors("./src/unit_tests/zklogin_v2_test_vectors.json")
            .into_iter()
            .next()
            .unwrap();
    let intent_message = IntentMessage::new(
        Intent::personal_message(),
        PersonalMessage {
            message: b"zkLogin V2 serialization".to_vec(),
        },
    );
    let v2_public_key = PublicKey::from_zklogin_v2_inputs(&inputs).unwrap();
    let authenticator = ZkLoginAuthenticator::new(
        inputs,
        10,
        crate::crypto::Signature::new_secure(&intent_message, &keypair),
    );
    let v1_signature = GenericSignature::ZkLoginAuthenticator(authenticator.clone());
    let signature = GenericSignature::ZkLoginAuthenticatorV2(authenticator.into());

    assert_eq!(
        signature.as_ref()[0],
        SignatureScheme::ZkLoginAuthenticatorV2.flag()
    );
    assert_eq!(
        GenericSignature::from_bytes(signature.as_ref()).unwrap(),
        signature
    );
    assert_eq!(
        SuiAddress::try_from(&signature).unwrap(),
        (&v2_public_key).into()
    );
    assert_ne!(
        SuiAddress::try_from(&signature).unwrap(),
        (&v1_public_key).into()
    );
    assert_eq!(&signature.as_ref()[1..], &v1_signature.as_ref()[1..]);
    assert!(ZkLoginAuthenticator::from_bytes(signature.as_ref()).is_err());
    assert!(ZkLoginAuthenticatorV2::from_bytes(v1_signature.as_ref()).is_err());
}

#[test]
fn test_zklogin_v2_rejects_unpadded_address() {
    let (keypair, _, inputs) = load_test_vectors("./src/unit_tests/zklogin_v2_test_vectors.json")
        .into_iter()
        .next()
        .unwrap();
    let inputs =
        ZkLoginInputs::from_json(&serde_json::to_string(&inputs).unwrap(), SHORT_ADDRESS_SEED)
            .unwrap();
    let padded_address = SuiAddress::try_from_zklogin_v2(&inputs).unwrap();
    let unpadded_address = SuiAddress::try_from_unpadded(&inputs).unwrap();
    assert_ne!(padded_address, unpadded_address);

    let intent_message = IntentMessage::new(
        Intent::personal_message(),
        PersonalMessage {
            message: b"zkLogin V2 padded address".to_vec(),
        },
    );
    let signature = GenericSignature::ZkLoginAuthenticatorV2(
        ZkLoginAuthenticator::new(inputs, 10, Signature::new_secure(&intent_message, &keypair))
            .into(),
    );
    assert_eq!(SuiAddress::try_from(&signature).unwrap(), padded_address);

    let verify_params = VerifyParams::new(
        pinned_jwks(),
        vec![],
        ZkLoginEnv::Test,
        1,
        true,
        true,
        true,
        Some(30),
        true,
        true,
    );
    let error = signature
        .verify_authenticator(
            &intent_message,
            unpadded_address,
            0,
            &verify_params,
            Arc::new(VerifiedDigestCache::new_empty()),
        )
        .unwrap_err();
    assert!(matches!(error.as_inner(), SuiErrorKind::InvalidAddress));
}

#[test]
fn test_zklogin_v1_v2_cache_and_verifier_isolation() {
    let intent_message = IntentMessage::new(
        Intent::personal_message(),
        PersonalMessage {
            message: b"zkLogin circuit isolation".to_vec(),
        },
    );
    let verify_params = VerifyParams::new(
        pinned_jwks(),
        vec![],
        ZkLoginEnv::Test,
        1,
        true,
        true,
        true,
        Some(30),
        true,
        true,
    );

    let verify = |authenticator: ZkLoginAuthenticator,
                  author: SuiAddress,
                  valid_version: CircuitVersion| {
        let cache = Arc::new(VerifiedDigestCache::new_empty());
        let mut wrong_epoch_authenticator = authenticator.clone();
        *wrong_epoch_authenticator.max_epoch_mut_for_testing() = 9;
        let wrong_epoch = match valid_version {
            CircuitVersion::V1 => GenericSignature::ZkLoginAuthenticator(wrong_epoch_authenticator),
            CircuitVersion::V2 => {
                GenericSignature::ZkLoginAuthenticatorV2(wrong_epoch_authenticator.into())
            }
        };
        let v1 = GenericSignature::ZkLoginAuthenticator(authenticator.clone());
        let v2 = GenericSignature::ZkLoginAuthenticatorV2(authenticator.into());
        let (valid, wrong_version) = match valid_version {
            CircuitVersion::V1 => (v1, v2),
            CircuitVersion::V2 => (v2, v1),
        };

        let result =
            valid.verify_authenticator(&intent_message, author, 0, &verify_params, cache.clone());
        assert!(result.is_ok(), "{valid_version:?}: {result:?}");

        let result = wrong_epoch.verify_authenticator(
            &intent_message,
            author,
            0,
            &verify_params,
            cache.clone(),
        );
        assert!(
            result.is_err(),
            "{valid_version:?} proof accepted with the wrong max_epoch"
        );

        let result =
            wrong_version.verify_authenticator(&intent_message, author, 0, &verify_params, cache);
        assert!(
            result.is_err(),
            "{valid_version:?} proof accepted by other circuit"
        );
    };

    let v1_inputs =
        ZkLoginInputs::from_json(PINNED_V1_PROOF_JSON, PINNED_PROOF_ADDRESS_SEED).unwrap();
    let v1_author = SuiAddress::from(&PublicKey::from_zklogin_inputs(&v1_inputs).unwrap());
    let v1_keypair = SuiKeyPair::Ed25519(fastcrypto::ed25519::Ed25519KeyPair::generate(
        &mut StdRng::from_seed([0; 32]),
    ));
    verify(
        ZkLoginAuthenticator::new(
            v1_inputs,
            10,
            Signature::new_secure(&intent_message, &v1_keypair),
        ),
        v1_author,
        CircuitVersion::V1,
    );

    let (v2_keypair, _, v2_inputs) =
        load_test_vectors("./src/unit_tests/zklogin_v2_test_vectors.json")
            .into_iter()
            .next()
            .unwrap();
    let v2_author = SuiAddress::try_from_zklogin_v2(&v2_inputs).unwrap();
    verify(
        ZkLoginAuthenticator::new(
            v2_inputs,
            10,
            Signature::new_secure(&intent_message, &v2_keypair),
        ),
        v2_author,
        CircuitVersion::V2,
    );
}

#[test]
fn test_zklogin_v1_v2_epoch_validation() {
    let (keypair, _, inputs) = load_test_vectors("./src/unit_tests/zklogin_v2_test_vectors.json")
        .into_iter()
        .next()
        .unwrap();
    let intent_message = IntentMessage::new(
        Intent::personal_message(),
        PersonalMessage {
            message: b"zkLogin epoch validation".to_vec(),
        },
    );
    let authenticator =
        ZkLoginAuthenticator::new(inputs, 10, Signature::new_secure(&intent_message, &keypair));

    for signature in [
        GenericSignature::ZkLoginAuthenticator(authenticator.clone()),
        GenericSignature::ZkLoginAuthenticatorV2(authenticator.into()),
    ] {
        assert!(
            signature
                .verify_user_authenticator_epoch(10, Some(0))
                .is_ok()
        );
        assert!(
            signature
                .verify_user_authenticator_epoch(11, None)
                .unwrap_err()
                .to_string()
                .contains("ZKLogin expired at epoch 10")
        );
        assert!(
            signature
                .verify_user_authenticator_epoch(0, Some(9))
                .unwrap_err()
                .to_string()
                .contains("ZKLogin max epoch too large 10")
        );
    }
}

#[test]
fn test_serde_zk_public_identifier() {
    let (_, _, inputs) = &load_test_vectors("./src/unit_tests/zklogin_test_vectors.json")[0];
    let modified_inputs =
        ZkLoginInputs::from_json(&serde_json::to_string(&inputs).unwrap(), SHORT_ADDRESS_SEED)
            .unwrap();

    let mut bytes = Vec::new();
    let binding = OIDCProvider::Twitch.get_config();
    let iss_bytes = binding.iss.as_bytes();
    bytes.extend([iss_bytes.len() as u8]);
    bytes.extend(iss_bytes);
    // length here is 31 bytes and left unpadded.
    let address_seed = Bn254FrElement::from_str(SHORT_ADDRESS_SEED).unwrap();
    bytes.extend(address_seed.unpadded());

    let pk1 = PublicKey::ZkLogin(ZkLoginPublicIdentifier(bytes));
    assert_eq!(
        pk1.scheme().flag(),
        SignatureScheme::ZkLoginAuthenticator.flag()
    );
    let serialized = bcs::to_bytes(&pk1).unwrap();
    let deserialized: PublicKey = bcs::from_bytes(&serialized).unwrap();
    assert_eq!(deserialized, pk1);
    assert_eq!(
        SuiAddress::try_from_unpadded(&modified_inputs).unwrap(),
        SuiAddress::from(&pk1)
    );

    let pk2 = PublicKey::ZkLogin(
        ZkLoginPublicIdentifier::new(
            &binding.iss,
            &Bn254FrElement::from_str(SHORT_ADDRESS_SEED).unwrap(),
        )
        .unwrap(),
    );
    assert_eq!(
        pk2.scheme().flag(),
        SignatureScheme::ZkLoginAuthenticator.flag()
    );
    let serialized2 = bcs::to_bytes(&pk2).unwrap();
    let deserialized2: PublicKey = bcs::from_bytes(&serialized2).unwrap();
    assert_eq!(deserialized2, pk2);
    assert_eq!(
        SuiAddress::try_from_padded(&modified_inputs).unwrap(),
        SuiAddress::from(&pk2)
    );

    assert_eq!(serialized.len() + 1, serialized2.len());
}

#[test]
fn zklogin_sign_personal_message() {
    let data = PersonalMessage {
        message: b"hello world".to_vec(),
    };
    let (user_address, authenticator) = sign_zklogin_personal_msg(data.clone());
    let intent_msg = IntentMessage::new(Intent::personal_message(), data);
    let parsed: ImHashMap<JwkId, JWK> = parse_jwks(DEFAULT_JWK_BYTES, &OIDCProvider::Twitch, true)
        .unwrap()
        .into_iter()
        .collect();

    // Construct the required info to verify a zk login authenticator, jwks, supported providers list and env (prod/test).
    let aux_verify_data = VerifyParams::new(
        parsed,
        vec![],
        ZkLoginEnv::Test,
        0, // v1 circuit mode only
        true,
        true,
        true,
        Some(30),
        true,
        true,
    );
    let res = authenticator.verify_authenticator(
        &intent_msg,
        user_address,
        0,
        &aux_verify_data,
        Arc::new(VerifiedDigestCache::new_empty()),
    );
    // Verify passes.
    assert!(res.is_ok());
}
