// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

#[cfg(test)]
#[path = "unit_tests/gcp_attestation_tests.rs"]
mod gcp_attestation_tests;

/// Maximum JWT token size to prevent excessive parsing costs.
const MAX_JWT_TOKEN_SIZE: usize = 16 * 1024;
/// Minimum RSA modulus size: ring's RSA_PKCS1_2048_8192_SHA256 enforces this floor.
const MIN_RSA_MODULUS_SIZE: usize = 256;
/// Maximum RSA modulus size (4096-bit key = 512 bytes).
const MAX_RSA_MODULUS_SIZE: usize = 512;
/// Minimum RSA exponent size. GCP uses e = 65537 (3 bytes); reject weaker exponents (e.g. e=3).
const MIN_RSA_EXPONENT_SIZE: usize = 3;
/// Maximum RSA exponent size.
const MAX_RSA_EXPONENT_SIZE: usize = 8;
const EXPECTED_ISSUER: &str = "https://confidentialcomputing.googleapis.com";

/// Error type for GCP attestation verification.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GcpAttestationError {
    #[error("parse error: {0}")]
    ParseError(String),
    #[error("verify error: {0}")]
    VerifyError(String),
}

/// Extracted claims from a verified GCP Confidential Spaces attestation JWT.
#[derive(Debug, Clone)]
pub struct GcpAttestationDocument {
    pub iss: Vec<u8>,
    pub sub: Vec<u8>,
    pub aud: Vec<u8>,
    /// Expiration time, seconds since Unix epoch.
    pub exp: u64,
    /// Issued-at time, seconds since Unix epoch.
    pub iat: u64,
    /// EAT nonce values (GCP allows multiple).
    pub eat_nonce: Vec<Vec<u8>>,
    pub secboot: bool,
    pub hwmodel: Vec<u8>,
    pub swname: Vec<u8>,
    pub dbgstat: Vec<u8>,
    pub swversion: Vec<Vec<u8>>,
    pub image_digest: Vec<u8>,
    pub image_reference: Vec<u8>,
    pub restart_policy: Vec<u8>,
}

// JWT header; only `alg` is needed for algorithm validation.
#[derive(serde::Deserialize)]
struct JwtHeader {
    alg: String,
}

// A JSON value that may be either a bare string or an array of strings.
// Used for `aud` and `eat_nonce`, which GCP Confidential Spaces encodes both ways.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum StringOrArray {
    Single(String),
    Array(Vec<String>),
}

impl Default for StringOrArray {
    fn default() -> Self {
        StringOrArray::Array(Vec::new())
    }
}

impl StringOrArray {
    fn into_first_bytes(self) -> Vec<u8> {
        match self {
            StringOrArray::Single(s) => s.into_bytes(),
            StringOrArray::Array(mut arr) => {
                if arr.is_empty() {
                    Vec::new()
                } else {
                    arr.swap_remove(0).into_bytes()
                }
            }
        }
    }

    fn into_all_bytes(self) -> Vec<Vec<u8>> {
        match self {
            StringOrArray::Single(s) => vec![s.into_bytes()],
            StringOrArray::Array(arr) => arr.into_iter().map(|s| s.into_bytes()).collect(),
        }
    }
}

// Typed payload struct; serde skips all unrecognised GCP-specific fields in a single pass,
// avoiding the full DOM construction that serde_json::Value requires.
#[derive(serde::Deserialize)]
struct GcpPayload {
    iss: String,
    exp: u64,
    iat: u64,
    // nbf is optional; when present it may be later than iat (e.g. scheduled validity).
    #[serde(default)]
    nbf: Option<u64>,
    #[serde(default)]
    sub: String,
    #[serde(default)]
    aud: StringOrArray,
    #[serde(default)]
    eat_nonce: StringOrArray,
    #[serde(default)]
    secboot: bool,
    #[serde(default)]
    hwmodel: String,
    #[serde(default)]
    swname: String,
    #[serde(default)]
    dbgstat: String,
    #[serde(default)]
    swversion: Vec<String>,
    #[serde(default)]
    submods: GcpSubmods,
}

#[derive(serde::Deserialize, Default)]
struct GcpSubmods {
    #[serde(default)]
    container: GcpContainer,
}

#[derive(serde::Deserialize, Default)]
struct GcpContainer {
    #[serde(default)]
    image_digest: String,
    #[serde(default)]
    image_reference: String,
    #[serde(default)]
    restart_policy: String,
}

/// Extract the `kid` (key ID) from the JWT header.
///
/// Decodes the base64url header, parses the JSON, and returns the `kid` field as a String.
#[cfg(test)]
pub(crate) fn extract_kid_from_jwt(token: &[u8]) -> Result<String, GcpAttestationError> {
    if token.len() > MAX_JWT_TOKEN_SIZE {
        return Err(GcpAttestationError::ParseError(
            "JWT token too large".to_string(),
        ));
    }
    let token_str =
        std::str::from_utf8(token).map_err(|e| GcpAttestationError::ParseError(e.to_string()))?;
    let header_b64 = token_str
        .split('.')
        .next()
        .ok_or_else(|| GcpAttestationError::ParseError("missing JWT header".to_string()))?;
    let header_bytes = URL_SAFE_NO_PAD
        .decode(header_b64)
        .map_err(|e| GcpAttestationError::ParseError(format!("header base64: {}", e)))?;
    let header: serde_json::Value = serde_json::from_slice(&header_bytes)
        .map_err(|e| GcpAttestationError::ParseError(format!("header JSON: {}", e)))?;
    let kid = header
        .get("kid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GcpAttestationError::ParseError("missing 'kid' in header".to_string()))?;
    Ok(kid.to_string())
}

/// Verify a GCP Confidential Spaces attestation JWT and extract its claims.
///
/// The token is a standard RS256 JWT. The caller is responsible for supplying
/// a trusted RSA public key (n, e) in big-endian byte representation.
///
/// # Replay protection
///
/// This function is stateless and does **not** enforce nonce uniqueness. To prevent
/// replay attacks the caller must verify that `GcpAttestationDocument::eat_nonce`
/// contains a freshly-generated nonce that was embedded in the attestation request
/// before passing the token to a workload. Tokens are valid for ~1 hour; without
/// nonce checking any intercepted unexpired token can be replayed.
///
/// Returns `Err(GcpAttestationError::ParseError(...))` for structural problems and
/// `Err(GcpAttestationError::VerifyError(...))` for signature or claim validation failures.
pub fn verify_gcp_attestation(
    token: &[u8],
    jwk_n: &[u8],
    jwk_e: &[u8],
    current_timestamp_ms: u64,
) -> Result<GcpAttestationDocument, GcpAttestationError> {
    if token.len() > MAX_JWT_TOKEN_SIZE {
        return Err(GcpAttestationError::ParseError(
            "JWT token too large".to_string(),
        ));
    }
    if jwk_n.len() < MIN_RSA_MODULUS_SIZE || jwk_n.len() > MAX_RSA_MODULUS_SIZE {
        return Err(GcpAttestationError::ParseError(
            "RSA modulus size out of bounds".to_string(),
        ));
    }
    if jwk_e.len() < MIN_RSA_EXPONENT_SIZE || jwk_e.len() > MAX_RSA_EXPONENT_SIZE {
        return Err(GcpAttestationError::ParseError(
            "RSA exponent size out of bounds".to_string(),
        ));
    }

    let token_str =
        std::str::from_utf8(token).map_err(|e| GcpAttestationError::ParseError(e.to_string()))?;

    let parts: Vec<&str> = token_str.splitn(3, '.').collect();
    if parts.len() != 3 {
        return Err(GcpAttestationError::ParseError(
            "JWT must have exactly three dot-separated sections".to_string(),
        ));
    }
    let (header_b64, payload_b64, signature_b64) = (parts[0], parts[1], parts[2]);

    let header_bytes = URL_SAFE_NO_PAD
        .decode(header_b64)
        .map_err(|e| GcpAttestationError::ParseError(format!("header base64: {}", e)))?;
    let header: JwtHeader = serde_json::from_slice(&header_bytes)
        .map_err(|e| GcpAttestationError::ParseError(format!("header JSON: {}", e)))?;

    // Reject all algorithms other than RS256 (including 'none').
    // Do not echo the attacker-controlled alg value in the error.
    if header.alg != "RS256" {
        return Err(GcpAttestationError::VerifyError(
            "unsupported algorithm".to_string(),
        ));
    }

    let signature_bytes = URL_SAFE_NO_PAD
        .decode(signature_b64)
        .map_err(|e| GcpAttestationError::ParseError(format!("signature base64: {}", e)))?;

    // Zero-copy: reuse the contiguous header.payload prefix already in token_str.
    let signed_data = &token_str[..header_b64.len() + 1 + payload_b64.len()];

    // Verify RSA-PKCS#1v15 SHA-256 via ring's BoringSSL-backed implementation.
    // ring enforces a minimum key size of 2048 bits, providing an additional safety floor.
    ring::signature::RsaPublicKeyComponents { n: jwk_n, e: jwk_e }
        .verify(
            &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            signed_data.as_bytes(),
            &signature_bytes,
        )
        .map_err(|_| {
            GcpAttestationError::VerifyError("signature verification failed".to_string())
        })?;

    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|e| GcpAttestationError::ParseError(format!("payload base64: {}", e)))?;
    let payload: GcpPayload = serde_json::from_slice(&payload_bytes)
        .map_err(|e| GcpAttestationError::ParseError(format!("payload JSON: {}", e)))?;

    if payload.iss != EXPECTED_ISSUER {
        return Err(GcpAttestationError::VerifyError(
            "invalid issuer".to_string(),
        ));
    }

    // Guard against timestamps that would overflow when converted to milliseconds.
    // u64::MAX / 1000 ≈ year 584,554,530 — any real token is far below this.
    const MAX_TIMESTAMP_SECS: u64 = u64::MAX / 1000;
    if payload.exp > MAX_TIMESTAMP_SECS || payload.iat > MAX_TIMESTAMP_SECS {
        return Err(GcpAttestationError::VerifyError(
            "token timestamp out of range".to_string(),
        ));
    }

    if payload.exp * 1000 <= current_timestamp_ms {
        return Err(GcpAttestationError::VerifyError(
            "token has expired".to_string(),
        ));
    }

    if payload.iat * 1000 > current_timestamp_ms {
        return Err(GcpAttestationError::VerifyError(
            "token issued in the future".to_string(),
        ));
    }

    // nbf (not-before): when present, the token must not be used before this time.
    // Falls back to iat when absent.
    let nbf = payload.nbf.unwrap_or(payload.iat);
    if nbf > MAX_TIMESTAMP_SECS {
        return Err(GcpAttestationError::VerifyError(
            "token timestamp out of range".to_string(),
        ));
    }
    if nbf * 1000 > current_timestamp_ms {
        return Err(GcpAttestationError::VerifyError(
            "token not yet valid".to_string(),
        ));
    }

    Ok(GcpAttestationDocument {
        iss: payload.iss.into_bytes(),
        sub: payload.sub.into_bytes(),
        aud: payload.aud.into_first_bytes(),
        exp: payload.exp,
        iat: payload.iat,
        eat_nonce: payload.eat_nonce.into_all_bytes(),
        secboot: payload.secboot,
        hwmodel: payload.hwmodel.into_bytes(),
        swname: payload.swname.into_bytes(),
        dbgstat: payload.dbgstat.into_bytes(),
        swversion: payload
            .swversion
            .into_iter()
            .map(|s| s.into_bytes())
            .collect(),
        image_digest: payload.submods.container.image_digest.into_bytes(),
        image_reference: payload.submods.container.image_reference.into_bytes(),
        restart_policy: payload.submods.container.restart_policy.into_bytes(),
    })
}
