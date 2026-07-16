// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Transaction fallback tests for [`crate::store::DataStore`]. Covers the local-hit
//! path and the remote-fallback pre-fork guard. Wired in from `store.rs` via a
//! `#[path]` module so it has `super::*` access to `pub(crate)` items.

use std::path::Path;

use fastcrypto::encoding::Base64 as FastCryptoBase64;
use fastcrypto::encoding::Encoding;
use move_core_types::ident_str;
use serde_json::json;
use sui_types::base_types::ObjectID;
use sui_types::digests::CheckpointDigest;
use sui_types::event::Event;
use sui_types::full_checkpoint_content::ExecutedTransaction;
use sui_types::gas_coin::GAS;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::storage::ReadStore;
use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
use sui_types::transaction::Transaction as SuiTransaction;
use sui_types::transaction::VerifiedTransaction;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_partial_json;
use wiremock::matchers::method;
use wiremock::matchers::path;

use crate::rpc::reader::ForkRpcReader;
use crate::runtime::ForkRuntime;

use super::*;

fn checkpoint_with_transaction(
    sequence: u64,
) -> (VerifiedCheckpoint, CheckpointContents, ExecutedTransaction) {
    let checkpoint = TestCheckpointBuilder::new(sequence)
        .start_transaction(0)
        .finish_transaction()
        .build_checkpoint();
    let executed = checkpoint
        .transactions
        .into_iter()
        .next()
        .expect("checkpoint should have one transaction");
    (
        VerifiedCheckpoint::new_unchecked(checkpoint.summary),
        checkpoint.contents,
        executed,
    )
}

fn checkpoint_with_event_transaction(
    sequence: u64,
) -> (VerifiedCheckpoint, CheckpointContents, ExecutedTransaction) {
    let checkpoint = TestCheckpointBuilder::new(sequence)
        .start_transaction(0)
        .with_events(vec![Event::new(
            &ObjectID::ZERO,
            ident_str!("test"),
            TestCheckpointBuilder::derive_address(0),
            GAS::type_(),
            vec![1, 2, 3],
        )])
        .finish_transaction()
        .build_checkpoint();
    let executed = checkpoint
        .transactions
        .into_iter()
        .next()
        .expect("checkpoint should have one transaction");
    (
        VerifiedCheckpoint::new_unchecked(checkpoint.summary),
        checkpoint.contents,
        executed,
    )
}

fn executed_transaction() -> ExecutedTransaction {
    let (_, _, executed) = checkpoint_with_transaction(1);
    executed
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

async fn mount_events_error_mock(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(json!({ "variables": { "first": 50 } })))
        .respond_with(ResponseTemplate::new(500))
        .mount(server)
        .await;
}

fn open_test_runtime(root: &Path, forked_at_checkpoint: CheckpointSequenceNumber) -> ForkRuntime {
    ForkRuntime::open(
        root,
        "custom".to_owned(),
        forked_at_checkpoint,
        CheckpointDigest::new([9; 32]).into(),
    )
    .expect("fork runtime should open")
}

fn test_data_store(root: &Path) -> (DataStore, ForkRuntime) {
    let runtime = open_test_runtime(root, 0);
    let store = DataStore::new_for_testing(root.to_path_buf(), runtime.fork_rpc_store());
    (store, runtime)
}

fn test_data_store_with_remote(
    root: &Path,
    gql_url: String,
    forked_at_checkpoint: CheckpointSequenceNumber,
) -> (DataStore, ForkRuntime) {
    let runtime = open_test_runtime(root, forked_at_checkpoint);
    let store = DataStore::new_for_testing_with_remote(
        root.to_path_buf(),
        gql_url,
        forked_at_checkpoint,
        runtime.fork_rpc_store(),
    );
    (store, runtime)
}

fn checkpoint_response(
    checkpoint: &VerifiedCheckpoint,
    contents: &CheckpointContents,
) -> serde_json::Value {
    json!({
        "data": {
            "checkpoint": {
                "summaryBcs": FastCryptoBase64::from_bytes(
                    &bcs::to_bytes(checkpoint.data()).expect("summary should serialize"),
                )
                .encoded(),
                "contentBcs": FastCryptoBase64::from_bytes(
                    &bcs::to_bytes(contents).expect("contents should serialize"),
                )
                .encoded(),
                "validatorSignatures": {
                    "signature": FastCryptoBase64::from_bytes(
                        checkpoint.auth_sig().signature.as_ref(),
                    )
                    .encoded(),
                    "signersMap": checkpoint
                        .auth_sig()
                        .signers_map
                        .iter()
                        .map(|index| i32::try_from(index).expect("signer index fits in i32"))
                        .collect::<Vec<_>>(),
                },
            }
        }
    })
}

async fn mount_checkpoint_mock(
    server: &MockServer,
    checkpoint: &VerifiedCheckpoint,
    contents: &CheckpointContents,
) {
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(json!({
            "variables": {
                "sequenceNumber": checkpoint.data().sequence_number,
            }
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(checkpoint_response(checkpoint, contents)),
        )
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

async fn mount_transaction_mock(
    server: &MockServer,
    digest: &TransactionDigest,
    executed: &ExecutedTransaction,
    checkpoint: u64,
) {
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(json!({
            "variables": {
                "digest": digest.base58_encode(),
            }
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(transaction_response(executed, checkpoint)),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn rpc_store_hit_returns_transaction_without_remote() {
    let temp = tempfile::tempdir().expect("tempdir");
    let (store, runtime) = test_data_store(temp.path());

    let (checkpoint, contents, executed) = checkpoint_with_transaction(1);
    let verified = signed_transaction(&executed);
    let digest = *verified.digest();

    // Pre-populate rpc-store; the fake GraphQL URL in `new_for_testing`
    // would fail if the remote path were reached.
    let rpc_store = runtime.fork_rpc_store();
    rpc_store
        .save_checkpoint(&checkpoint, &contents)
        .expect("checkpoint should be saved");
    rpc_store
        .save_transaction(
            &checkpoint,
            &contents,
            &verified,
            &executed.effects,
            &TransactionEvents::default(),
        )
        .expect("transaction should be saved");

    let got = DataStore::get_transaction(&store, &digest)
        .expect("local hit should not error")
        .expect("transaction should be saved");
    assert_eq!(*got.digest(), digest);

    let got_effects = DataStore::get_transaction_effects(&store, &digest)
        .expect("local hit should not error")
        .expect("effects should be saved");
    assert_eq!(got_effects, executed.effects);
}

#[tokio::test]
async fn remote_fallback_saves_pre_fork_transaction_in_rpc_store() {
    let server = MockServer::start().await;
    let (checkpoint, contents, executed) = checkpoint_with_transaction(5);
    let verified = signed_transaction(&executed);
    let digest = *verified.digest();

    mount_events_mock(&server).await;
    mount_checkpoint_mock(&server, &checkpoint, &contents).await;
    mount_transaction_mock(
        &server,
        &digest,
        &executed,
        checkpoint.data().sequence_number,
    )
    .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let (store, runtime) = test_data_store_with_remote(temp.path(), server.uri(), 10);

    let got = DataStore::get_transaction(&store, &digest)
        .expect("remote fetch should succeed")
        .expect("transaction should be fetched");
    assert_eq!(*got.digest(), digest);

    let reader = runtime.reader();
    let rpc_tx = ReadStore::get_transaction(&reader, &digest)
        .expect("transaction should be saved in rpc store");
    assert_eq!(*rpc_tx.digest(), digest);
    let rpc_effects = ReadStore::get_transaction_effects(&reader, &digest)
        .expect("effects should be saved in rpc store");
    assert_eq!(rpc_effects, executed.effects);
    let rpc_events =
        ReadStore::get_events(&reader, &digest).expect("events should be saved in rpc store");
    assert_eq!(rpc_events, TransactionEvents::default());
    assert_eq!(
        ReadStore::get_transaction_checkpoint(&reader, &digest),
        Some(checkpoint.data().sequence_number),
    );

    let fallback_temp = tempfile::tempdir().expect("tempdir");
    let fallback_store = DataStore::new_for_testing_with_remote(
        fallback_temp.path().to_path_buf(),
        "http://localhost:1".to_owned(),
        store.forked_at_checkpoint(),
        runtime.fork_rpc_store(),
    );

    let fallback_tx = DataStore::get_transaction(&fallback_store, &digest)
        .expect("rpc-store transaction lookup should succeed")
        .expect("transaction should be read from rpc store");
    assert_eq!(*fallback_tx.digest(), digest);
    let fallback_effects = DataStore::get_transaction_effects(&fallback_store, &digest)
        .expect("rpc-store effects lookup should succeed")
        .expect("effects should be read from rpc store");
    assert_eq!(fallback_effects, executed.effects);
    assert_eq!(
        DataStore::get_transaction_checkpoint(&fallback_store, &digest)
            .expect("rpc-store checkpoint lookup should succeed"),
        Some(checkpoint.data().sequence_number),
    );
    let fallback_reader = ForkRpcReader::new(runtime.reader(), fallback_store);
    assert_eq!(
        ReadStore::get_events(&fallback_reader, &digest)
            .expect("events should be read from rpc store"),
        TransactionEvents::default(),
    );
}

#[tokio::test]
async fn remote_fallback_errors_when_required_events_fail_to_fetch() {
    let server = MockServer::start().await;
    let (checkpoint, contents, executed) = checkpoint_with_event_transaction(5);
    let verified = signed_transaction(&executed);
    let digest = *verified.digest();

    mount_events_error_mock(&server).await;
    mount_checkpoint_mock(&server, &checkpoint, &contents).await;
    mount_transaction_mock(
        &server,
        &digest,
        &executed,
        checkpoint.data().sequence_number,
    )
    .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let (store, runtime) = test_data_store_with_remote(temp.path(), server.uri(), 10);

    let err = DataStore::get_transaction(&store, &digest)
        .expect_err("required event fetch failure should fail transaction save");
    assert!(
        err.to_string()
            .contains("failed to fetch transaction events"),
        "unexpected error: {err:#}",
    );
    assert!(
        ReadStore::get_transaction(&runtime.reader(), &digest).is_none(),
        "failed save must not persist the transaction",
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
    let (store, _runtime) = test_data_store_with_remote(temp.path(), server.uri(), 10);

    let got = DataStore::get_transaction(&store, &digest)
        .expect("remote fetch should not error on post-fork digest");
    assert!(got.is_none(), "post-fork transaction must not be returned");
}
