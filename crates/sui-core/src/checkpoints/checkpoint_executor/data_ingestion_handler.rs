// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoints::checkpoint_executor::{CheckpointExecutionData, CheckpointTransactionData};
use crate::execution_cache::TransactionCacheRead;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use sui_storage::blob::{Blob, BlobEncoding};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::{SuiError, SuiResult};
use sui_types::full_checkpoint_content::{
    Checkpoint, CheckpointData, ExecutedTransaction, ObjectSet,
};
use sui_types::storage::ObjectStore;

pub(crate) fn store_checkpoint_locally(
    path: impl AsRef<Path>,
    checkpoint_data: &CheckpointData,
) -> SuiResult {
    let path = path.as_ref();
    let file_name = format!("{}.chk", checkpoint_data.checkpoint_summary.sequence_number);

    std::fs::create_dir_all(path).map_err(|err| {
        SuiError::FileIOError(format!(
            "failed to save full checkpoint content locally {:?}",
            err
        ))
    })?;

    Blob::encode(&checkpoint_data, BlobEncoding::Bcs)
        .map_err(|_| SuiError::TransactionSerializationError {
            error: "failed to serialize full checkpoint content".to_string(),
        }) // Map the first error
        .and_then(|blob| {
            std::fs::write(path.join(file_name), blob.to_bytes()).map_err(|_| {
                SuiError::FileIOError("failed to save full checkpoint content locally".to_string())
            })
        })?;

    Ok(())
}

pub(crate) fn load_checkpoint(
    ckpt_data: &CheckpointExecutionData,
    ckpt_tx_data: &CheckpointTransactionData,
    object_store: &dyn ObjectStore,
    transaction_cache_reader: &dyn TransactionCacheRead,
) -> SuiResult<Checkpoint> {
    let event_tx_digests = ckpt_tx_data
        .effects
        .iter()
        .flat_map(|fx| fx.events_digest().map(|_| fx.transaction_digest()).copied())
        .collect::<Vec<_>>();

    let mut events = transaction_cache_reader
        .multi_get_events(&event_tx_digests)
        .into_iter()
        .zip(event_tx_digests)
        .map(|(maybe_event, tx_digest)| {
            maybe_event
                .ok_or(SuiError::TransactionEventsNotFound { digest: tx_digest })
                .map(|event| (tx_digest, event))
        })
        .collect::<SuiResult<HashMap<_, _>>>()?;

    let mut transactions = Vec::with_capacity(ckpt_tx_data.transactions.len());
    for (tx, fx) in ckpt_tx_data
        .transactions
        .iter()
        .zip(ckpt_tx_data.effects.iter())
    {
        let events = fx.events_digest().map(|_event_digest| {
            events
                .remove(fx.transaction_digest())
                .expect("event was already checked to be present")
        });

        let transaction = ExecutedTransaction {
            transaction: tx.transaction_data().clone(),
            signatures: tx.tx_signatures().to_vec(),
            effects: fx.clone(),
            events,
            unchanged_loaded_runtime_objects: transaction_cache_reader
                .get_unchanged_loaded_runtime_objects(tx.digest())
                .ok_or_else(|| {
                    sui_types::storage::error::Error::custom(format!(
                        "unabled to load unchanged_loaded_runtime_objects for tx {}",
                        tx.digest(),
                    ))
                })?,
        };
        transactions.push(transaction);
    }

    let object_set = {
        let refs = transactions
            .iter()
            .flat_map(|tx| {
                sui_types::storage::get_transaction_object_set(
                    &tx.transaction,
                    &tx.effects,
                    &tx.unchanged_loaded_runtime_objects,
                )
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        let objects = object_store.multi_get_objects_by_key(&refs);

        let mut object_set = ObjectSet::default();
        for (idx, object) in objects.into_iter().enumerate() {
            object_set.insert(object.ok_or_else(|| {
                sui_types::storage::error::Error::custom(format!(
                    "unabled to load object {:?}",
                    refs[idx]
                ))
            })?);
        }
        object_set
    };
    let checkpoint = Checkpoint {
        summary: ckpt_data.checkpoint.clone().into(),
        contents: ckpt_data.checkpoint_contents.clone(),
        transactions,
        object_set,
    };
    Ok(checkpoint)
}
