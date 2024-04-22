// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::IndexerMetrics;

use super::fetcher::CheckpointDownloadData;
use super::interface::Handler;

// Limit indexing parallelism on big checkpoints to avoid OOM,
// by limiting the total size of batch checkpoints to ~20MB.
// On testnet, most checkpoints are < 200KB, some can go up to 50MB.
const CHECKPOINT_PROCESSING_BATCH_DATA_LIMIT: usize = 20000000;
const CHECKPOINT_PROCESSING_BATCH_SIZE: usize = 100;

pub async fn run<S>(stream: S, mut handlers: Vec<Box<dyn Handler>>, metrics: IndexerMetrics)
where
    S: futures::Stream<Item = CheckpointDownloadData> + std::marker::Unpin,
{
    use futures::StreamExt;
    let batch_size = std::env::var("CHECKPOINT_PROCESSING_BATCH_SIZE")
        .unwrap_or(CHECKPOINT_PROCESSING_BATCH_SIZE.to_string())
        .parse::<usize>()
        .unwrap();
    let data_limit = std::env::var("CHECKPOINT_PROCESSING_BATCH_DATA_LIMIT")
        .unwrap_or(CHECKPOINT_PROCESSING_BATCH_DATA_LIMIT.to_string())
        .parse::<usize>()
        .unwrap();
    tracing::info!("Indexer runner is starting with {batch_size}");
    let mut chunks: futures::stream::ReadyChunks<S> = stream.ready_chunks(batch_size);
    while let Some(checkpoints) = chunks.next().await {
        //TODO create tracing spans for processing
        let mut cp_batch = vec![];
        let mut cp_batch_total_size = 0;
        for checkpoint in checkpoints.iter() {
            cp_batch_total_size += checkpoint.size;
            cp_batch.push(checkpoint.data.clone());
            if cp_batch_total_size >= data_limit {
                futures::future::join_all(handlers.iter_mut().map(|handler| async {
                    handler.process_checkpoints(&cp_batch).await.unwrap()
                }))
                .await;

                metrics.indexing_batch_size.set(cp_batch_total_size as i64);
                cp_batch = vec![];
                cp_batch_total_size = 0;
            }
        }
        if !cp_batch.is_empty() {
            futures::future::join_all(
                handlers
                    .iter_mut()
                    .map(|handler| async { handler.process_checkpoints(&cp_batch).await.unwrap() }),
            )
            .await;
            metrics.indexing_batch_size.set(cp_batch_total_size as i64);
        }
    }
}
