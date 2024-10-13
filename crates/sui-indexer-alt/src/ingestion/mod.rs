// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use backoff::backoff::Constant;
use client::IngestionClient;
use futures::{future::try_join_all, stream, StreamExt, TryStreamExt};
use mysten_metrics::spawn_monitored_task;
use sui_types::full_checkpoint_content::CheckpointData;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use url::Url;

use crate::ingestion::error::{Error, Result};
use crate::metrics::IndexerMetrics;

mod client;
pub mod error;

pub struct IngestionService {
    config: IngestionConfig,
    client: IngestionClient,
    metrics: Arc<IndexerMetrics>,
    subscribers: Vec<mpsc::Sender<Arc<CheckpointData>>>,
    cancel: CancellationToken,
}

#[derive(clap::Args, Debug, Clone)]
pub struct IngestionConfig {
    /// Remote Store to fetch checkpoints from.
    #[arg(long)]
    remote_store_url: Url,

    /// First checkpoint to start ingestion from.
    #[arg(long, default_value_t = 0)]
    start_checkpoint: u64,

    /// Maximum size of checkpoint backlog across all workers downstream of the ingestion service.
    #[arg(long, default_value_t = 5000)]
    buffer_size: usize,

    /// Maximum number of checkpoint to attempt to fetch concurrently.
    #[arg(long, default_value_t = 200)]
    concurrency: usize,

    /// Polling interval to retry fetching checkpoints that do not exist.
    #[arg(
        long,
        default_value = "200",
        value_name = "MILLISECONDS",
        value_parser = |s: &str| s.parse().map(Duration::from_millis)
    )]
    retry_interval: Duration,
}

impl IngestionService {
    pub fn new(
        config: IngestionConfig,
        metrics: IndexerMetrics,
        cancel: CancellationToken,
    ) -> Result<Self> {
        let metrics = Arc::new(metrics);
        Ok(Self {
            client: IngestionClient::new(config.remote_store_url.clone(), metrics.clone())?,
            subscribers: Vec::new(),
            config,
            metrics,
            cancel,
        })
    }

    /// Add a new subscription to the ingestion service. Note that the service is susceptible to
    /// the "slow receiver" problem: If one receiver is slower to process checkpoints than the
    /// checkpoint ingestion rate, it will eventually hold up all receivers.
    pub fn subscribe(&mut self) -> mpsc::Receiver<Arc<CheckpointData>> {
        let (sender, receiver) = mpsc::channel(self.config.buffer_size);
        self.subscribers.push(sender);
        receiver
    }

    /// Start the ingestion service as a background task, consuming it in the process.
    ///
    /// Checkpoints are fetched concurrently starting with the configured `start_checkpoint`, and
    /// pushed to subscribers' channels (potentially out-of-order). Subscribers can communicate
    /// with the ingestion service via their channels in the following ways:
    ///
    /// - If a subscriber is lagging (not receiving checkpoints fast enough), it will eventually
    ///   provide back-pressure to the ingestion service, which will stop fetching new checkpoints.
    /// - If a subscriber closes its channel, the ingestion service will intepret that as a signal
    ///   to shutdown as well.
    ///
    /// If ingestion reaches the leading edge of the network, it will encounter checkpoints that do
    /// not exist yet. These will be retried repeatedly on a fixed `retry_interval` until they
    /// become available.
    pub async fn run(self) -> Result<JoinHandle<()>> {
        let IngestionService {
            config,
            client,
            metrics,
            subscribers,
            cancel,
        } = self;

        /// Individual iterations of `try_for_each_concurrent` communicate with the supervisor by
        /// returning an `Err` with a `Break` variant.
        enum Break {
            Cancelled,
            Err(u64, Error),
        }

        if subscribers.is_empty() {
            return Err(Error::NoSubscribers);
        }

        Ok(spawn_monitored_task!(async move {
            let start = config.start_checkpoint;
            info!(start, "Starting ingestion service");

            match stream::iter(start..)
                .map(Ok)
                .try_for_each_concurrent(/* limit */ config.concurrency, |cp| {
                    let client = client.clone();
                    let metrics = metrics.clone();
                    let subscribers = subscribers.clone();

                    // One clone is for the supervisor to signal a cancel if it detects a
                    // subscriber that wants to wind down ingestion, and the other is to pass to
                    // each worker to detect cancellation.
                    let supervisor_cancel = cancel.clone();
                    let cancel = cancel.clone();

                    // Repeatedly retry if the checkpoint is not found, assuming that we are at the
                    // tip of the network and it will become available soon.
                    let backoff = Constant::new(config.retry_interval);
                    let fetch = move || {
                        let client = client.clone();
                        let metrics = metrics.clone();
                        let cancel = cancel.clone();

                        async move {
                            use backoff::Error as BE;
                            if cancel.is_cancelled() {
                                return Err(BE::permanent(Break::Cancelled));
                            }

                            client.fetch(cp).await.map_err(|e| match e {
                                Error::NotFound(checkpoint) => {
                                    debug!(checkpoint, "Checkpoint not found, retrying...");
                                    metrics.total_ingested_not_found_retries.inc();
                                    BE::transient(Break::Err(cp, e))
                                }
                                e => BE::permanent(Break::Err(cp, e)),
                            })
                        }
                    };

                    async move {
                        let checkpoint = backoff::future::retry(backoff, fetch).await?;
                        let futures = subscribers.iter().map(|s| s.send(checkpoint.clone()));

                        if try_join_all(futures).await.is_err() {
                            info!("Subscription dropped, signalling shutdown");
                            supervisor_cancel.cancel();
                            Err(Break::Cancelled)
                        } else {
                            Ok(())
                        }
                    }
                })
                .await
            {
                Ok(()) => {}
                Err(Break::Cancelled) => {
                    info!("Shutdown received, stopping ingestion service");
                }

                Err(Break::Err(checkpoint, e)) => {
                    error!(checkpoint, "Ingestion service failed: {}", e);
                }
            }
        }))
    }
}
