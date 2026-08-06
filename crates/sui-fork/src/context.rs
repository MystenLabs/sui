// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use prometheus::Registry;
use rand::rngs::OsRng;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::sync::broadcast;

use simulacrum::Simulacrum;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::services::ServiceManager;
use crate::store::ForkStore;

type ForkedSimulacrum = Simulacrum<OsRng, ForkStore>;

/// Metadata for a checkpoint created by the forked network.
///
/// Callers only need these fields for RPC responses and finality metadata, and the checkpoint
/// itself is re-read from the store by the indexer.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CreatedCheckpointMetadata {
    /// Sequence number of the created checkpoint.
    pub(crate) sequence_number: CheckpointSequenceNumber,

    /// Timestamp of the created checkpoint, in milliseconds.
    pub(crate) timestamp_ms: u64,
}

/// Shared context for the forked network, holding the simulacrum and the service manager running
/// the embedded indexer.
pub struct Context {
    simulacrum: Arc<RwLock<ForkedSimulacrum>>,
    services: ServiceManager,
    checkpoint_publication_lock: Mutex<()>,
}

impl Context {
    /// Build a `Context` whose Simulacrum is backed by a started [`ServiceManager`].
    ///
    /// Starts the embedded `sui-rpc-store` indexer over `checkpoint_sender` before returning, so
    /// committed local checkpoints get indexed for RPC reads. The indexer's broadcast pipeline
    /// owns `checkpoint_sender` from here on and is what publishes to subscribers, so their
    /// ordering follows indexing rather than sealing.
    pub(crate) async fn new(
        simulacrum: Simulacrum<OsRng, ForkStore>,
        mut services: ServiceManager,
        checkpoint_sender: broadcast::Sender<Arc<Checkpoint>>,
        registry: &Registry,
    ) -> Result<Self> {
        let simulacrum = Arc::new(RwLock::new(simulacrum));
        services
            .start_indexer(simulacrum.clone(), checkpoint_sender, registry)
            .await?;
        Ok(Self {
            simulacrum,
            services,
            checkpoint_publication_lock: Mutex::new(()),
        })
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

    /// Resolve when the embedded rpc-store indexer stops. The server loop uses this as a liveness
    /// watchdog, so an indexer failure surfaces immediately instead of as a publication timeout on
    /// the next executed transaction.
    pub(crate) async fn indexer_stopped(&self) -> anyhow::Result<()> {
        self.services.indexer_stopped().await
    }

    /// Execute `operation`, create a checkpoint afterward, and publish that checkpoint to
    /// subscribers.
    ///
    /// This is the main entry point for any execution that requires checkpoint advancement, and it
    /// returns only once `sui-rpc-store` has indexed the checkpoint.
    ///
    /// # Panics
    ///
    /// Panics if the indexer cannot index the checkpoint before publishing.
    pub(crate) async fn run_with_new_checkpoint<T, F>(
        &self,
        operation: F,
    ) -> (T, CreatedCheckpointMetadata)
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
    /// # Panics
    ///
    /// Panics if the indexer cannot index the checkpoint before publishing.
    pub(crate) async fn try_run_with_new_checkpoint<T, E, F>(
        &self,
        operation: F,
    ) -> std::result::Result<(T, CreatedCheckpointMetadata), E>
    where
        T: Send,
        E: Send,
        F: FnOnce(&mut ForkedSimulacrum) -> std::result::Result<T, E> + Send,
    {
        let _checkpoint_publication_guard = self.checkpoint_publication_lock.lock().await;
        let (output, metadata) = {
            let mut sim = self.simulacrum.write().await;
            let output = operation(&mut sim)?;
            let metadata = Self::create_checkpoint(&mut sim);
            (output, metadata)
        };

        // Publication is the indexer catching up: it pulls the sealed
        // checkpoint back out of the store, writes the derived indexes, and
        // broadcasts to subscribers. Returning before it has caught up would
        // let an RPC read observe a transaction whose derived state is not
        // there yet.
        self.services
            .wait_for_indexed_checkpoint(metadata.sequence_number)
            .await
            .unwrap_or_else(|err| {
                panic!(
                    "failed to publish checkpoint {}: {err:#}",
                    metadata.sequence_number
                )
            });

        Ok((output, metadata))
    }

    fn create_checkpoint(sim: &mut ForkedSimulacrum) -> CreatedCheckpointMetadata {
        let verified = sim.create_checkpoint();
        CreatedCheckpointMetadata {
            sequence_number: verified.data().sequence_number,
            timestamp_ms: verified.data().timestamp_ms,
        }
    }
}
