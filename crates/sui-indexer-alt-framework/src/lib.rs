// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::ops::Bound;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Context;
use anyhow::bail;
use anyhow::ensure;
use cohort::CohortSlot;
use cohort::MergeContext;
use cohort::cohorts;
use ingestion::ArcStreamingClient;
use ingestion::ClientArgs;
use ingestion::IngestionConfig;
use ingestion::IngestionFactory;
use ingestion::IngestionService;
use ingestion::ingestion_client::IngestionClient;
use metrics::IndexerMetrics;
use prometheus::Registry;
use sui_indexer_alt_framework_store_traits::ConcurrentStore;
use sui_indexer_alt_framework_store_traits::Connection;
use sui_indexer_alt_framework_store_traits::SequentialStore;
use sui_indexer_alt_framework_store_traits::Store;
use sui_indexer_alt_framework_store_traits::pipeline_task;
use tracing::info;

use crate::metrics::IngestionMetrics;
use crate::pipeline::Processor;
use crate::pipeline::concurrent::ConcurrentConfig;
use crate::pipeline::concurrent::{self};
use crate::pipeline::sequential::SequentialConfig;
use crate::pipeline::sequential::{self};
use crate::service::Service;

pub use sui_field_count::FieldCount;
pub use sui_futures::service;
/// External users access the store trait through framework::store
pub use sui_indexer_alt_framework_store_traits as store;
pub use sui_types as types;

#[cfg(feature = "cluster")]
pub mod cluster;
mod cohort;
pub mod config;
pub mod ingestion;
pub mod metrics;
pub mod pipeline;
#[cfg(feature = "postgres")]
pub mod postgres;

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
    #[arg(long, requires = "reader_interval_ms")]
    task: Option<String>,

    /// The interval in milliseconds at which each of the pipelines on a tasked indexer should
    /// refetch its main pipeline's reader watermark.
    ///
    /// This is required when `--task` is set and should should ideally be set to a value that is
    /// an order of magnitude smaller than the main pipeline's pruning interval, to ensure this
    /// task pipeline can pick up the new reader watermark before the main pipeline prunes up to
    /// it.
    ///
    /// If the main pipeline does not have pruning enabled, this value can be set to some high
    /// value, as the tasked pipeline will never see an updated reader watermark.
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

    /// Creates the services that download and disseminate checkpoint data -- one per cohort of
    /// pipelines with similar distances from the network tip, determined in [Self::run].
    ingestion_factory: IngestionFactory,

    /// Optional override of the checkpoint lowerbound. When set, pipelines without a committer
    /// watermark will start processing at this checkpoint.
    first_checkpoint: Option<u64>,

    /// Optional override of the checkpoint upperbound. When set, the indexer will stop ingestion at
    /// this checkpoint.
    last_checkpoint: Option<u64>,

    /// The network's latest checkpoint, when the indexer was started.
    latest_checkpoint: u64,

    /// The minimum `next_checkpoint` across all pipelines. This is the checkpoint for the indexer
    /// to start ingesting from.
    next_checkpoint: u64,

    /// The minimum `next_checkpoint` across all sequential pipelines. This is used to initialize
    /// the regulator to prevent ingestion from running too far ahead of sequential pipelines.
    next_sequential_checkpoint: Option<u64>,

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

    /// Registered pipelines, waiting to be grouped into ingestion cohorts and started when the
    /// indexer is run.
    pipelines: Vec<PendingPipeline>,
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

/// A pipeline that has been registered but whose tasks have not been started yet. Pipelines are
/// held in this form until [Indexer::run], when they are grouped into ingestion cohorts by how
/// far behind the network tip they will resume from.
struct PendingPipeline {
    /// The pipeline's name, for logging cohort composition.
    name: &'static str,

    /// The checkpoint this pipeline will resume processing from.
    next_checkpoint: u64,

    /// Deferred constructor, invoked in [Indexer::run] once the pipeline's cohort has an
    /// ingestion service to subscribe to.
    build: PipelineBuilder,
}

/// Subscribes a pipeline to its cohort's ingestion service and starts the pipeline's tasks,
/// returning the service handle over those tasks.
///
/// `Sync` is required (in addition to `Send`) because the `Indexer` holding these builders is kept
/// alive across await points, and the simulator test runtime requires the resulting futures to be
/// `Sync`. The builder closures only capture `Send + Sync` state (the handler, store, config, and
/// metrics), so this bound is satisfied.
type PipelineBuilder = Box<dyn FnOnce(&mut IngestionService) -> Service + Send + Sync>;

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
    ) -> anyhow::Result<Self> {
        let ingestion_factory =
            IngestionFactory::new(client_args, ingestion_config, metrics_prefix, registry)?;
        Self::with_ingestion_factory(
            store,
            indexer_args,
            ingestion_factory,
            metrics_prefix,
            registry,
        )
        .await
    }

    /// Variant of [`Self::new`] that accepts pre-built ingestion clients, bypassing
    /// [`ClientArgs`]-driven construction. Callers that supply their own
    /// [`IngestionClientTrait`] / [`CheckpointStreamingClient`] implementations — for example,
    /// when embedding the indexer in a fullnode that already has checkpoint data on hand — hand
    /// them in here, and the indexer creates its ingestion services from them.
    ///
    /// All the indexer's ingestion services clone `ingestion_client` and report to its metrics
    /// handle, and each gets its own clone of the streaming client (if any).
    ///
    /// [`IngestionClientTrait`]: crate::ingestion::ingestion_client::IngestionClientTrait
    /// [`CheckpointStreamingClient`]: crate::ingestion::streaming_client::CheckpointStreamingClient
    pub async fn with_ingestion_clients(
        store: S,
        indexer_args: IndexerArgs,
        ingestion_client: IngestionClient,
        streaming_client: Option<ArcStreamingClient>,
        ingestion_config: IngestionConfig,
        metrics_prefix: Option<&str>,
        registry: &Registry,
    ) -> anyhow::Result<Self> {
        let ingestion_factory =
            IngestionFactory::with_clients(ingestion_client, streaming_client, ingestion_config);
        Self::with_ingestion_factory(
            store,
            indexer_args,
            ingestion_factory,
            metrics_prefix,
            registry,
        )
        .await
    }

    /// Common assembly point for [`Self::new`] and [`Self::with_ingestion_clients`]: probes the
    /// network tip through the factory and stamps the fields onto the struct.
    async fn with_ingestion_factory(
        store: S,
        indexer_args: IndexerArgs,
        ingestion_factory: IngestionFactory,
        metrics_prefix: Option<&str>,
        registry: &Registry,
    ) -> anyhow::Result<Self> {
        let IndexerArgs {
            first_checkpoint,
            last_checkpoint,
            pipeline,
            task,
        } = indexer_args;

        let metrics = IndexerMetrics::new(metrics_prefix, registry);

        let latest_checkpoint = ingestion_factory.latest_checkpoint_number().await?;

        info!(latest_checkpoint);

        Ok(Self {
            store,
            metrics,
            ingestion_factory,
            first_checkpoint,
            last_checkpoint,
            latest_checkpoint,
            next_checkpoint: u64::MAX,
            next_sequential_checkpoint: None,
            task: task.into_task(),
            enabled_pipelines: if pipeline.is_empty() {
                None
            } else {
                Some(pipeline.into_iter().collect())
            },
            added_pipelines: BTreeSet::new(),
            pipelines: vec![],
        })
    }

    /// The store used by the indexer.
    pub fn store(&self) -> &S {
        &self.store
    }

    /// The ingestion client used by the indexer to fetch checkpoints.
    pub fn ingestion_client(&self) -> &IngestionClient {
        self.ingestion_factory.ingestion_client()
    }

    /// The indexer's metrics.
    pub fn indexer_metrics(&self) -> &Arc<IndexerMetrics> {
        &self.metrics
    }

    /// The ingestion metrics shared by all of this indexer's ingestion services.
    pub fn ingestion_metrics(&self) -> &Arc<IngestionMetrics> {
        self.ingestion_factory.metrics()
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

    /// Group the registered pipelines into ingestion cohorts by how far behind the network tip
    /// they will resume from, and start one ingestion service per cohort. Each cohort ingests
    /// from the smallest `next_checkpoint` among its members, so pipelines near the tip are not
    /// held back (through channel backpressure) by pipelines that have a long backfill ahead of
    /// them.
    ///
    /// Cohorts merge back together as they converge: once a trailing cohort's ingestion frontier
    /// comes within `cohort_merge_threshold` checkpoints of the cohort ahead of it, its
    /// pipelines are handed off to that cohort's ingestion service (exactly once -- no gaps, no
    /// duplicates) and its own service winds down, so a fully caught-up indexer ends up back on
    /// a single ingestion service.
    ///
    /// Ingestion will stop after consuming the configured `last_checkpoint` if one is provided.
    /// Note that a pipeline dropping its checkpoint subscription winds down its own cohort's
    /// ingestion service (including pipelines merged into that cohort); other cohorts keep
    /// running.
    pub async fn run(self) -> anyhow::Result<Service> {
        let Self {
            ingestion_factory,
            last_checkpoint,
            latest_checkpoint,
            enabled_pipelines,
            pipelines,
            ..
        } = self;

        if let Some(enabled_pipelines) = enabled_pipelines {
            ensure!(
                enabled_pipelines.is_empty(),
                "Tried to enable pipelines that this indexer does not know about: \
                {enabled_pipelines:#?}",
            );
        }

        ensure!(!pipelines.is_empty(), "No pipelines registered to run");

        let groups = cohorts(
            pipelines,
            latest_checkpoint,
            ingestion_factory.config().min_cohort_boundary,
        );

        info!(
            cohorts = groups.len(),
            latest_checkpoint, "Grouped pipelines into ingestion cohorts"
        );

        let end = last_checkpoint.map_or(Bound::Unbounded, Bound::Included);

        // The table through which cohorts coordinate merges, with one slot per cohort.
        let threshold = ingestion_factory.config().cohort_merge_threshold;
        let table = (groups.len() > 1).then(|| {
            Arc::new(Mutex::new(
                groups
                    .iter()
                    .map(|_| CohortSlot::default())
                    .collect::<Vec<_>>(),
            ))
        });

        let mut service = Service::new();
        for (cohort, group) in groups.into_iter().enumerate() {
            let mut ingestion = ingestion_factory.create(cohort);

            let mut start = u64::MAX;
            let mut names = Vec::with_capacity(group.len());
            let mut members = Vec::with_capacity(group.len());
            for pending in group {
                start = start.min(pending.next_checkpoint);
                names.push(pending.name);
                members.push((pending.build)(&mut ingestion));
            }

            info!(cohort, start, end = last_checkpoint, pipelines = ?names, "Ingestion range");

            if let Some(table) = &table {
                ingestion.set_merge_context(MergeContext {
                    table: table.clone(),
                    cohort,
                    threshold,
                });
            }

            let mut cohort_service = ingestion
                .run((Bound::Included(start), end))
                .await
                .context("Failed to start ingestion service")?;

            for member in members {
                cohort_service = cohort_service.merge(member);
            }

            service = service.merge(cohort_service);
        }

        Ok(service)
    }

    /// Determine the checkpoint for the pipeline to resume processing from. This is either the
    /// checkpoint after its watermark, or if that doesn't exist, then the provided
    /// [Self::first_checkpoint], and if that is not set, then 0 (genesis).
    ///
    /// Update the starting ingestion checkpoint as the minimum across all the next checkpoints
    /// calculated above.
    ///
    /// Returns `Ok(None)` if the pipeline is disabled.
    async fn add_pipeline<P: Processor + 'static>(
        &mut self,
        pipeline_task: String,
        retention: Option<u64>,
    ) -> anyhow::Result<Option<u64>> {
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

        // Create a new record based on `proposed_next_checkpoint` if one does not exist.
        // Otherwise, use the existing record and disregard the proposed value.
        let proposed_next_checkpoint = if let Some(first_checkpoint) = self.first_checkpoint {
            first_checkpoint
        } else if let Some(retention) = retention {
            self.latest_checkpoint.saturating_sub(retention)
        } else {
            0
        };
        let mut conn = self.store.connect().await?;
        let init_watermark = conn
            .init_watermark(&pipeline_task, proposed_next_checkpoint.checked_sub(1))
            .await
            .with_context(|| format!("Failed to init watermark for {pipeline_task}"))?;

        let next_checkpoint = if let Some(init_watermark) = init_watermark {
            if let Some(checkpoint_hi_inclusive) = init_watermark.checkpoint_hi_inclusive {
                checkpoint_hi_inclusive + 1
            } else {
                0
            }
        } else {
            proposed_next_checkpoint
        };

        self.next_checkpoint = next_checkpoint.min(self.next_checkpoint);

        Ok(Some(next_checkpoint))
    }
}

impl<S: ConcurrentStore> Indexer<S> {
    /// Adds a new pipeline to this indexer. Its tasks are started when the indexer is run, and
    /// will be idle until its cohort's ingestion service serves it checkpoint data.
    ///
    /// Concurrent pipelines commit checkpoint data out-of-order to maximise throughput, and they
    /// keep the watermark table up-to-date with the highest point they can guarantee all data
    /// exists for, for their pipeline.
    pub async fn concurrent_pipeline<H: concurrent::Handler<Store = S>>(
        &mut self,
        handler: H,
        config: ConcurrentConfig,
    ) -> anyhow::Result<()> {
        let pipeline_task =
            pipeline_task::<S>(H::NAME, self.task.as_ref().map(|t| t.task.as_str()))?;
        let retention = config.pruner.as_ref().map(|p| p.retention);
        let Some(next_checkpoint) = self.add_pipeline::<H>(pipeline_task, retention).await? else {
            return Ok(());
        };

        let store = self.store.clone();
        let task = self.task.clone();
        let metrics = self.metrics.clone();
        self.pipelines.push(PendingPipeline {
            name: H::NAME,
            next_checkpoint,
            build: Box::new(move |ingestion| {
                let checkpoint_rx = ingestion
                    .subscribe_bounded(config.ingestion.subscriber_channel_size(), next_checkpoint);
                concurrent::pipeline::<H>(
                    handler,
                    next_checkpoint,
                    config,
                    store,
                    task,
                    checkpoint_rx,
                    metrics,
                )
            }),
        });

        Ok(())
    }
}

impl<T: SequentialStore> Indexer<T> {
    /// Adds a new pipeline to this indexer. Its tasks are started when the indexer is run, and
    /// will be idle until its cohort's ingestion service serves it checkpoint data.
    ///
    /// Sequential pipelines commit checkpoint data in-order which sacrifices throughput, but may be
    /// required to handle pipelines that modify data in-place (where each update is not an insert,
    /// but could be a modification of an existing row, where ordering between updates is
    /// important).
    pub async fn sequential_pipeline<H: sequential::Handler<Store = T>>(
        &mut self,
        handler: H,
        config: SequentialConfig,
    ) -> anyhow::Result<()> {
        if self.task.is_some() {
            bail!(
                "Sequential pipelines do not support pipeline tasks. \
                These pipelines guarantee that each checkpoint is committed exactly once and in order. \
                Running the same pipeline under a different task would violate these guarantees."
            );
        }

        let Some(next_checkpoint) = self.add_pipeline::<H>(H::NAME.to_owned(), None).await? else {
            return Ok(());
        };

        // Track the minimum next_checkpoint across all sequential pipelines
        self.next_sequential_checkpoint = Some(
            self.next_sequential_checkpoint
                .map_or(next_checkpoint, |n| n.min(next_checkpoint)),
        );

        let store = self.store.clone();
        let metrics = self.metrics.clone();
        self.pipelines.push(PendingPipeline {
            name: H::NAME,
            next_checkpoint,
            build: Box::new(move |ingestion| {
                let checkpoint_rx = ingestion
                    .subscribe_bounded(config.ingestion.subscriber_channel_size(), next_checkpoint);
                sequential::pipeline::<H>(
                    handler,
                    next_checkpoint,
                    config,
                    store,
                    checkpoint_rx,
                    metrics,
                )
            }),
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use clap::Parser;
    use sui_indexer_alt_framework_store_traits::PrunerWatermark;
    use sui_synthetic_ingestion::synthetic_ingestion;
    use tokio::sync::watch;

    use crate::FieldCount;
    use crate::config::ConcurrencyConfig;
    use crate::ingestion::ingestion_client::IngestionClientArgs;
    use crate::ingestion::store_client::ObjectStoreWatermark;
    use crate::ingestion::store_client::WATERMARK_PATH;
    use crate::mocks::store::FallibleMockStore;
    use crate::pipeline::CommitterConfig;
    use crate::pipeline::Processor;
    use crate::pipeline::concurrent::ConcurrentConfig;
    use crate::store::CommitterWatermark;
    use crate::store::ConcurrentConnection as _;
    use crate::store::Connection as _;

    use super::*;

    #[allow(dead_code)]
    #[derive(Clone, FieldCount)]
    struct MockValue(u64);

    /// A handler that can be controlled externally to block checkpoint processing: it only
    /// processes checkpoints at or below a limit adjusted through the returned `watch` sender.
    macro_rules! controllable_handler {
        ($handler:ident, $name:literal) => {
            struct $handler {
                /// Process checkpoints less than or equal to this value.
                process_below: watch::Receiver<u64>,
            }

            impl $handler {
                fn with_limit(limit: u64) -> (Self, watch::Sender<u64>) {
                    let (tx, rx) = watch::channel(limit);
                    (Self { process_below: rx }, tx)
                }
            }

            #[async_trait]
            impl Processor for $handler {
                const NAME: &'static str = $name;
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
            impl concurrent::Handler for $handler {
                type Store = FallibleMockStore;
                type Batch = Vec<MockValue>;

                fn batch(
                    &self,
                    batch: &mut Self::Batch,
                    values: &mut std::vec::IntoIter<Self::Value>,
                ) -> concurrent::BatchStatus {
                    batch.extend(values);
                    concurrent::BatchStatus::Ready
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
        };
    }

    controllable_handler!(ControllableHandler, "controllable");

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
                type Store = FallibleMockStore;
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
                type Store = FallibleMockStore;
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

    fn init_ingestion_dir(latest_checkpoint: Option<u64>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        if let Some(cp) = latest_checkpoint {
            let watermark_path = dir.path().join(WATERMARK_PATH);
            std::fs::create_dir_all(watermark_path.parent().unwrap()).unwrap();
            let watermark = ObjectStoreWatermark {
                checkpoint_hi_inclusive: cp,
            };
            std::fs::write(watermark_path, serde_json::to_string(&watermark).unwrap()).unwrap();
        }
        dir
    }

    /// If `ingestion_data` is `Some((num_checkpoints, checkpoint_size))`, synthetic ingestion
    /// data will be generated in the temp directory before creating the indexer.
    async fn create_test_indexer(
        store: FallibleMockStore,
        indexer_args: IndexerArgs,
        registry: &Registry,
        ingestion_data: Option<(u64, u64)>,
    ) -> (Indexer<FallibleMockStore>, tempfile::TempDir) {
        let temp_dir = init_ingestion_dir(None);
        if let Some((num_checkpoints, checkpoint_size)) = ingestion_data {
            synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
                ingestion_dir: temp_dir.path().to_owned(),
                starting_checkpoint: 0,
                num_checkpoints,
                checkpoint_size,
            })
            .await;
        }
        let client_args = ClientArgs {
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(temp_dir.path().to_owned()),
                ..Default::default()
            },
            ..Default::default()
        };
        let indexer = Indexer::new(
            store,
            indexer_args,
            client_args,
            IngestionConfig::default(),
            None,
            registry,
        )
        .await
        .unwrap();
        (indexer, temp_dir)
    }

    async fn set_committer_watermark(
        conn: &mut <FallibleMockStore as Store>::Connection<'_>,
        name: &str,
        hi: u64,
    ) {
        conn.set_committer_watermark(
            name,
            CommitterWatermark {
                checkpoint_hi_inclusive: hi,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }

    async fn add_concurrent<H: concurrent::Handler<Store = FallibleMockStore>>(
        indexer: &mut Indexer<FallibleMockStore>,
        handler: H,
    ) {
        indexer
            .concurrent_pipeline(handler, ConcurrentConfig::default())
            .await
            .unwrap();
    }

    async fn add_sequential<H: sequential::Handler<Store = FallibleMockStore>>(
        indexer: &mut Indexer<FallibleMockStore>,
        handler: H,
    ) {
        indexer
            .sequential_pipeline(handler, SequentialConfig::default())
            .await
            .unwrap();
    }

    /// Seed `store` so `near` resumes just past a fake network tip of 100,000 (its distance
    /// from the tip saturates to zero) while `far` resumes 99,990 checkpoints behind it, and
    /// return an ingestion directory advertising that tip with checkpoints 0..60 on disk.
    /// Cohorts are determined by distance from the advertised tip, but ingestion stops at
    /// `last_checkpoint`. Checkpoint 0 must exist because the ingestion client derives the
    /// chain identifier from it, and retries indefinitely until it appears.
    async fn multi_cohort_setup(
        store: &FallibleMockStore,
        near: &str,
        far: &str,
    ) -> tempfile::TempDir {
        let mut conn = store.connect().await.unwrap();
        set_committer_watermark(&mut conn, near, 100_100).await;
        set_committer_watermark(&mut conn, far, 9).await;

        let temp_dir = init_ingestion_dir(Some(100_000));
        synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
            ingestion_dir: temp_dir.path().to_owned(),
            starting_checkpoint: 0,
            num_checkpoints: 60,
            checkpoint_size: 1,
        })
        .await;

        temp_dir
    }

    macro_rules! assert_out_of_order {
        ($metrics:expr, $pipeline:expr, $expected:expr) => {
            assert_eq!(
                $metrics
                    .total_watermarks_out_of_order
                    .get_metric_with_label_values(&[$pipeline])
                    .unwrap()
                    .get(),
                $expected,
            );
        };
    }

    async fn test_init_watermark(
        first_checkpoint: Option<u64>,
        is_concurrent: bool,
    ) -> (Option<CommitterWatermark>, Option<PrunerWatermark>) {
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        test_pipeline!(A, "pipeline_name");

        let mut conn = store.connect().await.unwrap();

        let indexer_args = IndexerArgs {
            first_checkpoint,
            ..IndexerArgs::default()
        };
        let (mut indexer, _temp_dir) =
            create_test_indexer(store.clone(), indexer_args, &registry, None).await;

        if is_concurrent {
            add_concurrent(&mut indexer, A).await;
        } else {
            add_sequential(&mut indexer, A).await;
        }

        (
            conn.committer_watermark(A::NAME).await.unwrap(),
            conn.pruner_watermark(A::NAME, Duration::ZERO)
                .await
                .unwrap(),
        )
    }

    const LATEST_CHECKPOINT: u64 = 10;

    /// Set up an indexer as if the network is at `latest_checkpoint` (the next checkpoint to
    /// ingest). Runs a single concurrent pipeline with the given config. If `watermark` is
    /// provided, the pipeline's high watermark is pre-set; `first_checkpoint` controls where
    /// ingestion begins.
    async fn test_next_checkpoint(
        watermark: Option<u64>,
        first_checkpoint: Option<u64>,
        concurrent_config: ConcurrentConfig,
    ) -> Indexer<FallibleMockStore> {
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        test_pipeline!(A, "concurrent_a");

        if let Some(checkpoint_hi_inclusive) = watermark {
            let mut conn = store.connect().await.unwrap();
            conn.set_committer_watermark(
                A::NAME,
                CommitterWatermark {
                    checkpoint_hi_inclusive,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        }

        let temp_dir = init_ingestion_dir(Some(LATEST_CHECKPOINT));
        let mut indexer = Indexer::new(
            store,
            IndexerArgs {
                first_checkpoint,
                ..Default::default()
            },
            ClientArgs {
                ingestion: IngestionClientArgs {
                    local_ingestion_path: Some(temp_dir.path().to_owned()),
                    ..Default::default()
                },
                ..Default::default()
            },
            IngestionConfig::default(),
            None,
            &registry,
        )
        .await
        .unwrap();

        assert_eq!(indexer.latest_checkpoint, LATEST_CHECKPOINT);

        indexer
            .concurrent_pipeline::<A>(A, concurrent_config)
            .await
            .unwrap();

        indexer
    }

    fn pruner_config(retention: u64) -> ConcurrentConfig {
        ConcurrentConfig {
            pruner: Some(concurrent::PrunerConfig {
                retention,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn test_arg_parsing() {
        #[derive(Parser)]
        struct Args {
            #[clap(flatten)]
            indexer: IndexerArgs,
        }

        let args = Args::try_parse_from([
            "cmd",
            "--first-checkpoint",
            "10",
            "--last-checkpoint",
            "100",
            "--pipeline",
            "a",
            "--pipeline",
            "b",
            "--task",
            "t",
            "--reader-interval-ms",
            "5000",
        ])
        .unwrap();

        assert_eq!(args.indexer.first_checkpoint, Some(10));
        assert_eq!(args.indexer.last_checkpoint, Some(100));
        assert_eq!(args.indexer.pipeline, vec!["a", "b"]);
        assert_eq!(args.indexer.task.task, Some("t".to_owned()));
        assert_eq!(args.indexer.task.reader_interval_ms, Some(5000));
    }

    /// next_checkpoint is smallest among existing watermarks + 1.
    #[tokio::test]
    async fn test_next_checkpoint_all_pipelines_have_watermarks() {
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");
        test_pipeline!(D, "sequential_d");

        let mut conn = store.connect().await.unwrap();

        conn.init_watermark(A::NAME, Some(0)).await.unwrap();
        set_committer_watermark(&mut conn, A::NAME, 100).await;

        conn.init_watermark(B::NAME, Some(0)).await.unwrap();
        set_committer_watermark(&mut conn, B::NAME, 10).await;

        conn.init_watermark(C::NAME, Some(0)).await.unwrap();
        set_committer_watermark(&mut conn, C::NAME, 1).await;

        conn.init_watermark(D::NAME, Some(0)).await.unwrap();
        set_committer_watermark(&mut conn, D::NAME, 50).await;

        let (mut indexer, _temp_dir) =
            create_test_indexer(store, IndexerArgs::default(), &registry, None).await;

        add_concurrent(&mut indexer, A).await;
        add_concurrent(&mut indexer, B).await;
        add_sequential(&mut indexer, C).await;
        add_sequential(&mut indexer, D).await;

        assert_eq!(indexer.first_checkpoint, None);
        assert_eq!(indexer.last_checkpoint, None);
        assert_eq!(indexer.latest_checkpoint, 0);
        assert_eq!(indexer.next_checkpoint, 2);
        assert_eq!(indexer.next_sequential_checkpoint, Some(2));
    }

    /// next_checkpoint is 0 when at least one pipeline has no watermark.
    #[tokio::test]
    async fn test_next_checkpoint_not_all_pipelines_have_watermarks() {
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");
        test_pipeline!(D, "sequential_d");

        let mut conn = store.connect().await.unwrap();
        set_committer_watermark(&mut conn, B::NAME, 10).await;
        set_committer_watermark(&mut conn, C::NAME, 1).await;

        let (mut indexer, _temp_dir) =
            create_test_indexer(store, IndexerArgs::default(), &registry, None).await;

        add_concurrent(&mut indexer, A).await;
        add_concurrent(&mut indexer, B).await;
        add_sequential(&mut indexer, C).await;
        add_sequential(&mut indexer, D).await;

        assert_eq!(indexer.first_checkpoint, None);
        assert_eq!(indexer.last_checkpoint, None);
        assert_eq!(indexer.latest_checkpoint, 0);
        assert_eq!(indexer.next_checkpoint, 0);
        assert_eq!(indexer.next_sequential_checkpoint, Some(0));
    }

    /// next_checkpoint is 1 when smallest committer watermark is 0.
    #[tokio::test]
    async fn test_next_checkpoint_smallest_is_0() {
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");
        test_pipeline!(D, "sequential_d");

        let mut conn = store.connect().await.unwrap();
        set_committer_watermark(&mut conn, A::NAME, 100).await;
        set_committer_watermark(&mut conn, B::NAME, 10).await;
        set_committer_watermark(&mut conn, C::NAME, 1).await;
        set_committer_watermark(&mut conn, D::NAME, 0).await;

        let (mut indexer, _temp_dir) =
            create_test_indexer(store, IndexerArgs::default(), &registry, None).await;

        add_concurrent(&mut indexer, A).await;
        add_concurrent(&mut indexer, B).await;
        add_sequential(&mut indexer, C).await;
        add_sequential(&mut indexer, D).await;

        assert_eq!(indexer.next_checkpoint, 1);
    }

    /// next_checkpoint is first_checkpoint when at least one pipeline has no
    /// watermark, and first_checkpoint is smallest.
    #[tokio::test]
    async fn test_next_checkpoint_first_checkpoint_and_no_watermark() {
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");
        test_pipeline!(D, "sequential_d");

        let mut conn = store.connect().await.unwrap();
        set_committer_watermark(&mut conn, B::NAME, 50).await;
        set_committer_watermark(&mut conn, C::NAME, 10).await;

        let indexer_args = IndexerArgs {
            first_checkpoint: Some(5),
            ..Default::default()
        };
        let (mut indexer, _temp_dir) =
            create_test_indexer(store, indexer_args, &registry, None).await;

        add_concurrent(&mut indexer, A).await;
        add_concurrent(&mut indexer, B).await;
        add_sequential(&mut indexer, C).await;
        add_sequential(&mut indexer, D).await;

        assert_eq!(indexer.first_checkpoint, Some(5));
        assert_eq!(indexer.last_checkpoint, None);
        assert_eq!(indexer.latest_checkpoint, 0);
        assert_eq!(indexer.next_checkpoint, 5);
        assert_eq!(indexer.next_sequential_checkpoint, Some(5));
    }

    /// next_checkpoint is smallest among existing watermarks + 1 if
    /// all pipelines have watermarks (ignores first_checkpoint).
    #[tokio::test]
    async fn test_next_checkpoint_ignore_first_checkpoint() {
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");

        let mut conn = store.connect().await.unwrap();
        set_committer_watermark(&mut conn, B::NAME, 50).await;
        set_committer_watermark(&mut conn, C::NAME, 10).await;

        let indexer_args = IndexerArgs {
            first_checkpoint: Some(5),
            ..Default::default()
        };
        let (mut indexer, _temp_dir) =
            create_test_indexer(store, indexer_args, &registry, None).await;

        add_concurrent(&mut indexer, B).await;
        add_sequential(&mut indexer, C).await;

        assert_eq!(indexer.first_checkpoint, Some(5));
        assert_eq!(indexer.last_checkpoint, None);
        assert_eq!(indexer.latest_checkpoint, 0);
        assert_eq!(indexer.next_checkpoint, 11);
        assert_eq!(indexer.next_sequential_checkpoint, Some(11));
    }

    /// If the first_checkpoint is being considered, because pipelines are missing watermarks, it
    /// will not be used as the starting point if it is not the smallest valid committer watermark
    /// to resume ingesting from.
    #[tokio::test]
    async fn test_next_checkpoint_large_first_checkpoint() {
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");

        let mut conn = store.connect().await.unwrap();
        set_committer_watermark(&mut conn, B::NAME, 50).await;
        set_committer_watermark(&mut conn, C::NAME, 10).await;

        let indexer_args = IndexerArgs {
            first_checkpoint: Some(24),
            ..Default::default()
        };
        let (mut indexer, _temp_dir) =
            create_test_indexer(store, indexer_args, &registry, None).await;

        add_concurrent(&mut indexer, A).await;
        add_concurrent(&mut indexer, B).await;
        add_sequential(&mut indexer, C).await;

        assert_eq!(indexer.first_checkpoint, Some(24));
        assert_eq!(indexer.last_checkpoint, None);
        assert_eq!(indexer.latest_checkpoint, 0);
        assert_eq!(indexer.next_checkpoint, 11);
        assert_eq!(indexer.next_sequential_checkpoint, Some(11));
    }

    /// latest_checkpoint is read from the watermark file.
    #[tokio::test]
    async fn test_latest_checkpoint_from_watermark() {
        let registry = Registry::new();
        let store = FallibleMockStore::default();
        let temp_dir = init_ingestion_dir(Some(30));
        let indexer = Indexer::new(
            store,
            IndexerArgs::default(),
            ClientArgs {
                ingestion: IngestionClientArgs {
                    local_ingestion_path: Some(temp_dir.path().to_owned()),
                    ..Default::default()
                },
                ..Default::default()
            },
            IngestionConfig::default(),
            None,
            &registry,
        )
        .await
        .unwrap();

        assert_eq!(indexer.latest_checkpoint, 30);
    }

    /// No watermark, no first_checkpoint, pruner with retention:
    /// next_checkpoint = LATEST_CHECKPOINT - retention.
    #[tokio::test]
    async fn test_next_checkpoint_with_pruner_uses_retention() {
        let retention = LATEST_CHECKPOINT - 1;
        let indexer = test_next_checkpoint(None, None, pruner_config(retention)).await;
        assert_eq!(indexer.next_checkpoint, LATEST_CHECKPOINT - retention);
    }

    /// No watermark, no first_checkpoint, no pruner: falls back to 0.
    #[tokio::test]
    async fn test_next_checkpoint_without_pruner_falls_back_to_genesis() {
        let indexer = test_next_checkpoint(None, None, ConcurrentConfig::default()).await;
        assert_eq!(indexer.next_checkpoint, 0);
    }

    /// Watermark at 5 takes priority over latest_checkpoint - retention.
    #[tokio::test]
    async fn test_next_checkpoint_watermark_takes_priority_over_pruner() {
        let retention = LATEST_CHECKPOINT - 1;
        let indexer = test_next_checkpoint(Some(5), None, pruner_config(retention)).await;
        assert_eq!(indexer.next_checkpoint, 6);
    }

    /// first_checkpoint takes priority over latest_checkpoint - retention when
    /// there's no watermark.
    #[tokio::test]
    async fn test_next_checkpoint_first_checkpoint_takes_priority_over_pruner() {
        let retention = LATEST_CHECKPOINT - 1;
        let indexer = test_next_checkpoint(None, Some(2), pruner_config(retention)).await;
        assert_eq!(indexer.next_checkpoint, 2);
    }

    /// When retention exceeds latest_checkpoint, saturating_sub clamps to 0.
    #[tokio::test]
    async fn test_next_checkpoint_retention_exceeds_latest_checkpoint() {
        let retention = LATEST_CHECKPOINT + 1;
        let indexer = test_next_checkpoint(None, None, pruner_config(retention)).await;
        assert_eq!(indexer.next_checkpoint, 0);
    }

    // test ingestion, all pipelines have watermarks, no first_checkpoint provided
    #[tokio::test]
    async fn test_indexer_ingestion_existing_watermarks_no_first_checkpoint() {
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");
        test_pipeline!(D, "sequential_d");

        let mut conn = store.connect().await.unwrap();
        set_committer_watermark(&mut conn, A::NAME, 5).await;
        set_committer_watermark(&mut conn, B::NAME, 10).await;
        set_committer_watermark(&mut conn, C::NAME, 15).await;
        set_committer_watermark(&mut conn, D::NAME, 20).await;

        let indexer_args = IndexerArgs {
            last_checkpoint: Some(29),
            ..Default::default()
        };
        let (mut indexer, _temp_dir) =
            create_test_indexer(store.clone(), indexer_args, &registry, Some((30, 1))).await;

        add_concurrent(&mut indexer, A).await;
        add_concurrent(&mut indexer, B).await;
        add_sequential(&mut indexer, C).await;
        add_sequential(&mut indexer, D).await;

        let ingestion_metrics = indexer.ingestion_metrics().clone();
        let indexer_metrics = indexer.indexer_metrics().clone();

        indexer.run().await.unwrap().join().await.unwrap();

        assert_eq!(
            ingestion_metrics
                .total_ingested_checkpoints
                .with_label_values(&["0"])
                .get(),
            24
        );
        // Pipelines never receive checkpoints below their own watermark, even though the
        // cohort's ingestion starts at the minimum watermark across members.
        assert_out_of_order!(indexer_metrics, A::NAME, 0);
        assert_out_of_order!(indexer_metrics, B::NAME, 0);
        assert_out_of_order!(indexer_metrics, C::NAME, 0);
        assert_out_of_order!(indexer_metrics, D::NAME, 0);
    }

    // test ingestion, no pipelines missing watermarks, first_checkpoint provided
    #[tokio::test]
    async fn test_indexer_ingestion_existing_watermarks_ignore_first_checkpoint() {
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");
        test_pipeline!(D, "sequential_d");

        let mut conn = store.connect().await.unwrap();
        set_committer_watermark(&mut conn, A::NAME, 5).await;
        set_committer_watermark(&mut conn, B::NAME, 10).await;
        set_committer_watermark(&mut conn, C::NAME, 15).await;
        set_committer_watermark(&mut conn, D::NAME, 20).await;

        let indexer_args = IndexerArgs {
            first_checkpoint: Some(3),
            last_checkpoint: Some(29),
            ..Default::default()
        };
        let (mut indexer, _temp_dir) =
            create_test_indexer(store.clone(), indexer_args, &registry, Some((30, 1))).await;

        add_concurrent(&mut indexer, A).await;
        add_concurrent(&mut indexer, B).await;
        add_sequential(&mut indexer, C).await;
        add_sequential(&mut indexer, D).await;

        let ingestion_metrics = indexer.ingestion_metrics().clone();
        let metrics = indexer.indexer_metrics().clone();
        indexer.run().await.unwrap().join().await.unwrap();

        assert_eq!(
            ingestion_metrics
                .total_ingested_checkpoints
                .with_label_values(&["0"])
                .get(),
            24
        );
        assert_out_of_order!(metrics, A::NAME, 0);
        assert_out_of_order!(metrics, B::NAME, 0);
        assert_out_of_order!(metrics, C::NAME, 0);
        assert_out_of_order!(metrics, D::NAME, 0);
    }

    // test ingestion, some pipelines missing watermarks, no first_checkpoint provided
    #[tokio::test]
    async fn test_indexer_ingestion_missing_watermarks_no_first_checkpoint() {
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");
        test_pipeline!(D, "sequential_d");

        let mut conn = store.connect().await.unwrap();
        set_committer_watermark(&mut conn, B::NAME, 10).await;
        set_committer_watermark(&mut conn, C::NAME, 15).await;
        set_committer_watermark(&mut conn, D::NAME, 20).await;

        let indexer_args = IndexerArgs {
            last_checkpoint: Some(29),
            ..Default::default()
        };
        let (mut indexer, _temp_dir) =
            create_test_indexer(store.clone(), indexer_args, &registry, Some((30, 1))).await;

        add_concurrent(&mut indexer, A).await;
        add_concurrent(&mut indexer, B).await;
        add_sequential(&mut indexer, C).await;
        add_sequential(&mut indexer, D).await;

        let ingestion_metrics = indexer.ingestion_metrics().clone();
        let metrics = indexer.indexer_metrics().clone();
        indexer.run().await.unwrap().join().await.unwrap();

        assert_eq!(
            ingestion_metrics
                .total_ingested_checkpoints
                .with_label_values(&["0"])
                .get(),
            30
        );
        assert_out_of_order!(metrics, A::NAME, 0);
        assert_out_of_order!(metrics, B::NAME, 0);
        assert_out_of_order!(metrics, C::NAME, 0);
        assert_out_of_order!(metrics, D::NAME, 0);
    }

    // test ingestion, some pipelines missing watermarks, use first_checkpoint
    #[tokio::test]
    async fn test_indexer_ingestion_use_first_checkpoint() {
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");
        test_pipeline!(C, "sequential_c");
        test_pipeline!(D, "sequential_d");

        let mut conn = store.connect().await.unwrap();
        set_committer_watermark(&mut conn, B::NAME, 10).await;
        set_committer_watermark(&mut conn, C::NAME, 15).await;
        set_committer_watermark(&mut conn, D::NAME, 20).await;

        let indexer_args = IndexerArgs {
            first_checkpoint: Some(10),
            last_checkpoint: Some(29),
            ..Default::default()
        };
        let (mut indexer, _temp_dir) =
            create_test_indexer(store.clone(), indexer_args, &registry, Some((30, 1))).await;

        add_concurrent(&mut indexer, A).await;
        add_concurrent(&mut indexer, B).await;
        add_sequential(&mut indexer, C).await;
        add_sequential(&mut indexer, D).await;

        let ingestion_metrics = indexer.ingestion_metrics().clone();
        let metrics = indexer.indexer_metrics().clone();
        indexer.run().await.unwrap().join().await.unwrap();

        assert_eq!(
            ingestion_metrics
                .total_ingested_checkpoints
                .with_label_values(&["0"])
                .get(),
            20
        );
        assert_out_of_order!(metrics, A::NAME, 0);
        assert_out_of_order!(metrics, B::NAME, 0);
        assert_out_of_order!(metrics, C::NAME, 0);
        assert_out_of_order!(metrics, D::NAME, 0);
    }

    /// Pipelines that share a cohort don't receive checkpoints below their own watermark, even
    /// though the cohort's ingestion range starts at the minimum watermark across its members.
    #[tokio::test]
    async fn test_shared_cohort_pipeline_never_reprocesses_committed_checkpoints() {
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        test_pipeline!(A, "concurrent_a");
        test_pipeline!(B, "concurrent_b");

        let mut conn = store.connect().await.unwrap();
        set_committer_watermark(&mut conn, A::NAME, 5).await;
        set_committer_watermark(&mut conn, B::NAME, 20).await;

        let indexer_args = IndexerArgs {
            last_checkpoint: Some(29),
            ..Default::default()
        };
        let (mut indexer, _temp_dir) =
            create_test_indexer(store.clone(), indexer_args, &registry, Some((30, 1))).await;

        add_concurrent(&mut indexer, A).await;
        add_concurrent(&mut indexer, B).await;

        let ingestion_metrics = indexer.ingestion_metrics().clone();
        let indexer_metrics = indexer.indexer_metrics().clone();

        indexer.run().await.unwrap().join().await.unwrap();

        // Each checkpoint in 6..=29 is fetched exactly once for the cohort.
        assert_eq!(
            ingestion_metrics
                .total_ingested_checkpoints
                .with_label_values(&["0"])
                .get(),
            24
        );

        // B was only served the checkpoints past its own watermark (21..=29).
        assert_eq!(
            indexer_metrics
                .total_handler_checkpoints_received
                .get_metric_with_label_values(&[B::NAME])
                .unwrap()
                .get(),
            9
        );
        assert_out_of_order!(indexer_metrics, A::NAME, 0);
        assert_out_of_order!(indexer_metrics, B::NAME, 0);

        // B never re-committed checkpoints at or below its watermark.
        let committed = store.data.get(B::NAME).unwrap();
        for cp in 6..21 {
            assert!(
                !committed.contains_key(&cp),
                "B re-committed checkpoint {cp}"
            );
        }
        for cp in 21..30 {
            assert!(
                committed.contains_key(&cp),
                "B did not commit checkpoint {cp}"
            );
        }
    }

    #[tokio::test]
    async fn test_init_watermark_concurrent_no_first_checkpoint() {
        let (committer_watermark, pruner_watermark) = test_init_watermark(None, true).await;
        assert_eq!(committer_watermark, None);
        assert_eq!(pruner_watermark, None);
    }

    #[tokio::test]
    async fn test_init_watermark_concurrent_first_checkpoint_0() {
        let (committer_watermark, pruner_watermark) = test_init_watermark(Some(0), true).await;
        assert_eq!(committer_watermark, None);
        assert_eq!(pruner_watermark, None);
    }

    #[tokio::test]
    async fn test_init_watermark_concurrent_first_checkpoint_1() {
        let (committer_watermark, pruner_watermark) = test_init_watermark(Some(1), true).await;

        let committer_watermark = committer_watermark.unwrap();
        assert_eq!(committer_watermark.checkpoint_hi_inclusive, 0);

        let pruner_watermark = pruner_watermark.unwrap();
        assert_eq!(pruner_watermark.reader_lo, 1);
        assert_eq!(pruner_watermark.pruner_hi, 1);
    }

    #[tokio::test]
    async fn test_init_watermark_sequential() {
        let (committer_watermark, pruner_watermark) = test_init_watermark(Some(1), false).await;

        let committer_watermark = committer_watermark.unwrap();
        assert_eq!(committer_watermark.checkpoint_hi_inclusive, 0);

        let pruner_watermark = pruner_watermark.unwrap();
        assert_eq!(pruner_watermark.reader_lo, 1);
        assert_eq!(pruner_watermark.pruner_hi, 1);
    }

    #[tokio::test]
    async fn test_multiple_sequential_pipelines_next_checkpoint() {
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        let mut conn = store.connect().await.unwrap();
        set_committer_watermark(&mut conn, MockHandler::NAME, 10).await;
        set_committer_watermark(&mut conn, SequentialHandler::NAME, 5).await;

        let indexer_args = IndexerArgs {
            first_checkpoint: None,
            last_checkpoint: Some(19),
            pipeline: vec![],
            ..Default::default()
        };
        let (mut indexer, _temp_dir) =
            create_test_indexer(store.clone(), indexer_args, &registry, Some((20, 2))).await;

        // Add first sequential pipeline
        add_sequential(&mut indexer, MockHandler).await;

        // Verify next_sequential_checkpoint is set correctly (10 + 1 = 11)
        assert_eq!(
            indexer.next_sequential_checkpoint(),
            Some(11),
            "next_sequential_checkpoint should be 11"
        );

        // Add second sequential pipeline
        add_sequential(&mut indexer, SequentialHandler).await;

        // Should change to 6 (minimum of 6 and 11)
        assert_eq!(
            indexer.next_sequential_checkpoint(),
            Some(6),
            "next_sequential_checkpoint should still be 6"
        );

        // Run indexer to verify it can make progress past the initial hi and finish ingesting.
        indexer.run().await.unwrap().join().await.unwrap();

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
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        // Mock the store as if we have a main pipeline with a committer watermark at `10` and a
        // reader watermark at `7`.
        let mut conn = store.connect().await.unwrap();
        set_committer_watermark(&mut conn, MockCheckpointSequenceNumberHandler::NAME, 10).await;
        conn.set_reader_watermark(MockCheckpointSequenceNumberHandler::NAME, 7)
            .await
            .unwrap();

        // Start a tasked indexer that will ingest from checkpoint 0. Checkpoints 0 through 6 should
        // be ignored by the tasked indexer.
        let indexer_args = IndexerArgs {
            first_checkpoint: Some(0),
            last_checkpoint: Some(15),
            task: TaskArgs::tasked("task".to_string(), 10),
            ..Default::default()
        };
        let (mut tasked_indexer, _temp_dir) =
            create_test_indexer(store.clone(), indexer_args, &registry, Some((16, 2))).await;

        add_concurrent(&mut tasked_indexer, MockCheckpointSequenceNumberHandler).await;

        let ingestion_metrics = tasked_indexer.ingestion_metrics().clone();
        let metrics = tasked_indexer.indexer_metrics().clone();

        tasked_indexer.run().await.unwrap().join().await.unwrap();

        assert_eq!(
            ingestion_metrics
                .total_ingested_checkpoints
                .with_label_values(&["0"])
                .get(),
            16
        );
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
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        let mut conn = store.connect().await.unwrap();
        set_committer_watermark(&mut conn, "test", 10).await;
        conn.set_reader_watermark("test", 5).await.unwrap();

        // Start a tasked indexer that will ingest from checkpoint 9 and go past the main pipeline's
        // watermarks.
        let indexer_args = IndexerArgs {
            first_checkpoint: Some(9),
            last_checkpoint: Some(25),
            task: TaskArgs::tasked("task".to_string(), 10),
            ..Default::default()
        };
        let (mut tasked_indexer, _temp_dir) =
            create_test_indexer(store.clone(), indexer_args, &registry, Some((26, 2))).await;

        add_concurrent(&mut tasked_indexer, MockCheckpointSequenceNumberHandler).await;

        let ingestion_metrics = tasked_indexer.ingestion_metrics().clone();
        let metrics = tasked_indexer.indexer_metrics().clone();

        tasked_indexer.run().await.unwrap().join().await.unwrap();

        assert_eq!(
            ingestion_metrics
                .total_ingested_checkpoints
                .with_label_values(&["0"])
                .get(),
            17
        );
        assert_out_of_order!(metrics, "test", 0);
        assert_eq!(
            metrics
                .total_collector_skipped_checkpoints
                .get_metric_with_label_values(&[MockCheckpointSequenceNumberHandler::NAME])
                .unwrap()
                .get(),
            0
        );

        let data = store.data.get("test").unwrap();
        assert_eq!(data.len(), 17);
        for i in 0..9 {
            assert!(data.get(&i).is_none());
        }
        for i in 9..26 {
            assert!(data.get(&i).is_some());
        }
        let main_pipeline_watermark = store.watermark("test").unwrap();
        // assert that the main pipeline's watermarks are not updated
        assert_eq!(main_pipeline_watermark.checkpoint_hi_inclusive, Some(10));
        assert_eq!(main_pipeline_watermark.reader_lo, 5);
        let tasked_pipeline_watermark = store.watermark("test@task").unwrap();
        assert_eq!(tasked_pipeline_watermark.checkpoint_hi_inclusive, Some(25));
        assert_eq!(tasked_pipeline_watermark.reader_lo, 9);
    }

    /// Test that when the collector observes `reader_lo = X`, that all checkpoints >= X will be
    /// committed, and any checkpoints inflight < X will be skipped.
    #[tokio::test]
    async fn test_tasked_pipelines_skip_checkpoints_trailing_main_reader_lo() {
        let registry = Registry::new();
        let store = FallibleMockStore::default();
        let mut conn = store.connect().await.unwrap();
        // Set the main pipeline watermark.
        set_committer_watermark(&mut conn, ControllableHandler::NAME, 11).await;

        // Generate 500 checkpoints upfront, for the indexer to process all at once.
        let indexer_args = IndexerArgs {
            first_checkpoint: Some(0),
            last_checkpoint: Some(500),
            task: TaskArgs::tasked("task".to_string(), 10 /* reader_interval_ms */),
            ..Default::default()
        };
        let (mut tasked_indexer, _temp_dir) =
            create_test_indexer(store.clone(), indexer_args, &registry, Some((501, 2))).await;
        let mut allow_process = 10;
        // Limit the pipeline to process only checkpoints `[0, 10]`.
        let (controllable_handler, process_below) = ControllableHandler::with_limit(allow_process);
        let _ = tasked_indexer
            .concurrent_pipeline(
                controllable_handler,
                ConcurrentConfig {
                    committer: CommitterConfig {
                        collect_interval_ms: 10,
                        watermark_interval_ms: 10,
                        ..Default::default()
                    },
                    // High fixed concurrency so all checkpoints can be processed
                    // concurrently despite out-of-order arrival.
                    fanout: Some(ConcurrencyConfig::Fixed { value: 501 }),
                    ..Default::default()
                },
            )
            .await;
        let metrics = tasked_indexer.indexer_metrics().clone();

        let mut s_indexer = tasked_indexer.run().await.unwrap();

        // Wait for pipeline to commit up to configured checkpoint 10 inclusive. With the main
        // pipeline `reader_lo` currently unset, all checkpoints are allowed and should be
        // committed.
        store
            .wait_for_watermark(
                &pipeline_task::<FallibleMockStore>(ControllableHandler::NAME, Some("task"))
                    .unwrap(),
                10,
                Duration::from_secs(10),
            )
            .await;

        // Set the reader_lo to 250, simulating the main pipeline getting ahead. The
        // track_main_reader_lo task will eventually pick this up and update the atomic. The
        // collector reads from the atomic when it receives checkpoints, so we release checkpoints
        // one at a time until the collector_reader_lo metric shows the new value.
        conn.set_reader_watermark(ControllableHandler::NAME, 250)
            .await
            .unwrap();

        let reader_lo = metrics
            .collector_reader_lo
            .with_label_values(&[ControllableHandler::NAME]);

        // Send checkpoints one at a time at 10ms intervals. The tasked indexer has a reader refresh
        // interval of 10ms as well, so the collector should pick up the new reader_lo after a few
        // checkpoints have been processed.
        let mut interval = tokio::time::interval(Duration::from_millis(10));
        while reader_lo.get() != 250 {
            interval.tick().await;
            // allow_process is initialized to 11, bump to 11 for the next checkpoint
            allow_process += 1;
            assert!(
                allow_process <= 500,
                "Released all checkpoints but collector never observed new reader_lo"
            );
            process_below.send(allow_process).ok();
        }

        // At this point, the collector has observed reader_lo = 250. Release all remaining
        // checkpoints. Guarantees:
        // - [0, 10]: committed (before reader_lo was set)
        // - [11, allow_process]: some committed, some skipped (timing-dependent during detection)
        // - (allow_process, 250): skipped (in-flight, filtered by collector)
        // - [250, 500]: committed (>= reader_lo)
        process_below.send(500).ok();

        s_indexer.join().await.unwrap();

        let data = store.data.get(ControllableHandler::NAME).unwrap();

        // Checkpoints (allow_process, 250) must be skipped.
        for chkpt in (allow_process + 1)..250 {
            assert!(
                data.get(&chkpt).is_none(),
                "Checkpoint {chkpt} should have been skipped"
            );
        }

        // Checkpoints >= reader_lo must be committed.
        for chkpt in 250..=500 {
            assert!(
                data.get(&chkpt).is_some(),
                "Checkpoint {chkpt} should have been committed (>= reader_lo)"
            );
        }

        // Baseline: checkpoints [0, 10] were committed before reader_lo was set.
        for chkpt in 0..=10 {
            assert!(
                data.get(&chkpt).is_some(),
                "Checkpoint {chkpt} should have been committed (baseline)"
            );
        }
    }

    /// Pipelines whose distances from the network tip are far apart are split into separate
    /// ingestion cohorts, each served by its own ingestion service, and the indexer completes
    /// even though the cohorts' ingestion services finish at different times.
    #[tokio::test]
    async fn test_multi_cohort_ingestion() {
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        test_pipeline!(A, "near_tip");
        test_pipeline!(B, "far_behind");

        let temp_dir = multi_cohort_setup(&store, A::NAME, B::NAME).await;

        let mut indexer = Indexer::new(
            store.clone(),
            IndexerArgs {
                last_checkpoint: Some(59),
                ..Default::default()
            },
            ClientArgs {
                ingestion: IngestionClientArgs {
                    local_ingestion_path: Some(temp_dir.path().to_owned()),
                    ..Default::default()
                },
                ..Default::default()
            },
            IngestionConfig::default(),
            None,
            &registry,
        )
        .await
        .unwrap();

        add_concurrent(&mut indexer, A).await;
        add_concurrent(&mut indexer, B).await;

        let ingestion_metrics = indexer.ingestion_metrics().clone();
        let indexer_metrics = indexer.indexer_metrics().clone();
        indexer.run().await.unwrap().join().await.unwrap();

        // Both cohorts' services report to the shared metrics, each under its own cohort label.
        // Only the lagging cohort (1) had anything to ingest; the near-tip cohort (0)'s range
        // [100_101, 59] is empty.
        assert_eq!(
            ingestion_metrics
                .total_ingested_checkpoints
                .with_label_values(&["0"])
                .get(),
            0
        );
        assert_eq!(
            ingestion_metrics
                .total_ingested_checkpoints
                .with_label_values(&["1"])
                .get(),
            50
        );

        // The near-tip pipeline received no checkpoints at all, proving it was not subscribed
        // to the lagging cohort's backfill (unsplit, it would have received all 50).
        let received = |name| {
            indexer_metrics
                .total_handler_checkpoints_received
                .get_metric_with_label_values(&[name])
                .unwrap()
                .get()
        };
        assert_eq!(received(A::NAME), 0);
        assert_eq!(received(B::NAME), 50);

        // The lagging pipeline committed everything up to `last_checkpoint`, while the near-tip
        // pipeline had nothing to process.
        let watermark = store.watermark(B::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(59));
        let watermark = store.watermark(A::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(100_100));
    }

    /// The client-driven constructor groups pipelines into cohorts too, with all cohorts
    /// sharing the supplied ingestion client and its metrics handle.
    #[tokio::test]
    async fn test_multi_cohort_ingestion_with_clients() {
        let registry = Registry::new();
        let store = FallibleMockStore::default();

        test_pipeline!(A, "near_tip");
        test_pipeline!(B, "far_behind");

        let temp_dir = multi_cohort_setup(&store, A::NAME, B::NAME).await;

        let ingestion_metrics = IngestionMetrics::new(None, &registry);
        let ingestion_client = IngestionClient::new(
            IngestionClientArgs {
                local_ingestion_path: Some(temp_dir.path().to_owned()),
                ..Default::default()
            },
            ingestion_metrics.clone(),
        )
        .unwrap();

        let mut indexer = Indexer::with_ingestion_clients(
            store.clone(),
            IndexerArgs {
                last_checkpoint: Some(59),
                ..Default::default()
            },
            ingestion_client,
            None,
            IngestionConfig::default(),
            None,
            &registry,
        )
        .await
        .unwrap();

        add_concurrent(&mut indexer, A).await;
        add_concurrent(&mut indexer, B).await;

        let indexer_metrics = indexer.indexer_metrics().clone();
        indexer.run().await.unwrap().join().await.unwrap();

        // Both cohorts' services report to the one shared metrics handle, each under its own
        // cohort label, and only the lagging cohort (1) ingested checkpoints.
        assert_eq!(
            ingestion_metrics
                .total_ingested_checkpoints
                .with_label_values(&["0"])
                .get(),
            0
        );
        assert_eq!(
            ingestion_metrics
                .total_ingested_checkpoints
                .with_label_values(&["1"])
                .get(),
            50
        );
        assert_eq!(
            indexer_metrics
                .total_handler_checkpoints_received
                .get_metric_with_label_values(&[A::NAME])
                .unwrap()
                .get(),
            0
        );

        let watermark = store.watermark(B::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(59));
    }

    /// Hard isolation: a pipeline at the tip runs to completion even while a far-behind pipeline in
    /// a separate cohort is wedged and backpressuring its own ingestion service. Before cohorts,
    /// both pipelines shared one ingestion service, so the far-behind pipeline's full subscriber
    /// channel would have throttled the shared broadcaster and held the tip pipeline back too.
    #[tokio::test]
    async fn test_near_tip_cohort_progresses_while_far_behind_stalls() {
        const TIP: u64 = 100;

        let registry = Registry::new();
        let store = FallibleMockStore::default();

        test_pipeline!(A, "near_tip");

        // The far-behind pipeline's handler blocks every checkpoint in its range (all > 9), so its
        // cohort's ingestion service fills its subscriber channel and backpressures to a halt. Keep
        // the release sender alive for the whole test -- dropping it would unblock the handler.
        let (far_behind, _release) = ControllableHandler::with_limit(9);

        let temp_dir = init_ingestion_dir(Some(TIP));
        let mut conn = store.connect().await.unwrap();
        set_committer_watermark(&mut conn, A::NAME, 94).await; // near tip: resumes at 95
        set_committer_watermark(&mut conn, ControllableHandler::NAME, 9).await; // far behind: resumes at 10
        synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
            ingestion_dir: temp_dir.path().to_owned(),
            starting_checkpoint: 0,
            num_checkpoints: TIP + 1, // checkpoints [0, TIP]
            checkpoint_size: 1,
        })
        .await;

        let mut indexer = Indexer::new(
            store.clone(),
            IndexerArgs {
                last_checkpoint: Some(TIP),
                ..Default::default()
            },
            ClientArgs {
                ingestion: IngestionClientArgs {
                    local_ingestion_path: Some(temp_dir.path().to_owned()),
                    ..Default::default()
                },
                ..Default::default()
            },
            // A small cohort boundary splits the near-tip and far-behind pipelines into separate
            // cohorts without having to generate tens of thousands of checkpoints, and a zero
            // merge threshold keeps them separate for the whole test (the wedged far-behind
            // cohort's frontier never reaches the near-tip cohort's): at this scale the two
            // cohorts start within the default merge threshold of each other.
            IngestionConfig {
                min_cohort_boundary: 10,
                cohort_merge_threshold: 0,
                ..Default::default()
            },
            None,
            &registry,
        )
        .await
        .unwrap();

        // Near-tip pipeline: commit promptly so its watermark advances without waiting on the
        // default collector/watermark intervals.
        indexer
            .concurrent_pipeline(
                A,
                ConcurrentConfig {
                    committer: CommitterConfig {
                        collect_interval_ms: 10,
                        watermark_interval_ms: 10,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        // Far-behind pipeline: its handler blocks, so it never commits regardless of config.
        add_concurrent(&mut indexer, far_behind).await;

        let service = indexer.run().await.unwrap();

        // The near-tip cohort ingests [95, 100] and commits to completion even though the
        // far-behind cohort is wedged. Were the two not isolated, the far-behind pipeline's
        // backpressure would stall this too and the wait would time out.
        store
            .wait_for_watermark(A::NAME, TIP, Duration::from_secs(10))
            .await;

        // The far-behind pipeline made no progress: still at its initial watermark, nothing
        // committed -- it is stalled inside its own cohort, not holding the tip pipeline back.
        assert_eq!(
            store
                .watermark(ControllableHandler::NAME)
                .unwrap()
                .checkpoint_hi_inclusive,
            Some(9),
        );
        assert!(store.data.get(ControllableHandler::NAME).is_none());

        // The merged service never completes on its own while a cohort is wedged; drop aborts it.
        drop(service);
    }

    /// End-to-end cohort merging: a far-behind pipeline backfills, and once its ingestion
    /// frontier comes within the merge threshold of the near-tip cohort, its subscription is
    /// handed off to that cohort's ingestion service (exactly once -- no gaps, no duplicates)
    /// and its own service winds down; the far pipeline then follows new checkpoints through
    /// the near cohort's service.
    #[tokio::test]
    async fn test_cohorts_merge_as_they_converge() {
        const TIP: u64 = 100;
        const END: u64 = 160;

        let registry = Registry::new();
        let store = FallibleMockStore::default();

        test_pipeline!(B, "far_behind");

        // The near pipeline initially blocks every checkpoint above 99, so its cohort's
        // broadcaster backpressures to a halt mid-chunk with its published frontier pinned at
        // 95, giving the far cohort a stationary target to converge on. Keep the release
        // sender alive for the whole test.
        let (near, release) = ControllableHandler::with_limit(99);

        let temp_dir = init_ingestion_dir(Some(TIP));
        let mut conn = store.connect().await.unwrap();
        set_committer_watermark(&mut conn, ControllableHandler::NAME, 94).await; // resumes at 95
        set_committer_watermark(&mut conn, B::NAME, 9).await; // resumes at 10
        synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
            ingestion_dir: temp_dir.path().to_owned(),
            starting_checkpoint: 0,
            num_checkpoints: END + 1, // checkpoints [0, END]
            checkpoint_size: 1,
        })
        .await;

        let mut indexer = Indexer::new(
            store.clone(),
            // Unbounded: the merged service keeps following the tip.
            IndexerArgs::default(),
            ClientArgs {
                ingestion: IngestionClientArgs {
                    local_ingestion_path: Some(temp_dir.path().to_owned()),
                    ..Default::default()
                },
                ..Default::default()
            },
            // A small cohort boundary splits the pipelines into two cohorts (distances 5 and
            // 90 from the advertised tip), and the far cohort merges into the near one when it
            // gets within 20 checkpoints of it -- at its chunk boundary 90, against the near
            // cohort's pinned frontier of 95.
            IngestionConfig {
                min_cohort_boundary: 10,
                cohort_merge_threshold: 20,
                ..Default::default()
            },
            None,
            &registry,
        )
        .await
        .unwrap();

        // Commit promptly so watermarks advance without waiting on the default intervals, and
        // give the near pipeline a small subscriber channel so blocking its handler parks its
        // broadcaster after only a few buffered checkpoints.
        let committer = || CommitterConfig {
            collect_interval_ms: 10,
            watermark_interval_ms: 10,
            ..Default::default()
        };
        indexer
            .concurrent_pipeline(
                near,
                ConcurrentConfig {
                    committer: committer(),
                    ingestion: crate::pipeline::IngestionConfig {
                        subscriber_channel_size: Some(4),
                    },
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        indexer
            .concurrent_pipeline(
                B,
                ConcurrentConfig {
                    committer: committer(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        let ingestion_metrics = indexer.ingestion_metrics().clone();
        let indexer_metrics = indexer.indexer_metrics().clone();
        let service = indexer.run().await.unwrap();

        // The far cohort backfills to its chunk boundary at 90, finds the near cohort's
        // frontier (95) within the merge threshold, and merges into it.
        tokio::time::timeout(Duration::from_secs(10), async {
            while ingestion_metrics.total_cohort_merges.get() == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("timed out waiting for the cohorts to merge");

        // Unblock the near pipeline: the merged service absorbs the far pipeline's subscription
        // at the handoff checkpoint, and deliveries to both pipelines flow through to the end
        // of the data.
        release.send(u64::MAX).unwrap();

        store
            .wait_for_watermark(ControllableHandler::NAME, END, Duration::from_secs(10))
            .await;
        store
            .wait_for_watermark(B::NAME, END, Duration::from_secs(10))
            .await;

        // Exactly-once delivery across the handoff: each handler saw each checkpoint in its
        // range exactly once (a gap would have stalled its watermark above; a duplicate would
        // inflate its received count).
        let received = |name| {
            indexer_metrics
                .total_handler_checkpoints_received
                .get_metric_with_label_values(&[name])
                .unwrap()
                .get()
        };
        assert_eq!(received(ControllableHandler::NAME), END - 95 + 1);
        assert_eq!(received(B::NAME), END - 10 + 1);

        // The far cohort stopped ingesting at its handoff checkpoint -- the end of the near
        // cohort's parked chunk [95, 115) -- instead of following the data to the end itself.
        assert_eq!(ingestion_metrics.total_cohort_merges.get(), 1);
        let far_ingested = ingestion_metrics
            .total_ingested_checkpoints
            .with_label_values(&["1"])
            .get();
        assert_eq!(far_ingested, 115 - 10);

        // The merged service follows the tip indefinitely; drop aborts it.
        drop(service);
    }

    /// A subscription handed off once can be handed off again: the far cohort merges into the
    /// mid cohort, the mid cohort absorbs the handed-off subscription and later re-registers it
    /// (alongside its own) when it merges into the near cohort -- and every pipeline sees every
    /// checkpoint in its range exactly once across three ingestion services.
    #[tokio::test]
    async fn test_three_cohorts_cascade_merge() {
        const TIP: u64 = 100;
        const END: u64 = 160;

        let registry = Registry::new();
        let store = FallibleMockStore::default();

        controllable_handler!(Near, "cascade_near");
        controllable_handler!(Mid, "cascade_mid");
        controllable_handler!(Far, "cascade_far");

        // Every cohort parks at startup: near blocks above 99 (broadcaster parked in
        // [95, 115)), mid and far block everything from their resume points (parked in
        // [56, 76) and [10, 30)) until released. Keep the senders alive for the whole test.
        let (near, release_near) = Near::with_limit(99);
        let (mid, release_mid) = Mid::with_limit(55);
        let (far, release_far) = Far::with_limit(9);

        let temp_dir = init_ingestion_dir(Some(TIP));
        let mut conn = store.connect().await.unwrap();
        set_committer_watermark(&mut conn, Near::NAME, 94).await; // resumes at 95, distance 5
        set_committer_watermark(&mut conn, Mid::NAME, 55).await; // resumes at 56, distance 44
        set_committer_watermark(&mut conn, Far::NAME, 9).await; // resumes at 10, distance 90
        synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
            ingestion_dir: temp_dir.path().to_owned(),
            starting_checkpoint: 0,
            num_checkpoints: END + 1, // checkpoints [0, END]
            checkpoint_size: 1,
        })
        .await;

        let mut indexer = Indexer::new(
            store.clone(),
            // Unbounded: the surviving service keeps following the tip.
            IndexerArgs::default(),
            ClientArgs {
                ingestion: IngestionClientArgs {
                    local_ingestion_path: Some(temp_dir.path().to_owned()),
                    ..Default::default()
                },
                ..Default::default()
            },
            // Distances 5 / 44 / 90 form three cohorts (mid's boundary is 2 * 44 = 88 < 90),
            // each merging into the one ahead once within 20 checkpoints; chunk = 20.
            IngestionConfig {
                min_cohort_boundary: 10,
                cohort_merge_threshold: 20,
                ..Default::default()
            },
            None,
            &registry,
        )
        .await
        .unwrap();

        // Commit promptly, and use small subscriber channels so a blocked handler parks its
        // broadcaster after only a few buffered checkpoints.
        let config = || ConcurrentConfig {
            committer: CommitterConfig {
                collect_interval_ms: 10,
                watermark_interval_ms: 10,
                ..Default::default()
            },
            ingestion: crate::pipeline::IngestionConfig {
                subscriber_channel_size: Some(4),
            },
            ..Default::default()
        };
        indexer.concurrent_pipeline(near, config()).await.unwrap();
        indexer.concurrent_pipeline(mid, config()).await.unwrap();
        indexer.concurrent_pipeline(far, config()).await.unwrap();

        let ingestion_metrics = indexer.ingestion_metrics().clone();
        let indexer_metrics = indexer.indexer_metrics().clone();
        let service = indexer.run().await.unwrap();

        let ingested = |cohort: &str| {
            ingestion_metrics
                .total_ingested_checkpoints
                .with_label_values(&[cohort])
                .get()
        };
        async fn wait_until(what: &str, cond: impl Fn() -> bool) {
            tokio::time::timeout(Duration::from_secs(10), async {
                while !cond() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
        }

        // Both cohorts ahead must have committed their first (parked) chunks -- publishing
        // their frontiers and join points -- before the far cohort starts converging.
        wait_until(
            "the near and mid cohorts to publish their first chunks",
            || ingested("0") > 0 && ingested("1") > 0,
        )
        .await;

        // The far cohort backfills to its chunk boundary at 50, finds mid's frontier (56)
        // within the threshold, hands its subscription off at mid's join point (76), delivers
        // [10, 76), and winds down.
        release_far.send(u64::MAX).unwrap();
        wait_until("the far cohort to merge into the mid cohort", || {
            ingestion_metrics.total_cohort_merges.get() == 1
        })
        .await;

        // Mid completes [56, 76), absorbs far's subscription there, then finds near's frontier
        // (95) within the threshold and re-registers both subscriptions at near's join point
        // (115).
        release_mid.send(u64::MAX).unwrap();
        wait_until("the mid cohort to merge into the near cohort", || {
            ingestion_metrics.total_cohort_merges.get() == 2
        })
        .await;

        // Unblock near: it absorbs both subscriptions at its clean state (115) and carries all
        // three pipelines to the end of the data.
        release_near.send(u64::MAX).unwrap();
        for name in [Near::NAME, Mid::NAME, Far::NAME] {
            store
                .wait_for_watermark(name, END, Duration::from_secs(10))
                .await;
        }

        // Exactly-once across both handoffs: each pipeline saw each checkpoint in its range
        // exactly once (a gap would have stalled its watermark above; a duplicate would
        // inflate its received count).
        let received = |name| {
            indexer_metrics
                .total_handler_checkpoints_received
                .get_metric_with_label_values(&[name])
                .unwrap()
                .get()
        };
        assert_eq!(received(Near::NAME), END - 95 + 1);
        assert_eq!(received(Mid::NAME), END - 56 + 1);
        assert_eq!(received(Far::NAME), END - 10 + 1);

        // Each mergee stopped ingesting at its handoff: far at mid's join point (76), mid at
        // near's (115).
        assert_eq!(ingested("2"), 76 - 10);
        assert_eq!(ingested("1"), 115 - 56);

        // The surviving service follows the tip indefinitely; drop aborts it.
        drop(service);
    }
}
