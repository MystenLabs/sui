// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use move_core_types::identifier::Identifier;
use rsa::RsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::signature::{RandomizedSigner, SignatureEncoding};
use rsa::traits::PublicKeyParts;
use sha2::Sha256;
use sui_macros::sim_test;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{CallArg, TransactionData, TransactionKind};
use test_cluster::TestClusterBuilder;

// --- Helpers ---------------------------------------------------------------

/// Generates a 2048-bit RSA-PKCS#1v15-SHA256 signing key and its public (n, e) big-endian bytes.
fn make_test_key() -> (SigningKey<Sha256>, Vec<u8>, Vec<u8>) {
    let mut rng = rand::thread_rng();
    let priv_key = RsaPrivateKey::new(&mut rng, 2048).expect("key generation failed");
    let n = priv_key.n().to_bytes_be();
    let e = priv_key.e().to_bytes_be();
    (SigningKey::<Sha256>::new(priv_key), n, e)
}

/// Builds a minimal GCP Confidential Spaces JWT payload with the given timestamps.
fn make_gcp_payload(exp: u64, iat: u64) -> serde_json::Value {
    serde_json::json!({
        "iss": "https://confidentialcomputing.googleapis.com",
        "sub": "//gce/projects/test-project/zones/us-central1-a/instances/test-vm",
        "aud": "test-audience",
        "exp": exp,
        "iat": iat,
        "eat_nonce": ["deadbeef01020304050607080910111213141516"],
        "secboot": true,
        "hwmodel": "GCP_AMD_SEV",
        "swname": "CONFIDENTIAL_SPACE",
        "dbgstat": "disabled-since-boot",
        "swversion": ["241201"],
        "submods": {
            "container": {
                "image_digest": "sha256:abc123def456",
                "image_reference": "us-docker.pkg.dev/test-project/repo/image:latest",
                "restart_policy": "Never"
            }
        }
    })
}

/// Signs a JWT (RS256) with the given key and returns the compact serialization as bytes.
fn sign_gcp_jwt(signing_key: &SigningKey<Sha256>, payload: &serde_json::Value) -> Vec<u8> {
    let header = serde_json::json!({"alg": "RS256", "typ": "JWT"});
    let header_b64 =
        URL_SAFE_NO_PAD.encode(serde_json::to_string(&header).unwrap().as_bytes());
    let payload_b64 =
        URL_SAFE_NO_PAD.encode(serde_json::to_string(payload).unwrap().as_bytes());
    let signing_input = format!("{}.{}", header_b64, payload_b64);
    let mut rng = rand::thread_rng();
    let sig = signing_key.sign_with_rng(&mut rng, signing_input.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(sig.to_bytes().as_ref());
    format!("{}.{}", signing_input, sig_b64).into_bytes()
}

/// Returns current Unix time in seconds, used to set JWT timestamps relative to the cluster clock.
fn unix_now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_secs()
}

// --- Tests -----------------------------------------------------------------

/// A valid RS256 JWT with matching (n, e) must execute successfully on-chain.
#[sim_test]
async fn test_gcp_attestation_verifies_on_chain() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    let test_cluster = TestClusterBuilder::new().build().await;

    let (signing_key, jwk_n, jwk_e) = make_test_key();
    let now = unix_now_secs();
    let token = sign_gcp_jwt(&signing_key, &make_gcp_payload(now + 3600, now - 10));

    let sender = test_cluster.get_address_0();
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .wallet
        .gas_objects(sender)
        .await
        .expect("failed to get gas objects")
        .pop()
        .expect("no gas objects")
        .1
        .compute_object_reference();

    let mut ptb = ProgrammableTransactionBuilder::new();
    let token_arg = ptb.pure(token).unwrap();
    let n_arg = ptb.pure(jwk_n).unwrap();
    let e_arg = ptb.pure(jwk_e).unwrap();
    // CallArg::CLOCK_IMM is the pre-built immutable shared reference to the on-chain Clock (0x6).
    let clock_arg = ptb.input(CallArg::CLOCK_IMM).unwrap();
    ptb.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("gcp_attestation").unwrap(),
        Identifier::new("verify_gcp_attestation").unwrap(),
        vec![],
        vec![token_arg, n_arg, e_arg, clock_arg],
    );

    let tx_data = TransactionData::new(
        TransactionKind::ProgrammableTransaction(ptb.finish()),
        sender,
        gas,
        10_000_000,
        rgp,
    );
    let tx = test_cluster.wallet.sign_transaction(&tx_data).await;
    // execute_transaction panics on failure, providing a clear assertion message.
    test_cluster.execute_transaction(tx).await;
}

/// A JWT signed with key A but verified against key B's public key must abort.
#[sim_test]
async fn test_gcp_attestation_rejects_wrong_key() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    let test_cluster = TestClusterBuilder::new().build().await;

    let (signing_key_a, _, _) = make_test_key();
    let (_, jwk_n_b, jwk_e_b) = make_test_key(); // mismatched public key
    let now = unix_now_secs();
    let token = sign_gcp_jwt(&signing_key_a, &make_gcp_payload(now + 3600, now - 10));

    let sender = test_cluster.get_address_0();
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .wallet
        .gas_objects(sender)
        .await
        .expect("failed to get gas objects")
        .pop()
        .expect("no gas objects")
        .1
        .compute_object_reference();

    let mut ptb = ProgrammableTransactionBuilder::new();
    let token_arg = ptb.pure(token).unwrap();
    let n_arg = ptb.pure(jwk_n_b).unwrap();
    let e_arg = ptb.pure(jwk_e_b).unwrap();
    let clock_arg = ptb.input(CallArg::CLOCK_IMM).unwrap();
    ptb.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("gcp_attestation").unwrap(),
        Identifier::new("verify_gcp_attestation").unwrap(),
        vec![],
        vec![token_arg, n_arg, e_arg, clock_arg],
    );

    let tx_data = TransactionData::new(
        TransactionKind::ProgrammableTransaction(ptb.finish()),
        sender,
        gas,
        10_000_000,
        rgp,
    );
    let tx = test_cluster.wallet.sign_transaction(&tx_data).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(tx)
        .await
        .expect("execution transport error");
    assert!(
        effects.status().is_err(),
        "wrong public key must cause transaction failure"
    );
}
