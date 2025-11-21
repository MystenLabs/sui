// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use anyhow::{Context, ensure};
use futures::future;
use ingestion::{ClientArgs, IngestionConfig, IngestionService, ingestion_client::IngestionClient};
use metrics::IndexerMetrics;
use pipeline::{
    Processor,
    concurrent::{self, ConcurrentConfig},
    sequential::{self, Handler, SequentialConfig},
};
use prometheus::Registry;
use sui_indexer_alt_framework_store_traits::{
    CommitterWatermark, Connection, Store, TransactionalStore,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

pub use anyhow::Result;
pub use sui_field_count::FieldCount;
/// External users access the store trait through framework::store
pub use sui_indexer_alt_framework_store_traits as store;
pub use sui_types as types;

use crate::metrics::IngestionMetrics;

#[cfg(feature = "cluster")]
pub mod cluster;
pub mod ingestion;
pub mod metrics;
pub mod pipeline;
#[cfg(feature = "postgres")]
pub mod postgres;
pub mod task;

#[cfg(test)]
pub mod mocks;

/// Command-line arguments for the indexer
#[derive(clap::Args, Default, Debug, Clone)]
pub struct IndexerArgs {
    /// Override for the checkpoint to start ingestion from -- useful for backfills. Otherwise, by
    /// default, ingestion will start just after the lowest checkpoint watermark across all active
    /// pipelines.
    ///
    /// For both concurrent and sequential pipelines, if a first checkpoint is configured, and a
    /// watermark does not exist for the pipeline, the indexer will also tell the pipeline to start
    /// from this value.
    ///
    /// Unless `--skip-watermark` is set, this value must be less than or equal to the global high
    /// watermark (preventing the indexer from introducing a gap in the data). This exception only
    /// applies to concurrent pipelines, and these pipelines will also not report watermark updates.
    ///
    /// Sequential pipelines will always start committing from the next checkpoint after its
    /// watermark.
    ///
    /// Concurrent pipelines will always start committing from `first_checkpoint`. These pipelines
    /// will not report watermark updates if `skip_watermark` is set.
    #[arg(long)]
    pub first_checkpoint: Option<u64>,

    /// Override for the checkpoint to end ingestion at (inclusive) -- useful for backfills. By
    /// default, ingestion will not stop, and will continue to poll for new checkpoints.
    #[arg(long)]
    pub last_checkpoint: Option<u64>,

    /// Only run the following pipelines. If not provided, all pipelines found in the
    /// configuration file will be run.
    #[arg(long, action = clap::ArgAction::Append)]
    pub pipeline: Vec<String>,

    /// Don't write to the watermark tables for concurrent pipelines.
    #[arg(long)]
    pub skip_watermark: bool,
}

pub struct Indexer<S: Store> {
    /// The storage backend that the indexer uses to write and query indexed data. This
    /// generic implementation allows for plugging in different storage solutions that implement the
    /// `Store` trait.
    store: S,

    /// Prometheus Metrics.
    metrics: Arc<IndexerMetrics>,

    /// Service for downloading and disseminating checkpoint data.
    ingestion_service: IngestionService,

    /// Optional override of the checkpoint lowerbound.
    first_checkpoint: Option<u64>,

    /// Optional override of the checkpoint upperbound.
    last_checkpoint: Option<u64>,

    /// Don't write to the watermark tables for concurrent pipelines.
    skip_watermark: bool,

    /// Optional filter for pipelines to run. If `None`, all pipelines added to the indexer will
    /// run. Any pipelines that are present in this filter but not added to the indexer will yield
    /// a warning when the indexer is run.
    enabled_pipelines: Option<BTreeSet<String>>,

    /// Pipelines that have already been registered with the indexer. Used to make sure a pipeline
    /// with the same name isn't added twice.
    added_pipelines: BTreeSet<&'static str>,

    /// Cancellation token shared among all continuous tasks in the service.
    cancel: CancellationToken,

    /// The checkpoint lowerbound derived from watermarks of pipelines added to the indexer. When
    /// the indexer runs, it will start from this point, unless this has been overridden by
    /// [Self::first_checkpoint].
    first_checkpoint_from_watermark: u64,

    /// The minimum next_checkpoint across all sequential pipelines. This is used to initialize
    /// the regulator to prevent ingestion from running too far ahead of sequential pipelines.
    next_sequential_checkpoint: Option<u64>,

    /// The handles for every task spawned by this indexer, used to manage graceful shutdown.
    handles: Vec<JoinHandle<()>>,
}

impl<S: Store> Indexer<S> {
    /// Create a new instance of the indexer framework from a store that implements the `Store`
    /// trait, along with `indexer_args`, `client_args`, and `ingestion_config`. Together, these
    /// arguments configure the following:
    ///
    /// - What is indexed (which checkpoints, which pipelines, whether to track and update
    ///   watermarks) and where to serve metrics from,
    /// - Where to download checkpoints from,
    /// - Concurrency and buffering parameters for downloading checkpoints.
    ///
    /// After initialization, at least one pipeline must be added using [Self::concurrent_pipeline]
    /// or [Self::sequential_pipeline], before the indexer is started using [Self::run].
    pub async fn new(
        store: S,
        indexer_args: IndexerArgs,
        client_args: ClientArgs,
        ingestion_config: IngestionConfig,
        metrics_prefix: Option<&str>,
        registry: &Registry,
        cancel: CancellationToken,
    ) -> Result<Self> {
        let IndexerArgs {
            first_checkpoint,
            last_checkpoint,
            pipeline,
            skip_watermark,
        } = indexer_args;

        let metrics = IndexerMetrics::new(metrics_prefix, registry);

        let ingestion_service = IngestionService::new(
            client_args,
            ingestion_config,
            metrics_prefix,
            registry,
            cancel.clone(),
        )?;

        Ok(Self {
            store,
            metrics,
            ingestion_service,
            first_checkpoint,
            last_checkpoint,
            skip_watermark,
            enabled_pipelines: if pipeline.is_empty() {
                None
            } else {
                Some(pipeline.into_iter().collect())
            },
            added_pipelines: BTreeSet::new(),
            cancel,
            first_checkpoint_from_watermark: u64::MAX,
            next_sequential_checkpoint: None,
            handles: vec![],
        })
    }

    /// The store used by the indexer.
    pub fn store(&self) -> &S {
        &self.store
    }

    /// The ingestion client used by the indexer to fetch checkpoints.
    pub fn ingestion_client(&self) -> &IngestionClient {
        self.ingestion_service.ingestion_client()
    }

    /// The indexer's metrics.
    pub fn indexer_metrics(&self) -> &Arc<IndexerMetrics> {
        &self.metrics
    }

    /// The ingestion service's metrics.
    pub fn ingestion_metrics(&self) -> &Arc<IngestionMetrics> {
        self.ingestion_service.metrics()
    }

    /// The pipelines that this indexer will run.
    pub fn pipelines(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.added_pipelines.iter().copied().filter(|p| {
            self.enabled_pipelines
                .as_ref()
                .is_none_or(|e| e.contains(*p))
        })
    }

    /// The minimum next checkpoint across all sequential pipelines. This value is used to
    /// initialize the ingestion regulator's high watermark to prevent ingestion from running
    /// too far ahead of sequential pipelines.
    pub fn next_sequential_checkpoint(&self) -> Option<u64> {
        self.next_sequential_checkpoint
    }

    /// Adds a new pipeline to this indexer and starts it up. Although their tasks have started,
    /// they will be idle until the ingestion service starts, and serves it checkpoint data.
    ///
    /// Concurrent pipelines commit checkpoint data out-of-order to maximise throughput, and they
    /// keep the watermark table up-to-date with the highest point they can guarantee all data
    /// exists for, for their pipeline.
    pub async fn concurrent_pipeline<H>(
        &mut self,
        handler: H,
        config: ConcurrentConfig,
    ) -> Result<()>
    where
        H: concurrent::Handler<Store = S> + Send + Sync + 'static,
    {
        let Some(watermark) = self.add_pipeline::<H>().await? else {
            return Ok(());
        };

        // If `first_checkpoint` does not violate the consistency check, concurrent pipelines will
        // prefer to resume from the `first_checkpoint` if configured.
        let next_checkpoint = match (watermark, self.first_checkpoint) {
            (Some(watermark), Some(first_checkpoint)) => {
                // Setting `skip_watermark` allows concurrent pipelines to not be considered in the
                // consistency check. The indexer will still fail to start if `first_checkpoint`
                // fails for a sequential pipeline in the indexer.
                if !self.skip_watermark {
                    ensure!(
                        first_checkpoint <= watermark.checkpoint_hi_inclusive + 1,
                        "For pipeline {}, first checkpoint override {} is too far ahead of watermark {}. \
                        This could create gaps in the data.",
                        H::NAME,
                        first_checkpoint,
                        watermark.checkpoint_hi_inclusive,
                    );
                }
                first_checkpoint
            }
            (Some(watermark), _) => watermark.checkpoint_hi_inclusive + 1,
            (_, Some(first_checkpoint)) => first_checkpoint,
            (None, None) => 0,
        };

        self.handles.push(concurrent::pipeline::<H>(
            handler,
            next_checkpoint,
            config,
            self.skip_watermark,
            self.store.clone(),
            self.ingestion_service.subscribe().0,
            self.metrics.clone(),
            self.cancel.clone(),
        ));

        Ok(())
    }

    /// Start ingesting checkpoints. Ingestion either starts from the
    /// `first_checkpoint_from_watermark` calculated based on the smallest watermark of all active
    /// pipelines or `first_checkpoint` if configured. Individual pipelines will start processing
    /// and committing once the ingestion service has caught up to their respective watermarks.
    ///
    /// Ingestion will stop after consuming the configured `last_checkpoint`, if one is provided, or
    /// will continue until it tracks the tip of the network.
    pub async fn run(mut self) -> Result<JoinHandle<()>> {
        if let Some(enabled_pipelines) = self.enabled_pipelines {
            ensure!(
                enabled_pipelines.is_empty(),
                "Tried to enable pipelines that this indexer does not know about: \
                {enabled_pipelines:#?}",
            );
        }

        // If an override has been provided, start ingestion from there, otherwise start ingestion
        // from just after the lowest committer watermark across all enabled pipelines.
        let first_checkpoint = self
            .first_checkpoint
            .unwrap_or(self.first_checkpoint_from_watermark);

        let last_checkpoint = self.last_checkpoint.unwrap_or(u64::MAX);

        info!(first_checkpoint, last_checkpoint = ?self.last_checkpoint, "Ingestion range");

        let broadcaster_handle = self
            .ingestion_service
            .run(
                first_checkpoint..=last_checkpoint,
                self.next_sequential_checkpoint,
            )
            .await
            .context("Failed to start ingestion service")?;

        self.handles.push(broadcaster_handle);

        Ok(tokio::spawn(async move {
            // Wait for the ingestion service and all its related tasks to wind down gracefully:
            // If ingestion has been configured to only handle a specific range of checkpoints, we
            // want to make sure that tasks are allowed to run to completion before shutting them
            // down.
            future::join_all(self.handles).await;
            info!("Indexing pipeline gracefully shut down");
        }))
    }

    /// Update the indexer's starting ingestion checkpoint based on the watermark for the pipeline
    /// by adding for handler `H` (as long as it's enabled). Returns `Ok(None)` if the pipeline is
    /// disabled, `Ok(Some(None))` if the pipeline is enabled but its watermark is not found, and
    /// `Ok(Some(Some(watermark)))` if the pipeline is enabled and the watermark is found.
    async fn add_pipeline<P: Processor + 'static>(
        &mut self,
    ) -> Result<Option<Option<CommitterWatermark>>> {
        ensure!(
            self.added_pipelines.insert(P::NAME),
            "Pipeline {:?} already added",
            P::NAME,
        );

        if let Some(enabled_pipelines) = &mut self.enabled_pipelines
            && !enabled_pipelines.remove(P::NAME)
        {
            info!(pipeline = P::NAME, "Skipping");
            return Ok(None);
        }

        let mut conn = self
            .store
            .connect()
            .await
            .context("Failed to establish connection to store")?;

        let watermark = conn
            .committer_watermark(P::NAME)
            .await
            .with_context(|| format!("Failed to get watermark for {}", P::NAME))?;

        let expected_first_checkpoint = watermark
            .as_ref()
            .map(|w| w.checkpoint_hi_inclusive + 1)
            .unwrap_or_default();

        self.first_checkpoint_from_watermark =
            expected_first_checkpoint.min(self.first_checkpoint_from_watermark);

        Ok(Some(watermark))
    }
}

impl<T: TransactionalStore> Indexer<T> {
    /// Adds a new pipeline to this indexer and starts it up. Although their tasks have started,
    /// they will be idle until the ingestion service starts, and serves it checkpoint data.
    ///
    /// Sequential pipelines commit checkpoint data in-order which sacrifices throughput, but may
    /// be required to handle pipelines that modify data in-place (where each update is not an
    /// insert, but could be a modification of an existing row, where ordering between updates is
    /// important).
    ///
    /// The pipeline can optionally be configured to lag behind the ingestion service by a fixed
    /// number of checkpoints (configured by `checkpoint_lag`).
    pub async fn sequential_pipeline<H>(
        &mut self,
        handler: H,
        config: SequentialConfig,
    ) -> Result<()>
    where
        H: Handler<Store = T> + Send + Sync + 'static,
    {
        let Some(watermark) = self.add_pipeline::<H>().await? else {
            return Ok(());
        };

        if self.skip_watermark {
            warn!(
                pipeline = H::NAME,
                "--skip-watermarks enabled and ignored for sequential pipeline"
            );
        }

        // If `first_checkpoint` does not violate the consistency check, sequential pipelines will
        // prefer to resume from the existing watermark unless no watermark exists.
        let next_checkpoint = match (watermark, self.first_checkpoint) {
            (Some(watermark), Some(first_checkpoint)) => {
                // Sequential pipelines must write data in the order of checkpoints. If there is a
                // gap, this violates the property.
                ensure!(
                    first_checkpoint <= watermark.checkpoint_hi_inclusive + 1,
                    "For pipeline {}, first checkpoint override {} is too far ahead of watermark {}. \
                     This could create gaps in the data.",
                    H::NAME,
                    first_checkpoint,
                    watermark.checkpoint_hi_inclusive,
                );
                // Otherwise, sequential pipelines will wait until the processed checkpoint next
                // after its current watermark.
                warn!(
                    pipeline = H::NAME,
                    first_checkpoint,
                    committer_hi = watermark.checkpoint_hi_inclusive,
                    "Ignoring --first-checkpoint and will resume from committer_hi",
                );
                watermark.checkpoint_hi_inclusive + 1
            }
            // If a watermark exists, the pipeline will wait for the processed checkpoint next after
            // its watermark.
            (Some(watermark), _) => watermark.checkpoint_hi_inclusive + 1,
            // If no watermark exists, the first checkpoint can be anything.
            (_, Some(first_checkpoint)) => first_checkpoint,
            (None, None) => 0,
        };

        // Track the minimum next_checkpoint across all sequential pipelines
        self.next_sequential_checkpoint = Some(
            self.next_sequential_checkpoint
                .map_or(next_checkpoint, |n| n.min(next_checkpoint)),
        );

        let (checkpoint_rx, watermark_tx) = self.ingestion_service.subscribe();

        self.handles.push(sequential::pipeline::<H>(
            handler,
            next_checkpoint,
            config,
            self.store.clone(),
            checkpoint_rx,
            watermark_tx,
            self.metrics.clone(),
            self.cancel.clone(),
        ));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FieldCount;
    use crate::ingestion::ingestion_client::IngestionClientArgs;
    use crate::mocks::store::MockStore;
    use crate::pipeline::{Processor, concurrent::ConcurrentConfig};
    use crate::store::CommitterWatermark;
    use async_trait::async_trait;
    use std::sync::Arc;
    use sui_synthetic_ingestion::synthetic_ingestion;
    use tokio_util::sync::CancellationToken;

    #[async_trait]
    impl Processor for MockHandler {
        const NAME: &'static str = "test_processor";
        type Value = MockValue;
        async fn process(
            &self,
            _checkpoint: &Arc<sui_types::full_checkpoint_content::Checkpoint>,
        ) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![MockValue(1)])
        }
    }

    #[allow(dead_code)]
    #[derive(Clone, FieldCount)]
    struct MockValue(u64);

    struct MockHandler;

    #[async_trait]
    impl crate::pipeline::concurrent::Handler for MockHandler {
        type Store = MockStore;
        type Batch = Vec<MockValue>;

        fn batch(
            &self,
            batch: &mut Self::Batch,
            values: &mut std::vec::IntoIter<Self::Value>,
        ) -> crate::pipeline::concurrent::BatchStatus {
            batch.extend(values);
            crate::pipeline::concurrent::BatchStatus::Pending
        }

        async fn commit<'a>(
            &self,
            _batch: &Self::Batch,
            _conn: &mut <Self::Store as Store>::Connection<'a>,
        ) -> anyhow::Result<usize> {
            Ok(1)
        }
    }

    #[async_trait]
    impl crate::pipeline::sequential::Handler for MockHandler {
        type Store = MockStore;
        type Batch = Vec<Self::Value>;

        fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Self::Value>) {
            batch.extend(values);
        }

        async fn commit<'a>(
            &self,
            _batch: &Self::Batch,
            _conn: &mut <Self::Store as Store>::Connection<'a>,
        ) -> anyhow::Result<usize> {
            Ok(1)
        }
    }

    // One more test handler for testing multiple sequential pipelines
    struct SequentialHandler;

    #[async_trait]
    impl Processor for SequentialHandler {
        const NAME: &'static str = "sequential_handler";
        type Value = MockValue;
        async fn process(
            &self,
            _checkpoint: &Arc<sui_types::full_checkpoint_content::Checkpoint>,
        ) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![MockValue(1)])
        }
    }

    #[async_trait]
    impl crate::pipeline::sequential::Handler for SequentialHandler {
        type Store = MockStore;
        type Batch = Vec<MockValue>;

        fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Self::Value>) {
            batch.extend(values);
        }

        async fn commit<'a>(
            &self,
            _batch: &Self::Batch,
            _conn: &mut <Self::Store as Store>::Connection<'a>,
        ) -> anyhow::Result<usize> {
            Ok(1)
        }
    }

    #[tokio::test]
    async fn test_first_checkpoint_from_watermark() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();

        let store = MockStore::default();
        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            "test_processor",
            CommitterWatermark {
                epoch_hi_inclusive: 1,
                checkpoint_hi_inclusive: 100,
                tx_hi: 1000,
                timestamp_ms_hi_inclusive: 1000000,
            },
        )
        .await
        .unwrap();

        let indexer_args = IndexerArgs {
            first_checkpoint: Some(50),
            last_checkpoint: None,
            pipeline: vec![],
            skip_watermark: false,
        };
        let temp_dir = tempfile::tempdir().unwrap();
        let client_args = ClientArgs {
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(temp_dir.path().to_owned()),
                ..Default::default()
            },
            ..Default::default()
        };

        let ingestion_config = IngestionConfig::default();

        let mut indexer = Indexer::new(
            store,
            indexer_args,
            client_args,
            ingestion_config,
            None,
            &registry,
            cancel,
        )
        .await
        .unwrap();

        indexer
            .concurrent_pipeline::<MockHandler>(MockHandler, ConcurrentConfig::default())
            .await
            .unwrap();

        assert_eq!(indexer.first_checkpoint_from_watermark, 101);
    }

    #[tokio::test]
    async fn test_indexer_concurrent_pipeline_disallow_inconsistent_first_checkpoint() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();

        let store = MockStore::default();
        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            "test_processor",
            CommitterWatermark {
                epoch_hi_inclusive: 1,
                checkpoint_hi_inclusive: 100,
                tx_hi: 1000,
                timestamp_ms_hi_inclusive: 1000000,
            },
        )
        .await
        .unwrap();

        let indexer_args = IndexerArgs {
            first_checkpoint: Some(1001),
            last_checkpoint: None,
            pipeline: vec![],
            skip_watermark: false,
        };
        let temp_dir = tempfile::tempdir().unwrap();
        let client_args = ClientArgs {
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(temp_dir.path().to_owned()),
                ..Default::default()
            },
            ..Default::default()
        };

        let ingestion_config = IngestionConfig::default();

        let mut indexer = Indexer::new(
            store,
            indexer_args,
            client_args,
            ingestion_config,
            None,
            &registry,
            cancel,
        )
        .await
        .unwrap();

        let result = indexer
            .concurrent_pipeline::<MockHandler>(MockHandler, ConcurrentConfig::default())
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_indexer_concurrent_pipeline_allow_inconsistent_first_checkpoint_with_skip_watermark()
     {
        let cancel = CancellationToken::new();
        let registry = Registry::new();

        let store = MockStore::default();
        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            "test_processor",
            CommitterWatermark {
                epoch_hi_inclusive: 1,
                checkpoint_hi_inclusive: 100,
                tx_hi: 1000,
                timestamp_ms_hi_inclusive: 1000000,
            },
        )
        .await
        .unwrap();

        let indexer_args = IndexerArgs {
            first_checkpoint: Some(1001),
            last_checkpoint: None,
            pipeline: vec![],
            skip_watermark: true,
        };
        let temp_dir = tempfile::tempdir().unwrap();
        let client_args = ClientArgs {
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(temp_dir.path().to_owned()),
                ..Default::default()
            },
            ..Default::default()
        };

        let ingestion_config = IngestionConfig::default();

        let mut indexer = Indexer::new(
            store,
            indexer_args,
            client_args,
            ingestion_config,
            None,
            &registry,
            cancel,
        )
        .await
        .unwrap();

        let result = indexer
            .concurrent_pipeline::<MockHandler>(MockHandler, ConcurrentConfig::default())
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_indexer_sequential_pipeline_disallow_inconsistent_first_checkpoint() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();

        let store = MockStore::default();
        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            "test_processor",
            CommitterWatermark {
                epoch_hi_inclusive: 1,
                checkpoint_hi_inclusive: 100,
                tx_hi: 1000,
                timestamp_ms_hi_inclusive: 1000000,
            },
        )
        .await
        .unwrap();

        let indexer_args = IndexerArgs {
            first_checkpoint: Some(1001),
            last_checkpoint: None,
            pipeline: vec![],
            skip_watermark: false,
        };
        let temp_dir = tempfile::tempdir().unwrap();
        let client_args = ClientArgs {
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(temp_dir.path().to_owned()),
                ..Default::default()
            },
            ..Default::default()
        };

        let ingestion_config = IngestionConfig::default();

        let mut indexer = Indexer::new(
            store,
            indexer_args,
            client_args,
            ingestion_config,
            None,
            &registry,
            cancel,
        )
        .await
        .unwrap();

        let result = indexer
            .sequential_pipeline::<MockHandler>(MockHandler, SequentialConfig::default())
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_indexer_sequential_pipeline_disallow_inconsistent_first_checkpoint_with_skip_watermark()
     {
        let cancel = CancellationToken::new();
        let registry = Registry::new();

        let store = MockStore::default();
        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            "test_processor",
            CommitterWatermark {
                epoch_hi_inclusive: 1,
                checkpoint_hi_inclusive: 100,
                tx_hi: 1000,
                timestamp_ms_hi_inclusive: 1000000,
            },
        )
        .await
        .unwrap();

        let indexer_args = IndexerArgs {
            first_checkpoint: Some(1001),
            last_checkpoint: None,
            pipeline: vec![],
            skip_watermark: true,
        };
        let temp_dir = tempfile::tempdir().unwrap();
        let client_args = ClientArgs {
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(temp_dir.path().to_owned()),
                ..Default::default()
            },
            ..Default::default()
        };

        let ingestion_config = IngestionConfig::default();

        let mut indexer = Indexer::new(
            store,
            indexer_args,
            client_args,
            ingestion_config,
            None,
            &registry,
            cancel,
        )
        .await
        .unwrap();

        let result = indexer
            .sequential_pipeline::<MockHandler>(MockHandler, SequentialConfig::default())
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_indexer_sequential_pipeline_always_resume_from_watermark() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();
        let pipeline_checkpoint_hi = 10;
        let indexer_first_checkpoint = 5;
        let num_ingested_checkpoints = 10;

        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            "test_processor",
            CommitterWatermark {
                epoch_hi_inclusive: 1,
                checkpoint_hi_inclusive: pipeline_checkpoint_hi,
                tx_hi: 1000,
                timestamp_ms_hi_inclusive: 1000000,
            },
        )
        .await
        .unwrap();

        let indexer_args = IndexerArgs {
            first_checkpoint: Some(indexer_first_checkpoint),
            last_checkpoint: Some(indexer_first_checkpoint + num_ingested_checkpoints - 1),
            pipeline: vec![],
            skip_watermark: true,
        };
        let temp_dir = tempfile::tempdir().unwrap();
        synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
            ingestion_dir: temp_dir.path().to_owned(),
            starting_checkpoint: indexer_first_checkpoint,
            num_checkpoints: num_ingested_checkpoints,
            checkpoint_size: 2,
        })
        .await;

        let client_args = ClientArgs {
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(temp_dir.path().to_owned()),
                ..Default::default()
            },
            ..Default::default()
        };

        let ingestion_config = IngestionConfig::default();

        let mut indexer = Indexer::new(
            store,
            indexer_args,
            client_args,
            ingestion_config,
            None,
            &registry,
            cancel,
        )
        .await
        .unwrap();

        let _ = indexer
            .sequential_pipeline::<MockHandler>(MockHandler, SequentialConfig::default())
            .await;

        let ingestion_metrics = indexer.ingestion_metrics().clone();
        let indexer_metrics = indexer.indexer_metrics().clone();

        indexer.run().await.unwrap().await.unwrap();

        assert_eq!(
            ingestion_metrics.total_ingested_checkpoints.get(),
            num_ingested_checkpoints
        );
        assert_eq!(
            indexer_metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&["test_processor"])
                .unwrap()
                .get(),
            pipeline_checkpoint_hi - indexer_first_checkpoint + 1
        );
    }

    #[tokio::test]
    async fn test_indexer_concurrent_pipeline_always_resume_from_first_checkpoint() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();
        let pipeline_checkpoint_hi = 10;
        let indexer_first_checkpoint = 5;
        let num_ingested_checkpoints = 10;

        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            "test_processor",
            CommitterWatermark {
                epoch_hi_inclusive: 1,
                checkpoint_hi_inclusive: pipeline_checkpoint_hi,
                tx_hi: 1000,
                timestamp_ms_hi_inclusive: 1000000,
            },
        )
        .await
        .unwrap();

        let indexer_args = IndexerArgs {
            first_checkpoint: Some(indexer_first_checkpoint),
            last_checkpoint: Some(indexer_first_checkpoint + num_ingested_checkpoints - 1),
            pipeline: vec![],
            skip_watermark: true,
        };
        let temp_dir = tempfile::tempdir().unwrap();
        synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
            ingestion_dir: temp_dir.path().to_owned(),
            starting_checkpoint: indexer_first_checkpoint,
            num_checkpoints: num_ingested_checkpoints,
            checkpoint_size: 2,
        })
        .await;

        let client_args = ClientArgs {
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(temp_dir.path().to_owned()),
                ..Default::default()
            },
            ..Default::default()
        };

        let ingestion_config = IngestionConfig::default();

        let mut indexer = Indexer::new(
            store,
            indexer_args,
            client_args,
            ingestion_config,
            None,
            &registry,
            cancel,
        )
        .await
        .unwrap();

        let _ = indexer
            .concurrent_pipeline::<MockHandler>(MockHandler, ConcurrentConfig::default())
            .await;

        let ingestion_metrics = indexer.ingestion_metrics().clone();
        let indexer_metrics = indexer.indexer_metrics().clone();

        indexer.run().await.unwrap().await.unwrap();

        assert_eq!(
            ingestion_metrics.total_ingested_checkpoints.get(),
            num_ingested_checkpoints
        );
        assert_eq!(
            indexer_metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&["test_processor"])
                .unwrap()
                .get(),
            0
        );
    }

    #[tokio::test]
    async fn test_multiple_sequential_pipelines_next_checkpoint() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();

        // Set up different watermarks for three different sequential pipelines
        let mut conn = store.connect().await.unwrap();

        // First handler at checkpoint 10
        conn.set_committer_watermark(
            MockHandler::NAME,
            CommitterWatermark {
                epoch_hi_inclusive: 0,
                checkpoint_hi_inclusive: 10,
                tx_hi: 20,
                timestamp_ms_hi_inclusive: 10000,
            },
        )
        .await
        .unwrap();

        // SequentialHandler at checkpoint 5
        conn.set_committer_watermark(
            SequentialHandler::NAME,
            CommitterWatermark {
                epoch_hi_inclusive: 0,
                checkpoint_hi_inclusive: 5,
                tx_hi: 10,
                timestamp_ms_hi_inclusive: 5000,
            },
        )
        .await
        .unwrap();

        // Create synthetic ingestion data
        let temp_dir = tempfile::tempdir().unwrap();
        synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
            ingestion_dir: temp_dir.path().to_owned(),
            starting_checkpoint: 0,
            num_checkpoints: 20,
            checkpoint_size: 2,
        })
        .await;

        let indexer_args = IndexerArgs {
            first_checkpoint: None,
            last_checkpoint: Some(19),
            pipeline: vec![],
            skip_watermark: false,
        };

        let client_args = ClientArgs {
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(temp_dir.path().to_owned()),
                ..Default::default()
            },
            ..Default::default()
        };

        let ingestion_config = IngestionConfig::default();

        let mut indexer = Indexer::new(
            store.clone(),
            indexer_args,
            client_args,
            ingestion_config,
            None,
            &registry,
            cancel.clone(),
        )
        .await
        .unwrap();

        // Add first sequential pipeline
        indexer
            .sequential_pipeline(
                MockHandler,
                pipeline::sequential::SequentialConfig::default(),
            )
            .await
            .unwrap();

        // Verify next_sequential_checkpoint is set correctly (10 + 1 = 11)
        assert_eq!(
            indexer.next_sequential_checkpoint(),
            Some(11),
            "next_sequential_checkpoint should be 11"
        );

        // Add second sequential pipeline
        indexer
            .sequential_pipeline(
                SequentialHandler,
                pipeline::sequential::SequentialConfig::default(),
            )
            .await
            .unwrap();

        // Should change to 6 (minimum of 6 and 11)
        assert_eq!(
            indexer.next_sequential_checkpoint(),
            Some(6),
            "next_sequential_checkpoint should still be 6"
        );

        // Run indexer to verify it can make progress past the initial hi and finish ingesting.
        indexer.run().await.unwrap().await.unwrap();

        // Verify each pipeline made some progress independently
        let watermark1 = conn.committer_watermark(MockHandler::NAME).await.unwrap();
        let watermark2 = conn
            .committer_watermark(SequentialHandler::NAME)
            .await
            .unwrap();

        assert_eq!(watermark1.unwrap().checkpoint_hi_inclusive, 19);
        assert_eq!(watermark2.unwrap().checkpoint_hi_inclusive, 19);
    }
}
