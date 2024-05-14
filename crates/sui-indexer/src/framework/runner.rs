// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_rest_api::CheckpointData;
use tokio_util::sync::CancellationToken;

use super::interface::Handler;

pub async fn run<S>(stream: S, mut handlers: Vec<Box<dyn Handler>>, cancel: CancellationToken)
where
    S: futures::Stream<Item = CheckpointData> + std::marker::Unpin,
{
    use futures::StreamExt;

    let batch_size = std::env::var("CHECKPOINT_PROCESSING_BATCH_SIZE")
        .unwrap_or(25.to_string())
        .parse::<usize>()
        .unwrap();
    tracing::info!("Indexer runner is starting with {batch_size}");
    let mut chunks: futures::stream::ReadyChunks<S> = stream.ready_chunks(batch_size);
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::info!("Indexer runner is cancelled");
                return;
            }
            chunk = chunks.next() => {
                if let Some(checkpoints) = chunk {
                    // TODO: create tracing spans for processing
                    futures::future::join_all(
                        handlers
                            .iter_mut()
                            .map(|handler| async {
                                handler.process_checkpoints(&checkpoints).await.unwrap()
                            }),
                    )
                    .await;
                } else {
                    // Handle the end of the stream if needed
                    break;
                }
            }
        }
    }
}
