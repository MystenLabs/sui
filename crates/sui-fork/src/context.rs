// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use prometheus::Registry;
use rand::rngs::OsRng;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::sync::broadcast;
use tracing::error;
use tracing::info;

use simulacrum::Simulacrum;
use simulacrum::SimulatorStore as _;
use sui_futures::service::Service;
use sui_types::effects::TransactionEffectsAPI as _;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait as _;

use crate::fork::ClockAdvanced;
use crate::fork::CreatedCheckpoint;
use crate::fork::ForkStatus;
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

    /// Advance the fork's clock by `duration` and seal the resulting clock transaction into a
    /// checkpoint. Both the forking gRPC service and the in-process handle delegate here, so the
    /// two surfaces share one contract.
    pub(crate) async fn advance_clock(&self, duration: Duration) -> ClockAdvanced {
        let ((tx_digest, timestamp_ms), checkpoint) = self
            .run_with_new_checkpoint(move |sim| {
                let effects = sim.advance_clock(duration);
                let tx_digest = *effects.transaction_digest();
                let timestamp_ms = sim.store().get_clock().timestamp_ms;
                (tx_digest, timestamp_ms)
            })
            .await;

        info!(
            %tx_digest,
            duration_ms = duration.as_millis() as u64,
            timestamp_ms,
            checkpoint_sequence_number = checkpoint.sequence_number,
            "clock advanced"
        );

        ClockAdvanced {
            tx_digest,
            timestamp_ms,
            checkpoint,
        }
    }

    /// Seal all pending transactions into a new checkpoint.
    pub(crate) async fn create_checkpoint(&self) -> CreatedCheckpoint {
        let ((), checkpoint) = self.run_with_new_checkpoint(|_| ()).await;

        info!(
            checkpoint_sequence_number = checkpoint.sequence_number,
            timestamp_ms = checkpoint.timestamp_ms,
            "checkpoint created"
        );

        checkpoint
    }

    /// The fork's current epoch, checkpoint tip, clock, and fork point.
    pub(crate) async fn status(&self) -> ForkStatus {
        let sim = self.simulacrum.read().await;
        ForkStatus {
            epoch: sim.epoch_start_state().epoch(),
            checkpoint_sequence_number: sim
                .store()
                .get_highest_checkpint()
                .map(|cp| cp.data().sequence_number)
                .unwrap_or(0),
            timestamp_ms: sim.store().get_clock().timestamp_ms,
            forked_at_checkpoint: sim.store().forked_at_checkpoint(),
        }
    }

    /// Execute `operation`, create a checkpoint afterward, and publish that checkpoint to
    /// subscribers.
    ///
    /// This is the main entry point for any execution that requires checkpoint advancement, and it
    /// normally returns only once `sui-rpc-store` has indexed the checkpoint.
    pub(crate) async fn run_with_new_checkpoint<T, F>(&self, operation: F) -> (T, CreatedCheckpoint)
    where
        T: Send + 'static,
        F: FnOnce(&mut ForkedSimulacrum) -> T + Send + 'static,
    {
        self.try_run_with_new_checkpoint(|sim| Ok::<_, std::convert::Infallible>(operation(sim)))
            .await
            .unwrap_or_else(|never| match never {})
    }

    /// Fallible variant of [`Self::run_with_new_checkpoint`]. If `operation` returns an error, no
    /// checkpoint is created. The publication lock is intentionally held through indexing so
    /// subscribers observe the same order that Simulacrum used to create checkpoints.
    ///
    /// `operation` runs on a blocking thread: it executes the Move VM and can make synchronous
    /// remote reads through the store, either of which would otherwise park a runtime worker for
    /// the whole time the simulacrum write lock is held.
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
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&mut ForkedSimulacrum) -> std::result::Result<T, E> + Send + 'static,
    {
        let _checkpoint_publication_guard = self.checkpoint_publication_lock.lock().await;
        let mut sim = self.simulacrum.clone().write_owned().await;
        let (output, metadata) = tokio::task::spawn_blocking(move || {
            let output = operation(&mut sim)?;
            let metadata = Self::seal_checkpoint(&mut sim);
            Ok((output, metadata))
        })
        .await
        .unwrap_or_else(|join_error| match join_error.try_into_panic() {
            Ok(panic) => std::panic::resume_unwind(panic),
            Err(join_error) => panic!("execution task failed to join: {join_error}"),
        })?;

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
