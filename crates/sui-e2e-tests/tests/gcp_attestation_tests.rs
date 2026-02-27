// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
#[cfg(msim)]
use fastcrypto_zkp::bn254::zk_login::{JWK, JwkId};
use move_core_types::identifier::Identifier;
use rsa::RsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::signature::{RandomizedSigner, SignatureEncoding};
use rsa::traits::PublicKeyParts;
use sha2::Sha256;
use sui_macros::sim_test;
use sui_types::base_types::ObjectID;
use sui_types::object::Owner;
use sui_types::transaction::{CallArg, ObjectArg, SharedObjectMutability, TransactionData, TransactionKind};
use sui_types::{SUI_AUTHENTICATOR_STATE_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use test_cluster::TestClusterBuilder;

const GCP_ISS: &str = "https://confidentialcomputing.googleapis.com";
const TEST_KID: &str = "gcp-test-key-001";

// --- Helpers ---------------------------------------------------------------

/// Generates a 2048-bit RSA-PKCS#1v15-SHA256 signing key and returns:
/// - the signing key
/// - base64url-encoded modulus (n) as a string, matching AuthenticatorState's JWK format
/// - base64url-encoded exponent (e) as a string
fn make_test_key() -> (SigningKey<Sha256>, String, String) {
    let mut rng = rand::thread_rng();
    let priv_key = RsaPrivateKey::new(&mut rng, 2048).expect("key generation failed");
    let n_b64 = URL_SAFE_NO_PAD.encode(priv_key.n().to_bytes_be());
    let e_b64 = URL_SAFE_NO_PAD.encode(priv_key.e().to_bytes_be());
    (SigningKey::<Sha256>::new(priv_key), n_b64, e_b64)
}

/// Builds a minimal GCP Confidential Spaces JWT payload with the given timestamps.
fn make_gcp_payload(exp: u64, iat: u64) -> serde_json::Value {
    serde_json::json!({
        "iss": GCP_ISS,
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
fn sign_gcp_jwt(signing_key: &SigningKey<Sha256>, payload: &serde_json::Value, kid: &str) -> Vec<u8> {
    let header = serde_json::json!({"alg": "RS256", "typ": "JWT", "kid": kid});
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

fn unix_now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_secs()
}

/// Returns the initial shared version of a shared object by reading it from the chain.
async fn get_initial_shared_version(cluster: &test_cluster::TestCluster, id: ObjectID) -> sui_types::base_types::SequenceNumber {
    let obj = cluster
        .get_object_from_fullnode_store(&id)
        .await
        .expect("object should exist");
    match obj.owner() {
        Owner::Shared { initial_shared_version } => *initial_shared_version,
        other => panic!("expected shared object, got {:?}", other),
    }
}

// --- Tests -----------------------------------------------------------------

/// A valid RS256 JWT whose key is present in AuthenticatorState must verify on-chain.
///
/// The test injects a GCP JWK via the simtest injector, triggers an epoch change
/// so the key is written into AuthenticatorState, then submits a PTB that calls
/// `verify_gcp_attestation` with the matching token.
#[sim_test]
async fn test_gcp_attestation_verifies_on_chain() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    let (signing_key, _n_b64, _e_b64) = make_test_key();

    // Register the test GCP JWK in the simtest injector before building the cluster.
    // The GCP JWK updater task will pick it up on the first fetch cycle and submit
    // it to consensus; the subsequent epoch change writes it into AuthenticatorState.
    #[cfg(msim)]
    sui_node::set_gcp_jwk_injector(vec![(
        JwkId { iss: GCP_ISS.to_string(), kid: TEST_KID.to_string() },
        JWK { kty: "RSA".to_string(), e: _e_b64, n: _n_b64, alg: "RS256".to_string() },
    )]);

    let test_cluster = TestClusterBuilder::new()
        .with_jwk_fetch_interval(std::time::Duration::from_secs(1))
        .build()
        .await;

    // Trigger an epoch change to flush JWKs through consensus into AuthenticatorState.
    test_cluster.trigger_reconfiguration().await;

    let now = unix_now_secs();
    let token = sign_gcp_jwt(&signing_key, &make_gcp_payload(now + 3600, now - 10), TEST_KID);

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

    let auth_state_version =
        get_initial_shared_version(&test_cluster, SUI_AUTHENTICATOR_STATE_OBJECT_ID).await;

    let mut ptb = ProgrammableTransactionBuilder::new();
    let token_arg = ptb.pure(token).unwrap();
    let auth_state_arg = ptb
        .input(CallArg::Object(ObjectArg::SharedObject {
            id: SUI_AUTHENTICATOR_STATE_OBJECT_ID,
            initial_shared_version: auth_state_version,
            mutability: SharedObjectMutability::Immutable,
        }))
        .unwrap();
    let kid_arg = ptb.pure(TEST_KID.as_bytes().to_vec()).unwrap();
    let clock_arg = ptb.input(CallArg::CLOCK_IMM).unwrap();
    ptb.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("gcp_attestation").unwrap(),
        Identifier::new("verify_gcp_attestation").unwrap(),
        vec![],
        vec![token_arg, auth_state_arg, kid_arg, clock_arg],
    );

    let tx_data = TransactionData::new(
        TransactionKind::ProgrammableTransaction(ptb.finish()),
        sender,
        gas,
        10_000_000,
        rgp,
    );
    let tx = test_cluster.wallet.sign_transaction(&tx_data).await;
    test_cluster.execute_transaction(tx).await;
}

/// A JWT whose kid is not present in AuthenticatorState must abort.
#[sim_test]
async fn test_gcp_attestation_rejects_unknown_kid() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    let (signing_key, _, _) = make_test_key();

    // Do NOT inject any GCP JWKs — AuthenticatorState has no GCP keys.
    let test_cluster = TestClusterBuilder::new().build().await;

    let now = unix_now_secs();
    let token = sign_gcp_jwt(&signing_key, &make_gcp_payload(now + 3600, now - 10), TEST_KID);

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

    let auth_state_version =
        get_initial_shared_version(&test_cluster, SUI_AUTHENTICATOR_STATE_OBJECT_ID).await;

    let mut ptb = ProgrammableTransactionBuilder::new();
    let token_arg = ptb.pure(token).unwrap();
    let auth_state_arg = ptb
        .input(CallArg::Object(ObjectArg::SharedObject {
            id: SUI_AUTHENTICATOR_STATE_OBJECT_ID,
            initial_shared_version: auth_state_version,
            mutability: SharedObjectMutability::Immutable,
        }))
        .unwrap();
    let kid_arg = ptb.pure(TEST_KID.as_bytes().to_vec()).unwrap();
    let clock_arg = ptb.input(CallArg::CLOCK_IMM).unwrap();
    ptb.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("gcp_attestation").unwrap(),
        Identifier::new("verify_gcp_attestation").unwrap(),
        vec![],
        vec![token_arg, auth_state_arg, kid_arg, clock_arg],
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
        "unknown kid must cause transaction failure"
    );
}
