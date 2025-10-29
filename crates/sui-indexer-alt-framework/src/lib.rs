// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use anyhow::{Context, bail, ensure};
use futures::future;
use ingestion::{ClientArgs, IngestionConfig, IngestionService, client::IngestionClient};
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
    /// An optional task name for this indexer. When set, pipelines will record watermarks using the
    /// format `{pipeline}{delimiter}{task}`. This allows the same pipelines to run under multiple
    /// indexers (e.g. for backfills or temporary workflows) while maintaining separate watermark
    /// entries in the database.
    ///
    /// By default there is no task name, and watermarks are keyed only by `pipeline`.
    ///
    /// Sequential pipelines cannot be attached to a tasked indexer.
    ///
    /// The framework ensures that tasked pipelines never commit checkpoints below the main
    /// pipeline’s pruner watermark. If pruning is enabled for the main pipeline, the tasked
    /// pipeline should use the same pruning configuration to correctly track both the main
    /// pipeline’s pruning and reader watermarks.
    #[arg(long)]
    pub task: Option<String>,

    /// Override for the checkpoint to start ingestion from.
    ///
    /// An untasked indexer complains if this first checkpoint is larger than any of its pipelines'
    /// committer watermark.
    ///
    /// If the pipeline or task has never been run before, the pipeline is initialized to start
    /// committing from `--first-checkpoint`, or from genesis if the value is not set. If the
    /// pipeline or task has an existing watermark, the pipeline will ignore this flag and resume
    /// committing from the existing watermark.
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

    /// An optional task name associated with this indexer instance. When set, the indexer records
    /// all pipeline watermarks using the format `{pipeline}{delimiter}{task}`. This allows multiple
    /// indexers to run the same pipelines concurrently (e.g. for backfills or temporary workflows)
    /// while maintaining separate watermark entries in the database.
    ///
    /// By default there is no task name, and watermarks are keyed only by `pipeline`.
    ///
    /// Sequential pipelines cannot be attached to a tasked indexer.
    ///
    /// The framework ensures that tasked pipelines never commit checkpoints below the main
    /// pipeline’s reader watermark. If pruning is enabled for the main pipeline, the tasked
    /// pipeline should use the same pruning configuration to correctly track both the main
    /// pipeline’s pruning and reader watermarks.
    task: Option<String>,

    /// Optional override of the checkpoint lowerbound. By default, ingestion will start just after
    /// the lowest committer watermark across all active pipelines. When set, the indexer will start
    /// ingestion from this checkpoint. Pipelines will start committing from this checkpoint if they
    /// do not have a watermark row. Otherwise, pipelines will ignore this flag and resume
    /// committing from the existing watermark.
    first_checkpoint: Option<u64>,

    /// Optional override of the checkpoint upperbound. When set, the indexer will stop ingestion at
    /// this checkpoint.
    last_checkpoint: Option<u64>,

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
            task,
        } = indexer_args;

        let metrics = IndexerMetrics::new(metrics_prefix, registry);

        let ingestion_service = IngestionService::new(
            client_args,
            ingestion_config,
            metrics.clone(),
            cancel.clone(),
        )?;

        Ok(Self {
            store,
            metrics,
            ingestion_service,
            task,
            first_checkpoint,
            last_checkpoint,
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
        self.ingestion_service.client()
    }

    /// The indexer's metrics.
    pub fn metrics(&self) -> &Arc<IndexerMetrics> {
        &self.metrics
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
    ///
    /// If `--first-checkpoint` is set, this value is obeyed only if the pipeline does not already
    /// have a watermark row. Otherwise, the concurrent pipeline will ignore the configured value
    /// and instead resume committing from the existing committer watermark.
    ///
    /// Additionally, pipelines with a task name must respect the main pipeline's watermark row by
    /// committing checkpoints no less than the main pipeline's pruner watermark.
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

        let next_checkpoint = match (watermark, self.first_checkpoint) {
            (Some(watermark), Some(first_checkpoint)) => {
                // If this is not a tasked indexer, we're dealing with main pipelines. Check that
                // the `--first-checkpoint` is not greater than this pipeline's committer watermark,
                // as this would cause the pipeline to stall forever in wait of a checkpoint that
                // will never be ingested.
                if self.task.is_none() {
                    ensure!(
                        first_checkpoint <= watermark.checkpoint_hi_inclusive + 1,
                        "For pipeline {}, first checkpoint override {} is too far ahead of watermark {}. \
                            This could create gaps in the data.",
                        H::NAME,
                        first_checkpoint,
                        watermark.checkpoint_hi_inclusive,
                    );
                }
                watermark.checkpoint_hi_inclusive + 1
            }
            (Some(watermark), _) => watermark.checkpoint_hi_inclusive + 1,
            (_, Some(first_checkpoint)) => first_checkpoint,
            (None, None) => 0,
        };

        self.handles.push(concurrent::pipeline::<H>(
            handler,
            next_checkpoint,
            config,
            self.store.clone(),
            self.task.clone(),
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
        // Validate pipeline name doesn't contain the store's delimiter
        ensure!(
            !P::NAME.contains(S::DELIMITER),
            "Invalid pipeline name '{}': cannot contain delimiter '{}'",
            P::NAME,
            S::DELIMITER
        );

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

        let watermark_key = S::watermark_key(P::NAME, self.task.as_deref());

        let watermark = conn
            .committer_watermark(&watermark_key)
            .await
            .with_context(|| format!("Failed to get watermark for pipeline {}", watermark_key))?;

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
    /// Sequential pipelines commit checkpoint data in-order which sacrifices throughput, but may be
    /// required to handle pipelines that modify data in-place (where each update is not an insert,
    /// but could be a modification of an existing row, where ordering between updates is
    /// important).
    ///
    /// The pipeline can optionally be configured to lag behind the ingestion service by a fixed
    /// number of checkpoints (configured by `checkpoint_lag`).
    ///
    /// If `--first-checkpoint` is set, this value is obeyed only if the pipeline does not already
    /// have a watermark row. Otherwise, the sequential pipeline will ignore the configured value
    /// and instead resume committing from the existing committer watermark.
    ///
    /// Sequential pipelines do not support pipeline tasks. Because pipelines guarantee that each
    /// checkpoint is committed exactly once and in order, running the same pipeline under a
    /// different task would violate these guarantees.
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

        if self.task.is_some() {
            bail!(
                "Sequential pipelines do not support pipeline tasks. \
                These pipelines guarantee that each checkpoint is committed exactly once and in order. \
                Running the same pipeline under a different task would violate these guarantees."
            );
        }

        let next_checkpoint = match (watermark, self.first_checkpoint) {
            (Some(watermark), Some(first_checkpoint)) => {
                // Sequential pipelines must write data in the order of checkpoints. If there is a
                // gap, this would cause the pipeline to stall forever in wait of a checkpoint that
                // will never be ingested.
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
    use std::sync::Arc;

    use async_trait::async_trait;
    use sui_synthetic_ingestion::synthetic_ingestion;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::FieldCount;
    use crate::mocks::store::{MockConnection, MockStore};
    use crate::pipeline::{
        Processor,
        concurrent::{ConcurrentConfig, PrunerConfig},
    };
    use crate::store::CommitterWatermark;

    #[allow(dead_code)]
    #[derive(Clone, FieldCount)]
    struct MockValue(u64);

    struct MockHandler;

    struct MockCheckpointSequenceNumberHandler;

    #[async_trait]
    impl Processor for MockHandler {
        const NAME: &'static str = "test_processor";
        type Value = MockValue;
        async fn process(
            &self,
            _checkpoint: &Arc<sui_types::full_checkpoint_content::CheckpointData>,
        ) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![MockValue(1)])
        }
    }

    #[async_trait]
    impl crate::pipeline::concurrent::Handler for MockHandler {
        type Store = MockStore;

        async fn commit<'a>(
            _values: &[Self::Value],
            _conn: &mut MockConnection<'a>,
        ) -> anyhow::Result<usize> {
            Ok(1)
        }
    }

    #[async_trait]
    impl crate::pipeline::sequential::Handler for MockHandler {
        type Store = MockStore;
        type Batch = Vec<Self::Value>;

        fn batch(batch: &mut Self::Batch, values: Vec<Self::Value>) {
            batch.extend(values);
        }

        async fn commit<'a>(
            _batch: &Self::Batch,
            _conn: &mut MockConnection<'a>,
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
            _checkpoint: &Arc<sui_types::full_checkpoint_content::CheckpointData>,
        ) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![MockValue(1)])
        }
    }

    #[async_trait]
    impl crate::pipeline::sequential::Handler for SequentialHandler {
        type Store = MockStore;
        type Batch = Vec<MockValue>;

        fn batch(batch: &mut Self::Batch, values: Vec<Self::Value>) {
            batch.extend(values);
        }

        async fn commit<'a>(
            _batch: &Self::Batch,
            _conn: &mut <Self::Store as Store>::Connection<'a>,
        ) -> anyhow::Result<usize> {
            Ok(1)
        }
    }

    #[async_trait]
    impl Processor for MockCheckpointSequenceNumberHandler {
        const NAME: &'static str = "test";
        type Value = MockValue;
        async fn process(
            &self,
            checkpoint: &Arc<sui_types::full_checkpoint_content::CheckpointData>,
        ) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![MockValue(
                checkpoint.checkpoint_summary.sequence_number,
            )])
        }
    }

    #[async_trait]
    impl crate::pipeline::concurrent::Handler for MockCheckpointSequenceNumberHandler {
        type Store = MockStore;

        async fn commit<'a>(
            values: &[Self::Value],
            conn: &mut MockConnection<'a>,
        ) -> anyhow::Result<usize> {
            for value in values {
                conn.0
                    .commit_data(Self::NAME, value.0, vec![value.0])
                    .await?;
            }
            Ok(values.len())
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
            ..Default::default()
        };
        let temp_dir = tempfile::tempdir().unwrap();
        let client_args = ClientArgs {
            local_ingestion_path: Some(temp_dir.path().to_owned()),
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
            ..Default::default()
        };
        let temp_dir = tempfile::tempdir().unwrap();
        let client_args = ClientArgs {
            local_ingestion_path: Some(temp_dir.path().to_owned()),
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
            ..Default::default()
        };
        let temp_dir = tempfile::tempdir().unwrap();
        let client_args = ClientArgs {
            local_ingestion_path: Some(temp_dir.path().to_owned()),
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
    async fn test_indexer_sequential_pipeline_disallow_task() {
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
            task: Some("should_fail".to_string()),
        };
        let temp_dir = tempfile::tempdir().unwrap();
        let client_args = ClientArgs {
            local_ingestion_path: Some(temp_dir.path().to_owned()),
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
            ..Default::default()
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
            local_ingestion_path: Some(temp_dir.path().to_owned()),
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

        let metrics = indexer.metrics().clone();

        indexer.run().await.unwrap().await.unwrap();

        assert_eq!(
            metrics.total_ingested_checkpoints.get(),
            num_ingested_checkpoints
        );
        assert_eq!(
            metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&["test_processor"])
                .unwrap()
                .get(),
            pipeline_checkpoint_hi - indexer_first_checkpoint + 1
        );
    }

    #[tokio::test]
    async fn test_indexer_concurrent_pipeline_always_resume_from_watermark() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();
        let pipeline_checkpoint_hi = 10;
        let indexer_first_checkpoint = 5;
        let checkpoints_to_ingest = 10;

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
            last_checkpoint: Some(indexer_first_checkpoint + checkpoints_to_ingest - 1),
            pipeline: vec![],
            ..Default::default()
        };
        let temp_dir = tempfile::tempdir().unwrap();
        synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
            ingestion_dir: temp_dir.path().to_owned(),
            starting_checkpoint: indexer_first_checkpoint,
            num_checkpoints: checkpoints_to_ingest,
            checkpoint_size: 2,
        })
        .await;

        let client_args = ClientArgs {
            local_ingestion_path: Some(temp_dir.path().to_owned()),
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

        let metrics = indexer.metrics().clone();

        indexer.run().await.unwrap().await.unwrap();

        assert_eq!(
            metrics.total_ingested_checkpoints.get(),
            checkpoints_to_ingest
        );
        assert_eq!(
            metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&["test_processor"])
                .unwrap()
                .get(),
            6
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
            ..Default::default()
        };

        let client_args = ClientArgs {
            local_ingestion_path: Some(temp_dir.path().to_owned()),
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

    /// When a tasked indexer is initialized such that a tasked pipeline is run with a
    /// `first_checkpoint` less than the main pipeline's reader_lo, the indexer will correctly skip
    /// committing checkpoints less than the main pipeline's reader watermark.
    #[tokio::test]
    async fn test_tasked_pipelines_ignore_below_main_reader_lo() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();

        // Mock the store as if we have a main pipeline with a committer watermark at `10` and a
        // reader watermark at `7`.
        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            MockCheckpointSequenceNumberHandler::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_reader_watermark(MockCheckpointSequenceNumberHandler::NAME, 7)
            .await
            .unwrap();

        // Start a tasked indexer that will ingest from checkpoint 0. Checkpoints 0 through 6 should
        // be ignored by the tasked indexer.
        let indexer_args = IndexerArgs {
            first_checkpoint: Some(0),
            last_checkpoint: Some(15),
            pipeline: vec![],
            task: Some("task".to_string()),
        };
        let temp_dir = tempfile::tempdir().unwrap();
        synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
            ingestion_dir: temp_dir.path().to_owned(),
            starting_checkpoint: 0,
            num_checkpoints: 16,
            checkpoint_size: 2,
        })
        .await;

        let client_args = ClientArgs {
            local_ingestion_path: Some(temp_dir.path().to_owned()),
            ..Default::default()
        };

        let ingestion_config = IngestionConfig::default();

        let mut tasked_indexer = Indexer::new(
            store.clone(),
            indexer_args,
            client_args,
            ingestion_config,
            None,
            &registry,
            cancel,
        )
        .await
        .unwrap();

        let _ = tasked_indexer
            .concurrent_pipeline(
                MockCheckpointSequenceNumberHandler,
                ConcurrentConfig {
                    pruner: Some(PrunerConfig {
                        interval_ms: 10,
                        delay_ms: 1000,
                        retention: 10,
                        max_chunk_size: 10,
                        prune_concurrency: 1,
                    }),
                    ..ConcurrentConfig::default()
                },
            )
            .await;

        let metrics = tasked_indexer.metrics().clone();

        tasked_indexer.run().await.unwrap().await.unwrap();

        assert_eq!(metrics.total_ingested_checkpoints.get(), 16);
        assert_eq!(
            metrics
                .collector_skipped_checkpoints
                .get_metric_with_label_values(&[MockCheckpointSequenceNumberHandler::NAME])
                .unwrap()
                .get(),
            7
        );
        let data = store
            .data
            .get(MockCheckpointSequenceNumberHandler::NAME)
            .unwrap();
        assert_eq!(data.len(), 9);
        for i in 0..7 {
            assert!(data.get(&i).is_none());
        }
        for i in 7..16 {
            assert!(data.get(&i).is_some());
        }
    }

    /// Tasked pipelines can run ahead of the main pipeline's committer watermark.
    #[tokio::test]
    async fn test_tasked_pipelines_surpass_main_pipeline_committer_hi() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();

        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            "test",
            CommitterWatermark {
                checkpoint_hi_inclusive: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_reader_watermark("test", 5).await.unwrap();

        // Start a tasked indexer that will ingest from checkpoint 9 and go past the main pipeline's
        // watermarks.
        let indexer_args = IndexerArgs {
            first_checkpoint: Some(9),
            last_checkpoint: Some(25),
            pipeline: vec![],
            task: Some("task".to_string()),
        };
        let temp_dir = tempfile::tempdir().unwrap();
        synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
            ingestion_dir: temp_dir.path().to_owned(),
            starting_checkpoint: 9,
            num_checkpoints: 17,
            checkpoint_size: 2,
        })
        .await;

        let client_args = ClientArgs {
            local_ingestion_path: Some(temp_dir.path().to_owned()),
            ..Default::default()
        };

        let ingestion_config = IngestionConfig::default();

        let mut tasked_indexer = Indexer::new(
            store.clone(),
            indexer_args,
            client_args,
            ingestion_config,
            None,
            &registry,
            cancel,
        )
        .await
        .unwrap();

        let _ = tasked_indexer
            .concurrent_pipeline(
                MockCheckpointSequenceNumberHandler,
                ConcurrentConfig {
                    pruner: Some(PrunerConfig {
                        interval_ms: 10,
                        delay_ms: 1000,
                        retention: 10,
                        max_chunk_size: 10,
                        prune_concurrency: 1,
                    }),
                    ..ConcurrentConfig::default()
                },
            )
            .await;

        let metrics = tasked_indexer.metrics().clone();

        tasked_indexer.run().await.unwrap().await.unwrap();

        assert_eq!(metrics.total_ingested_checkpoints.get(), 17);
        assert_eq!(
            metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&["test"])
                .unwrap()
                .get(),
            0
        );
        assert_eq!(
            metrics
                .collector_skipped_checkpoints
                .get_metric_with_label_values(&[MockCheckpointSequenceNumberHandler::NAME])
                .unwrap()
                .get(),
            0
        );

        let data = store.data.get("test").unwrap();
        assert!(data.len() == 17);
        for i in 0..9 {
            assert!(data.get(&i).is_none());
        }
        for i in 9..26 {
            assert!(data.get(&i).is_some());
        }
        let main_pipeline_watermark = store.watermark("test").unwrap();
        // assert that the main pipeline's watermarks are not updated
        assert_eq!(main_pipeline_watermark.checkpoint_hi_inclusive, 10);
        assert_eq!(main_pipeline_watermark.reader_lo, 5);
        let tasked_pipeline_watermark = store.watermark("test@task").unwrap();
        assert_eq!(tasked_pipeline_watermark.checkpoint_hi_inclusive, 25);
        assert_eq!(tasked_pipeline_watermark.reader_lo, 0);
    }

    /// During a run, the tasked pipeline will stop processing checkpoints before the main
    /// pipeline's reader watermark.
    #[tokio::test]
    async fn test_tasked_pipelines_stop_when_trailing_main_reader_lo() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();

        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            MockCheckpointSequenceNumberHandler::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        // Start a tasked indexer that will ingest from genesis to checkpoint 500.
        let indexer_args = IndexerArgs {
            first_checkpoint: Some(0),
            last_checkpoint: Some(500),
            pipeline: vec![],
            task: Some("task".to_string()),
        };
        let temp_dir = tempfile::tempdir().unwrap();
        synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
            ingestion_dir: temp_dir.path().to_owned(),
            starting_checkpoint: 0,
            num_checkpoints: 501,
            checkpoint_size: 2,
        })
        .await;

        let client_args = ClientArgs {
            local_ingestion_path: Some(temp_dir.path().to_owned()),
            ..Default::default()
        };

        let ingestion_config = IngestionConfig::default();

        let mut tasked_indexer = Indexer::new(
            store.clone(),
            indexer_args,
            client_args,
            ingestion_config,
            None,
            &registry,
            cancel,
        )
        .await
        .unwrap();

        let _ = tasked_indexer
            .concurrent_pipeline(
                MockCheckpointSequenceNumberHandler,
                ConcurrentConfig {
                    pruner: Some(PrunerConfig {
                        interval_ms: 10,
                        delay_ms: 1000,
                        retention: 10,
                        max_chunk_size: 10,
                        prune_concurrency: 1,
                    }),
                    ..ConcurrentConfig::default()
                },
            )
            .await;

        let metrics = tasked_indexer.metrics().clone();

        let indexer_handle = tokio::spawn(async move {
            tasked_indexer.run().await.unwrap().await.unwrap();
        });

        // This wait ensures that the tasked pipeline has started committing data.
        store
            .wait_for_any_data(
                MockCheckpointSequenceNumberHandler::NAME,
                std::time::Duration::from_millis(5000),
            )
            .await;

        // Artificially bump the reader watermark to checkpoint 250. The tasked pipeline should only
        // commit checkpoints >= 250 once it ticks.
        conn.set_committer_watermark(
            MockCheckpointSequenceNumberHandler::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 300,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_reader_watermark(MockCheckpointSequenceNumberHandler::NAME, 250)
            .await
            .unwrap();

        indexer_handle.await.unwrap();

        let data = store
            .data
            .get(MockCheckpointSequenceNumberHandler::NAME)
            .unwrap();
        // All 500+1 checkpoints should have been ingested.
        assert_eq!(metrics.total_ingested_checkpoints.get(), 501);

        let ge_250 = data.iter().filter(|e| *e.key() >= 250).count();
        let lt_250 = data.iter().filter(|e| *e.key() < 250).count();
        // Checkpoints 250 to 500 inclusive must have been committed.
        assert_eq!(ge_250, 251);
        // Lenient check that not all checkpoints < 250 were committed.
        assert!(lt_250 < 250);
    }
}
