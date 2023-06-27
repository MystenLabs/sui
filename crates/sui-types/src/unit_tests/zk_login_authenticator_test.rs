// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use crate::signature::{AuthenticatorTrait, AuxVerifyData};
use crate::utils::{make_transaction, make_zklogin_tx};
use crate::{
    base_types::SuiAddress,
    crypto::{get_key_pair_from_rng, DefaultHash, SignatureScheme, SuiKeyPair},
    signature::GenericSignature,
    zk_login_authenticator::{AddressParams, PublicInputs, ZkLoginAuthenticator, ZkLoginProof},
};
use fastcrypto::encoding::{Base64, Encoding};
use fastcrypto::hash::HashFunction;
use fastcrypto::traits::ToFromBytes;
use fastcrypto_zkp::bn254::zk_login::{
    big_int_str_to_bytes, AuxInputs, OAuthProvider, SupportedKeyClaim,
};
use rand::{rngs::StdRng, SeedableRng};
use shared_crypto::intent::{Intent, IntentMessage};

pub const TEST_JWK_BYTES: &[u8] = r#"{
    "keys": [
        {
          "kty": "RSA",
          "e": "AQAB",
          "alg": "RS256",
          "kid": "2d9a5ef5b12623c91671a7093cb323333cd07d09",
          "use": "sig",
          "n": "0NDRXWtH6_HnmuSuTAisgYVZ3Z67PQjHbRFz4XNYuD95BKx0wQr0GWOi_UCGLfI0col3i6J3_AF-b1YrTFTMEr_bL8CYDdK2CYLcGUzc5bLRDAySsqnKdlhWkneqfFdr3J66mHu11KUaIIRWiLsCkR9QFF-8o2PtZzv3F-3Uh7L4q7i_Evs1s7SJlO0OAnI4ew4rP2HbRaO0Q2zK0DL_d1eoAC72apQuEzz-2aXfQ-QYSTlVK74McBhP1MRtgD6zGF2lwg4uhgb55fDDQQh0VHWQSxwbvAL0Oox69zzpkFgpjJAJUqaxegzETU1jf3iKs1vyFIB0C4N-Jr__zwLQZw=="
        },
        {
          "alg": "RS256",
          "use": "sig",
          "n": "1qrQCTst3RF04aMC9Ye_kGbsE0sftL4FOtB_WrzBDOFdrfVwLfflQuPX5kJ-0iYv9r2mjD5YIDy8b-iJKwevb69ISeoOrmL3tj6MStJesbbRRLVyFIm_6L7alHhZVyqHQtMKX7IaNndrfebnLReGntuNk76XCFxBBnRaIzAWnzr3WN4UPBt84A0KF74pei17dlqHZJ2HB2CsYbE9Ort8m7Vf6hwxYzFtCvMCnZil0fCtk2OQ73l6egcvYO65DkAJibFsC9xAgZaF-9GYRlSjMPd0SMQ8yU9i3W7beT00Xw6C0FYA9JAYaGaOvbT87l_6ZkAksOMuvIPD_jNVfTCPLQ==",
          "e": "AQAB",
          "kty": "RSA",
          "kid": "6083dd5981673f661fde9dae646b6f0380a0145c"
        }
      ]
  }"#.as_bytes();

#[test]
fn zklogin_authenticator_scenarios() {
    let (user_address, tx, authenticator) = make_zklogin_tx();

    let intent_msg = IntentMessage::new(
        Intent::sui_transaction(),
        tx.into_data().transaction_data().clone(),
    );

    // Construct the required info required to verify a zk login authenticator
    // in authority server (i.e. epoch and default JWK).
    let aux_verify_data = AuxVerifyData::new(Some(0), Some(TEST_JWK_BYTES.to_vec()));

    // Verify passes.
    assert!(authenticator
        .verify_secure_generic(&intent_msg, user_address, aux_verify_data)
        .is_ok());
    // Malformed JWK in aux verify data.
    let aux_verify_data = AuxVerifyData::new(Some(9999), Some(vec![0, 0, 0]));

    // Verify fails.
    assert!(authenticator
        .verify_secure_generic(&intent_msg, user_address, aux_verify_data)
        .is_err());
}

#[test]
fn test_serde_zk_login_signature() {
    let user_key: SuiKeyPair =
        SuiKeyPair::Ed25519(get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1);

    let proof = ZkLoginProof::from_json("{\"pi_a\":[\"21079899190337156604543197959052999786745784780153100922098887555507822163222\",\"4490261504756339299022091724663793329121338007571218596828748539529998991610\",\"1\"],\"pi_b\":[[\"9379167206161123715528853149920855132656754699464636503784643891913740439869\",\"15902897771112804794883785114808675393618430194414793328415185511364403970347\"],[\"16152736996630746506267683507223054358516992879195296708243566008238438281201\",\"15230917601041350929970534508991793588662911174494137634522926575255163535339\"],[\"1\",\"0\"]],\"pi_c\":[\"8242734018052567627683363270753907648903210541694662698981939667442011573249\",\"1775496841914332445297048246214170486364407018954976081505164205395286250461\",\"1\"],\"protocol\":\"groth16\"}").unwrap();
    let aux_inputs = AuxInputs::from_json("{ \"claims\": { \"iss\": { \"value_base64\": \"yJpc3MiOiJodHRwczovL2FjY291bnRzLmdvb2dsZS5jb20iLC\", \"index_mod_4\": 1 }, \"aud\": { \"value_base64\": \"CJhdWQiOiI1NzU1MTkyMDQyMzctbXNvcDllcDQ1dTJ1bzk4aGFwcW1uZ3Y4ZDg0cWRjOGsuYXBwcy5nb29nbGV1c2VyY29udGVudC5jb20iLC\", \"index_mod_4\": 1 } }, \"header_base64\": \"eyJhbGciOiJSUzI1NiIsImtpZCI6ImM5YWZkYTM2ODJlYmYwOWViMzA1NWMxYzRiZDM5Yjc1MWZiZjgxOTUiLCJ0eXAiOiJKV1QifQ\", \"addr_seed\": \"15604334753912523265015800787270404628529489918817818174033741053550755333691\", \"eph_public_key\": [ \"17932473587154777519561053972421347139\", \"134696963602902907403122104327765350261\" ], \"max_epoch\": 10000, \"key_claim_name\": \"sub\", \"modulus\": \"24501106890748714737552440981790137484213218436093327306276573863830528169633224698737117584784274166505493525052788880030500250025091662388617070057693555892212025614452197230081503494967494355047321073341455279175776092624566907541405624967595499832566905567072654796017464431878680118805774542185299632150122052530877100261682728356139724202453050155758294697161107805717430444408191365063957162605112787073991150691398970840390185880092832325216009234084152827135531285878617366639283552856146367480314853517993661640450694829038343380576312039548353544096265483699391507882147093626719041048048921352351403884619\" }").unwrap();
    let public_inputs = PublicInputs::from_json(
        "[\"6049184272607241856912886413680599526372437331989542437266935645748489874658\"]",
    )
    .unwrap();

    let mut hasher = DefaultHash::default();
    hasher.update([SignatureScheme::ZkLoginAuthenticator.flag()]);
    let address_params = AddressParams::new(
        OAuthProvider::Google.get_config().0.to_owned(),
        SupportedKeyClaim::Sub.to_string(),
    );
    hasher.update(bcs::to_bytes(&address_params).unwrap());
    hasher.update(big_int_str_to_bytes(aux_inputs.get_address_seed()));
    let user_address = SuiAddress::from_bytes(hasher.finalize().digest).unwrap();

    // Sign the user transaction with the user's ephemeral key.
    let tx = make_transaction(user_address, &user_key, Intent::sui_transaction());
    let s = match tx.inner().tx_signatures.first().unwrap() {
        GenericSignature::Signature(s) => s,
        _ => panic!("Expected a signature"),
    };

    // Construct the authenticator with all user submitted components.
    let authenticator = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
        proof,
        public_inputs,
        aux_inputs,
        s.clone(),
    ));

    let serialized = authenticator.as_ref();
    println!("serialized: {:?}", serialized);
    let deserialized = GenericSignature::from_bytes(serialized).unwrap();
    assert_eq!(deserialized, authenticator);

    let sig = GenericSignature::from_bytes(&Base64::decode("BQNNMjA0ODE2ODc4ODk1NzQ2NDg1NjY0MjgwMTk4NTQxMDc0MjI2NjYyNTE1NjM2MDA3OTA1MDY2ODU2OTQ5MTEyNTk4MjMzNTcyMTAyMzNNMTEwMzM3NjUyNTU1NjExOTEyMTQ5NDgwNjAyNDUzOTQ3Nzc3NjQ1MDYxMjQ3Njk1MDk1Nzg4NDc2Nzg4NTgyNTQxOTg1ODcxOTU5ODgBMQMCTTEzODQ5OTAzNzExNTYxMzEzODI5OTM0OTE3OTY5NjUyMDkyODM4OTc4MTU2NzY5NDA0Njk5NTExMjk5MDg5MzEyMTYwNjYzNDk4MDM2TTE3MTY1MDgzOTQyNDQ5MjU0MTk5MjgzMTY5MDU4NTU0MjExMTkwMTAzMjQyNjg3MjEzMzY0NzM5MDYwNTIxMzk1ODE1NjE1MzIxMTYzAk0xMzkxMTE5MzgwODEzMzI3MTk5MTgyMjk5NDYwOTEyMDQzMTQ4NTM2NTMyNjEyODI2NTg4NTc1Njk4MzQxMTYyNDIzMDM1MTk4NTM2Nk0xODI5NDUwMzI3NzU4ODgwODYzMjMyNzIxNTYxMTg0MDI0MjMxOTQxNTAyNjIyNDQzODE5NTIzOTgxNzgyODg2NTMxMzkxNjc5ODkwMAIBMQEwA0wyNzQ1MTA0NTk0MTg1NjU0Mjg2ODIzMjIyNDYyNDQ4Mzc3OTAyNTkyMzkzNDY3NTE2ODUzMzg3MzUxNjE4NTkxMDM0NTI0MDgwMjUwTDYwNjEyODQ3NDIwMDA5MDA5NDIzMzA2MTAxNTU1NzQ5MTk5OTk5NTUxODI1NjQ4ODMxMDUwODE3MjEwNTc2MjI5Mjg3NzExNzY3MjQBMQdncm90aDE2AU0xNzk0MzY2NzcyOTYxNzYzNzQ0MDUwOTA2MjcyMTYwNTE2NDI3OTQ3OTYxMzI0MjgzNDMwOTEwOTc0NzQ5OTUyOTYzOTY2NjE3NjM5Nk0xNTk4MTg1NzUzNzkxNDAwMzE4OTg4Nzg2MDExODM2MzIzMzU3MTU3NTIxMDA0NjMwNzU5MjM4NjYwODQ0NDgxMzU0OTY2Mzc2MTkyNwInMTY2OTY1MDA0OTAwMDAxOTY5Mjg4OTM1NzU3MzIwMzQ0Nzg5MjAzJzMwNTA5MTE5MTEzODgyMzM1MjU1MTQ2MzEzMTk3OTAyNDQ5NzM2MAInMjEyNDg4MjE2MTkzNzM4NDA0Njk4NzMxNzA1OTc2NzY4ODc2MjI2JzI2NTY4NDg5MzExNTIyMjg5MTM4NDc5ODY4MzUwMDM2NzY0NzgwOdYCYmZISGx2STFXbVUtV2Z2UTlXZExRVGJFUlhJbFZVWW45MmhjTWxQNW1NaHNvNGw1U1Z3aDI5RFJRdV9DSjBDbm1pMWdTZFV0em4xS0Z1MWMyZUZhRlZXNUFwYkdXZEF3azdlOXBPcGkzQW9JZF9XOUdOOE5uanA5RW9nc3FURlF3SFl6MlBiejdLV1lOSUdMd3ExS1pBc2swY2l4QTNQd3RKUFpIbkI0dFhGOUJHQVM0WFdEcEs0aGpxR0NKWWlRVjFsYWJuY2VkYmp4YnhsN2owSnRFRS1KZ3hBNHltZ3hfT1I0YXpLSWpSUG9HZlQtdWZJaEVPS0FEN3BfYXVXNkNORS04djhWRVgyMFJFV05WM2l3MldZVHFwQ0dScVZUbDkycHNseFJkVTJlRWNDYS1YMFA2X3h0dkZaVnMzQXdjUmpiZVBTMWl2cC1NRnFybmhteFN3A3N1YoAGZXlKaGJHY2lPaUpTVXpJMU5pSXNJbXRwWkNJNklqWXdPRE5rWkRVNU9ERTJOek5tTmpZeFptUmxPV1JoWlRZME5tSTJaakF6T0RCaE1ERTBOV01pTENKMGVYQWlPaUpLVjFRaWZRLj15SnBjM01pT2lKb2RIUndjem92TDJGalkyOTFiblJ6TG1kdmIyZHNaUzVqYjIwaUxDPT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT1DSmhkV1FpT2lJNU5EWTNNekV6TlRJeU56WXRjR3MxWjJ4alp6aGpjVzh6T0c1a1lqTTVhRGRxTURrelpuQnpjR2gxYzNVdVlYQndjeTVuYjI5bmJHVjFjMlZ5WTI5dWRHVnVkQzVqYjIwaUxDPT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT09gAAAAAAAAAAAFagAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAxQAAAAAAAAALTgJnAGEAuMkTd7vlBccNSDSMWlzC59+JG5NOborYmJVW9Ta5WAR8IXunloWEiaXlwx04taR9x6P/W/lXIiJlu8q0oCg0DH2cSk59huVthLdyL3A/YNPlhm0Td7i/8cGw3f2RHPrQ").unwrap()).unwrap();
    let user_address: SuiAddress = (&sig).try_into().unwrap();
    assert_eq!(
        user_address,
        SuiAddress::from_str("0xd5cb3a8f365cf9e0486df1675538fa7b95875e1ef3ebbe1d392e163f0aa12a11")
            .unwrap()
    );
}
