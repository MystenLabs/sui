// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use anyhow::anyhow;
use futures::{StreamExt, TryStreamExt};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use sui_storage::object_store::util::{build_object_store, fetch_checkpoint};
use sui_types::messages_checkpoint::{CheckpointSequenceNumber, VerifiedCheckpoint};
use sui_types::storage::WriteStore;

pub(crate) async fn read_summaries_for_list_no_verify<S>(
    ingestion_url: String,
    concurrency: usize,
    store: S,
    checkpoints: Vec<CheckpointSequenceNumber>,
    checkpoint_counter: Arc<AtomicU64>,
) -> Result<()>
where
    S: WriteStore + Clone,
{
    let client = build_object_store(&ingestion_url, vec![]);
    futures::stream::iter(checkpoints)
        .map(|sq| fetch_checkpoint(&client, sq))
        .buffer_unordered(concurrency)
        .try_for_each(|checkpoint| {
            let result = store
                .insert_checkpoint(&VerifiedCheckpoint::new_unchecked(
                    checkpoint.summary.clone(),
                ))
                .map_err(|e| anyhow!("Failed to insert checkpoint: {e}"));
            checkpoint_counter.fetch_add(1, Ordering::Relaxed);
            futures::future::ready(result)
        })
        .await
}
