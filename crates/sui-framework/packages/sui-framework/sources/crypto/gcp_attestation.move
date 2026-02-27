// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::gcp_attestation;

use sui::clock::Clock;

#[allow(unused_const)]
/// Error that the feature is not available on this network.
const ENotSupportedError: u64 = 0;
#[allow(unused_const)]
/// Error that the attestation input failed to be parsed.
const EParseError: u64 = 1;
#[allow(unused_const)]
/// Error that the attestation failed to be verified.
const EVerifyError: u64 = 2;

/// Verified claims extracted from a GCP Confidential Spaces attestation JWT.
public struct GcpAttestationDocument has drop {
    /// JWT issuer (always https://confidentialcomputing.googleapis.com).
    iss: vector<u8>,
    /// Subject identifier for the workload.
    sub: vector<u8>,
    /// Audience claim.
    aud: vector<u8>,
    /// Expiration time, seconds since Unix epoch.
    exp: u64,
    /// Issued-at time, seconds since Unix epoch.
    iat: u64,
    /// EAT nonce values (GCP allows multiple).
    eat_nonce: vector<vector<u8>>,
    /// Whether secure boot was enabled.
    secboot: bool,
    /// Hardware model (e.g., GCP_AMD_SEV).
    hwmodel: vector<u8>,
    /// Software name (e.g., CONFIDENTIAL_SPACE).
    swname: vector<u8>,
    /// Debug status (e.g., disabled-since-boot).
    dbgstat: vector<u8>,
    /// Software version strings.
    swversion: vector<vector<u8>>,
    /// Container image digest.
    image_digest: vector<u8>,
    /// Container image reference.
    image_reference: vector<u8>,
    /// Container restart policy.
    restart_policy: vector<u8>,
}

/// Verify a GCP Confidential Spaces attestation JWT and return the extracted claims.
///
/// @param token: The RS256 JWT token bytes (UTF-8 encoded header.payload.signature).
/// @param jwk_n: RSA public key modulus in big-endian bytes.
/// @param jwk_e: RSA public key exponent in big-endian bytes.
/// @param clock: The clock object used to check token expiry.
///
/// Aborts with ENotSupportedError if the feature is disabled,
/// EParseError if the token cannot be parsed,
/// EVerifyError if the signature or claims are invalid.
entry fun verify_gcp_attestation(
    token: vector<u8>,
    jwk_n: vector<u8>,
    jwk_e: vector<u8>,
    clock: &Clock,
): GcpAttestationDocument {
    verify_gcp_attestation_internal(&token, &jwk_n, &jwk_e, clock.timestamp_ms())
}

public fun iss(doc: &GcpAttestationDocument): &vector<u8> {
    &doc.iss
}

public fun sub(doc: &GcpAttestationDocument): &vector<u8> {
    &doc.sub
}

/// Returns the first audience value. When the token carries multiple audiences,
/// only the first is returned; callers requiring multi-audience validation should
/// use the raw JWT claims directly.
public fun aud(doc: &GcpAttestationDocument): &vector<u8> {
    &doc.aud
}

public fun exp(doc: &GcpAttestationDocument): u64 {
    doc.exp
}

public fun iat(doc: &GcpAttestationDocument): u64 {
    doc.iat
}

public fun eat_nonce(doc: &GcpAttestationDocument): &vector<vector<u8>> {
    &doc.eat_nonce
}

/// Returns whether secure boot was enabled. Defaults to `false` when the
/// `secboot` field is absent from the token; callers that require secure boot
/// must assert this value is `true` rather than treating absence as enabled.
public fun secboot(doc: &GcpAttestationDocument): bool {
    doc.secboot
}

public fun hwmodel(doc: &GcpAttestationDocument): &vector<u8> {
    &doc.hwmodel
}

public fun swname(doc: &GcpAttestationDocument): &vector<u8> {
    &doc.swname
}

public fun dbgstat(doc: &GcpAttestationDocument): &vector<u8> {
    &doc.dbgstat
}

public fun swversion(doc: &GcpAttestationDocument): &vector<vector<u8>> {
    &doc.swversion
}

public fun image_digest(doc: &GcpAttestationDocument): &vector<u8> {
    &doc.image_digest
}

public fun image_reference(doc: &GcpAttestationDocument): &vector<u8> {
    &doc.image_reference
}

public fun restart_policy(doc: &GcpAttestationDocument): &vector<u8> {
    &doc.restart_policy
}

/// Internal native function.
native fun verify_gcp_attestation_internal(
    token: &vector<u8>,
    jwk_n: &vector<u8>,
    jwk_e: &vector<u8>,
    current_timestamp_ms: u64,
): GcpAttestationDocument;

