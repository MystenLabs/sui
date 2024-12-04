// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::try_join_all;
use std::sync::Arc;
use sui_types::full_checkpoint_content::CheckpointData;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::{ingestion::error::Error, task::TrySpawnStreamExt};

use super::{client::IngestionClient, IngestionConfig};

/// The broadcaster task is responsible for taking a stream of checkpoint sequence numbers from
/// `checkpoint_rx`, fetching them using the `client` and disseminating them to all subscribers in
/// `subscribers`.
///
/// The task will shut down if the `cancel` token is signalled, or if the `checkpoint_rx` channel
/// closes.
pub(super) fn broadcaster(
    config: IngestionConfig,
    client: IngestionClient,
    checkpoint_rx: mpsc::Receiver<u64>,
    subscribers: Vec<mpsc::Sender<Arc<CheckpointData>>>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        info!("Starting ingestion broadcaster");
        let retry_interval = config.retry_interval();

        match ReceiverStream::new(checkpoint_rx)
            .try_for_each_spawned(/* limit */ config.ingest_concurrency, |cp| {
                let client = client.clone();
                let subscribers = subscribers.clone();

                // One clone is for the supervisor to signal a cancel if it detects a
                // subscriber that wants to wind down ingestion, and the other is to pass to
                // each worker to detect cancellation.
                let supervisor_cancel = cancel.clone();
                let cancel = cancel.clone();

                async move {
                    // Repeatedly retry if the checkpoint is not found, assuming that we are at the
                    // tip of the network and it will become available soon.
                    let checkpoint = client.wait_for(cp, retry_interval, &cancel).await?;

                    let futures = subscribers.iter().map(|s| s.send(checkpoint.clone()));
                    if try_join_all(futures).await.is_err() {
                        info!("Subscription dropped, signalling shutdown");
                        supervisor_cancel.cancel();
                        Err(Error::Cancelled)
                    } else {
                        Ok(())
                    }
                }
            })
            .await
        {
            Ok(()) => {
                info!("Checkpoints done, stopping ingestion broadcaster");
            }

            Err(Error::Cancelled) => {
                info!("Shutdown received, stopping ingestion broadcaster");
            }

            Err(e) => {
                error!("Ingestion broadcaster failed: {}", e);
                cancel.cancel();
            }
        }
    })
}
