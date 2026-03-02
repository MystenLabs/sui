// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::gcp_attestation_tests;

use std::string;
use sui::authenticator_state;
use sui::gcp_attestation;

// A minimal valid JWT structure with RS256 header but an invalid signature,
// producing EVerifyError.
// header:  base64url({"alg":"RS256","typ":"JWT"})
// payload: base64url({"iss":"https://confidentialcomputing.googleapis.com","exp":9999999999,"iat":1700000000})
// sig:     342 base64url chars encoding 256 zero bytes (invalid RS256 signature for any key)
const WELL_FORMED_INVALID_SIG_TOKEN: vector<u8> =
    b"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL2NvbmZpZGVudGlhbGNvbXB1dGluZy5nb29nbGVhcGlzLmNvbSIsImV4cCI6OTk5OTk5OTk5OSwiaWF0IjoxNzAwMDAwMDAwfQ.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

const GCP_ISS: vector<u8> = b"https://confidentialcomputing.googleapis.com";
const TEST_KID: vector<u8> = b"test-kid-001";

// Real GCP Confidential Spaces JWKS key in base64url format (as stored in AuthenticatorState).
// This is a 2048-bit RSA public key; using the real modulus ensures the ring size check passes
// so that the signature verification (rather than the key-size check) determines the outcome.
const TEST_JWK_N_B64: vector<u8> =
    b"sy6c7es_W1fEvt8JqYcRKIbyiRB24N6N9aHoJR5L0ooyfwOMX8pLoz4bAEg1AYM3-9u6dV0MxektPxQkLd7UR0xdJ_rlVe1YusixjeXnS8DbUq3d8sbscRsU53oqRD8rEi1YrDm9eyoqeDAnxGAdDiT9edceh8Wv-5meqbOHZcaVkcMeJ3NF4PTOrii5S5GPwBnkeL4f0rvRzfLuieZJF7jE2YFcO6xiK6P4ZRvwYDCXtaFHbBExFHhaN9DJSccYZwE6ZyAmfgacyrOGLBueMiO91PLgxAzneCW-AKAQMoHyyQcg7M40YjWunwJO8hclAKw_R9-aT2ThPpNY977KhQ";
const TEST_JWK_E_B64: vector<u8> = b"AQAB";

fun make_auth_state(ctx: &mut tx_context::TxContext): authenticator_state::AuthenticatorState {
    let mut auth_state = authenticator_state::new_for_testing(ctx);
    let jwk = authenticator_state::create_active_jwk_with_n_e(
        string::utf8(GCP_ISS),
        string::utf8(TEST_KID),
        string::utf8(b"RSA"),
        string::utf8(TEST_JWK_N_B64),
        string::utf8(TEST_JWK_E_B64),
        0,
    );
    authenticator_state::set_active_jwks_for_testing(&mut auth_state, vector[jwk]);
    auth_state
}

#[test]
#[expected_failure(abort_code = gcp_attestation::EParseError)]
fun test_gcp_attestation_invalid_token_bytes() {
    let mut ctx = tx_context::dummy();
    let mut clock = sui::clock::create_for_testing(&mut ctx);
    clock.set_for_testing(1_700_000_000_000);
    let auth_state = make_auth_state(&mut ctx);

    // Non-UTF8 bytes cannot be parsed as a JWT.
    gcp_attestation::verify_gcp_attestation(x"fffe", &auth_state, TEST_KID, &clock);

    auth_state.destroy_for_testing();
    clock.destroy_for_testing();
}

#[test]
#[expected_failure(abort_code = gcp_attestation::EParseError)]
fun test_gcp_attestation_not_three_parts() {
    let mut ctx = tx_context::dummy();
    let mut clock = sui::clock::create_for_testing(&mut ctx);
    clock.set_for_testing(1_700_000_000_000);
    let auth_state = make_auth_state(&mut ctx);

    // Valid UTF-8 but not "header.payload.signature" format.
    gcp_attestation::verify_gcp_attestation(
        b"not_a_valid_jwt_token",
        &auth_state,
        TEST_KID,
        &clock,
    );

    auth_state.destroy_for_testing();
    clock.destroy_for_testing();
}

#[test]
#[expected_failure(abort_code = gcp_attestation::EVerifyError)]
fun test_gcp_attestation_invalid_signature() {
    let mut ctx = tx_context::dummy();
    let mut clock = sui::clock::create_for_testing(&mut ctx);
    // Timestamp is before expiry (exp = 9999999999 seconds).
    clock.set_for_testing(1_700_000_000_000);
    let auth_state = make_auth_state(&mut ctx);

    gcp_attestation::verify_gcp_attestation(
        WELL_FORMED_INVALID_SIG_TOKEN,
        &auth_state,
        TEST_KID,
        &clock,
    );

    auth_state.destroy_for_testing();
    clock.destroy_for_testing();
}

#[test]
#[expected_failure(abort_code = gcp_attestation::EVerifyError)]
fun test_gcp_attestation_kid_not_found() {
    let mut ctx = tx_context::dummy();
    let mut clock = sui::clock::create_for_testing(&mut ctx);
    clock.set_for_testing(1_700_000_000_000);
    // AuthenticatorState with no JWKs — kid lookup must fail with EVerifyError.
    let auth_state = authenticator_state::new_for_testing(&mut ctx);

    gcp_attestation::verify_gcp_attestation(
        WELL_FORMED_INVALID_SIG_TOKEN,
        &auth_state,
        TEST_KID,
        &clock,
    );

    auth_state.destroy_for_testing();
    clock.destroy_for_testing();
}
