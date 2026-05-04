// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use criterion::*;
use rsa::pkcs1v15::{SigningKey, VerifyingKey};
use rsa::signature::{RandomizedSigner, SignatureEncoding, Verifier};
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use sha2::Sha256;
use sui_types::gcp_attestation::verify_gcp_attestation;

// Timestamp between the realistic payload's iat (1_748_882_825s) and far-future exp.
const BENCH_TIMESTAMP_MS: u64 = 1_748_882_826_000;

fn make_key_2048() -> (RsaPrivateKey, RsaPublicKey) {
    let mut rng = rand::thread_rng();
    let priv_key = RsaPrivateKey::new(&mut rng, 2048).expect("key generation failed");
    let pub_key = RsaPublicKey::from(&priv_key);
    (priv_key, pub_key)
}

/// Builds a realistic GCP Confidential Spaces payload including all standard top-level fields
/// and the full submods structure to get representative token sizes.
fn make_realistic_payload() -> serde_json::Value {
    serde_json::json!({
        "iss": "https://confidentialcomputing.googleapis.com",
        "sub": "https://www.googleapis.com/compute/v1/projects/k8s-clusters-397521/zones/us-central1-b/instances/confspace-hello",
        "aud": "https://my-verifier.example",
        "exp": 9_999_999_999u64,
        "iat": 1_748_882_825u64,
        "nbf": 1_748_882_825u64,
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
    })
}

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

fn gcp_attestation_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("gcp_attestation");

    let (priv_key, pub_key) = make_key_2048();
    let signing_key = SigningKey::<Sha256>::new(priv_key);
    let verifying_key = VerifyingKey::<Sha256>::new(pub_key.clone());
    let n_bytes = pub_key.n().to_bytes_be();
    let e_bytes = pub_key.e().to_bytes_be();

    let payload = make_realistic_payload();
    let jwt_bytes = make_jwt(&signing_key, &payload);
    let jwt_str = std::str::from_utf8(&jwt_bytes).unwrap();

    // Extract components for the isolated benchmarks.
    let parts: Vec<&str> = jwt_str.splitn(3, '.').collect();
    let (header_b64, payload_b64, sig_b64) = (parts[0], parts[1], parts[2]);
    let signed_message = &jwt_bytes[..header_b64.len() + 1 + payload_b64.len()];
    let sig_bytes = URL_SAFE_NO_PAD.decode(sig_b64).unwrap();
    let rsa_sig =
        rsa::pkcs1v15::Signature::try_from(sig_bytes.as_slice()).expect("valid signature");

    // Benchmark the header + payload base64-decode and JSON-parse work (no crypto).
    let header_bytes = URL_SAFE_NO_PAD.decode(header_b64).unwrap();
    let payload_bytes = URL_SAFE_NO_PAD.decode(payload_b64).unwrap();
    group.bench_function("jwt_parse_only", |b| {
        b.iter(|| {
            let _hb = URL_SAFE_NO_PAD
                .decode(black_box(header_b64))
                .expect("decode");
            let _: serde_json::Value =
                serde_json::from_slice(black_box(&header_bytes)).expect("parse");
            let _pb = URL_SAFE_NO_PAD
                .decode(black_box(payload_b64))
                .expect("decode");
            let _: serde_json::Value =
                serde_json::from_slice(black_box(&payload_bytes)).expect("parse");
        })
    });

    // Benchmark just the RSA-2048 PKCS#1v15 SHA-256 signature verification.
    group.bench_function("rsa_2048_verify_only", |b| {
        b.iter(|| {
            verifying_key
                .verify(black_box(signed_message), black_box(&rsa_sig))
                .expect("signature should verify");
        })
    });

    // Benchmark the full end-to-end verify_gcp_attestation path.
    group.bench_function("full_verify_gcp_attestation", |b| {
        b.iter(|| {
            verify_gcp_attestation(
                black_box(&jwt_bytes),
                black_box(&n_bytes),
                black_box(&e_bytes),
                black_box(BENCH_TIMESTAMP_MS),
            )
            .expect("verification should succeed")
        })
    });

    group.finish();
}

criterion_group!(benches, gcp_attestation_benchmark);
criterion_main!(benches);
