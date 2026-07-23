// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use crate::authority::authority_per_epoch_store::ExecutionIndices;
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::consensus_handler::{SequencedConsensusTransaction, SequencedConsensusTransactionKind};
use fastcrypto_zkp::bn254::zk_login::{JWK, JwkId};
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::TransactionDigest;
use sui_types::messages_consensus::{AuthorityIndex, ConsensusTransaction};
use sui_types::transaction::TransactionKey;
use tokio::time::timeout;

/// Minimal base64url (no padding) encoder, so these tests don't need a `base64` crate
/// dependency just to build fixture strings.
fn base64url_nopad(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        let chars = [
            ALPHABET[(n >> 18 & 0x3F) as usize],
            ALPHABET[(n >> 12 & 0x3F) as usize],
            ALPHABET[(n >> 6 & 0x3F) as usize],
            ALPHABET[(n & 0x3F) as usize],
        ];
        out.push_str(std::str::from_utf8(&chars).unwrap());
        out.truncate(out.len() - (3 - chunk.len()));
    }
    out
}

/// A structurally valid GCP-issuer JWK: RSA/RS256, a 2048-bit odd modulus (0x80 followed by
/// 255 bytes of 0xFF) with no leading zero byte, and exponent 65537 ("AQAB").
fn valid_gcp_jwk() -> (JwkId, JWK) {
    let mut n = vec![0xFFu8; 256];
    n[0] = 0x80; // top bit set: exactly 2048 numeric bits, no leading zero byte.
    (
        JwkId {
            iss: sui_types::gcp_attestation::GCP_ISSUER.to_string(),
            kid: "test-kid".to_string(),
        },
        JWK {
            kty: "RSA".to_string(),
            e: "AQAB".to_string(),
            n: base64url_nopad(&n),
            alg: sui_types::gcp_attestation::RS256_ALG.to_string(),
        },
    )
}

/// Wraps a `NewJWKFetched` transaction as the `SequencedConsensusTransaction` that
/// `AuthorityPerEpochStore::verify_consensus_transaction` receives during both live commit
/// processing and catch-up/replay of previously-sequenced commits (there is a single code
/// path for both).
fn sequenced_jwk_fetched(
    authority: sui_types::base_types::AuthorityName,
    id: JwkId,
    jwk: JWK,
) -> SequencedConsensusTransaction {
    SequencedConsensusTransaction {
        certificate_author_index: 0 as AuthorityIndex,
        certificate_author: authority,
        consensus_index: ExecutionIndices::default(),
        transaction: SequencedConsensusTransactionKind::External(
            ConsensusTransaction::new_jwk_fetched(authority, id, jwk),
        ),
    }
}

fn protocol_config_with_gcp_consensus_validation(gated_on: bool) -> ProtocolConfig {
    let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
    config.set_enable_gcp_consensus_validation_for_testing(gated_on);
    config
}

/// Catch-up/replay verification (`verify_consensus_transaction`) must reject a structurally
/// invalid GCP JWK exactly like the live consensus validator does, when the feature is gated
/// on. This exercises the same code path used both while live-processing a freshly sequenced
/// commit and while replaying/catching-up on commits sequenced while the node was behind.
#[tokio::test]
async fn verify_consensus_transaction_rejects_invalid_gcp_jwk_when_gated_on() {
    let state = TestAuthorityBuilder::new()
        .with_protocol_config(protocol_config_with_gcp_consensus_validation(true))
        .build()
        .await;
    let epoch_store = state.epoch_store_for_testing();

    let (id, mut jwk) = valid_gcp_jwk();
    jwk.kty = "EC".to_string(); // structurally invalid: wrong key type.
    let seq = sequenced_jwk_fetched(state.name, id, jwk);

    let result = epoch_store.verify_consensus_transaction(seq);
    assert!(
        result.is_none(),
        "invalid GCP JWK must be filtered out by catch-up/replay verification when gated on"
    );
}

/// A structurally valid GCP JWK must still pass catch-up/replay verification when the gate
/// is on.
#[tokio::test]
async fn verify_consensus_transaction_accepts_valid_gcp_jwk_when_gated_on() {
    let state = TestAuthorityBuilder::new()
        .with_protocol_config(protocol_config_with_gcp_consensus_validation(true))
        .build()
        .await;
    let epoch_store = state.epoch_store_for_testing();

    let (id, jwk) = valid_gcp_jwk();
    let seq = sequenced_jwk_fetched(state.name, id, jwk);

    let result = epoch_store.verify_consensus_transaction(seq);
    assert!(
        result.is_some(),
        "valid GCP JWK should pass catch-up/replay verification"
    );
}

/// When `enable_gcp_consensus_validation` is off, catch-up/replay verification must not run
/// GCP-specific checks at all (identical gating to the live consensus validator).
#[tokio::test]
async fn verify_consensus_transaction_accepts_invalid_gcp_jwk_when_gated_off() {
    let state = TestAuthorityBuilder::new()
        .with_protocol_config(protocol_config_with_gcp_consensus_validation(false))
        .build()
        .await;
    let epoch_store = state.epoch_store_for_testing();

    let (id, mut jwk) = valid_gcp_jwk();
    jwk.kty = "EC".to_string(); // structurally invalid, but the gate is off.
    let seq = sequenced_jwk_fetched(state.name, id, jwk);

    let result = epoch_store.verify_consensus_transaction(seq);
    assert!(
        result.is_some(),
        "when the gate is off, catch-up/replay verification must not reject GCP JWKs"
    );
}

/// Generic (non-GCP) issuers must never be touched by GCP validation during catch-up/replay,
/// even when the gate is on: an "invalid" generic JWK sails through untouched, exactly as
/// before (zkLogin regression guard).
#[tokio::test]
async fn verify_consensus_transaction_accepts_invalid_non_gcp_jwk_when_gated_on() {
    let state = TestAuthorityBuilder::new()
        .with_protocol_config(protocol_config_with_gcp_consensus_validation(true))
        .build()
        .await;
    let epoch_store = state.epoch_store_for_testing();

    let id = JwkId {
        iss: "https://accounts.google.com".to_string(),
        kid: "generic-kid".to_string(),
    };
    let jwk = JWK {
        kty: "not-even-RSA".to_string(),
        e: "not-base64!!".to_string(),
        n: "not-base64!!".to_string(),
        alg: "none".to_string(),
    };
    let seq = sequenced_jwk_fetched(state.name, id, jwk);

    let result = epoch_store.verify_consensus_transaction(seq);
    assert!(
        result.is_some(),
        "non-GCP issuers must be unaffected by GCP JWK validation during catch-up/replay"
    );
}

#[tokio::test]
async fn test_notify_read_executed_transactions_to_checkpoint() {
    let authority_state = TestAuthorityBuilder::new().build().await;
    let store = authority_state.epoch_store_for_testing();
    let checkpoint_sequence_1 = 10;
    let checkpoint_sequence_2 = 12;

    let txes_to_be_notified = vec![
        TransactionDigest::random(),
        TransactionDigest::random(),
        TransactionDigest::random(),
    ];

    // Insert only the first transaction already
    store
        .insert_finalized_transactions(
            vec![txes_to_be_notified[0]].as_slice(),
            checkpoint_sequence_1,
        )
        .expect("Should not fail");

    // Now register to get notified for the addition of some of the above transactions
    let txes_to_be_notified_cloned = txes_to_be_notified.clone();
    let handle = tokio::spawn(async move {
        let notify = store.transactions_executed_in_checkpoint_notify(txes_to_be_notified_cloned);
        notify.await
    });

    // Now insert the rest of the transactions
    let store = authority_state.epoch_store_for_testing();
    store
        .insert_finalized_transactions(&txes_to_be_notified[1..], checkpoint_sequence_2)
        .expect("Should not fail");

    // We should get notified about all the transactions having been executed via checkpoints
    let _ = timeout(Duration::from_secs(5), handle)
        .await
        .expect("Should not timeout")
        .expect("Should not fail");

    // And the transactions should be found into the table
    let result = store
        .multi_get_transaction_checkpoint(txes_to_be_notified.as_slice())
        .expect("Should not fail");
    assert_eq!(result.len(), txes_to_be_notified.len());

    assert_eq!(result[0].unwrap(), checkpoint_sequence_1);
    assert_eq!(result[1].unwrap(), checkpoint_sequence_2);
    assert_eq!(result[2].unwrap(), checkpoint_sequence_2);
}

/// Verifies that calling `notify_barrier_executed` with an `AccumulatorSettlement`
/// key resolves `notify_read_tx_key_to_digest`, which is the mechanism used by the
/// scheduler to detect that the barrier transaction has already been executed
/// (e.g. by the checkpoint executor).
///
/// Unlike `insert_tx_key`, `notify_barrier_executed` is in-memory-only (no DB
/// persistence), so it won't leave stale entries that survive a crash while
/// the effects may not.
#[tokio::test]
async fn test_notify_barrier_executed_resolves_tx_key_wait() {
    let authority_state = TestAuthorityBuilder::new().build().await;
    let store = authority_state.epoch_store_for_testing();

    let epoch = store.epoch();
    let checkpoint_height = 42u64;
    let key = TransactionKey::AccumulatorSettlement(epoch, checkpoint_height);
    let barrier_digest = TransactionDigest::random();

    let store_clone = authority_state.epoch_store_for_testing();
    let handle = tokio::spawn(async move {
        let keys = [key];
        store_clone
            .notify_read_tx_key_to_digest(&keys)
            .await
            .is_ok()
    });

    // Give the spawned task time to register with notify_read_tx_key_to_digest.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Simulate what commit_certificate does after writing effects:
    // it fires an in-memory notify for barrier transactions.
    store.notify_barrier_executed(key, barrier_digest);

    let resolved_via_tx_key = timeout(Duration::from_secs(5), handle)
        .await
        .expect("should not timeout")
        .expect("task should not panic");

    assert!(
        resolved_via_tx_key,
        "notify_read_tx_key_to_digest should resolve after notify_barrier_executed"
    );
}
