// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc, time::Duration};

use anyhow::{Context, bail, ensure};
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
    Connection, Store, TransactionalStore, pipeline_task,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;

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
    /// Override the next checkpoint for all pipelines without a committer watermark to start
    /// processing from, which is 0 by default. Pipelines with existing watermarks will ignore this
    /// setting and always resume from their committer watermark + 1.
    ///
    /// Setting this value indirectly affects ingestion, as the checkpoint to start ingesting from
    /// is the minimum across all pipelines' next checkpoints.
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

    /// Additional configurations for running a tasked indexer.
    #[clap(flatten)]
    pub task: TaskArgs,
}

/// Command-line arguments for configuring a tasked indexer.
#[derive(clap::Parser, Default, Debug, Clone)]
pub struct TaskArgs {
    /// An optional task name for this indexer. When set, pipelines will record watermarks using the
    /// delimiter defined on the store. This allows the same pipelines to run under multiple
    /// indexers (e.g. for backfills or temporary workflows) while maintaining separate watermark
    /// entries in the database.
    ///
    /// By default there is no task name, and watermarks are keyed only by `pipeline`.
    ///
    /// Sequential pipelines cannot be attached to a tasked indexer.
    ///
    /// The framework ensures that tasked pipelines never commit checkpoints below the main
    /// pipeline’s pruner watermark. Requires `--reader-interval-ms`.
    #[arg(long, requires = "reader-interval-ms")]
    task: Option<String>,

    /// The interval in milliseconds at which each of the pipelines on a tasked indexer should
    /// refetch its main pipeline's reader watermark. This is required when `--task` is set.
    #[arg(long, requires = "task")]
    reader_interval_ms: Option<u64>,
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

    /// The next checkpoint for a pipeline without a committer watermark to start processing from,
    /// which will be 0 by default. Pipelines with existing watermarks will ignore this setting and
    /// always resume from their committer watermark + 1.
    ///
    /// Setting this value indirectly affects ingestion, as the checkpoint to start ingesting from
    /// is the minimum across all pipelines' next checkpoints.
    default_next_checkpoint: u64,

    /// Optional override of the checkpoint upperbound. When set, the indexer will stop ingestion at
    /// this checkpoint.
    last_checkpoint: Option<u64>,

    /// An optional task name for this indexer. When set, pipelines will record watermarks using the
    /// delimiter defined on the store. This allows the same pipelines to run under multiple
    /// indexers (e.g. for backfills or temporary workflows) while maintaining separate watermark
    /// entries in the database.
    ///
    /// By default there is no task name, and watermarks are keyed only by `pipeline`.
    ///
    /// Sequential pipelines cannot be attached to a tasked indexer.
    ///
    /// The framework ensures that tasked pipelines never commit checkpoints below the main
    /// pipeline’s pruner watermark.
    task: Option<Task>,

    /// Optional filter for pipelines to run. If `None`, all pipelines added to the indexer will
    /// run. Any pipelines that are present in this filter but not added to the indexer will yield
    /// a warning when the indexer is run.
    enabled_pipelines: Option<BTreeSet<String>>,

    /// Pipelines that have already been registered with the indexer. Used to make sure a pipeline
    /// with the same name isn't added twice.
    added_pipelines: BTreeSet<&'static str>,

    /// Cancellation token shared among all continuous tasks in the service.
    cancel: CancellationToken,

    /// The checkpoint for the indexer to start ingesting from. This is derived from the committer
    /// watermarks of pipelines added to the indexer. Pipelines without watermarks default to 0,
    /// unless overridden by [Self::default_next_checkpoint].
    first_ingestion_checkpoint: u64,

    /// The minimum next_checkpoint across all sequential pipelines. This is used to initialize
    /// the regulator to prevent ingestion from running too far ahead of sequential pipelines.
    next_sequential_checkpoint: Option<u64>,

    /// The handles for every task spawned by this indexer, used to manage graceful shutdown.
    handles: Vec<JoinHandle<()>>,
}

/// Configuration for a tasked indexer.
#[derive(Clone)]
pub(crate) struct Task {
    /// Name of the tasked indexer, to be used with the delimiter defined on the indexer's store to
    /// record pipeline watermarks.
    task: String,
    /// The interval at which each of the pipelines on a tasked indexer should refecth its main
    /// pipeline's reader watermark.
    reader_interval: Duration,
}

impl TaskArgs {
    pub fn tasked(task: String, reader_interval_ms: u64) -> Self {
        Self {
            task: Some(task),
            reader_interval_ms: Some(reader_interval_ms),
        }
    }

    fn into_task(self) -> Option<Task> {
        Some(Task {
            task: self.task?,
            reader_interval: Duration::from_millis(self.reader_interval_ms?),
        })
    }
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
            metrics_prefix,
            registry,
            cancel.clone(),
        )?;

        Ok(Self {
            store,
            metrics,
            ingestion_service,
            default_next_checkpoint: first_checkpoint.unwrap_or_default(),
            last_checkpoint,
            enabled_pipelines: if pipeline.is_empty() {
                None
            } else {
                Some(pipeline.into_iter().collect())
            },
            added_pipelines: BTreeSet::new(),
            cancel,
            first_ingestion_checkpoint: u64::MAX,
            next_sequential_checkpoint: None,
            handles: vec![],
            task: task.into_task(),
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
        let Some(next_checkpoint) = self.add_pipeline::<H>().await? else {
            return Ok(());
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

    /// Start ingesting checkpoints from `first_ingestion_checkpoint`. Individual pipelines
    /// will start processing and committing once the ingestion service has caught up to their
    /// respective watermarks.
    ///
    /// Ingestion will stop after consuming the configured `last_checkpoint` if one is provided.
    pub async fn run(mut self) -> Result<JoinHandle<()>> {
        if let Some(enabled_pipelines) = self.enabled_pipelines {
            ensure!(
                enabled_pipelines.is_empty(),
                "Tried to enable pipelines that this indexer does not know about: \
                {enabled_pipelines:#?}",
            );
        }

        let last_checkpoint = self.last_checkpoint.unwrap_or(u64::MAX);

        info!(self.first_ingestion_checkpoint, last_checkpoint = ?self.last_checkpoint, "Ingestion range");

        let broadcaster_handle = self
            .ingestion_service
            .run(
                self.first_ingestion_checkpoint..=last_checkpoint,
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

    /// Determine the checkpoint for the pipeline to resume processing from. This is either the
    /// checkpoint after its watermark, or if that doesn't exist, then the provided
    /// [Self::first_checkpoint], and if that is not set, then 0 (genesis).
    ///
    /// Update the starting ingestion checkpoint as the minimum across all the next checkpoints
    /// calculated above.
    ///
    /// Returns `Ok(None)` if the pipeline is disabled.
    async fn add_pipeline<P: Processor + 'static>(&mut self) -> Result<Option<u64>> {
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

        let pipeline_task =
            pipeline_task::<S>(P::NAME, self.task.as_ref().map(|t| t.task.as_str()))?;

        let watermark = conn
            .committer_watermark(&pipeline_task)
            .await
            .with_context(|| format!("Failed to get watermark for {pipeline_task}"))?;

        let next_checkpoint = watermark
            .as_ref()
            .map(|w| w.checkpoint_hi_inclusive + 1)
            .unwrap_or(self.default_next_checkpoint);

        self.first_ingestion_checkpoint = next_checkpoint.min(self.first_ingestion_checkpoint);

        Ok(Some(next_checkpoint))
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
    pub async fn sequential_pipeline<H>(
        &mut self,
        handler: H,
        config: SequentialConfig,
    ) -> Result<()>
    where
        H: Handler<Store = T> + Send + Sync + 'static,
    {
        let Some(next_checkpoint) = self.add_pipeline::<H>().await? else {
            return Ok(());
        };

        if self.task.is_some() {
            bail!(
                "Sequential pipelines do not support pipeline tasks. \
                These pipelines guarantee that each checkpoint is committed exactly once and in order. \
                Running the same pipeline under a different task would violate these guarantees."
            );
        }

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
    use tokio::sync::watch;
    use tokio_util::sync::CancellationToken;

    use crate::FieldCount;
    use crate::ingestion::ingestion_client::IngestionClientArgs;
    use crate::mocks::store::MockStore;
    use crate::pipeline::CommitterConfig;
    use crate::pipeline::{Processor, concurrent::ConcurrentConfig};
    use crate::store::CommitterWatermark;

    use super::*;

    #[allow(dead_code)]
    #[derive(Clone, FieldCount)]
    struct MockValue(u64);

    /// A handler that can be controlled externally to block checkpoint processing.
    struct ControllableHandler {
        /// Process checkpoints less than or equal to this value.
        process_below: watch::Receiver<u64>,
    }

    impl ControllableHandler {
        fn with_limit(limit: u64) -> (Self, watch::Sender<u64>) {
            let (tx, rx) = watch::channel(limit);
            (Self { process_below: rx }, tx)
        }
    }

    #[async_trait]
    impl Processor for ControllableHandler {
        const NAME: &'static str = "controllable";
        type Value = MockValue;

        async fn process(
            &self,
            checkpoint: &Arc<sui_types::full_checkpoint_content::Checkpoint>,
        ) -> anyhow::Result<Vec<Self::Value>> {
            let cp_num = checkpoint.summary.sequence_number;

            // Wait until the checkpoint is allowed to be processed
            self.process_below
                .clone()
                .wait_for(|&limit| cp_num <= limit)
                .await
                .ok();

            Ok(vec![MockValue(cp_num)])
        }
    }

    #[async_trait]
    impl crate::pipeline::concurrent::Handler for ControllableHandler {
        type Store = MockStore;
        type Batch = Vec<MockValue>;

        fn batch(
            &self,
            batch: &mut Self::Batch,
            values: &mut std::vec::IntoIter<Self::Value>,
        ) -> crate::pipeline::concurrent::BatchStatus {
            batch.extend(values);
            crate::pipeline::concurrent::BatchStatus::Ready
        }

        async fn commit<'a>(
            &self,
            batch: &Self::Batch,
            conn: &mut <Self::Store as Store>::Connection<'a>,
        ) -> anyhow::Result<usize> {
            for value in batch {
                conn.0
                    .commit_data(Self::NAME, value.0, vec![value.0])
                    .await?;
            }
            Ok(batch.len())
        }
    }

    macro_rules! test_pipeline {
        ($handler:ident, $name:literal) => {
            struct $handler;

            #[async_trait]
            impl Processor for $handler {
                const NAME: &'static str = $name;
                type Value = MockValue;
                async fn process(
                    &self,
                    checkpoint: &Arc<sui_types::full_checkpoint_content::Checkpoint>,
                ) -> anyhow::Result<Vec<Self::Value>> {
                    Ok(vec![MockValue(checkpoint.summary.sequence_number)])
                }
            }

            #[async_trait]
            impl crate::pipeline::concurrent::Handler for $handler {
                type Store = MockStore;
                type Batch = Vec<Self::Value>;

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
                    batch: &Self::Batch,
                    conn: &mut <Self::Store as Store>::Connection<'a>,
                ) -> anyhow::Result<usize> {
                    for value in batch {
                        conn.0
                            .commit_data(Self::NAME, value.0, vec![value.0])
                            .await?;
                    }
                    Ok(batch.len())
                }
            }

            #[async_trait]
            impl crate::pipeline::sequential::Handler for $handler {
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
        };
    }

    test_pipeline!(MockHandler, "test_processor");
    test_pipeline!(SequentialHandler, "sequential_handler");
    test_pipeline!(MockCheckpointSequenceNumberHandler, "test");

    /// first_ingestion_checkpoint is smallest among existing watermarks + 1.
    #[tokio::test]
    async fn test_first_ingestion_checkpoint_all_pipelines_have_watermarks() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");
        test_pipeline!(D, "sequential_d");

        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            A::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 100,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            B::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            C::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 1,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            D::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 50,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let indexer_args = IndexerArgs::default();
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
            .concurrent_pipeline::<A>(A, ConcurrentConfig::default())
            .await
            .unwrap();
        indexer
            .concurrent_pipeline::<B>(B, ConcurrentConfig::default())
            .await
            .unwrap();
        indexer
            .sequential_pipeline::<C>(C, SequentialConfig::default())
            .await
            .unwrap();
        indexer
            .sequential_pipeline::<D>(D, SequentialConfig::default())
            .await
            .unwrap();

        assert_eq!(indexer.first_ingestion_checkpoint, 2);
    }

    /// first_ingestion_checkpoint is 0 when at least one pipeline has no watermark.
    #[tokio::test]
    async fn test_first_ingestion_checkpoint_not_all_pipelines_have_watermarks() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");
        test_pipeline!(D, "sequential_d");

        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            B::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            C::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 1,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let indexer_args = IndexerArgs::default();
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
            .concurrent_pipeline::<A>(A, ConcurrentConfig::default())
            .await
            .unwrap();
        indexer
            .concurrent_pipeline::<B>(B, ConcurrentConfig::default())
            .await
            .unwrap();
        indexer
            .sequential_pipeline::<C>(C, SequentialConfig::default())
            .await
            .unwrap();
        indexer
            .sequential_pipeline::<D>(D, SequentialConfig::default())
            .await
            .unwrap();

        assert_eq!(indexer.first_ingestion_checkpoint, 0);
    }

    /// first_ingestion_checkpoint is 1 when smallest committer watermark is 0.
    #[tokio::test]
    async fn test_first_ingestion_checkpoint_smallest_is_0() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");
        test_pipeline!(D, "sequential_d");

        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            A::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 100,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            B::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            C::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 1,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(D::NAME, CommitterWatermark::default())
            .await
            .unwrap();

        let indexer_args = IndexerArgs::default();
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
            .concurrent_pipeline::<A>(A, ConcurrentConfig::default())
            .await
            .unwrap();
        indexer
            .concurrent_pipeline::<B>(B, ConcurrentConfig::default())
            .await
            .unwrap();
        indexer
            .sequential_pipeline::<C>(C, SequentialConfig::default())
            .await
            .unwrap();
        indexer
            .sequential_pipeline::<D>(D, SequentialConfig::default())
            .await
            .unwrap();

        assert_eq!(indexer.first_ingestion_checkpoint, 1);
    }

    /// first_ingestion_checkpoint is first_checkpoint when at least one pipeline has no
    /// watermark, and first_checkpoint is smallest.
    #[tokio::test]
    async fn test_first_ingestion_checkpoint_first_checkpoint_and_no_watermark() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");
        test_pipeline!(D, "sequential_d");

        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            B::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 50,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            C::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let indexer_args = IndexerArgs {
            first_checkpoint: Some(5),
            ..Default::default()
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
            .concurrent_pipeline::<A>(A, ConcurrentConfig::default())
            .await
            .unwrap();
        indexer
            .concurrent_pipeline::<B>(B, ConcurrentConfig::default())
            .await
            .unwrap();
        indexer
            .sequential_pipeline::<C>(C, SequentialConfig::default())
            .await
            .unwrap();
        indexer
            .sequential_pipeline::<D>(D, SequentialConfig::default())
            .await
            .unwrap();

        assert_eq!(indexer.first_ingestion_checkpoint, 5);
    }

    /// first_ingestion_checkpoint is smallest among existing watermarks + 1 if
    /// first_checkpoint but all pipelines have watermarks (ignores first_checkpoint).
    #[tokio::test]
    async fn test_first_ingestion_checkpoint_ignore_first_checkpoint() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();

        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");

        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            B::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 50,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            C::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let indexer_args = IndexerArgs {
            first_checkpoint: Some(5),
            ..Default::default()
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
            .concurrent_pipeline::<B>(B, ConcurrentConfig::default())
            .await
            .unwrap();
        indexer
            .sequential_pipeline::<C>(C, SequentialConfig::default())
            .await
            .unwrap();

        assert_eq!(indexer.first_ingestion_checkpoint, 11);
    }

    /// If the first_checkpoint is being considered, because pipelines are missing watermarks, it
    /// will not be used as the starting point if it is not the smallest valid committer watermark
    /// to resume ingesting from.
    #[tokio::test]
    async fn test_first_ingestion_checkpoint_large_first_checkpoint() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");

        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            B::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 50,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            C::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let indexer_args = IndexerArgs {
            first_checkpoint: Some(24),
            ..Default::default()
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
            .concurrent_pipeline::<A>(A, ConcurrentConfig::default())
            .await
            .unwrap();

        indexer
            .concurrent_pipeline::<B>(B, ConcurrentConfig::default())
            .await
            .unwrap();

        indexer
            .sequential_pipeline::<C>(C, SequentialConfig::default())
            .await
            .unwrap();

        assert_eq!(indexer.first_ingestion_checkpoint, 11);
    }

    // test ingestion, all pipelines have watermarks, no first_checkpoint provided
    #[tokio::test]
    async fn test_indexer_ingestion_existing_watermarks_no_first_checkpoint() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");
        test_pipeline!(D, "sequential_d");

        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            A::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 5,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            B::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            C::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 15,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            D::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 20,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        // Create synthetic ingestion data
        let temp_dir = tempfile::tempdir().unwrap();
        synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
            ingestion_dir: temp_dir.path().to_owned(),
            starting_checkpoint: 5,
            num_checkpoints: 25,
            checkpoint_size: 1,
        })
        .await;

        let indexer_args = IndexerArgs {
            last_checkpoint: Some(29),
            ..Default::default()
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

        indexer
            .concurrent_pipeline::<A>(A, ConcurrentConfig::default())
            .await
            .unwrap();
        indexer
            .concurrent_pipeline::<B>(B, ConcurrentConfig::default())
            .await
            .unwrap();
        indexer
            .sequential_pipeline::<C>(C, SequentialConfig::default())
            .await
            .unwrap();
        indexer
            .sequential_pipeline::<D>(D, SequentialConfig::default())
            .await
            .unwrap();

        let ingestion_metrics = indexer.ingestion_metrics().clone();
        let indexer_metrics = indexer.indexer_metrics().clone();

        indexer.run().await.unwrap().await.unwrap();

        assert_eq!(ingestion_metrics.total_ingested_checkpoints.get(), 24);
        assert_eq!(
            indexer_metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&[A::NAME])
                .unwrap()
                .get(),
            0
        );
        assert_eq!(
            indexer_metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&[B::NAME])
                .unwrap()
                .get(),
            5
        );
        assert_eq!(
            indexer_metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&[C::NAME])
                .unwrap()
                .get(),
            10
        );
        assert_eq!(
            indexer_metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&[D::NAME])
                .unwrap()
                .get(),
            15
        );
    }

    // test ingestion, no pipelines missing watermarks, first_checkpoint provided
    #[tokio::test]
    async fn test_indexer_ingestion_existing_watermarks_ignore_first_checkpoint() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");
        test_pipeline!(D, "sequential_d");

        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            A::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 5,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            B::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            C::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 15,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            D::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 20,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        // Create synthetic ingestion data
        let temp_dir = tempfile::tempdir().unwrap();
        synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
            ingestion_dir: temp_dir.path().to_owned(),
            starting_checkpoint: 5,
            num_checkpoints: 25,
            checkpoint_size: 1,
        })
        .await;

        let indexer_args = IndexerArgs {
            first_checkpoint: Some(3),
            last_checkpoint: Some(29),
            ..Default::default()
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

        indexer
            .concurrent_pipeline::<A>(A, ConcurrentConfig::default())
            .await
            .unwrap();
        indexer
            .concurrent_pipeline::<B>(B, ConcurrentConfig::default())
            .await
            .unwrap();
        indexer
            .sequential_pipeline::<C>(C, SequentialConfig::default())
            .await
            .unwrap();
        indexer
            .sequential_pipeline::<D>(D, SequentialConfig::default())
            .await
            .unwrap();

        let ingestion_metrics = indexer.ingestion_metrics().clone();
        let metrics = indexer.indexer_metrics().clone();
        indexer.run().await.unwrap().await.unwrap();

        assert_eq!(ingestion_metrics.total_ingested_checkpoints.get(), 24);
        assert_eq!(
            metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&[A::NAME])
                .unwrap()
                .get(),
            0
        );
        assert_eq!(
            metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&[B::NAME])
                .unwrap()
                .get(),
            5
        );
        assert_eq!(
            metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&[C::NAME])
                .unwrap()
                .get(),
            10
        );
        assert_eq!(
            metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&[D::NAME])
                .unwrap()
                .get(),
            15
        );
    }

    // test ingestion, some pipelines missing watermarks, no first_checkpoint provided
    #[tokio::test]
    async fn test_indexer_ingestion_missing_watermarks_no_first_checkpoint() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");
        test_pipeline!(D, "sequential_d");

        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            B::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            C::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 15,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            D::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 20,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        // Create synthetic ingestion data
        let temp_dir = tempfile::tempdir().unwrap();
        synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
            ingestion_dir: temp_dir.path().to_owned(),
            starting_checkpoint: 0,
            num_checkpoints: 30,
            checkpoint_size: 1,
        })
        .await;

        let indexer_args = IndexerArgs {
            last_checkpoint: Some(29),
            ..Default::default()
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

        indexer
            .concurrent_pipeline::<A>(A, ConcurrentConfig::default())
            .await
            .unwrap();
        indexer
            .concurrent_pipeline::<B>(B, ConcurrentConfig::default())
            .await
            .unwrap();
        indexer
            .sequential_pipeline::<C>(C, SequentialConfig::default())
            .await
            .unwrap();
        indexer
            .sequential_pipeline::<D>(D, SequentialConfig::default())
            .await
            .unwrap();

        let ingestion_metrics = indexer.ingestion_metrics().clone();
        let metrics = indexer.indexer_metrics().clone();
        indexer.run().await.unwrap().await.unwrap();

        assert_eq!(ingestion_metrics.total_ingested_checkpoints.get(), 30);
        assert_eq!(
            metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&[A::NAME])
                .unwrap()
                .get(),
            0
        );
        assert_eq!(
            metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&[B::NAME])
                .unwrap()
                .get(),
            11
        );
        assert_eq!(
            metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&[C::NAME])
                .unwrap()
                .get(),
            16
        );
        assert_eq!(
            metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&[D::NAME])
                .unwrap()
                .get(),
            21
        );
    }

    // test ingestion, some pipelines missing watermarks, use first_checkpoint
    #[tokio::test]
    async fn test_indexer_ingestion_use_first_checkpoint() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");
        test_pipeline!(D, "sequential_d");

        let mut conn = store.connect().await.unwrap();
        conn.set_committer_watermark(
            B::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            C::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 15,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_committer_watermark(
            D::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 20,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        // Create synthetic ingestion data
        let temp_dir = tempfile::tempdir().unwrap();
        synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
            ingestion_dir: temp_dir.path().to_owned(),
            starting_checkpoint: 5,
            num_checkpoints: 25,
            checkpoint_size: 1,
        })
        .await;

        let indexer_args = IndexerArgs {
            first_checkpoint: Some(10),
            last_checkpoint: Some(29),
            ..Default::default()
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

        indexer
            .concurrent_pipeline::<A>(A, ConcurrentConfig::default())
            .await
            .unwrap();
        indexer
            .concurrent_pipeline::<B>(B, ConcurrentConfig::default())
            .await
            .unwrap();
        indexer
            .sequential_pipeline::<C>(C, SequentialConfig::default())
            .await
            .unwrap();
        indexer
            .sequential_pipeline::<D>(D, SequentialConfig::default())
            .await
            .unwrap();

        let ingestion_metrics = indexer.ingestion_metrics().clone();
        let metrics = indexer.indexer_metrics().clone();
        indexer.run().await.unwrap().await.unwrap();

        assert_eq!(ingestion_metrics.total_ingested_checkpoints.get(), 20);
        assert_eq!(
            metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&[A::NAME])
                .unwrap()
                .get(),
            0
        );
        assert_eq!(
            metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&[B::NAME])
                .unwrap()
                .get(),
            1
        );
        assert_eq!(
            metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&[C::NAME])
                .unwrap()
                .get(),
            6
        );
        assert_eq!(
            metrics
                .total_watermarks_out_of_order
                .get_metric_with_label_values(&[D::NAME])
                .unwrap()
                .get(),
            11
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
            task: TaskArgs::tasked("task".to_string(), 10),
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
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(temp_dir.path().to_owned()),
                ..Default::default()
            },
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
                ConcurrentConfig::default(),
            )
            .await;

        let ingestion_metrics = tasked_indexer.ingestion_metrics().clone();
        let metrics = tasked_indexer.indexer_metrics().clone();

        tasked_indexer.run().await.unwrap().await.unwrap();

        assert_eq!(ingestion_metrics.total_ingested_checkpoints.get(), 16);
        assert_eq!(
            metrics
                .total_collector_skipped_checkpoints
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
            task: TaskArgs::tasked("task".to_string(), 10),
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
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(temp_dir.path().to_owned()),
                ..Default::default()
            },
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
                ConcurrentConfig::default(),
            )
            .await;

        let ingestion_metrics = tasked_indexer.ingestion_metrics().clone();
        let metrics = tasked_indexer.indexer_metrics().clone();

        tasked_indexer.run().await.unwrap().await.unwrap();

        assert_eq!(ingestion_metrics.total_ingested_checkpoints.get(), 17);
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
                .total_collector_skipped_checkpoints
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

    /// During a run, the tasked pipeline will stop sending checkpoints below the main pipeline's
    /// reader watermark to the committer. Committer watermark should still advance.
    #[tokio::test]
    async fn test_tasked_pipelines_skip_checkpoints_trailing_main_reader_lo() {
        let cancel = CancellationToken::new();
        let registry = Registry::new();
        let store = MockStore::default();

        let mut conn = store.connect().await.unwrap();

        // Start a tasked indexer that will ingest from genesis to checkpoint 500.
        let indexer_args = IndexerArgs {
            first_checkpoint: Some(0),
            last_checkpoint: Some(500),
            pipeline: vec![],
            task: TaskArgs::tasked("task".to_string(), 10 /* reader_interval_ms */),
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
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(temp_dir.path().to_owned()),
                ..Default::default()
            },
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

        let (controllable_handler, process_below) = ControllableHandler::with_limit(10);

        let _ = tasked_indexer
            .concurrent_pipeline(
                controllable_handler,
                ConcurrentConfig {
                    committer: CommitterConfig {
                        collect_interval_ms: 10,
                        watermark_interval_ms: 10,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .await;

        let ingestion_metrics = tasked_indexer.ingestion_metrics().clone();
        let metrics = tasked_indexer.indexer_metrics().clone();

        let indexer_handle = tokio::spawn(async move {
            tasked_indexer.run().await.unwrap().await.unwrap();
        });

        store
            .wait_for_watermark(
                &pipeline_task::<MockStore>(ControllableHandler::NAME, Some("task")).unwrap(),
                10,
                std::time::Duration::from_secs(10),
            )
            .await;

        // Bump the reader watermark to checkpoint 250. The tasked pipeline should only commit
        // checkpoints >= 250 once it ticks.
        conn.set_committer_watermark(
            ControllableHandler::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 300,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_reader_watermark(ControllableHandler::NAME, 250)
            .await
            .unwrap();

        // Sleep so that the new reader watermark can be picked up by the tasked indexer. Given the
        // `reader_interval_ms` is 10 ms, 1000 ms should be plenty of time.
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        // Allow the processor to resume.
        process_below.send(501).ok();

        indexer_handle.await.unwrap();

        let data = store.data.get(ControllableHandler::NAME).unwrap();
        // All 500+1 checkpoints should have been ingested.
        assert_eq!(ingestion_metrics.total_ingested_checkpoints.get(), 501);
        // Checkpoints 11 to 249 should have been skipped.
        assert_eq!(
            metrics
                .total_collector_skipped_checkpoints
                .get_metric_with_label_values(&[ControllableHandler::NAME])
                .unwrap()
                .get(),
            239
        );

        let ge_250 = data.iter().filter(|e| *e.key() >= 250).count();
        let lt_250 = data.iter().filter(|e| *e.key() < 250).count();
        // Checkpoints 250 to 500 inclusive must have been committed.
        assert_eq!(ge_250, 251);
        // Lenient check that not all checkpoints < 250 were committed.
        assert!(lt_250 < 250);
        assert_eq!(
            conn.committer_watermark(
                &pipeline_task::<MockStore>(ControllableHandler::NAME, Some("task")).unwrap()
            )
            .await
            .unwrap()
            .unwrap()
            .checkpoint_hi_inclusive,
            500
        );
    }
}
