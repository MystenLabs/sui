// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Transaction fallback tests for [`crate::store::DataStore`]. Covers the local-hit
//! path and the remote-fallback pre-fork guard. Wired in from `store.rs` via a
//! `#[path]` module so it has `super::*` access to `pub(crate)` items.

use fastcrypto::encoding::{Base64 as FastCryptoBase64, Encoding};
use serde_json::json;
use sui_types::full_checkpoint_content::ExecutedTransaction;
use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
use sui_types::transaction::{Transaction as SuiTransaction, VerifiedTransaction};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

fn executed_transaction() -> ExecutedTransaction {
    TestCheckpointBuilder::new(1)
        .start_transaction(0)
        .finish_transaction()
        .build_checkpoint()
        .transactions
        .into_iter()
        .next()
        .expect("checkpoint should have one transaction")
}

fn signed_transaction(executed: &ExecutedTransaction) -> VerifiedTransaction {
    VerifiedTransaction::new_unchecked(SuiTransaction::from_generic_sig_data(
        executed.transaction.clone(),
        executed.signatures.clone(),
    ))
}

fn empty_events_response() -> serde_json::Value {
    json!({
        "data": {
            "transaction": {
                "effects": {
                    "events": {
                        "nodes": [],
                        "pageInfo": {
                            "hasNextPage": false,
                            "endCursor": null
                        }
                    }
                }
            }
        }
    })
}

/// Mount a wiremock mock for the events query (any POST containing `"events"` in the query).
async fn mount_events_mock(server: &MockServer) {
    // The events query has an `$after` variable that the txn query does not.
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(json!({ "variables": { "first": 50 } })))
        .respond_with(ResponseTemplate::new(200).set_body_json(empty_events_response()))
        .mount(server)
        .await;
}

fn transaction_response(executed: &ExecutedTransaction, checkpoint: u64) -> serde_json::Value {
    let signatures: Vec<_> = executed
        .signatures
        .iter()
        .map(|sig| {
            json!({
                "signatureBytes": FastCryptoBase64::from_bytes(sig.as_ref()).encoded(),
            })
        })
        .collect();
    json!({
        "data": {
            "transaction": {
                "transactionBcs": FastCryptoBase64::from_bytes(
                    &bcs::to_bytes(&executed.transaction).expect("transaction data should serialize"),
                )
                .encoded(),
                "signatures": signatures,
                "effects": {
                    "checkpoint": { "sequenceNumber": checkpoint },
                    "effectsBcs": FastCryptoBase64::from_bytes(
                        &bcs::to_bytes(&executed.effects).expect("effects should serialize"),
                    )
                    .encoded(),
                }
            }
        }
    })
}

#[tokio::test]
async fn local_hit_returns_transaction_without_remote() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = DataStore::new_for_testing(temp.path().to_path_buf());

    let executed = executed_transaction();
    let verified = signed_transaction(&executed);
    let digest = *verified.digest();

    // Pre-populate the filesystem; the fake GraphQL URL in `new_for_testing`
    // would fail if the remote path were reached.
    store
        .local
        .write_transaction(&digest, &verified)
        .expect("write transaction");
    store
        .local
        .write_transaction_effects(&digest, &executed.effects)
        .expect("write effects");

    let got = DataStore::get_transaction(&store, &digest)
        .expect("local hit should not error")
        .expect("transaction should be cached");
    assert_eq!(*got.digest(), digest);

    let got_effects = DataStore::get_transaction_effects(&store, &digest)
        .expect("local hit should not error")
        .expect("effects should be cached");
    assert_eq!(got_effects, executed.effects);
}

#[tokio::test]
async fn remote_fallback_caches_pre_fork_transaction() {
    let server = MockServer::start().await;
    let executed = executed_transaction();
    let verified = signed_transaction(&executed);
    let digest = *verified.digest();
    let digest_str = digest.base58_encode();

    // Events mock must be mounted first — wiremock matches in reverse order,
    // so the more-specific events mock (with `"first": 50`) is tried before
    // the catch-all transaction mock.
    mount_events_mock(&server).await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(transaction_response(&executed, 5)))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let store = DataStore::new_for_testing_with_remote(temp.path().to_path_buf(), server.uri(), 10);

    let got = DataStore::get_transaction(&store, &digest)
        .expect("remote fetch should succeed")
        .expect("transaction should be fetched");
    assert_eq!(got.digest().base58_encode(), digest_str);

    // Second call should hit the filesystem cache, not the remote.
    assert!(
        store
            .local
            .get_transaction(&digest)
            .expect("local lookup after remote hit")
            .is_some(),
        "transaction should have been written back to local cache",
    );
    assert!(
        store
            .local
            .get_transaction_effects(&digest)
            .expect("local effects lookup after remote hit")
            .is_some(),
        "effects should have been written back to local cache",
    );
}

#[tokio::test]
async fn remote_fallback_rejects_post_fork_transaction() {
    let server = MockServer::start().await;
    let executed = executed_transaction();
    let verified = signed_transaction(&executed);
    let digest = *verified.digest();

    // Remote reports this tx was executed at checkpoint 42, but the fork is pinned at 10.
    // The post-fork guard rejects before reaching the events fetch, but mount
    // the events mock anyway for robustness.
    mount_events_mock(&server).await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(transaction_response(&executed, 42)))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let store = DataStore::new_for_testing_with_remote(temp.path().to_path_buf(), server.uri(), 10);

    let got = DataStore::get_transaction(&store, &digest)
        .expect("remote fetch should not error on post-fork digest");
    assert!(got.is_none(), "post-fork transaction must not be returned");

    // And it must not have been cached to disk.
    assert!(
        store
            .local
            .get_transaction(&digest)
            .expect("local lookup after rejected remote")
            .is_none(),
        "post-fork transaction must not be written to the local cache",
    );
}
