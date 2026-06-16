// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! ECDSA signature verification over the NIST P-384 (secp384r1) curve, backed by the
//! RustCrypto `p384` crate.
//!
//! This is the single verification path shared by the Nitro attestation verifier
//! ([`crate::nitro_attestation`]) and the `ecdsa_p384` Move native, so both route through
//! one implementation rather than duplicating curve handling.

use p384::ecdsa::signature::hazmat::PrehashVerifier;
use p384::ecdsa::{Signature, VerifyingKey};
use sha2::{Digest, Sha256, Sha384};

/// Hash function applied to the message before P-384 ECDSA verification. P-384 is paired
/// with SHA-384 by default, but X.509 / WebAuthn / Apple App Attest chains also use
/// `ecdsa-with-SHA256`, so both are supported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P384Hash {
    Sha256,
    Sha384,
}

/// Distinct failure modes for [`verify_secp384r1`]. Kept granular so callers can map each
/// case onto their own error type without collapsing them into a single "failed" variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P384VerifyError {
    /// `signature` is not a valid fixed-size `(r, s)` secp384r1 signature.
    InvalidSignature,
    /// `public_key` is not a valid SEC1-encoded secp384r1 point.
    InvalidPublicKey,
    /// The signature did not verify against the public key and message.
    VerificationFailed,
}

/// Verify a NIST P-384 (secp384r1) ECDSA signature.
///
/// - `signature`: fixed-size 96-byte `(r, s)` encoding (not ASN.1/DER).
/// - `public_key`: SEC1-encoded point (33-byte compressed prefix `02`/`03` -> 49 bytes, or
///   uncompressed prefix `04` -> 97 bytes).
/// - `msg`: the raw message, hashed with `hash` before verification.
///
/// Standard ECDSA signatures are accepted, including high-`s` signatures, for X.509 /
/// WebAuthn / App Attest compatibility. Signature malleability means the byte encoding is
/// not unique, so callers must canonicalize before using signature bytes as an identifier.
pub fn verify_secp384r1(
    signature: &[u8],
    public_key: &[u8],
    msg: &[u8],
    hash: P384Hash,
) -> Result<(), P384VerifyError> {
    let signature =
        Signature::from_slice(signature).map_err(|_| P384VerifyError::InvalidSignature)?;
    let verifying_key =
        VerifyingKey::from_sec1_bytes(public_key).map_err(|_| P384VerifyError::InvalidPublicKey)?;

    // `verify_prehash` (rather than the curve-default `verify`) lets us accept SHA-256 as
    // well as SHA-384: it reduces an arbitrary-length digest to the P-384 field, whereas the
    // `Digest`-based API requires the hash output to equal the 48-byte field size.
    let verified = match hash {
        P384Hash::Sha256 => {
            verifying_key.verify_prehash(Sha256::digest(msg).as_slice(), &signature)
        }
        P384Hash::Sha384 => {
            verifying_key.verify_prehash(Sha384::digest(msg).as_slice(), &signature)
        }
    };
    verified.map_err(|_| P384VerifyError::VerificationFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fastcrypto::encoding::{Encoding, Hex};
    use p384::ecdsa::signature::Signer;
    use p384::ecdsa::signature::hazmat::PrehashSigner;
    use p384::ecdsa::{Signature, SigningKey};

    // Fixed non-zero scalar (< curve order) so the generated vectors are deterministic and
    // reproducible. ECDSA signing here is deterministic (RFC6979), so the signatures are stable.
    const TEST_SCALAR: [u8; 48] = [0x42; 48];
    const TEST_MSG: &[u8] = b"Sui ecdsa_p384 native test message";

    fn vectors() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let sk = SigningKey::from_slice(&TEST_SCALAR).unwrap();
        let pk = sk
            .verifying_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec();
        let sig384: Signature = sk.sign(TEST_MSG);
        let sig256: Signature = sk
            .sign_prehash(Sha256::digest(TEST_MSG).as_slice())
            .unwrap();
        (pk, sig256.to_bytes().to_vec(), sig384.to_bytes().to_vec())
    }

    #[test]
    fn verify_roundtrip_both_hashes() {
        let (pk, sig256, sig384) = vectors();
        assert!(verify_secp384r1(&sig256, &pk, TEST_MSG, P384Hash::Sha256).is_ok());
        assert!(verify_secp384r1(&sig384, &pk, TEST_MSG, P384Hash::Sha384).is_ok());
    }

    #[test]
    fn verify_rejects_wrong_inputs() {
        let (pk, sig256, sig384) = vectors();
        // Mismatched hash flag.
        assert_eq!(
            verify_secp384r1(&sig384, &pk, TEST_MSG, P384Hash::Sha256),
            Err(P384VerifyError::VerificationFailed)
        );
        // Wrong message.
        assert_eq!(
            verify_secp384r1(&sig256, &pk, b"different message", P384Hash::Sha256),
            Err(P384VerifyError::VerificationFailed)
        );
        // Malformed signature / public key map to distinct errors.
        assert_eq!(
            verify_secp384r1(&[0u8; 10], &pk, TEST_MSG, P384Hash::Sha256),
            Err(P384VerifyError::InvalidSignature)
        );
        assert_eq!(
            verify_secp384r1(&sig256, &[0u8; 10], TEST_MSG, P384Hash::Sha256),
            Err(P384VerifyError::InvalidPublicKey)
        );
    }

    // Known-answer vectors generated from TEST_SCALAR over TEST_MSG (deterministic RFC6979),
    // pinned so a verification regression (independent of signing) is caught. These are the same
    // vectors embedded in the `ecdsa_p384` Move tests.
    #[test]
    fn verify_known_answer_vectors() {
        let pk = Hex::decode("0272ccde33753762245e015da92e48fa028495522dc42356c7e3df51dcf56a5e19de742acd3a19f79af372dc9705f560d8").unwrap();
        let sig256 = Hex::decode("c8ccb0f019761836eaf7d3b1d200fc79e0330c7741a28b3fb727d376600668b6308963ecce8c2baf9021af0b0353c7e9411cd0604dfe6503330c6103adfbf80a99cdb9fd2d9e3ead25d06d03b6a8f67cb8ff8d42daa268f5ce37c3c79f0510df").unwrap();
        let sig384 = Hex::decode("b1d61cff40b11cf6963e41b820b71a44393f63cae1a91663921477dc58f416c5a9ca099a96994b7748740fae52df2eeb432176564578358107ace808077053f9bc61001e49430b153cf7eef56e327add63c007b01d618c6d219bf1355e592edf").unwrap();
        assert!(verify_secp384r1(&sig256, &pk, TEST_MSG, P384Hash::Sha256).is_ok());
        assert!(verify_secp384r1(&sig384, &pk, TEST_MSG, P384Hash::Sha384).is_ok());
    }
}
