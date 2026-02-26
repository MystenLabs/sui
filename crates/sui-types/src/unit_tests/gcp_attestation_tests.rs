// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::gcp_attestation::{GcpAttestationError, extract_kid_from_jwt, verify_gcp_attestation};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rsa::pkcs1v15::SigningKey;
use rsa::signature::{RandomizedSigner, SignatureEncoding};
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use sha2::Sha256;

const EXPECTED_ISSUER: &str = "https://confidentialcomputing.googleapis.com";

/// Build a minimal GCP Confidential Spaces JWT payload.
fn make_payload(iss: &str, exp: u64, iat: u64) -> serde_json::Value {
    serde_json::json!({
        "iss": iss,
        "sub": "test-subject",
        "aud": "test-audience",
        "exp": exp,
        "iat": iat,
        "eat_nonce": ["nonce1", "nonce2"],
        "secboot": true,
        "hwmodel": "GCP_AMD_SEV",
        "swname": "CONFIDENTIAL_SPACE",
        "dbgstat": "disabled-since-boot",
        "swversion": ["231031"],
        "submods": {
            "container": {
                "image_digest": "sha256:abc123",
                "image_reference": "us-docker.pkg.dev/project/repo/image:latest",
                "restart_policy": "Never"
            }
        }
    })
}

/// Sign `header.payload` with the given signing key and return the full JWT string bytes.
fn make_jwt(signing_key: &SigningKey<Sha256>, payload: &serde_json::Value) -> Vec<u8> {
    let header = serde_json::json!({"alg": "RS256", "typ": "JWT"});
    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string().as_bytes());
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload.to_string().as_bytes());
    let signing_input = format!("{}.{}", header_b64, payload_b64);

    let mut rng = rand::thread_rng();
    let sig = signing_key.sign_with_rng(&mut rng, signing_input.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(sig.to_bytes().as_ref());

    format!("{}.{}", signing_input, sig_b64).into_bytes()
}

/// Extract (n_bytes, e_bytes) from an RsaPublicKey.
fn key_to_ne(pub_key: &RsaPublicKey) -> (Vec<u8>, Vec<u8>) {
    (pub_key.n().to_bytes_be(), pub_key.e().to_bytes_be())
}

fn make_test_key() -> (RsaPrivateKey, RsaPublicKey) {
    // 2048-bit keys: minimum accepted by ring's RSA_PKCS1_2048_8192_SHA256.
    let mut rng = rand::thread_rng();
    let priv_key = RsaPrivateKey::new(&mut rng, 2048).expect("key generation failed");
    let pub_key = RsaPublicKey::from(&priv_key);
    (priv_key, pub_key)
}

#[test]
fn test_valid_attestation() {
    let (priv_key, pub_key) = make_test_key();
    let signing_key = SigningKey::<Sha256>::new(priv_key);
    let (n, e) = key_to_ne(&pub_key);

    let now_ms: u64 = 1_700_000_000_000;
    let now_secs = now_ms / 1000;
    let payload = make_payload(EXPECTED_ISSUER, now_secs + 3600, now_secs - 10);
    let token = make_jwt(&signing_key, &payload);

    let doc = verify_gcp_attestation(&token, &n, &e, now_ms).expect("should verify");
    assert_eq!(doc.iss, EXPECTED_ISSUER.as_bytes());
    assert_eq!(doc.sub, b"test-subject");
    assert_eq!(doc.aud, b"test-audience");
    assert_eq!(doc.exp, now_secs + 3600);
    assert_eq!(doc.iat, now_secs - 10);
    assert_eq!(doc.eat_nonce, vec![b"nonce1".to_vec(), b"nonce2".to_vec()]);
    assert!(doc.secboot);
    assert_eq!(doc.hwmodel, b"GCP_AMD_SEV");
    assert_eq!(doc.swname, b"CONFIDENTIAL_SPACE");
    assert_eq!(doc.dbgstat, b"disabled-since-boot");
    assert_eq!(doc.swversion, vec![b"231031".to_vec()]);
    assert_eq!(doc.image_digest, b"sha256:abc123");
    assert_eq!(
        doc.image_reference,
        b"us-docker.pkg.dev/project/repo/image:latest"
    );
    assert_eq!(doc.restart_policy, b"Never");
}

#[test]
fn test_expired_token() {
    let (priv_key, pub_key) = make_test_key();
    let signing_key = SigningKey::<Sha256>::new(priv_key);
    let (n, e) = key_to_ne(&pub_key);

    let now_ms: u64 = 1_700_000_000_000;
    let now_secs = now_ms / 1000;
    // exp is in the past
    let payload = make_payload(EXPECTED_ISSUER, now_secs - 1, now_secs - 3600);
    let token = make_jwt(&signing_key, &payload);

    let err = verify_gcp_attestation(&token, &n, &e, now_ms).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::VerifyError("token has expired".to_string())
    );
}

#[test]
fn test_invalid_issuer() {
    let (priv_key, pub_key) = make_test_key();
    let signing_key = SigningKey::<Sha256>::new(priv_key);
    let (n, e) = key_to_ne(&pub_key);

    let now_ms: u64 = 1_700_000_000_000;
    let now_secs = now_ms / 1000;
    let payload = make_payload(
        "https://wrong.issuer.example.com",
        now_secs + 3600,
        now_secs - 10,
    );
    let token = make_jwt(&signing_key, &payload);

    let err = verify_gcp_attestation(&token, &n, &e, now_ms).unwrap_err();
    match err {
        GcpAttestationError::VerifyError(msg) => assert!(msg.contains("invalid issuer")),
        other => panic!("unexpected error: {:?}", other),
    }
}

#[test]
fn test_wrong_key() {
    let (priv_key, _) = make_test_key();
    let signing_key = SigningKey::<Sha256>::new(priv_key);

    // Use a different public key for verification
    let (_, wrong_pub_key) = make_test_key();
    let (n, e) = key_to_ne(&wrong_pub_key);

    let now_ms: u64 = 1_700_000_000_000;
    let now_secs = now_ms / 1000;
    let payload = make_payload(EXPECTED_ISSUER, now_secs + 3600, now_secs - 10);
    let token = make_jwt(&signing_key, &payload);

    let err = verify_gcp_attestation(&token, &n, &e, now_ms).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::VerifyError("signature verification failed".to_string())
    );
}

#[test]
fn test_tampered_payload() {
    let (priv_key, pub_key) = make_test_key();
    let signing_key = SigningKey::<Sha256>::new(priv_key);
    let (n, e) = key_to_ne(&pub_key);

    let now_ms: u64 = 1_700_000_000_000;
    let now_secs = now_ms / 1000;
    let payload = make_payload(EXPECTED_ISSUER, now_secs + 3600, now_secs - 10);
    let token_str = String::from_utf8(make_jwt(&signing_key, &payload)).unwrap();

    // Tamper with the payload section (replace with a different base64 value)
    let parts: Vec<&str> = token_str.splitn(3, '.').collect();
    let tampered_payload = URL_SAFE_NO_PAD.encode(
        b"{\"iss\":\"https://confidentialcomputing.googleapis.com\",\"exp\":9999999999,\"iat\":0}",
    );
    let tampered = format!("{}.{}.{}", parts[0], tampered_payload, parts[2]);

    let err = verify_gcp_attestation(tampered.as_bytes(), &n, &e, now_ms).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::VerifyError("signature verification failed".to_string())
    );
}

#[test]
fn test_reject_none_algorithm() {
    let (_priv_key, pub_key) = make_test_key();
    let (n, e) = key_to_ne(&pub_key);

    let now_ms: u64 = 1_700_000_000_000;
    let now_secs = now_ms / 1000;
    let payload = make_payload(EXPECTED_ISSUER, now_secs + 3600, now_secs - 10);

    // Craft a JWT with alg=none (signature is empty)
    let header = serde_json::json!({"alg": "none", "typ": "JWT"});
    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string().as_bytes());
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload.to_string().as_bytes());
    let token = format!("{}.{}.", header_b64, payload_b64);

    let err = verify_gcp_attestation(token.as_bytes(), &n, &e, now_ms).unwrap_err();
    match err {
        GcpAttestationError::VerifyError(msg) => assert!(msg.contains("unsupported algorithm")),
        other => panic!("unexpected error: {:?}", other),
    }
}

#[test]
fn test_oversized_token() {
    let (_, pub_key) = make_test_key();
    let (n, e) = key_to_ne(&pub_key);

    let oversized_token = vec![b'a'; 16 * 1024 + 1];
    let err = verify_gcp_attestation(&oversized_token, &n, &e, 1_700_000_000_000).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("JWT token too large".to_string())
    );
}

#[test]
fn test_oversized_modulus() {
    let oversized_n = vec![0u8; 513];
    let e = vec![0x01, 0x00, 0x01];
    let token = b"a.b.c";
    let err = verify_gcp_attestation(token, &oversized_n, &e, 1_700_000_000_000).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("RSA modulus size out of bounds".to_string())
    );
}

#[test]
fn test_invalid_rsa_key() {
    // 1-byte modulus is below the 256-byte (2048-bit) minimum; expect ParseError.
    let n = vec![0x00];
    let e = vec![0x00];
    let now_ms: u64 = 1_700_000_000_000;

    let header_b64 = URL_SAFE_NO_PAD.encode(b"{\"alg\":\"RS256\"}");
    let payload_b64 = URL_SAFE_NO_PAD.encode(
        b"{\"iss\":\"https://confidentialcomputing.googleapis.com\",\"exp\":9999999999,\"iat\":0}",
    );
    let sig_b64 = URL_SAFE_NO_PAD.encode(b"fakesig");
    let token = format!("{}.{}.{}", header_b64, payload_b64, sig_b64);

    let err = verify_gcp_attestation(token.as_bytes(), &n, &e, now_ms).unwrap_err();
    assert!(matches!(
        err,
        GcpAttestationError::VerifyError(_) | GcpAttestationError::ParseError(_)
    ));
}

#[test]
fn test_future_iat() {
    let (priv_key, pub_key) = make_test_key();
    let signing_key = SigningKey::<Sha256>::new(priv_key);
    let (n, e) = key_to_ne(&pub_key);

    let now_ms: u64 = 1_700_000_000_000;
    let now_secs = now_ms / 1000;
    // iat is in the future
    let payload = make_payload(EXPECTED_ISSUER, now_secs + 7200, now_secs + 3600);
    let token = make_jwt(&signing_key, &payload);

    let err = verify_gcp_attestation(&token, &n, &e, now_ms).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::VerifyError("token issued in the future".to_string())
    );
}

/// Exercises a real GCP Confidential Spaces payload structure (signed with a test key).
///
/// All field values are taken verbatim from a real GCP Confidential Spaces attestation
/// token to ensure our claim extractor handles every field correctly, including:
/// - eat_nonce as a bare string (not array)
/// - submods.container.{image_digest,image_reference,restart_policy}
/// - boolean secboot, multi-valued swversion
#[test]
fn test_real_gcp_payload_claims() {
    let (priv_key, pub_key) = make_test_key();
    let signing_key = SigningKey::<Sha256>::new(priv_key);
    let (n, e) = key_to_ne(&pub_key);

    // Claims taken from a real GCP Confidential Spaces attestation JWT.
    let iat: u64 = 1_748_882_825;
    let exp: u64 = 1_748_886_425;
    let payload = serde_json::json!({
        "iss": "https://confidentialcomputing.googleapis.com",
        "sub": "https://www.googleapis.com/compute/v1/projects/k8s-clusters-397521/zones/us-central1-b/instances/confspace-hello",
        "aud": "https://my-verifier.example",
        "exp": exp,
        "iat": iat,
        "nbf": iat,
        "eat_nonce": "407e7702-cec8-446c-8d46-46698723543c",
        "eat_profile": "https://cloud.google.com/confidential-computing/confidential-space/docs/reference/token-claims",
        "secboot": true,
        "oemid": 11129,
        "hwmodel": "GCP_AMD_SEV",
        "swname": "CONFIDENTIAL_SPACE",
        "swversion": ["250301"],
        "dbgstat": "disabled-since-boot",
        "submods": {
            "confidential_space": {
                "support_attributes": ["LATEST", "STABLE", "USABLE"],
                "monitoring_enabled": {"memory": false}
            },
            "container": {
                "image_reference": "us-central1-docker.pkg.dev/k8s-clusters-397521/confspace-test/hello_world@sha256:ac91dd368193efa938776f35ff715e29f907c2b6671bae6589d44f60c8d82a54",
                "image_digest": "sha256:ac91dd368193efa938776f35ff715e29f907c2b6671bae6589d44f60c8d82a54",
                "restart_policy": "Never",
                "image_id": "sha256:82a96b936b22366526a4b1a960da3b26135e7777d5c86ecc4f48742ec0a65557",
                "env": {
                    "HOSTNAME": "confspace-hello",
                    "PATH": "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                    "SSL_CERT_FILE": "/etc/ssl/certs/ca-certificates.crt"
                },
                "args": ["/hello_world"]
            },
            "gce": {
                "zone": "us-central1-b",
                "project_id": "k8s-clusters-397521",
                "project_number": "660892227407",
                "instance_name": "confspace-hello",
                "instance_id": "587736333007598652"
            }
        },
        "google_service_accounts": ["confspace-runner@k8s-clusters-397521.iam.gserviceaccount.com"]
    });

    // Use iat as the current time — well within the exp window.
    let token = make_jwt(&signing_key, &payload);
    let doc = verify_gcp_attestation(&token, &n, &e, iat * 1000).expect("should verify");

    assert_eq!(doc.iss, b"https://confidentialcomputing.googleapis.com");
    assert_eq!(
        doc.sub,
        b"https://www.googleapis.com/compute/v1/projects/k8s-clusters-397521/zones/us-central1-b/instances/confspace-hello"
    );
    assert_eq!(doc.aud, b"https://my-verifier.example");
    assert_eq!(doc.exp, exp);
    assert_eq!(doc.iat, iat);
    // eat_nonce was a bare string in the JWT; we normalise it to a single-element vec.
    assert_eq!(
        doc.eat_nonce,
        vec![b"407e7702-cec8-446c-8d46-46698723543c".to_vec()]
    );
    assert!(doc.secboot);
    assert_eq!(doc.hwmodel, b"GCP_AMD_SEV");
    assert_eq!(doc.swname, b"CONFIDENTIAL_SPACE");
    assert_eq!(doc.swversion, vec![b"250301".to_vec()]);
    assert_eq!(doc.dbgstat, b"disabled-since-boot");
    assert_eq!(
        doc.image_digest,
        b"sha256:ac91dd368193efa938776f35ff715e29f907c2b6671bae6589d44f60c8d82a54"
    );
    assert_eq!(
        doc.image_reference,
        b"us-central1-docker.pkg.dev/k8s-clusters-397521/confspace-test/hello_world@sha256:ac91dd368193efa938776f35ff715e29f907c2b6671bae6589d44f60c8d82a54"
    );
    assert_eq!(doc.restart_policy, b"Never");
}

#[test]
fn test_extract_kid_from_jwt() {
    let header = serde_json::json!({"alg": "RS256", "typ": "JWT", "kid": "key-id-abc123"});
    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string().as_bytes());
    let token = format!("{}.fakepayload.fakesig", header_b64);

    let kid = extract_kid_from_jwt(token.as_bytes()).expect("should extract kid");
    assert_eq!(kid, "key-id-abc123");
}

#[test]
fn test_extract_kid_missing() {
    let header = serde_json::json!({"alg": "RS256", "typ": "JWT"});
    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string().as_bytes());
    let token = format!("{}.fakepayload.fakesig", header_b64);

    let err = extract_kid_from_jwt(token.as_bytes()).unwrap_err();
    assert_eq!(
        err,
        GcpAttestationError::ParseError("missing 'kid' in header".to_string())
    );
}

