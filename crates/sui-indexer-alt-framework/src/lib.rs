// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use anyhow::{bail, ensure, Context};
use futures::future;
use ingestion::{client::IngestionClient, ClientArgs, IngestionConfig, IngestionService};
use metrics::IndexerMetrics;
use pipeline::{
    concurrent::{self, ConcurrentConfig},
    sequential::{self, Handler, SequentialConfig},
    Processor,
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
    /// An optional task name to support running one-off or temporary tasks, like backfills, in
    /// conjunction with `--first-checkpoint`. By default there is no task name, and pipelines run
    /// by an indexer without a configured task name are considered main pipelines. The same
    /// pipelines running on an indexer with a task name are considered task pipelines, and respect
    /// the main pipeline's watermarks. All pipelines running under a tasked-indexer will have the
    /// same task name.
    ///
    /// Sequential pipelines can only be run as main pipelines. Concurrent pipelines can be run as
    /// main and/ or task pipelines.
    ///
    /// Task pipelines must be configured to start from a checkpoint no less than the main
    /// pipeline's reader watermark, cannot enable pruning, and will push only their respective
    /// committer watermarks. Once instantiated and running, the main pipeline is responsible for
    /// setting the pruner and reader watermark of its tasks.
    #[arg(long)]
    pub task: Option<String>,

    /// Override for the checkpoint to start ingestion from -- useful for backfills. By default,
    /// ingestion will start just after the lowest checkpoint watermark across all active pipelines.
    /// If set, ingestion will start from the configured checkpoint.
    ///
    /// For an indexer without a task name, the indexer will check that the `--first-checkpoint` is
    /// not greater than any main pipeline's committer watermark, as this would create a gap in the
    /// indexed data. Its pipelines will start committing from `--first-checkpoint` if those
    /// pipelines do not have a committer watermark. Otherwise, both sequential and concurrent
    /// pipelines will wait until the indexer ingests the next checkpoint after each pipeline's
    /// committer watermark.
    ///
    /// An indexer with a task name has tasked pipelines that must respect each pipeline's main
    /// watermark. In this scenario, the indexer will check that the `--first-checkpoint` does not
    /// start before any main pipeline's reader watermark. If the tasked pipeline has an existing
    /// watermark row, the pipeline will wait until the indexer ingests the next checkpoint after
    /// its committer watermark.
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

    /// An optional task name configured on the indexer to support running one-off or temporary
    /// tasks, like backfills. By default there is no task name, and if left empty, watermark rows
    /// will be keyed by only the `pipeline`. If one is provided, the indexer will propagate the
    /// task name to each pipeline such that watermark rows will record both the pipeline and task
    /// values.
    ///
    /// Example: An indexer with task "backfill-2024-01" running pipelines "events" and "objects"
    /// will create watermark entries for (pipeline="events", task="backfill-2024-01") and
    /// (pipeline="objects", task="backfill-2024-01").
    task: Option<String>,

    /// Override for the checkpoint to start ingestion from. The exact behavior is dependent on a
    /// few factors:
    ///
    /// - If the pipeline and optional task combination has not been run before, the watermark will
    ///   be set up to expect this checkpoint as the next one, and the reader's lower bound will be
    ///   set to this checkpoint.
    /// - If the pipeline + task has already been run and there is an existing watermark entry, this
    ///   value will be ignored (with a warning) and ingestion will resume from the existing
    ///   committer_hi watermark, unless `--skip-watermark` is also supplied.
    /// - If `--skip-watermark` is supplied, the pipeline will write data without updating
    ///   watermarks, but only if `first_checkpoint` is greater than or equal to the current
    ///   `reader_lo`. If `first_checkpoint < reader_lo`, the operation will fail to prevent writing
    ///   data that the pruner has already cleaned up.
    ///
    /// By default (when not specified), ingestion will start just after the lowest checkpoint
    /// watermark across all active pipelines.
    first_checkpoint: Option<u64>,

    /// Optional override of the checkpoint upperbound.
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
    ///
    /// Optionally, a task name can be provided to the indexer to support running one-off or
    /// temporary tasks, like backfills. By default, there is no task name. In this scenario, the
    /// indexer will write watermark rows for pipeline = `pipeline`. If one is provided, the indexer
    /// will write watermark rows for pipeline = `pipeline` and task = `task`.
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
            task,
            store,
            metrics,
            ingestion_service,
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

    /// Adds a new pipeline to this indexer and starts it up. Although their tasks have started,
    /// they will be idle until the ingestion service starts, and serves it checkpoint data.
    ///
    /// Concurrent pipelines commit checkpoint data out-of-order to maximise throughput, and they
    /// keep the watermark table up-to-date with the highest point they can guarantee all data
    /// exists for, for their pipeline.
    ///
    /// If `first-checkpoint` is set, this value is obeyed only if the pipeline does not already
    /// have a watermark row. Otherwise, the concurrent pipeline will ignore the configured value
    /// and instead resume committing from the existing committer watermark.
    ///
    /// Additionally, pipelines with a task name must respect the main pipeline's watermark row by
    /// operating within the main pipeline's `[reader_lo, committer_hi]` range.
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

        let next_checkpoint = if let Some(task) = self.task.as_deref() {
            let mut conn = self
                .store
                .connect()
                .await
                .context("Failed to establish connection to store")?;

            let main_reader_watermark = conn
                .reader_watermark(H::NAME)
                .await
                .context("Failed to get reader watermark")?
                .unwrap_or_default();

            let next_checkpoint = watermark
                .map(|w| w.checkpoint_hi_inclusive + 1)
                .or(self.first_checkpoint)
                .unwrap_or_default();

            if next_checkpoint < main_reader_watermark.reader_lo {
                warn!(
                    pipeline = H::NAME,
                    task = task,
                    main_reader_lo = main_reader_watermark.reader_lo,
                    "first_checkpoint or task committer watermark is below main pipeline's reader_lo. \
                     Starting tasked pipeline from main pipeline's reader_lo to avoid data gaps."
                );
            }

            main_reader_watermark.reader_lo.max(next_checkpoint)
        } else {
            // If this is not a tasked indexer, we're dealing with main pipelines. Check that the
            // `--first-checkpoint` is not greater than any main pipeline's committer watermark, as
            // this would create a gap in the indexed data.
            match (watermark, self.first_checkpoint) {
                (Some(watermark), Some(first_checkpoint)) => {
                    ensure!(
                        first_checkpoint <= watermark.checkpoint_hi_inclusive + 1,
                        "For pipeline {}, first checkpoint override {} is too far ahead of watermark {}. \
                        This could create gaps in the data.",
                        H::NAME,
                        first_checkpoint,
                        watermark.checkpoint_hi_inclusive,
                );
                    watermark.checkpoint_hi_inclusive + 1
                }
                (Some(watermark), _) => watermark.checkpoint_hi_inclusive + 1,
                (_, Some(first_checkpoint)) => first_checkpoint,
                (None, None) => 0,
            }
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

        let (regulator_handle, broadcaster_handle) = self
            .ingestion_service
            .run(first_checkpoint..=last_checkpoint)
            .await
            .context("Failed to start ingestion service")?;

        self.handles.push(regulator_handle);
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

        if let Some(enabled_pipelines) = &mut self.enabled_pipelines {
            if !enabled_pipelines.remove(P::NAME) {
                info!(pipeline = P::NAME, "Skipping");
                return Ok(None);
            }
        }

        let mut conn = self
            .store
            .connect()
            .await
            .context("Failed to establish connection to store")?;

        let watermark = conn
            .committer_watermark(P::NAME, self.task.as_deref())
            .await
            .with_context(|| {
                if let Some(task) = self.task.as_deref() {
                    format!(
                        "Failed to get watermark for pipeline {} of task {}",
                        P::NAME,
                        task
                    )
                } else {
                    format!("Failed to get watermark for pipeline {}", P::NAME)
                }
            })?;

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
    /// If `first-checkpoint` is set, this value is obeyed only if the pipeline does not already
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
    use crate::mocks::store::MockStore;
    use crate::pipeline::concurrent::ConcurrentConfig;
    use crate::store::CommitterWatermark;
    use crate::FieldCount;
    use std::sync::Arc;
    use sui_synthetic_ingestion::synthetic_ingestion;
    use tokio_util::sync::CancellationToken;

    impl Processor for MockHandler {
        const NAME: &'static str = "test_processor";
        type Value = MockValue;
        fn process(
            &self,
            _checkpoint: &Arc<sui_types::full_checkpoint_content::CheckpointData>,
        ) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![MockValue(1)])
        }
    }

    #[allow(dead_code)]
    #[derive(Clone, FieldCount)]
    struct MockValue(u64);

    struct MockHandler;

    #[async_trait::async_trait]
    impl crate::pipeline::concurrent::Handler for MockHandler {
        type Store = MockStore;

        async fn commit<'a>(
            _values: &[Self::Value],
            _conn: &mut <Self::Store as Store>::Connection<'a>,
        ) -> anyhow::Result<usize> {
            Ok(1)
        }
    }

    #[async_trait::async_trait]
    impl crate::pipeline::sequential::Handler for MockHandler {
        type Store = MockStore;
        type Batch = Vec<Self::Value>;

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

    #[tokio::test]
    async fn test_first_checkpoint_from_watermark() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();

        let store = MockStore::default();
        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            "test_processor",
            None,
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
            task: None,
            first_checkpoint: Some(50),
            last_checkpoint: None,
            pipeline: vec![],
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
            None,
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
            task: None,
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
    async fn test_indexer_concurrent_pipeline_allow_inconsistent_first_checkpoint_with_skip_watermark(
    ) {
        let cancel = CancellationToken::new();
        let registry = Registry::new();

        let store = MockStore::default();
        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            "test_processor",
            None,
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
            task: None,
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
            None,
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
            task: None,
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
    async fn test_indexer_sequential_pipeline_disallow_inconsistent_first_checkpoint_with_skip_watermark(
    ) {
        let cancel = CancellationToken::new();
        let registry = Registry::new();

        let store = MockStore::default();
        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            "test_processor",
            None,
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
            task: None,
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
            None,
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
            task: None,
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
            None,
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
            task: None,
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
            .concurrent_pipeline::<MockHandler>(MockHandler, ConcurrentConfig::default())
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
            0
        );
    }
}
