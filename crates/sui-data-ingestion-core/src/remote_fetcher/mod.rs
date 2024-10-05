// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::remote_fetcher::hybrid_fetcher::HybridFetcher;
use crate::remote_fetcher::object_store_fetcher::ObjectStoreFetcher;
use crate::remote_fetcher::rest_fetcher::RestFetcher;
use backoff::backoff::Backoff;
use futures::StreamExt;
use mysten_metrics::spawn_monitored_task;
use std::sync::Arc;
use std::time::Duration;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tap::Pipe;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tracing::{debug, error, info};

mod hybrid_fetcher;
mod object_store_fetcher;
mod rest_fetcher;

#[async_trait::async_trait]
pub trait RemoteFetcherTrait: Sync + Send {
    /// Given a sequence number, fetches the corresponding checkpoint data.
    /// Returns the checkpoint data and the size of the response in bytes.
    async fn fetch_checkpoint(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> anyhow::Result<(Arc<CheckpointData>, usize)>;
}

pub struct RemoteFetcher {
    #[allow(clippy::type_complexity)]
    fetcher_receiver: Option<mpsc::Receiver<anyhow::Result<(Arc<CheckpointData>, usize)>>>,
    fetcher: Arc<dyn RemoteFetcherTrait>,
    batch_size: usize,
}

impl RemoteFetcher {
    pub fn new(
        url: String,
        remote_store_options: Vec<(String, String)>,
        timeout_secs: u64,
        batch_size: usize,
    ) -> Self {
        let fetcher: Arc<dyn RemoteFetcherTrait> = if let Some((fn_url, remote_url)) =
            url.split_once('|')
        {
            let archival_fetcher =
                ObjectStoreFetcher::new(remote_url.to_string(), remote_store_options, timeout_secs)
                    .expect("failed to create remote store client");
            let rest_fetcher = RestFetcher::new(fn_url.to_string());
            Arc::new(HybridFetcher::new(archival_fetcher, rest_fetcher))
        } else if url.ends_with("/rest") {
            Arc::new(RestFetcher::new(url))
        } else {
            Arc::new(
                ObjectStoreFetcher::new(url, remote_store_options, timeout_secs)
                    .expect("failed to create remote store client"),
            )
        };
        Self {
            fetcher_receiver: None,
            fetcher,
            batch_size,
        }
    }

    pub fn start_receiving(&mut self, start_checkpoint: CheckpointSequenceNumber) {
        if self.fetcher_receiver.is_some() {
            return;
        }
        let batch_size = self.batch_size;
        let (sender, receiver) = mpsc::channel(batch_size);
        self.fetcher_receiver = Some(receiver);
        let fetcher = self.fetcher.clone();
        spawn_monitored_task!(async move {
            let mut checkpoint_stream = (start_checkpoint..u64::MAX)
                .map(|checkpoint_number| Self::remote_fetch_checkpoint(&fetcher, checkpoint_number))
                .pipe(futures::stream::iter)
                .buffered(batch_size);

            while let Some(checkpoint) = checkpoint_stream.next().await {
                if sender.send(checkpoint).await.is_err() {
                    info!("remote reader dropped");
                    break;
                }
            }
        });
    }

    async fn remote_fetch_checkpoint(
        fetcher: &Arc<dyn RemoteFetcherTrait>,
        checkpoint_number: CheckpointSequenceNumber,
    ) -> anyhow::Result<(Arc<CheckpointData>, usize)> {
        // TODO: Should just use a fixed interval backoff.
        let mut backoff = backoff::ExponentialBackoff::default();
        backoff.max_elapsed_time = Some(Duration::from_secs(60));
        backoff.initial_interval = Duration::from_millis(100);
        backoff.current_interval = backoff.initial_interval;
        backoff.multiplier = 1.0;
        loop {
            match fetcher.fetch_checkpoint(checkpoint_number).await {
                Ok(data) => return Ok(data),
                Err(err) => match backoff.next_backoff() {
                    Some(duration) => {
                        if !err.to_string().contains("404") {
                            debug!(
                                "remote reader retry in {} ms. Error is {:?}",
                                duration.as_millis(),
                                err
                            );
                        }
                        tokio::time::sleep(duration).await
                    }
                    None => return Err(err),
                },
            }
        }
    }

    pub fn try_recv(&mut self) -> Option<(Arc<CheckpointData>, usize)> {
        match self.fetcher_receiver.as_mut().unwrap().try_recv() {
            Ok(Ok(data)) => Some(data),
            Ok(Err(err)) => {
                error!("remote reader transient error {:?}", err);
                self.stop_receiving();
                None
            }
            Err(TryRecvError::Disconnected) => {
                error!("remote reader channel disconnect error");
                self.stop_receiving();
                None
            }
            Err(TryRecvError::Empty) => None,
        }
    }

    pub fn stop_receiving(&mut self) {
        self.fetcher_receiver = None;
    }
}
