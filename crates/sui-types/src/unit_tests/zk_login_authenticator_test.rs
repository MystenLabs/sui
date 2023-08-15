// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::signature::{AuthenticatorTrait, VerifyParams};
use crate::utils::make_zklogin_tx;
use crate::{
    base_types::SuiAddress, signature::GenericSignature, zk_login_util::DEFAULT_JWK_BYTES,
};
use fastcrypto::traits::ToFromBytes;
use fastcrypto_zkp::bn254::zk_login::{parse_jwks, OIDCProvider, JWK};
use shared_crypto::intent::{Intent, IntentMessage};
use std::collections::HashMap;

#[test]
fn zklogin_authenticator_jwk() {
    let (user_address, tx, authenticator) = make_zklogin_tx();

    let intent_msg = IntentMessage::new(
        Intent::sui_transaction(),
        tx.into_data().transaction_data().clone(),
    );

    let parsed: HashMap<(String, String), JWK> =
        parse_jwks(DEFAULT_JWK_BYTES, OIDCProvider::Twitch)
            .unwrap()
            .into_iter()
            .collect();

    // Construct the required info required to verify a zk login authenticator
    // in authority server (i.e. epoch and default JWK).
    let aux_verify_data = VerifyParams::new(parsed.clone());

    let res =
        authenticator.verify_authenticator(&intent_msg, user_address, Some(0), &aux_verify_data);
    // Verify passes.
    assert!(res.is_ok());

    let parsed: HashMap<(String, String), JWK> = parsed
        .into_iter()
        .enumerate()
        .map(|(i, ((_, _), v))| ((format!("nosuchkey_{}", i), "".to_string()), v))
        .collect();

    // correct kid can no longer be found
    let aux_verify_data = VerifyParams::new(parsed);

    // Verify fails.
    assert!(authenticator
        .verify_authenticator(&intent_msg, user_address, Some(9999), &aux_verify_data)
        .is_err());
}

#[test]
fn test_serde_zk_login_signature() {
    let (user_address, _tx, authenticator) = make_zklogin_tx();
    let serialized = authenticator.as_ref();
    let deserialized = GenericSignature::from_bytes(serialized).unwrap();
    assert_eq!(deserialized, authenticator);

    let addr: SuiAddress = (&authenticator).try_into().unwrap();
    assert_eq!(addr, user_address);
}
