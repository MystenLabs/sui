// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::full_checkpoint_content::ExecutedTransaction;
use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
use sui_types::transaction::Transaction as SuiTransaction;

use crate::services::ServiceManager;

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

fn signed_transaction(executed: &ExecutedTransaction) -> VerifiedTransaction {
    VerifiedTransaction::new_unchecked(SuiTransaction::from_generic_sig_data(
        executed.transaction.clone(),
        executed.signatures.clone(),
    ))
}

#[test]
fn fallback_on_missing_returns_primary_success_without_calling_fallback() {
    let value = fallback_on_missing(Ok(7), || panic!("fallback should not be called"))
        .expect("primary value should be returned");

    assert_eq!(value, 7);
}

#[test]
fn fallback_on_missing_calls_fallback_only_for_missing_errors() {
    let value = fallback_on_missing(Err(StorageError::missing("missing")), || Ok(9))
        .expect("missing errors should use the fallback");

    assert_eq!(value, 9);
}

#[test]
fn fallback_on_missing_propagates_non_missing_errors() {
    let err = fallback_on_missing::<u8>(Err(StorageError::custom("boom")), || Ok(9))
        .expect_err("custom errors should propagate");

    assert_eq!(err.kind(), StorageErrorKind::Custom);
}

#[test]
fn ledger_indexes_delegate_to_rpc_store() {
    let temp = tempfile::tempdir().expect("tempdir");
    let services = ServiceManager::open(
        temp.path(),
        "custom".to_owned(),
        0,
        CheckpointDigest::new([9; 32]).into(),
    )
    .expect("service manager should open");
    let store = ForkStore::new_for_testing(temp.path().to_path_buf(), services.local_store());

    let (checkpoint, contents, executed) = checkpoint_with_transaction(1);
    let transaction = signed_transaction(&executed);
    let digest = *transaction.digest();
    let (tx_sequence_number, tx_offset) = contents
        .enumerate_transactions(checkpoint.data())
        .enumerate()
        .find_map(|(offset, (tx_seq, execution))| {
            (execution.transaction == digest).then_some((tx_seq, offset))
        })
        .expect("checkpoint contents should include transaction");
    let tx_offset = u32::try_from(tx_offset).expect("checkpoint offset fits in u32");

    services
        .local_store()
        .save_checkpoint(&checkpoint, &contents)
        .expect("checkpoint should persist");
    services
        .local_store()
        .save_transaction(
            &checkpoint,
            &contents,
            &transaction,
            &executed.effects,
            &TransactionEvents::default(),
        )
        .expect("transaction should persist");

    let row = RpcIndexes::ledger_tx_seq_digest(&store, tx_sequence_number)
        .expect("ledger lookup should read rpc store")
        .expect("ledger row should exist");
    assert_eq!(row.tx_sequence_number, tx_sequence_number);
    assert_eq!(row.digest, digest);
    assert_eq!(row.tx_offset, tx_offset);
    assert_eq!(row.checkpoint_number, checkpoint.data().sequence_number);

    let multi = RpcIndexes::ledger_tx_seq_digest_multi_get(&store, &[tx_sequence_number])
        .expect("multi-get should use ledger lookup");
    assert_eq!(multi, vec![Some(row)]);

    let rows = RpcIndexes::ledger_tx_seq_digest_iter(
        &store,
        tx_sequence_number,
        tx_sequence_number + 1,
        false,
    )
    .expect("ledger iterator should read rpc store")
    .collect::<Result<Vec<_>, _>>()
    .expect("ledger iterator should decode rows");
    assert_eq!(rows, vec![row]);

    let transaction_bitmap_rows =
        RpcIndexes::transaction_bitmap_bucket_iter(&store, vec![1], 0, 1, false)
            .expect("transaction bitmap iterator should read rpc store")
            .collect::<Result<Vec<_>, _>>()
            .expect("transaction bitmap iterator should decode rows");
    assert!(transaction_bitmap_rows.is_empty());

    let event_bitmap_rows = RpcIndexes::event_bitmap_bucket_iter(&store, vec![1], 0, 1, false)
        .expect("event bitmap iterator should read rpc store")
        .collect::<Result<Vec<_>, _>>()
        .expect("event bitmap iterator should decode rows");
    assert!(event_bitmap_rows.is_empty());
}
