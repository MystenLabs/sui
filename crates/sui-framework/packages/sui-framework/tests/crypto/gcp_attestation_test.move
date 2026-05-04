// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::gcp_attestation_tests;

use sui::gcp_attestation;

// A minimal valid JWT structure with RS256 header but an invalid signature,
// producing EVerifyError.
// header:  base64url({"alg":"RS256","typ":"JWT"})
// payload: base64url({"iss":"https://confidentialcomputing.googleapis.com","exp":9999999999,"iat":1700000000})
// sig:     342 base64url chars encoding 256 zero bytes (invalid RS256 signature for any key)
const WELL_FORMED_INVALID_SIG_TOKEN: vector<u8> =
    b"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL2NvbmZpZGVudGlhbGNvbXB1dGluZy5nb29nbGVhcGlzLmNvbSIsImV4cCI6OTk5OTk5OTk5OSwiaWF0IjoxNzAwMDAwMDAwfQ.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

const TEST_KID: vector<u8> = b"test-kid-001";

#[test]
#[expected_failure(abort_code = gcp_attestation::EParseError)]
fun test_gcp_attestation_invalid_token_bytes() {
    let mut ctx = tx_context::dummy();
    let mut clock = sui::clock::create_for_testing(&mut ctx);
    clock.set_for_testing(1_700_000_000_000);

    // Non-UTF8 bytes cannot be parsed as a JWT.
    gcp_attestation::verify_gcp_attestation(x"fffe", TEST_KID, &clock);

    clock.destroy_for_testing();
}

#[test]
#[expected_failure(abort_code = gcp_attestation::EParseError)]
fun test_gcp_attestation_not_three_parts() {
    let mut ctx = tx_context::dummy();
    let mut clock = sui::clock::create_for_testing(&mut ctx);
    clock.set_for_testing(1_700_000_000_000);

    // Valid UTF-8 but not "header.payload.signature" format.
    gcp_attestation::verify_gcp_attestation(
        b"not_a_valid_jwt_token",
        TEST_KID,
        &clock,
    );

    clock.destroy_for_testing();
}

#[test]
#[expected_failure(abort_code = gcp_attestation::EKidNotFoundError)]
fun test_gcp_attestation_kid_not_found() {
    let mut ctx = tx_context::dummy();
    let mut clock = sui::clock::create_for_testing(&mut ctx);
    clock.set_for_testing(1_700_000_000_000);

    // No JWKs in the extension, so kid lookup must fail with EKidNotFoundError.
    gcp_attestation::verify_gcp_attestation(
        WELL_FORMED_INVALID_SIG_TOKEN,
        TEST_KID,
        &clock,
    );

    clock.destroy_for_testing();
}
