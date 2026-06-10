// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::ecdsa_p384;

#[allow(unused_const)]
/// Hash function flag for SHA-256, used by `secp384r1_verify`.
const SHA256: u8 = 0;
#[allow(unused_const)]
/// Hash function flag for SHA-384, used by `secp384r1_verify`.
const SHA384: u8 = 1;

/// @param signature: A 96-byte signature in the form `(r, s)` produced with Secp384r1 /
/// NIST P-384. This is the fixed-size encoding, not ASN.1/DER.
/// @param public_key: The SEC1-encoded public key to verify the signature against
/// (33-byte prefix `02`/`03` compressed, or 65-byte prefix `04` uncompressed).
/// @param msg: The raw message the signature is signed against (hashed internally).
/// @param hash: The hash function flag used when signing: 0 = SHA-256, 1 = SHA-384.
///
/// Verifies a NIST P-384 ECDSA signature. This accepts standard ECDSA signatures, including
/// high-S signatures, for X.509 / WebAuthn / Apple App Attest compatibility. Because the
/// signature encoding is malleable, callers that need a unique signature identifier must
/// canonicalize the signature before using its bytes as a nullifier or map key.
///
/// If the signature is valid for the public key and hashed message, returns true. Else false.
public native fun secp384r1_verify(
    signature: &vector<u8>,
    public_key: &vector<u8>,
    msg: &vector<u8>,
    hash: u8,
): bool;
