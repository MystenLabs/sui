// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::SuiError;
use crate::signature::{AuthenticatorTrait, VerifyParams};
use crate::utils::{
    get_legacy_zklogin_user_address, get_zklogin_user_address, make_zklogin_tx,
    sign_zklogin_personal_msg,
};
use crate::{
    base_types::SuiAddress, signature::GenericSignature, zk_login_util::DEFAULT_JWK_BYTES,
};
use fastcrypto::encoding::Base64;
use fastcrypto::traits::ToFromBytes;
use fastcrypto_zkp::bn254::zk_login::{parse_jwks, JwkId, OIDCProvider, JWK};
use fastcrypto_zkp::bn254::zk_login_api::ZkLoginEnv;
use im::hashmap::HashMap as ImHashMap;
use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};

#[test]
fn zklogin_authenticator_jwk() {
    let (user_address, tx, authenticator) = make_zklogin_tx(get_zklogin_user_address(), false);
    let intent_msg = IntentMessage::new(
        Intent::sui_transaction(),
        tx.into_data().transaction_data().clone(),
    );

    // Create sui address derived with a legacy way.
    let (legacy_user_address, legacy_tx, legacy_authenticator) =
        make_zklogin_tx(get_legacy_zklogin_user_address(), true);
    let legacy_intent_msg = IntentMessage::new(
        Intent::sui_transaction(),
        legacy_tx.into_data().transaction_data().clone(),
    );

    let parsed: ImHashMap<JwkId, JWK> = parse_jwks(DEFAULT_JWK_BYTES, &OIDCProvider::Twitch)
        .unwrap()
        .into_iter()
        .collect();

    // Construct the required info to verify a zk login authenticator, jwks, supported providers list and env (prod/test).
    let aux_verify_data = VerifyParams::new(parsed.clone(), vec![], ZkLoginEnv::Test, true, true);

    let res =
        authenticator.verify_authenticator(&intent_msg, user_address, Some(0), &aux_verify_data);
    // Verify passes.
    assert!(res.is_ok());

    let res = legacy_authenticator.verify_authenticator(
        &legacy_intent_msg,
        legacy_user_address,
        Some(0),
        &aux_verify_data,
    );
    // Verify passes for legacy address derivation.
    assert_eq!(
        res.unwrap_err(),
        SuiError::InvalidSignature {
            error: "General cryptographic error: Groth16 proof verify failed".to_string()
        }
    );

    let aux_verify_data =
        VerifyParams::new(Default::default(), vec![], ZkLoginEnv::Test, true, true);
    let res =
        authenticator.verify_authenticator(&intent_msg, user_address, Some(0), &aux_verify_data);
    assert!(res.is_err());

    // Epoch expired fails to verify.
    let aux_verify_data = VerifyParams::new(parsed.clone(), vec![], ZkLoginEnv::Test, true, true);
    assert!(authenticator
        .verify_authenticator(&intent_msg, user_address, Some(11), &aux_verify_data)
        .is_err());
    let parsed: ImHashMap<JwkId, JWK> = parsed
        .into_iter()
        .map(|(jwk_id, v)| {
            (
                JwkId::new(format!("nosuchkey_{}", jwk_id.iss), jwk_id.kid),
                v,
            )
        })
        .collect();

    // Correct kid can no longer be found fails to verify.
    let aux_verify_data = VerifyParams::new(parsed, vec![], ZkLoginEnv::Test, true, true);
    assert!(authenticator
        .verify_authenticator(&intent_msg, user_address, Some(0), &aux_verify_data)
        .is_err());
}

#[test]
fn test_serde_zk_login_signature() {
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
    let aux_verify_data = VerifyParams::new(parsed, vec![], ZkLoginEnv::Test, true, true);
    let res =
        authenticator.verify_authenticator(&intent_msg, user_address, Some(0), &aux_verify_data);
    // Verify passes.
    assert!(res.is_ok());
}
