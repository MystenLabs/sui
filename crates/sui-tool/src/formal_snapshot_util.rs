// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use anyhow::anyhow;
use futures::{StreamExt, TryStreamExt};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use sui_data_ingestion_core::{CheckpointReader, create_remote_store_client};
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
    let client = Arc::new(create_remote_store_client(ingestion_url, vec![], 60)?);
    futures::stream::iter(checkpoints)
        .map(Ok::<_, anyhow::Error>)
        .try_for_each_concurrent(concurrency, |sq| {
            let client = client.clone();
            let store = store.clone();
            let checkpoint_counter = checkpoint_counter.clone();
            async move {
                let checkpoint = CheckpointReader::fetch_from_object_store(&*client, sq).await?;
                store
                    .insert_checkpoint(&VerifiedCheckpoint::new_unchecked(
                        checkpoint.0.checkpoint_summary.clone(),
                    ))
                    .map_err(|e| anyhow!("Failed to insert checkpoint: {e}"))?;
                checkpoint_counter.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
        })
        .await
}
