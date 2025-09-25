// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoints::checkpoint_executor::{CheckpointExecutionData, CheckpointTransactionData};
use crate::execution_cache::TransactionCacheRead;
use bytes::Bytes;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use sui_config::node::CheckpointExecutorConfig;
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

pub(crate) async fn store_checkpoint_in_object_store(
    object_store: &dyn object_store::ObjectStore,
    config: &CheckpointExecutorConfig,
    checkpoint: &Checkpoint,
) -> anyhow::Result<()> {
    use std::sync::LazyLock;
    use sui_rpc::field::FieldMask;
    use sui_rpc::field::FieldMaskTree;
    use sui_rpc::field::FieldMaskUtil;
    use sui_rpc::merge::Merge;

    static MASK: LazyLock<FieldMaskTree> = LazyLock::new(|| {
        use sui_rpc::proto::sui::rpc::v2::Checkpoint;

        FieldMask::from_paths([
            Checkpoint::path_builder().sequence_number(),
            Checkpoint::path_builder().summary().bcs().value(),
            Checkpoint::path_builder().signature().finish(),
            Checkpoint::path_builder().contents().bcs().value(),
            Checkpoint::path_builder()
                .transactions()
                .transaction()
                .bcs()
                .value(),
            Checkpoint::path_builder()
                .transactions()
                .effects()
                .bcs()
                .value(),
            Checkpoint::path_builder()
                .transactions()
                .effects()
                .unchanged_loaded_runtime_objects()
                .finish(),
            Checkpoint::path_builder()
                .transactions()
                .events()
                .bcs()
                .value(),
            Checkpoint::path_builder().objects().objects().bcs().value(),
        ])
        .into()
    });

    let file_name =
        object_store::path::Path::from(format!("{}.zst", checkpoint.summary.sequence_number));
    let level = config
        .checkpoint_upload_config
        .as_ref()
        .and_then(|c| c.level)
        .unwrap_or(19);

    let checkpoint = sui_rpc::proto::sui::rpc::v2::Checkpoint::merge_from(checkpoint, &MASK);

    let blob = tokio::task::spawn_blocking(move || compress_message(&checkpoint, level))
        .await
        .unwrap()?;

    object_store
        .put(&file_name, Bytes::from(blob).into())
        .await?;

    Ok(())
}

fn compress_message<M: prost::Message>(msg: &M, zstd_level: i32) -> anyhow::Result<Vec<u8>> {
    // 1) Serialize protobuf
    let buf = msg.encode_to_vec();

    // 2) Compress with zstd (single-shot)
    let compressed = zstd::encode_all(&buf[..], zstd_level)?;
    Ok(compressed)
}
