# GCP Confidential Spaces Attestation — Native Function Implementation

## Overview

Adds on-chain verification of GCP Confidential Spaces attestation JWTs to the Sui Move framework.
A Move contract calls `verify_gcp_attestation` with a JWT and the RSA public key components (`n`,
`e`) as raw big-endian bytes. The frontend/TS SDK is responsible for fetching the correct key
(e.g. from on-chain `AuthenticatorState` via RPC) and decoding it before calling the function.

---

## Architecture: Three Layers

```
Move entry function  (sui-framework/sources/crypto/gcp_attestation.move)
        │
        ▼
Native dispatch      (sui-execution/latest/sui-move-natives/src/crypto/gcp_attestation.rs)
        │
        ▼
Rust crypto core     (crates/sui-types/src/gcp_attestation.rs)
```

---

## Layer 1 — Move API (`sui::gcp_attestation`)

### Entry point

```move
entry fun verify_gcp_attestation(
    token:  vector<u8>,   // RS256 JWT (UTF-8: header.payload.signature)
    jwk_n:  vector<u8>,   // RSA modulus, big-endian bytes
    jwk_e:  vector<u8>,   // RSA exponent, big-endian bytes
    clock:  &Clock,
): GcpAttestationDocument
```

The caller (TS SDK / frontend) is responsible for:
1. Extracting the `kid` from the JWT JOSE header.
2. Reading `AuthenticatorState` on-chain via RPC to find the matching JWK.
3. Base64url-decoding the JWK `n` and `e` strings to raw bytes.
4. Passing those raw bytes here.

### Abort codes

| Constant | Code | Meaning |
|---|---|---|
| `ENotSupportedError` | 0 | Feature flag disabled on this network |
| `EParseError` | 1 | JWT structurally invalid |
| `EVerifyError` | 2 | Signature or claim check failed |

### `GcpAttestationDocument` struct (14 fields)

```move
public struct GcpAttestationDocument has drop {
    iss: vector<u8>,                  // always: https://confidentialcomputing.googleapis.com
    sub: vector<u8>,                  // workload subject
    aud: vector<u8>,                  // audience (first element if array)
    exp: u64,                         // expiry, seconds since epoch
    iat: u64,                         // issued-at, seconds since epoch
    eat_nonce: vector<vector<u8>>,    // EAT nonce(s) — use as tx-hash binding
    secboot: bool,                    // true iff secure boot was active
    hwmodel: vector<u8>,              // e.g. "GCP_AMD_SEV"
    swname: vector<u8>,               // e.g. "CONFIDENTIAL_SPACE"
    dbgstat: vector<u8>,              // e.g. "disabled-since-boot"
    swversion: vector<vector<u8>>,    // firmware/OS version strings
    image_digest: vector<u8>,         // container image SHA-256 digest
    image_reference: vector<u8>,      // container image reference
    restart_policy: vector<u8>,       // e.g. "Never"
}
```

All field accessors (`iss()`, `sub()`, `image_digest()`, etc.) are public functions.

### Native declaration

```move
native fun verify_gcp_attestation_internal(
    token: &vector<u8>,
    jwk_n: &vector<u8>,
    jwk_e: &vector<u8>,
    current_timestamp_ms: u64,
): GcpAttestationDocument;
```

---

## Layer 2 — Native Dispatch (`sui-move-natives`)

File: `sui-execution/latest/sui-move-natives/src/crypto/gcp_attestation.rs`

**Error codes returned to Move VM:**

| Constant | Value | Mapped from |
|---|---|---|
| `NOT_SUPPORTED_ERROR` | 0 | Feature flag off |
| `PARSE_ERROR` | 1 | `GcpAttestationError::ParseError` |
| `VERIFY_ERROR` | 2 | `GcpAttestationError::VerifyError` |

**Gas model:**

```
gas = verify_base_cost + verify_cost_per_byte × token.len()
```

**Feature flag checked:** `protocol_config.enable_gcp_attestation()`

**Struct packing (`pack_document`):** field order in `Struct::pack` must exactly match the Move
struct definition above. Fields are packed in declaration order (iss, sub, aud, exp, iat, ...).

---

## Layer 3 — Rust Crypto Core (`sui-types::gcp_attestation`)

File: `crates/sui-types/src/gcp_attestation.rs`

### Size guards (applied before any parsing)

```
MAX_JWT_TOKEN_SIZE    = 16 KiB
MAX_RSA_MODULUS_SIZE  = 512 bytes  (4096-bit key)
MAX_RSA_EXPONENT_SIZE = 8 bytes
```

### `verify_gcp_attestation` — step-by-step

1. Size-check token, `n`, `e`
2. Split JWT into `header.payload.signature` (exactly 3 parts)
3. Decode & parse header → assert `alg == "RS256"` (rejects `alg: none`)
4. Decode signature bytes
5. Construct `RsaPublicKey` from big-endian `n`, `e` via `rsa` crate
6. **Verify RSA-PKCS1v15-SHA256 signature** over `"base64url(header).base64url(payload)"`
7. Decode & parse payload JSON
8. Assert `iss == "https://confidentialcomputing.googleapis.com"`
9. Assert `exp * 1000 > current_timestamp_ms` (not expired)
10. Assert `iat * 1000 <= current_timestamp_ms` (not future-issued)
11. Extract all 14 claims → return `GcpAttestationDocument`

---

## Layer 4 — Validator JWK Fetcher (`sui-node`)

The validator fetches GCP public keys from:

```
https://www.googleapis.com/service_accounts/v1/metadata/jwk/signer@confidentialspace-sign.iam.gserviceaccount.com
```

and submits them through consensus into `AuthenticatorState` (the same pipeline as OIDC JWKs).
This makes them available for the TS SDK to read via RPC.

**Issuer stored on-chain:** `https://confidentialcomputing.googleapis.com`

**Filter:** only keys where `kty == "RSA" && alg == "RS256"` are accepted.

**Lifecycle:**

```
epoch start
    └─ if enable_gcp_attestation:
           start_gcp_jwk_updater (tokio task)
                loop:
                    fetch_gcp_jwks()
                    parse_gcp_jwks() → [(JwkId{iss,kid}, JWK{kty,n,e,alg})]
                    for each key:
                        check_total_jwk_size (≤ 2KB)
                        jwk_active_in_current_epoch? → skip
                        seen HashSet → dedup within epoch
                        MAX_JWK_KEYS_PER_FETCH → cap
                        submit via ConsensusAdapter
                    sleep(3600s)
```

## TS SDK Flow

```
1. Obtain GCP attestation JWT from the workload (IMDS endpoint)
2. Decode JWT JOSE header → extract `kid`
3. Read AuthenticatorState on-chain via Sui RPC
4. Find JWK where iss = "https://confidentialcomputing.googleapis.com" and kid matches
5. Base64url-decode JWK `n` and `e` fields → raw Uint8Array bytes
6. Call verify_gcp_attestation(token, n_bytes, e_bytes, clock)
7. Use returned GcpAttestationDocument fields to gate logic
```

---

## Protocol Config Feature Flag

| Flag | Version introduced | Networks enabled |
|---|---|---|
| `enable_gcp_attestation` | 115 | Devnet only (not mainnet/testnet) |

Gas costs (placeholder, pending benchmarking):
- `gcp_attestation_verify_base_cost` = `50_000 * 50`
- `gcp_attestation_verify_cost_per_byte` = `50`

---

## File Map

| File | Role |
|---|---|
| `crates/sui-types/src/gcp_attestation.rs` | JWT parsing + RSA verification core |
| `crates/sui-types/src/unit_tests/gcp_attestation_tests.rs` | 13 Rust unit tests |
| `sui-execution/latest/sui-move-natives/src/crypto/gcp_attestation.rs` | Native dispatch + gas charging |
| `crates/sui-framework/packages/sui-framework/sources/crypto/gcp_attestation.move` | Move API |
| `crates/sui-framework/packages/sui-framework/tests/crypto/gcp_attestation_test.move` | 3 Move tests |
| `crates/sui-node/src/lib.rs` | Validator JWKS fetcher (`start_gcp_jwk_updater`, `fetch_gcp_jwks`, `parse_gcp_jwks`) |
| `crates/sui-protocol-config/src/lib.rs` | Feature flag + gas config (version 115) |
| `crates/sui-framework/packages/sui-framework/sources/authenticator_state.move` | `get_jwk_by_kid` accessor (used by TS SDK path) |
