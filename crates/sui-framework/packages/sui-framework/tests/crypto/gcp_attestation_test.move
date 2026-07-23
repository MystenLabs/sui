// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::gcp_attestation_tests;

use sui::gcp_attestation;

// A minimal valid JWT structure with an RS256 header carrying `kid` = "test-kid-001", but an
// invalid signature.
// header:  base64url({"alg":"RS256","typ":"JWT","kid":"test-kid-001"})
// payload: base64url({"iss":"https://confidentialcomputing.googleapis.com","exp":9999999999,"iat":1700000000})
// sig:     342 base64url chars encoding 256 zero bytes (invalid RS256 signature for any key)
//
// The native implementation reads `kid` from this header itself -- there is no separate `kid`
// argument to `verify_gcp_attestation` (mirrors `nitro_attestation::load_nitro_attestation`).
const WELL_FORMED_INVALID_SIG_TOKEN: vector<u8> =
    b"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLTAwMSJ9.eyJpc3MiOiJodHRwczovL2NvbmZpZGVudGlhbGNvbXB1dGluZy5nb29nbGVhcGlzLmNvbSIsImV4cCI6OTk5OTk5OTk5OSwiaWF0IjoxNzAwMDAwMDAwfQ.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

// A JWT header missing the `kid` claim entirely: {"alg":"RS256","typ":"JWT"}.
const MISSING_KID_TOKEN: vector<u8> =
    b"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL2NvbmZpZGVudGlhbGNvbXB1dGluZy5nb29nbGVhcGlzLmNvbSIsImV4cCI6OTk5OTk5OTk5OSwiaWF0IjoxNzAwMDAwMDAwfQ.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

// Parse/verify abort paths need a trusted JWK present before the native parses the
// token. The Move unit-test harness installs an empty JwkMap, so those paths are
// covered by crates/sui-types unit tests and msim e2e instead.

#[test]
#[expected_failure(abort_code = gcp_attestation::EKidNotFoundError)]
fun test_gcp_attestation_kid_not_found() {
    let mut ctx = tx_context::dummy();
    let mut clock = sui::clock::create_for_testing(&mut ctx);
    clock.set_for_testing(1_700_000_000_000);

    // The header's `kid` ("test-kid-001") is not present in the (empty) JwkMap extension
    // installed by the Move unit-test harness, so lookup must fail with EKidNotFoundError.
    gcp_attestation::verify_gcp_attestation(
        WELL_FORMED_INVALID_SIG_TOKEN,
        &clock,
    );

    clock.destroy_for_testing();
}

#[test]
#[expected_failure(abort_code = gcp_attestation::EParseError)]
fun test_gcp_attestation_missing_kid_is_parse_error() {
    let mut ctx = tx_context::dummy();
    let mut clock = sui::clock::create_for_testing(&mut ctx);
    clock.set_for_testing(1_700_000_000_000);

    // A header with no `kid` claim at all must fail to parse before any key lookup occurs.
    gcp_attestation::verify_gcp_attestation(
        MISSING_KID_TOKEN,
        &clock,
    );

    clock.destroy_for_testing();
}
