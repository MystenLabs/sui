// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use crate::signature::{AuthenticatorTrait, VerifyParams};
use crate::utils::{make_transaction, make_zklogin_tx};
use crate::{
    base_types::SuiAddress,
    crypto::{get_key_pair_from_rng, DefaultHash, SignatureScheme, SuiKeyPair},
    signature::GenericSignature,
    zk_login_authenticator::{AddressParams, PublicInputs, ZkLoginAuthenticator, ZkLoginProof},
    zk_login_util::{parse_jwks, OAuthProviderContent, DEFAULT_JWK_BYTES},
};
use fastcrypto::encoding::{Base64, Encoding};
use fastcrypto::hash::HashFunction;
use fastcrypto::traits::ToFromBytes;
use fastcrypto_zkp::bn254::zk_login::{
    big_int_str_to_bytes, AuxInputs, OAuthProvider, SupportedKeyClaim,
};
use rand::{rngs::StdRng, SeedableRng};
use shared_crypto::intent::{Intent, IntentMessage};

#[test]
fn zklogin_authenticator_scenarios() {
    use im::hashmap::HashMap as ImHashMap;
    let (user_address, tx, authenticator) = make_zklogin_tx();

    let intent_msg = IntentMessage::new(
        Intent::sui_transaction(),
        tx.into_data().transaction_data().clone(),
    );

    let parsed: ImHashMap<String, OAuthProviderContent> =
        parse_jwks(DEFAULT_JWK_BYTES).unwrap().into_iter().collect();

    // Construct the required info required to verify a zk login authenticator
    // in authority server (i.e. epoch and default JWK).
    let aux_verify_data = VerifyParams::new(parsed.clone());

    // Verify passes.
    assert!(authenticator
        .verify_authenticator(&intent_msg, user_address, Some(0), &aux_verify_data)
        .is_ok());

    let parsed: ImHashMap<String, OAuthProviderContent> = parsed
        .into_iter()
        .enumerate()
        .map(|(i, (_, v))| (format!("nosuchkey_{}", i), v))
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
    let user_key: SuiKeyPair =
        SuiKeyPair::Ed25519(get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1);

    let proof = ZkLoginProof::from_json("{\"pi_a\":[\"14773032106069856668972396672350797076055861237383576052493955423994116090912\",\"10256481106886434602492891208282170223983625600287748851653074125391248309014\",\"1\"],\"pi_b\":[[\"11279307737533893899063186106984511177705318223976679158125470783510400431153\",\"10750728891610181825635823254337402092259745577964933136615591108170079085462\"],[\"5103373414836661910221794877118021615720837981629628049907178132389261290989\",\"14681020929085169399218842877266984710651553119645243809601357658254293671214\"],[\"1\",\"0\"]],\"pi_c\":[\"9289872310426503554963895690721473724352074264768900407255162398689856598144\",\"11987870476480102771637928245246115838352121746691800889675428205478757122604\",\"1\"],\"protocol\":\"groth16\"}").unwrap();
    let aux_inputs = AuxInputs::from_json("{\"addr_seed\":\"15604334753912523265015800787270404628529489918817818174033741053550755333691\",\"eph_public_key\":[\"17932473587154777519561053972421347139\",\"134696963602902907403122104327765350261\"],\"jwt_sha2_hash\":[\"148180057555400281455088754634676539629\",\"30382506788089954741979818497634535051\"],\"jwt_signature\":\"V9T2LoplvaMzyGFhOVxlUEIrp8iWIqPNFfE_h8GQKxutQc_MDY5KsKpGsTmfp9Qec1r6ojWDJplu6jw95BEHGJ-uyUlioUJyRR3v7GF5FkqD-eKkKSoFFWL34_mBjZV5vz2TmyY6ZjrQHyGIXH3X228wjO_8hDnAV1Hb8r1bUAKc0GA2CnWaDgP8SkL8-IbhE9KUeOWeOAAIoYbcBDf5QoyVLgMiB7f8-k7Phd_cTMDTco0rnOObh8BKFsud21ia5L6gA_OxjBHD3F0Xo36GGRd49i6XeXD5BXENJTCuHTVE42T8GsgwsslPaV8e1itfhVGOvB9aK5kmCXMmfgAr9A\",\"key_claim_name\":\"sub\",\"masked_content\":[101,121,74,104,98,71,99,105,79,105,74,83,85,122,73,49,78,105,73,115,73,109,116,112,90,67,73,54,73,106,103,121,77,106,103,122,79,71,77,120,89,122,104,105,90,106,108,108,90,71,78,109,77,87,89,49,77,68,85,119,78,106,89,121,90,84,85,48,89,109,78,105,77,87,70,107,89,106,86,105,78,87,89,105,76,67,74,48,101,88,65,105,79,105,74,75,86,49,81,105,102,81,46,61,121,74,112,99,51,77,105,79,105,74,111,100,72,82,119,99,122,111,118,76,50,70,106,89,50,57,49,98,110,82,122,76,109,100,118,98,50,100,115,90,83,53,106,98,50,48,105,76,67,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,67,74,104,100,87,81,105,79,105,73,49,78,122,85,49,77,84,107,121,77,68,81,121,77,122,99,116,98,88,78,118,99,68,108,108,99,68,81,49,100,84,74,49,98,122,107,52,97,71,70,119,99,87,49,117,90,51,89,52,90,68,103,48,99,87,82,106,79,71,115,117,89,88,66,119,99,121,53,110,98,50,57,110,98,71,86,49,99,50,86,121,89,50,57,117,100,71,86,117,100,67,53,106,98,50,48,105,76,67,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,61,128,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,18,120,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],\"max_epoch\":10000,\"num_sha2_blocks\":10,\"payload_len\":488,\"payload_start_index\":103}").unwrap();
    let public_inputs = PublicInputs::from_json(
        "[\"17502604849978740802263989016661989438644629010996707199169450653073498463837\"]",
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
