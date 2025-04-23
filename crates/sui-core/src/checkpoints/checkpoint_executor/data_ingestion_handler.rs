// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoints::checkpoint_executor::{CheckpointExecutionData, CheckpointTransactionData};
use crate::execution_cache::TransactionCacheRead;
use std::collections::HashMap;
use std::path::Path;
use sui_storage::blob::{Blob, BlobEncoding};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::{SuiError, SuiResult};
use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};
use sui_types::storage::ObjectStore;

pub(crate) fn load_checkpoint_data(
    ckpt_data: &CheckpointExecutionData,
    ckpt_tx_data: &CheckpointTransactionData,
    object_store: &dyn ObjectStore,
    transaction_cache_reader: &dyn TransactionCacheRead,
) -> SuiResult<CheckpointData> {
    let event_tx_digests = ckpt_tx_data
        .effects
        .iter()
        .flat_map(|fx| fx.events_digest().map(|_| fx.transaction_digest()).copied())
        .collect::<Vec<_>>();

    let events = transaction_cache_reader
        .multi_get_events(&event_tx_digests)
        .into_iter()
        .zip(event_tx_digests)
        .map(|(maybe_event, tx_digest)| {
            maybe_event
                .ok_or(SuiError::TransactionEventsNotFound { digest: tx_digest })
                .map(|event| (tx_digest, event))
        })
        .collect::<SuiResult<HashMap<_, _>>>()?;

    let mut full_transactions = Vec::with_capacity(ckpt_tx_data.transactions.len());
    for (tx, fx) in ckpt_tx_data
        .transactions
        .iter()
        .zip(ckpt_tx_data.effects.iter())
    {
        let events = fx.events_digest().map(|_event_digest| {
            events
                .get(fx.transaction_digest())
                .cloned()
                .expect("event was already checked to be present")
        });

        let input_objects = sui_types::storage::get_transaction_input_objects(object_store, fx)
            .map_err(|e| SuiError::Unknown(e.to_string()))?;
        let output_objects = sui_types::storage::get_transaction_output_objects(object_store, fx)
            .map_err(|e| SuiError::Unknown(e.to_string()))?;

        let full_transaction = CheckpointTransaction {
            transaction: (*tx).clone().into_unsigned().into(),
            effects: fx.clone(),
            events,
            input_objects,
            output_objects,
        };
        full_transactions.push(full_transaction);
    }
    let checkpoint_data = CheckpointData {
        checkpoint_summary: ckpt_data.checkpoint.clone().into(),
        checkpoint_contents: ckpt_data.checkpoint_contents.clone(),
        transactions: full_transactions,
    };
    Ok(checkpoint_data)
}

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
