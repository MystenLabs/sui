// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::gcp_attestation;

use sui::clock::{Self, Clock};

#[allow(unused_const)]
/// Error that the feature is not available on this network.
const ENotSupportedError: u64 = 0;
#[allow(unused_const)]
/// Error that the attestation input failed to be parsed.
const EParseError: u64 = 1;
#[allow(unused_const)]
/// Error that the attestation failed to be verified.
const EVerifyError: u64 = 2;
#[allow(unused_const)]
/// Error that the kid is not found in the trusted JWK set.
const EKidNotFoundError: u64 = 3;

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
/// The RSA public key is looked up internally from the consensus-validated JWK set
/// using the GCP issuer and the supplied `kid`. No AuthenticatorState input is needed.
///
/// @param token: The RS256 JWT token bytes (UTF-8 encoded header.payload.signature).
/// @param kid: The key ID from the JWT header, identifying which trusted key to use.
/// @param clock: The clock object used to check token expiry.
///
/// Aborts with ENotSupportedError if the feature is disabled,
/// EParseError if the token cannot be parsed,
/// EKidNotFoundError if `kid` is not found in the trusted JWK set,
/// EVerifyError if the signature or claims are invalid.
entry fun verify_gcp_attestation(
    token: vector<u8>,
    kid: vector<u8>,
    clock: &Clock,
): GcpAttestationDocument {
    verify_gcp_attestation_internal(
        &token,
        &kid,
        clock::timestamp_ms(clock),
    )
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
    kid: &vector<u8>,
    current_timestamp_ms: u64,
): GcpAttestationDocument;
