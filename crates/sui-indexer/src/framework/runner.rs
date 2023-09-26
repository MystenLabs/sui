// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_rest_api::CheckpointData;

use super::interface::Handler;

pub async fn run<S>(mut stream: S, mut handlers: Vec<Box<dyn Handler>>)
where
    S: futures::Stream<Item = CheckpointData> + std::marker::Unpin,
{
    use futures::StreamExt;

    while let Some(checkpoint) = stream.next().await {
        //TODO create tracing spans for processing
        futures::future::join_all(
            handlers
                .iter_mut()
                .map(|handler| async { handler.process_checkpoint(&checkpoint).await.unwrap() }),
        )
        .await;
    }
}
