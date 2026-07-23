// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use move_core_types::account_address::AccountAddress;

use crate::SUI_FRAMEWORK_ADDRESS;

#[cfg(test)]
#[path = "unit_tests/gcp_attestation_tests.rs"]
mod gcp_attestation_tests;

/// Maximum JWT token size to prevent excessive parsing costs.
const MAX_JWT_TOKEN_SIZE: usize = 16 * 1024;
/// Maximum size of the `kid` header claim, in UTF-8 bytes.
pub const MAX_KID_SIZE: usize = 4096;
/// Minimum RSA modulus size: ring's RSA_PKCS1_2048_8192_SHA256 enforces this floor.
pub const MIN_RSA_MODULUS_SIZE: usize = 256;
/// Maximum RSA modulus size (4096-bit key = 512 bytes).
pub const MAX_RSA_MODULUS_SIZE: usize = 512;
/// Maximum RSA exponent size.
const MAX_RSA_EXPONENT_SIZE: usize = 8;
/// Minimum acceptable RSA public exponent (Fermat F4). Reject weaker exponents such as e=3.
const MIN_RSA_EXPONENT: u64 = 65537;
/// Expected `iss` claim for GCP Confidential Spaces attestation tokens.
pub const GCP_ISSUER: &str = "https://confidentialcomputing.googleapis.com";
/// The only JWS algorithm accepted for GCP Confidential Spaces attestation tokens.
pub const RS256_ALG: &str = "RS256";
pub const GCP_ATTESTATION_MODULE_NAME: &str = "gcp_attestation";
pub const VERIFY_GCP_ATTESTATION_FUNCTION_NAME: &str = "verify_gcp_attestation";

/// Returns true only for the public GCP attestation entry point in the Sui framework.
pub fn is_gcp_attestation_call(package: AccountAddress, module: &str, function: &str) -> bool {
    package == SUI_FRAMEWORK_ADDRESS
        && module == GCP_ATTESTATION_MODULE_NAME
        && function == VERIFY_GCP_ATTESTATION_FUNCTION_NAME
}

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

// JWT header; `alg` is required, `kid` is required (validated by `ParsedGcpJwt::parse`).
#[derive(serde::Deserialize)]
struct JwtHeader {
    alg: String,
    #[serde(default)]
    kid: Option<String>,
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

/// Returns true if `e` is a valid RSA public exponent: odd and numerically >= 65537.
///
/// Compares the big-endian encoding against `65537u64.to_be_bytes()` with left-padding.
/// Rejects weak exponents such as `e = 3` even when padded to three bytes (`[0, 0, 3]`).
pub fn rsa_exponent_ok(e: &[u8]) -> bool {
    if e.is_empty() || e.len() > MAX_RSA_EXPONENT_SIZE {
        return false;
    }
    // Exponent must be odd.
    if e[e.len() - 1] & 1 == 0 {
        return false;
    }
    let mut padded = [0u8; 8];
    padded[8 - e.len()..].copy_from_slice(e);
    padded >= MIN_RSA_EXPONENT.to_be_bytes()
}

/// Validate an RSA public key's modulus bounds and exponent strength.
///
/// Centralizes the bounds enforced on every RSA public key used for GCP attestation
/// verification, whether ingested from the GCP JWKS endpoint (see `sui-node`) or looked up
/// natively at verification time. Defense-in-depth: native lookup keys may bypass node-side
/// ingestion checks (e.g. test JWK injection), so this is re-checked at verify time.
pub fn validate_rsa_public_key(n: &[u8], e: &[u8]) -> Result<(), GcpAttestationError> {
    if n.len() < MIN_RSA_MODULUS_SIZE || n.len() > MAX_RSA_MODULUS_SIZE {
        return Err(GcpAttestationError::ParseError(
            "RSA modulus size out of bounds".to_string(),
        ));
    }
    if !rsa_exponent_ok(e) {
        return Err(GcpAttestationError::ParseError(
            "RSA exponent out of bounds".to_string(),
        ));
    }
    Ok(())
}

fn decode_b64(segment: &str, what: &str) -> Result<Vec<u8>, GcpAttestationError> {
    URL_SAFE_NO_PAD
        .decode(segment)
        .map_err(|e| GcpAttestationError::ParseError(format!("{what} base64: {e}")))
}

/// A structurally-validated GCP Confidential Spaces attestation JWT.
///
/// [`ParsedGcpJwt::parse`] decodes the JWT structure and header exactly once: it splits the
/// token into header/payload/signature segments, base64url-decodes the header, parses its
/// JSON, validates `alg == RS256`, and extracts a required non-empty `kid` bounded to
/// [`MAX_KID_SIZE`] UTF-8 bytes. The exact bytes over which the signature was computed
/// (`header_b64 || '.' || payload_b64`) are retained so that [`ParsedGcpJwt::verify`] never
/// has to re-derive them or re-parse the header.
///
/// This mirrors the Nitro attestation split of parse-then-verify: native code can look up the
/// trusted key by [`ParsedGcpJwt::kid`] between the two steps without touching the header twice.
#[derive(Debug)]
pub struct ParsedGcpJwt {
    /// Key ID extracted from the JWT header. Guaranteed non-empty and <= MAX_KID_SIZE bytes.
    kid: String,
    /// Exact bytes over which the signature was computed: `header_b64 || '.' || payload_b64`.
    signed_data: Vec<u8>,
    /// base64url-encoded signature segment; decoded lazily in `verify`.
    signature_b64: String,
    /// base64url-encoded payload segment; decoded lazily in `verify`.
    payload_b64: String,
}

impl ParsedGcpJwt {
    /// Parse and structurally validate a JWT: size, structure, header JSON, `alg`, and `kid`.
    ///
    /// Does not touch the signature or payload; call [`ParsedGcpJwt::verify`] for that once the
    /// trusted key for `kid()` has been looked up.
    pub fn parse(token: &[u8]) -> Result<Self, GcpAttestationError> {
        if token.len() > MAX_JWT_TOKEN_SIZE {
            return Err(GcpAttestationError::ParseError(
                "JWT token too large".to_string(),
            ));
        }
        let token_str = std::str::from_utf8(token)
            .map_err(|e| GcpAttestationError::ParseError(e.to_string()))?;

        let parts: Vec<&str> = token_str.splitn(3, '.').collect();
        if parts.len() != 3 {
            return Err(GcpAttestationError::ParseError(
                "JWT must have exactly three dot-separated sections".to_string(),
            ));
        }
        let (header_b64, payload_b64, signature_b64) = (parts[0], parts[1], parts[2]);

        let header_bytes = decode_b64(header_b64, "header")?;
        let header: JwtHeader = serde_json::from_slice(&header_bytes)
            .map_err(|e| GcpAttestationError::ParseError(format!("header JSON: {e}")))?;

        // Reject all algorithms other than RS256 (including 'none').
        // Do not echo the attacker-controlled alg value in the error.
        if header.alg != RS256_ALG {
            return Err(GcpAttestationError::VerifyError(
                "unsupported algorithm".to_string(),
            ));
        }

        let kid = match header.kid {
            None => {
                return Err(GcpAttestationError::ParseError(
                    "missing 'kid' in header".to_string(),
                ));
            }
            Some(kid) if kid.is_empty() => {
                return Err(GcpAttestationError::ParseError(
                    "empty 'kid' in header".to_string(),
                ));
            }
            Some(kid) if kid.len() > MAX_KID_SIZE => {
                return Err(GcpAttestationError::ParseError(
                    "'kid' exceeds maximum size".to_string(),
                ));
            }
            Some(kid) => kid,
        };

        // Zero-copy at this point: reuse the contiguous header.payload prefix already in
        // token_str. It is copied into an owned buffer so that `ParsedGcpJwt` does not need to
        // borrow from `token`.
        let signed_data = token_str.as_bytes()[..header_b64.len() + 1 + payload_b64.len()].to_vec();

        Ok(Self {
            kid,
            signed_data,
            signature_b64: signature_b64.to_string(),
            payload_b64: payload_b64.to_string(),
        })
    }

    /// The `kid` extracted from the JWT header. Non-empty and <= [`MAX_KID_SIZE`] bytes.
    pub fn kid(&self) -> &str {
        &self.kid
    }

    /// Verify the signature and payload claims using the trusted key `(jwk_n, jwk_e)` looked up
    /// for `self.kid()`. Does not re-parse the header.
    ///
    /// The caller is responsible for supplying a trusted RSA public key (n, e) in big-endian
    /// byte representation, resolved from `self.kid()`.
    ///
    /// # Authorization policy
    ///
    /// This function does not validate `aud` or workload policy claims. Callers must compare the
    /// returned audience with their expected audience and enforce the required image, secure-boot,
    /// hardware-model, and debug-status claims.
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
    pub fn verify(
        &self,
        jwk_n: &[u8],
        jwk_e: &[u8],
        current_timestamp_ms: u64,
    ) -> Result<GcpAttestationDocument, GcpAttestationError> {
        validate_rsa_public_key(jwk_n, jwk_e)?;

        let signature_bytes = decode_b64(&self.signature_b64, "signature")?;

        // Verify RSA-PKCS#1v15 SHA-256 via ring's BoringSSL-backed implementation.
        // ring enforces a minimum key size of 2048 bits, providing an additional safety floor.
        ring::signature::RsaPublicKeyComponents { n: jwk_n, e: jwk_e }
            .verify(
                &ring::signature::RSA_PKCS1_2048_8192_SHA256,
                &self.signed_data,
                &signature_bytes,
            )
            .map_err(|_| {
                GcpAttestationError::VerifyError("signature verification failed".to_string())
            })?;

        let payload_bytes = decode_b64(&self.payload_b64, "payload")?;
        let payload: GcpPayload = serde_json::from_slice(&payload_bytes)
            .map_err(|e| GcpAttestationError::ParseError(format!("payload JSON: {e}")))?;

        if payload.iss != GCP_ISSUER {
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
}

/// Verify a GCP Confidential Spaces attestation JWT and extract its claims.
///
/// Convenience wrapper around [`ParsedGcpJwt::parse`] followed by [`ParsedGcpJwt::verify`] for
/// callers that already know which key to use (e.g. tests, or callers with a single trusted
/// key). Callers that need to look up the key by `kid` after parsing (e.g. the native
/// implementation) should call [`ParsedGcpJwt::parse`] directly.
pub fn verify_gcp_attestation(
    token: &[u8],
    jwk_n: &[u8],
    jwk_e: &[u8],
    current_timestamp_ms: u64,
) -> Result<GcpAttestationDocument, GcpAttestationError> {
    ParsedGcpJwt::parse(token)?.verify(jwk_n, jwk_e, current_timestamp_ms)
}
