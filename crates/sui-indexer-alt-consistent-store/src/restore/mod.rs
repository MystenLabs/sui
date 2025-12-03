// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};

use anyhow::{Context as _, ensure};
use prometheus::Registry;
use sui_indexer_alt_framework::service::Service;
use sui_indexer_alt_framework::{pipeline::Processor, types::object::Object};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::{
    db::{Db, config::DbConfig},
    handlers::balances::Balances,
    handlers::object_by_owner::ObjectByOwner,
    handlers::object_by_type::ObjectByType,
    store::Schema,
};

use self::{
    broadcaster::broadcaster,
    formal_snapshot::{FormalSnapshot, FormalSnapshotArgs},
    format::LiveObjects,
    metrics::RestorerMetrics,
    storage::StorageConnectionArgs,
    worker::worker,
};

mod broadcaster;
pub mod formal_snapshot;
mod format;
mod metrics;
pub mod storage;
mod worker;

/// Trait implemented by processors that support being restored from live objects in a formal
/// snapshot.
pub(crate) trait Restore<S: crate::store::Schema>: Processor {
    /// How much concurrency to use when processing live objects.
    const FANOUT: usize = 10;

    fn restore(schema: &S, object: &Object, batch: &mut rocksdb::WriteBatch) -> anyhow::Result<()>;
}

#[derive(clap::Args, Clone, Debug)]
pub struct RestoreArgs {
    /// Number of object files to download concurrently
    #[arg(long, default_value_t = Self::default().object_file_concurrency)]
    object_file_concurrency: usize,

    /// Maximum size of the backlog of object files waiting to be processed by one worker.
    #[arg(long, default_value_t = Self::default().object_file_buffer_size)]
    object_file_buffer_size: usize,
}

/// A service for restoring pipelines from live objects in a formal snapshot.
pub(crate) struct Restorer<S: Schema> {
    /// The RocksDB database to restore into.
    db: Arc<Db>,

    /// A schema over the database, to provide structured access to its contents.
    schema: Arc<S>,

    /// A source of live objects to restore from.
    snapshot: FormalSnapshot,

    /// Metrics related to the restoration process.
    metrics: Arc<RestorerMetrics>,

    /// Channels to send live object partitions down, one per pipeline being restored.
    restore_tx: BTreeMap<String, mpsc::Sender<Arc<LiveObjects>>>,

    /// Services spawned by the restorer, for individual pipelines.
    workers: Vec<Service>,

    /// Number of object files to download concurrenctly
    object_file_concurrency: usize,

    /// Maximum size of the backlog of object files waiting to be processed by one worker.
    object_file_buffer_size: usize,
}

pub struct Finalizer {
    db: Arc<Db>,
    pipelines: Vec<String>,
}

impl<S: Schema + Send + Sync + 'static> Restorer<S> {
    /// Create a new instance of the `Restorer`, configured to restore into the database at `path`.
    ///
    /// `formal_snapshot_args` describes where to load the formal snapshot from, `connection_args`
    /// controls how to connect to it, `restore_args` controls the restoration process itself, and
    /// `config` configures the RocksDB database.
    async fn new(
        path: impl AsRef<Path>,
        formal_snapshot_args: FormalSnapshotArgs,
        connection_args: StorageConnectionArgs,
        restore_args: RestoreArgs,
        config: DbConfig,
        metrics_prefix: Option<&str>,
        registry: &Registry,
    ) -> anyhow::Result<Self> {
        let RestoreArgs {
            object_file_concurrency,
            object_file_buffer_size,
        } = restore_args;

        let metrics = RestorerMetrics::new(metrics_prefix, registry);

        let snapshot = FormalSnapshot::new(formal_snapshot_args, connection_args)
            .await
            .context("Failed to connect to formal snapshot source")?;

        // This database will not be read from so we don't need to worry about snapshots.
        let snapshots = 0;
        let options: rocksdb::Options = config.into();
        let db = Db::open(path, options.clone(), snapshots, S::cfs(&options))
            .map(Arc::new)
            .context("Failed to open database")?;

        let schema = S::open(&db)
            .map(Arc::new)
            .context("Failed to open schema")?;

        Ok(Self {
            db,
            schema,
            snapshot,
            metrics,
            restore_tx: BTreeMap::new(),
            workers: vec![],
            object_file_concurrency,
            object_file_buffer_size,
        })
    }

    /// Register and start a new restoration pipeline implemented by `R`. Although their tasks have
    /// started, they will be idle until the restorer as a whole is run, and starts to fetch
    /// objects from the formal snapshot.
    fn restorer<R: Restore<S>>(&mut self) -> anyhow::Result<()> {
        let (tx, rx) = mpsc::channel(self.object_file_buffer_size);
        ensure!(
            self.restore_tx.insert(R::NAME.to_string(), tx).is_none(),
            "Pipeline {} already registered for restoration",
            R::NAME,
        );

        let watermark = self.snapshot.watermark();
        self.db.restore_at(R::NAME, watermark)?;

        self.workers.push(worker::<S, R>(
            rx,
            self.db.clone(),
            self.schema.clone(),
            self.metrics.clone(),
        ));

        Ok(())
    }

    /// Start restoring live objects across all registered pipelines. The service will run until it
    /// can confirm that every registered pipeline has been fully restored, at which point, it will
    /// clean up the restoration state and set the watermark for the restored pipeline.
    fn run(self) -> (Service, Finalizer) {
        // Remember the pipelines being restored for the clean-up process.
        let finalizer = Finalizer {
            db: self.db.clone(),
            pipelines: self.restore_tx.keys().cloned().collect(),
        };

        info!(pipelines = ?finalizer.pipelines, "Starting restoration");
        let mut service = broadcaster(
            self.object_file_concurrency,
            self.restore_tx,
            self.db,
            self.snapshot,
            self.metrics,
        );

        for worker in self.workers {
            service = service.merge(worker);
        }

        (service, finalizer)
    }
}

impl Finalizer {
    pub fn run(self) -> Service {
        Service::new().spawn_aborting(async move {
            // Clean up restoration state for each pipeline.
            for pipeline in self.pipelines {
                if let Err(e) = self.db.complete_restore(&pipeline) {
                    warn!(pipeline, "Failed to clear restoration state: {e:#}");
                } else {
                    info!(pipeline, "Restoration state cleared");
                }
            }

            Ok(())
        })
    }
}

impl Default for RestoreArgs {
    fn default() -> Self {
        Self {
            object_file_concurrency: 25,
            object_file_buffer_size: 25,
        }
    }
}

/// Set-up and run the Restorer, using the provided arguments (expected to be extracted from the
/// command-line). The service will run until restoration complete.
///
/// `path` is the path to the RocksDB database to restore into. It will be created if it does not
/// exist. `formal_snapshot_args` describes the formal snapshot source, `connection_args` controls
/// how to connect to it, `restore_args` controls the restoration process itself, and `config`
/// configures the RocksDB database.
pub async fn start_restorer(
    path: impl AsRef<Path>,
    formal_snapshot_args: FormalSnapshotArgs,
    connection_args: StorageConnectionArgs,
    restore_args: RestoreArgs,
    mut pipelines: BTreeSet<String>,
    config: DbConfig,
    registry: &Registry,
) -> anyhow::Result<(Service, Finalizer)> {
    let mut restorer: Restorer<crate::Schema> = Restorer::new(
        path,
        formal_snapshot_args,
        connection_args,
        restore_args,
        config,
        Some("restorer"),
        registry,
    )
    .await?;

    macro_rules! add_restorer {
        ($handler:ty) => {
            if pipelines.remove(<$handler as Processor>::NAME) {
                restorer.restorer::<$handler>()?;
            }
        };
    }

    add_restorer!(Balances);
    add_restorer!(ObjectByOwner);
    add_restorer!(ObjectByType);

    ensure!(
        pipelines.is_empty(),
        "Unknown pipelines to restore: {pipelines:?}"
    );

    Ok(restorer.run())
}
