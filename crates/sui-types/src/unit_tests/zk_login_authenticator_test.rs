// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use std::sync::Arc;

use crate::crypto::{PublicKey, Signature, SignatureScheme, ZkLoginPublicIdentifier};
use crate::multisig::MultiSig;
use crate::multisig::MultiSigPublicKey;
use crate::signature::VerifyParams;
use crate::signature_verification::VerifiedDigestCache;
use crate::utils::PINNED_V2_JWK_ID_JSON;
use crate::utils::load_zklogin_v2_test_vector;
use crate::utils::{SHORT_ADDRESS_SEED, load_test_vectors};
use crate::utils::{get_zklogin_user_address, make_zklogin_tx, sign_zklogin_personal_msg};
use crate::zk_login_authenticator::ZkLoginAuthenticator;
use crate::zk_login_authenticator::ZkLoginAuthenticatorV2;
use crate::{
    base_types::SuiAddress, signature::GenericSignature, zk_login_util::DEFAULT_JWK_BYTES,
};
use fastcrypto::encoding::{Base64, Encoding};
use fastcrypto::traits::ToFromBytes;

use fastcrypto_zkp::bn254::zk_login::{JWK, JwkId, OIDCProvider, ZkLoginInputs, parse_jwks};
use fastcrypto_zkp::bn254::zk_login_api::ZkLoginEnv;
use fastcrypto_zkp::zk_login_utils::Bn254FrElement;
use imbl::hashmap::HashMap as ImHashMap;
use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};

#[test]
fn test_serde_zk_login_signature() {
    // consistency test with typescript: sdk/typescript/test/unit/zklogin/signature.test.ts
    let (user_address, _tx, authenticator) = make_zklogin_tx(get_zklogin_user_address(), false);
    let serialized = authenticator.as_ref();
    assert_eq!(serialized, &Base64::decode("BQNNMTczMTgwODkxMjU5NTI0MjE3MzYzNDIyNjM3MTc5MzI3MTk0Mzc3MTc4NDQyODI0MTAxODc5NTc5ODQ3NTE5Mzk5NDI4OTgyNTEyNTBNMTEzNzM5NjY2NDU0NjkxMjI1ODIwNzQwODIyOTU5ODUzODgyNTg4NDA2ODE2MTgyNjg1OTM5NzY2OTczMjU4OTIyODA5MTU2ODEyMDcBMQMCTDU5Mzk4NzExNDczNDg4MzQ5OTczNjE3MjAxMjIyMzg5ODAxNzcxNTIzMDMyNzQzMTEwNDcyNDk5MDU5NDIzODQ5MTU3Njg2OTA4OTVMNDUzMzU2ODI3MTEzNDc4NTI3ODczMTIzNDU3MDM2MTQ4MjY1MTk5Njc0MDc5MTg4ODI4NTg2NDk2Njg4NDAzMjcxNzA0OTgxMTcwOAJNMTA1NjQzODcyODUwNzE1NTU0Njk3NTM5OTA2NjE0MTA4NDAxMTg2MzU5MjU0NjY1OTcwMzcwMTgwNTg3NzAwNDEzNDc1MTg0NjEzNjhNMTI1OTczMjM1NDcyNzc1NzkxNDQ2OTg0OTYzNzIyNDI2MTUzNjgwODU4MDEzMTMzNDMxNTU3MzU1MTEzMzAwMDM4ODQ3Njc5NTc4NTQCATEBMANNMTU3OTE1ODk0NzI1NTY4MjYyNjMyMzE2NDQ3Mjg4NzMzMzc2MjkwMTUyNjk5ODQ2OTk0MDQwNzM2MjM2MDMzNTI1Mzc2Nzg4MTMxNzFMNDU0Nzg2NjQ5OTI0ODg4MTQ0OTY3NjE2MTE1ODAyNDc0ODA2MDQ4NTM3MzI1MDAyOTQyMzkwNDExMzAxNzQyMjUzOTAzNzE2MjUyNwExMXdpYVhOeklqb2lhSFIwY0hNNkx5OXBaQzUwZDJsMFkyZ3VkSFl2YjJGMWRHZ3lJaXcCMmV5SmhiR2NpT2lKU1V6STFOaUlzSW5SNWNDSTZJa3BYVkNJc0ltdHBaQ0k2SWpFaWZRTTIwNzk0Nzg4NTU5NjIwNjY5NTk2MjA2NDU3MDIyOTY2MTc2OTg2Njg4NzI3ODc2MTI4MjIzNjI4MTEzOTE2MzgwOTI3NTAyNzM3OTExCgAAAAAAAABhAG6Bf8BLuaIEgvF8Lx2jVoRWKKRIlaLlEJxgvqwq5nDX+rvzJxYAUFd7KeQBd9upNx+CHpmINkfgj26jcHbbqAy5xu4WMO8+cRFEpkjbBruyKE9ydM++5T/87lA8waSSAA==").unwrap());
    let deserialized = GenericSignature::from_bytes(serialized).unwrap();
    assert_eq!(deserialized, authenticator);

    let GenericSignature::ZkLoginAuthenticator(original) = &authenticator else {
        panic!("expected a v1 zkLogin authenticator");
    };
    let decoded: ZkLoginAuthenticator = bcs::from_bytes(&serialized[1..]).unwrap();
    assert_eq!(decoded.get_iss(), original.get_iss());
    assert_eq!(decoded.inputs.get_kid(), original.inputs.get_kid());

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

fn single_member_multisig(signature: &GenericSignature) -> (SuiAddress, GenericSignature) {
    let key = signature.to_public_key().unwrap();
    let committee = MultiSigPublicKey::new(vec![key], vec![1], 1).unwrap();
    let address = SuiAddress::from(&committee);
    let multisig = MultiSig::combine(vec![signature.clone()], committee).unwrap();
    (address, multisig.into())
}

/// Checks each zkLogin version as a standalone signature and in a single-member multisig.
///
/// | Case | Circuit mode | zkLogin version | Standalone signature | Single-member multisig |
/// | --- | --- | --- | --- | --- |
/// | 1 | `0` (v1 only) | v1 | Pass | Pass |
/// | 2 | `0` (v1 only) | v2 | Reject | Reject |
/// | 3 | `1` (v1 and v2) | v1 | Pass | Pass |
/// | 4 | `1` (v1 and v2) | v2 | Pass | Pass |
#[test]
fn zklogin_sign_personal_message_v1_v2_scenarios() {
    let message = PersonalMessage {
        message: b"hello world".to_vec(),
    };
    let (v1_address, v1) = sign_zklogin_personal_msg(message.clone());
    let intent = IntentMessage::new(Intent::personal_message(), message);
    let (signer, public_key, inputs, jwk, max_epoch) = load_zklogin_v2_test_vector();
    let v2_address = SuiAddress::from(&public_key);
    let v2 = GenericSignature::from(
        ZkLoginAuthenticatorV2::new(inputs, max_epoch, Signature::new_secure(&intent, &signer))
            .unwrap(),
    );

    let mut jwks: ImHashMap<JwkId, JWK> =
        parse_jwks(DEFAULT_JWK_BYTES, &OIDCProvider::Twitch, true)
            .unwrap()
            .into_iter()
            .collect();
    jwks.insert(serde_json::from_str(PINNED_V2_JWK_ID_JSON).unwrap(), jwk);
    let mut params = VerifyParams::new(
        jwks,
        vec![],
        ZkLoginEnv::Test,
        0,
        true,
        true,
        true,
        Some(30),
        true,
        true,
    );

    for (mode, version, address, signature, should_pass) in [
        // 1. Mode 0 accepts a v1 signature.
        (0, "v1", v1_address, &v1, true),
        // 2. Mode 0 rejects a v2 signature.
        (0, "v2", v2_address, &v2, false),
        // 3. Mode 1 accepts a v1 signature.
        (1, "v1", v1_address, &v1, true),
        // 4. Mode 1 accepts a v2 signature.
        (1, "v2", v2_address, &v2, true),
    ] {
        params.zklogin_circuit_mode = mode;
        let (multisig_address, multisig) = single_member_multisig(signature);

        for (kind, address, signature, mode_0_v2_error) in [
            (
                "standalone",
                address,
                signature,
                "zkLogin v2 is not enabled",
            ),
            (
                "multisig",
                multisig_address,
                &multisig,
                "zkLogin v2 member not supported inside multisig",
            ),
        ] {
            let result = signature.verify_authenticator(
                &intent,
                address,
                0,
                &params,
                Arc::new(VerifiedDigestCache::new_empty()),
            );
            assert_eq!(
                result.is_ok(),
                should_pass,
                "mode {mode}, {version} {kind}: {result:?}",
            );
            if let Err(error) = result {
                assert!(error.to_string().contains(mode_0_v2_error), "{error}");
            }
        }
    }
}
