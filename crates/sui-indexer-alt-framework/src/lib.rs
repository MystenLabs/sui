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

/// Command-line arguments for the indexer
#[derive(clap::Args, Default, Debug, Clone)]
pub struct IndexerArgs {
    /// An optional task name configured on the indexer to support running one-off or temporary
    /// tasks, like backfills. By default there is no task name, and if left empty, watermark rows
    /// will be keyed by only the `pipeline`. If one is provided, the indexer will propagate the
    /// task name to each pipeline such that watermark rows will record both the pipeline and task
    /// values. The indexer without a task name is considered the main indexer, and similarly its
    /// pipelines are considered the main pipelines. This means that watermark rows keyed by just
    /// the pipeline are considered the main pipelines ... other tasks with the same pipeline name
    /// must respect the main pipeline's watermark row by operating within the `[reader_lo,
    /// committer_hi]` range.
    ///
    /// Example: An indexer with task "backfill-2024-01" running pipelines "events" and "objects"
    /// will create watermark entries for (pipeline="events", task="backfill-2024-01") and
    /// (pipeline="objects", task="backfill-2024-01").
    #[arg(long)]
    pub task: Option<String>,

    /// Override for the checkpoint to start ingestion from. The exact behavior is dependent on a
    /// few factors:
    ///
    /// - For main pipelines (pipelines without a task name), this value is ignored if a watermark
    ///   row already exists. The indexer will resume ingestion from the existing committer
    ///   watermark. Otherwise, the indexer will start ingestion from the provided
    ///   `first_checkpoint`.
    /// - For pipelines with a task name, if the corresponding watermark row already exists, this
    ///   value is ignored and the indexer will resume ingestion from the existing committer
    ///   watermark. Even if the watermark row does not exist, these tasked pipelines must resume
    ///   from a `checkpoint >= reader_lo` of the main pipeline.
    ///
    /// For all cases where the provided `first_checkpoint` is not respected, ingestion will start
    /// from the next checkpoint after the lowest watermark across all active pipelines.
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
    /// and instead resume from the existing committer watermark.
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
        let Some(mut watermark) = self.add_pipeline::<H>().await? else {
            return Ok(());
        };

        if self.first_checkpoint.is_some() {
            if let Some(existing_watermark) = &watermark {
                warn!(
                    "Pipeline {} has an existing watermark row, ignoring --first-checkpoint {} and resuming from committer_hi {}",
                    H::NAME, first_checkpoint, existing_watermark.checkpoint_hi_inclusive
                );
            } else {
                // considered main pipeline, can start arbitrarily
                if self.task.is_none() {
                    watermark =
                        initial_watermark_from_first_checkpoint(watermark, self.first_checkpoint);
                } else {
                    // need to make sure this task is running within the main pipeline's `[reader_lo, committer_hi]` range
                }
            }
            self.check_first_checkpoint_consistency::<H>(&watermark)?;
        } else {
            // if no checkpoint provided ... leave None for main
            // but task pipelines should be anchored to main's reader_lo
        }

        self.handles.push(concurrent::pipeline::<H>(
            handler,
            initial_watermark,
            config,
            false, // TODO (wlmyng) do we want to rip out skip_watermark everywhere? probably
            self.store.clone(),
            self.task.clone(),
            self.ingestion_service.subscribe().0,
            self.metrics.clone(),
            self.cancel.clone(),
        ));

        Ok(())
    }

    /// Checks that the first checkpoint override is consistent with the watermark for the pipeline.
    /// If the watermark does not exist, the override can be anything. If the watermark exists, the
    /// override must not leave any gap in the data: it can be in the past, or at the tip of the
    /// network, but not in the future.
    fn check_first_checkpoint_consistency<P: Processor>(
        &self,
        watermark: &Option<CommitterWatermark>,
    ) -> Result<()> {
        if let (Some(watermark), Some(first_checkpoint)) = (watermark, self.first_checkpoint) {
            ensure!(
                first_checkpoint <= watermark.checkpoint_hi_inclusive + 1,
                "For pipeline {}, first checkpoint override {} is too far ahead of watermark {}. \
                 This could create gaps in the data.",
                P::NAME,
                first_checkpoint,
                watermark.checkpoint_hi_inclusive,
            );
        }

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

    /// Add a pipeline to the indexer and retrieve its watermark from the database to update the
    /// indexer's `first_checkpoint_from_watermark`. If the watermark row for a pipeline does not
    /// exist, this value is 0.
    ///
    /// Returns:
    /// - `Ok(None)` if the pipeline is disabled (filtered out)
    /// - `Ok(Some(None))` if the pipeline is enabled but no watermark exists in the database
    /// - `Ok(Some(Some(watermark)))` if the pipeline is enabled and a watermark exists
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

        // I think the main logic goes here basiaclly
        // check that if main pipeline, start >= reader_lo
        // check if task pipeline, task_start >= main_reader_lo
        // now how do we reconcile with a race condition? i.e reader_lo correct at time, but by the time the pipelines run ...
        // pruner has caught up?
        // update pruner logic to check on all tasks of the same pipeline?

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
    /// and instead resume from the existing committer watermark.
    ///
    /// Sequential pipelines do not support pipeline tasks. These pipelines guarantee that each
    /// checkpoint is committed exactly once and in order. Running the same pipeline under a
    /// different task would violate these guarantees.
    pub async fn sequential_pipeline<H>(
        &mut self,
        handler: H,
        config: SequentialConfig,
    ) -> Result<()>
    where
        H: Handler<Store = T> + Send + Sync + 'static,
    {
        let Some(mut watermark) = self.add_pipeline::<H>().await? else {
            return Ok(());
        };

        if self.task.is_some() {
            bail!("Sequential pipelines do not support pipeline tasks. These pipelines guarantee that each checkpoint is committed exactly once and in order. Running the same pipeline under a different task would violate these guarantees.");
        }

        // For sequential pipelines, if watermark already exists, ignore first_checkpoint and resume
        // from the committer watermark.
        if let Some(first_checkpoint) = self.first_checkpoint {
            if let Some(existing_watermark) = &watermark {
                warn!(
            "Pipeline {} has an existing watermark row, ignoring --first-checkpoint {} and resuming from committer_hi {}",
            H::NAME, first_checkpoint, existing_watermark.checkpoint_hi_inclusive
        );
            } else {
                watermark =
                    initial_watermark_from_first_checkpoint(watermark, self.first_checkpoint);
            }
        }

        let (checkpoint_rx, watermark_tx) = self.ingestion_service.subscribe();

        self.handles.push(sequential::pipeline::<H>(
            handler,
            watermark,
            config,
            self.store.clone(),
            self.task.clone(),
            checkpoint_rx,
            watermark_tx,
            self.metrics.clone(),
            self.cancel.clone(),
        ));

        Ok(())
    }
}

/// Determine the correct starting watermark for a pipeline. This function assumes the
/// `first_checkpoint` is valid, and will return a synthetic watermark with its
/// `checkpoint_hi_inclusive` set to the `first_checkpoint` - 1. Otherwise, this function returns
/// the original pipeline watermark. If both values are `None`, this function returns `None`, which
/// indicates to the pipeline components that they should start indexing from 0.
fn initial_watermark_from_first_checkpoint(
    pipeline_watermark: Option<CommitterWatermark>,
    first_checkpoint: Option<u64>,
) -> Option<CommitterWatermark> {
    match (pipeline_watermark, first_checkpoint) {
        (_, Some(first_checkpoint)) => {
            // Special case - the pipelines assume that an extant watermark represents the last
            // committed checkpoint. Thus, if trying to start from 0, we do not create a synthetic
            // watermark.
            if first_checkpoint == 0 {
                None
            } else {
                Some(CommitterWatermark {
                    checkpoint_hi_inclusive: first_checkpoint.saturating_sub(1),
                    ..Default::default()
                })
            }
        }
        (Some(watermark), None) => Some(watermark),
        (None, None) => None,
    }
}

#[cfg(test)]
pub mod testing;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::concurrent::ConcurrentConfig;
    use crate::store::CommitterWatermark;
    use crate::testing::mock_store::MockStore;
    use crate::FieldCount;
    use std::sync::Arc;
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

    #[test]
    fn test_initial_watermark_from_first_checkpoint() {
        // Test case 1: No watermark + first_checkpoint provided
        let result = initial_watermark_from_first_checkpoint(None, Some(100));
        assert_eq!(result.unwrap().checkpoint_hi_inclusive, 99);

        // Test case 2: Existing watermark + no first_checkpoint
        let existing = CommitterWatermark::new_for_testing(50);
        let result = initial_watermark_from_first_checkpoint(Some(existing), None);
        assert_eq!(result.unwrap().checkpoint_hi_inclusive, 50);

        // Test case 3: No watermark + no first_checkpoint
        let result = initial_watermark_from_first_checkpoint(None, None);
        assert!(result.is_none());

        // Test case 4: Edge case - first_checkpoint = 0
        let result = initial_watermark_from_first_checkpoint(None, Some(0));
        assert!(result.is_none());
    }
}
