// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::gcp_attestation_tests;

use sui::gcp_attestation;

// RSA exponent e = 65537 in big-endian bytes (same for all GCP JWKS keys).
const TEST_JWK_E: vector<u8> = x"010001";

// A minimal valid JWT structure with RS256 header but an invalid signature,
// producing EVerifyError.
// header:  base64url({"alg":"RS256","typ":"JWT"})
// payload: base64url({"iss":"https://confidentialcomputing.googleapis.com","exp":9999999999,"iat":1700000000})
// sig:     342 base64url chars encoding 256 zero bytes (invalid RS256 signature for any key)
const WELL_FORMED_INVALID_SIG_TOKEN: vector<u8> =
    b"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL2NvbmZpZGVudGlhbGNvbXB1dGluZy5nb29nbGVhcGlzLmNvbSIsImV4cCI6OTk5OTk5OTk5OSwiaWF0IjoxNzAwMDAwMDAwfQ.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

// Real GCP Confidential Spaces JWKS key (kid: c6e6f04bed13a968c22fcfaaf5ef89afc5fe2333).
// Source: https://www.googleapis.com/service_accounts/v1/metadata/jwk/signer@confidentialspace-sign.iam.gserviceaccount.com
// This is a 256-byte (2048-bit) RSA modulus.
const TEST_JWK_N: vector<u8> =
    x"b32e9cedeb3f5b57c4bedf09a987112886f2891076e0de8df5a1e8251e4bd28a327f038c5fca4ba33e1b004835018337fbdbba755d0cc5e92d3f14242dded4474c5d27fae555ed58bac8b18de5e74bc0db52adddf2c6ec711b14e77a2a443f2b122d58ac39bd7b2a2a783027c4601d0e24fd79d71e87c5affb999ea9b38765c69591c31e277345e0f4ceae28b94b918fc019e478be1fd2bbd1cdf2ee89e64917b8c4d9815c3bac622ba3f8651bf0603097b5a1476c113114785a37d0c949c71867013a6720267e069ccab3862c1b9e3223bdd4f2e0c40ce77825be00a0103281f2c90720ecce346235ae9f024ef2172500ac3f47df9a4f64e13e9358f7beca85";

#[test]
#[expected_failure(abort_code = gcp_attestation::EParseError)]
fun test_gcp_attestation_invalid_token_bytes() {
    let mut ctx = tx_context::dummy();
    let mut clock = sui::clock::create_for_testing(&mut ctx);
    clock.set_for_testing(1_700_000_000_000);

    // Non-UTF8 bytes cannot be parsed as a JWT.
    let invalid_token = x"fffe";
    gcp_attestation::verify_gcp_attestation(invalid_token, TEST_JWK_N, TEST_JWK_E, &clock);

    clock.destroy_for_testing();
}

#[test]
#[expected_failure(abort_code = gcp_attestation::EParseError)]
fun test_gcp_attestation_not_three_parts() {
    let mut ctx = tx_context::dummy();
    let mut clock = sui::clock::create_for_testing(&mut ctx);
    clock.set_for_testing(1_700_000_000_000);

    // Valid UTF-8 but not "header.payload.signature" format.
    let invalid_token = b"not_a_valid_jwt_token";
    gcp_attestation::verify_gcp_attestation(invalid_token, TEST_JWK_N, TEST_JWK_E, &clock);

    clock.destroy_for_testing();
}

#[test]
#[expected_failure(abort_code = gcp_attestation::EVerifyError)]
fun test_gcp_attestation_invalid_signature() {
    let mut ctx = tx_context::dummy();
    let mut clock = sui::clock::create_for_testing(&mut ctx);
    // Timestamp is before expiry (exp = 9999999999 seconds).
    clock.set_for_testing(1_700_000_000_000);

    gcp_attestation::verify_gcp_attestation(
        WELL_FORMED_INVALID_SIG_TOKEN,
        TEST_JWK_N,
        TEST_JWK_E,
        &clock,
    );

    clock.destroy_for_testing();
}

