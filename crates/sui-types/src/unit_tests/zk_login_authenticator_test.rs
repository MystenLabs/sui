// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use std::sync::Arc;

use crate::crypto::{PublicKey, SignatureScheme, ZkLoginPublicIdentifier};

use crate::signature::VerifyParams;
use crate::signature_verification::VerifiedDigestCache;
use crate::utils::{get_zklogin_user_address, make_zklogin_tx, sign_zklogin_personal_msg};
use crate::utils::{load_test_vectors, SHORT_ADDRESS_SEED};
use crate::{
    base_types::SuiAddress, signature::GenericSignature, zk_login_util::DEFAULT_JWK_BYTES,
};
use fastcrypto::encoding::Base64;
use fastcrypto::traits::ToFromBytes;

use fastcrypto_zkp::bn254::zk_login::{parse_jwks, JwkId, OIDCProvider, ZkLoginInputs, JWK};
use fastcrypto_zkp::bn254::zk_login_api::ZkLoginEnv;
use fastcrypto_zkp::zk_login_utils::Bn254FrElement;
use im::hashmap::HashMap as ImHashMap;
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
    let parsed: ImHashMap<JwkId, JWK> = parse_jwks(DEFAULT_JWK_BYTES, &OIDCProvider::Twitch)
        .unwrap()
        .into_iter()
        .collect();

    // Construct the required info to verify a zk login authenticator, jwks, supported providers list and env (prod/test).
    let aux_verify_data = VerifyParams::new(parsed, vec![], ZkLoginEnv::Test, true, true, Some(30));
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
