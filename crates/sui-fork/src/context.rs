// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use prometheus::Registry;
use rand::rngs::OsRng;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::sync::broadcast;
use tracing::error;

use simulacrum::Simulacrum;
use sui_futures::service::Service;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::CreatedCheckpoint;
use crate::services::ServiceManager;
use crate::store::ForkStore;

type ForkedSimulacrum = Simulacrum<OsRng, ForkStore>;

/// Shared context for the forked network, holding the simulacrum and the service manager running
/// the embedded indexer.
pub struct Context {
    simulacrum: Arc<RwLock<ForkedSimulacrum>>,
    services: ServiceManager,
    checkpoint_publication_lock: Mutex<()>,
}

impl Context {
    /// Build a `Context` whose Simulacrum is backed by a started [`ServiceManager`], returning the
    /// embedded indexer as a [`Service`] the caller must keep alive (dropping it stops indexing).
    ///
    /// Starts the embedded `sui-rpc-store` indexer over `checkpoint_sender` before returning, so
    /// committed local checkpoints get indexed for RPC reads. The indexer's broadcast pipeline owns
    /// `checkpoint_sender` from here on and is what publishes to subscribers, so their ordering
    /// follows indexing rather than sealing.
    pub(crate) async fn new(
        simulacrum: Simulacrum<OsRng, ForkStore>,
        mut services: ServiceManager,
        checkpoint_sender: broadcast::Sender<Arc<Checkpoint>>,
        registry: &Registry,
    ) -> Result<(Self, Service)> {
        let simulacrum = Arc::new(RwLock::new(simulacrum));
        let indexer_service = services
            .start_indexer(simulacrum.clone(), checkpoint_sender, registry)
            .await?;
        Ok((
            Self {
                simulacrum,
                services,
                checkpoint_publication_lock: Mutex::new(()),
            },
            indexer_service,
        ))
    }

    pub(crate) fn simulacrum(&self) -> &Arc<RwLock<ForkedSimulacrum>> {
        &self.simulacrum
    }

    /// Return the service manager, for tests alone, because production reads go through the store
    /// handles created at startup.
    #[cfg(test)]
    pub(crate) fn services(&self) -> &ServiceManager {
        &self.services
    }

    /// Execute `operation`, create a checkpoint afterward, and publish that checkpoint to
    /// subscribers.
    ///
    /// This is the main entry point for any execution that requires checkpoint advancement, and it
    /// normally returns only once `sui-rpc-store` has indexed the checkpoint.
    pub(crate) async fn run_with_new_checkpoint<T, F>(&self, operation: F) -> (T, CreatedCheckpoint)
    where
        T: Send,
        F: FnOnce(&mut ForkedSimulacrum) -> T + Send,
    {
        self.try_run_with_new_checkpoint(|sim| Ok::<_, std::convert::Infallible>(operation(sim)))
            .await
            .unwrap_or_else(|never| match never {})
    }

    /// Fallible variant of [`Self::run_with_new_checkpoint`]. If `operation` returns an error, no
    /// checkpoint is created. The publication lock is intentionally held through indexing so
    /// subscribers observe the same order that Simulacrum used to create checkpoints.
    ///
    /// `operation` runs inline on the calling task and can block it for the duration of Move
    /// execution and any synchronous live-network reads. The fork is a sequential, command-driven
    /// node and its RPC read paths make the same synchronous remote reads, so the stall is
    /// accepted rather than dispatched to a blocking thread.
    ///
    /// The sealed checkpoint is durable before the publication wait begins, so an indexer that
    /// fails to index it in time cannot make the operation fail — reporting failure for a durable
    /// state change would invite retries that execute twice. The stall is logged as an error and
    /// the call succeeds; until the indexer catches up, derived reads (owned objects, balances)
    /// lag raw reads and subscribers are not notified.
    pub(crate) async fn try_run_with_new_checkpoint<T, E, F>(
        &self,
        operation: F,
    ) -> std::result::Result<(T, CreatedCheckpoint), E>
    where
        T: Send,
        E: Send,
        F: FnOnce(&mut ForkedSimulacrum) -> std::result::Result<T, E> + Send,
    {
        let _checkpoint_publication_guard = self.checkpoint_publication_lock.lock().await;
        let (output, metadata) = {
            let mut sim = self.simulacrum.write().await;
            let output = operation(&mut sim)?;
            let metadata = Self::seal_checkpoint(&mut sim);
            (output, metadata)
        };

        // Publication is the indexer catching up: it pulls the sealed
        // checkpoint back out of the store, writes the derived indexes, and
        // broadcasts to subscribers. Returning before it has caught up would
        // let an RPC read observe a transaction whose derived state is not
        // there yet.
        if let Err(wait_error) = self
            .services
            .wait_for_indexed_checkpoint(metadata.sequence_number)
            .await
        {
            error!(
                checkpoint_sequence_number = metadata.sequence_number,
                "sealed checkpoint was not indexed in time; derived reads lag and subscribers \
                 were not notified until the indexer catches up: {wait_error:#}"
            );
        }

        Ok((output, metadata))
    }

    fn seal_checkpoint(sim: &mut ForkedSimulacrum) -> CreatedCheckpoint {
        let verified = sim.create_checkpoint();
        CreatedCheckpoint {
            sequence_number: verified.data().sequence_number,
            timestamp_ms: verified.data().timestamp_ms,
        }
    }
}
