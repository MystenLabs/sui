// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use anyhow::Result;
use anyhow::anyhow;
use prometheus::Registry;
use rand::rngs::OsRng;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::sync::broadcast;

use simulacrum::Simulacrum;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::storage::ReadStore as _;

use crate::services::ServiceManager;
use crate::store::ForkStore;

type ForkedSimulacrum = Simulacrum<OsRng, ForkStore>;

/// Metadata for a checkpoint created by the forked network.
///
/// The full checkpoint payload is an internal publication detail; callers only
/// need these fields for RPC responses and finality metadata.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CreatedCheckpointMetadata {
    /// Checkpoint numbers
    pub(crate) sequence_number: CheckpointSequenceNumber,
    /// Checkpoint timestamp
    pub(crate) timestamp_ms: u64,
}

struct CheckpointPublication {
    metadata: CreatedCheckpointMetadata,
    payload: Checkpoint,
}

/// Shared context for the forked network: the simulacrum, the optional
/// service manager, and the producer half of the checkpoint subscription
/// channel.
pub struct Context {
    simulacrum: Arc<RwLock<ForkedSimulacrum>>,
    services: Option<ServiceManager>,
    checkpoint_sender: broadcast::Sender<Arc<Checkpoint>>,
    checkpoint_publication_lock: Mutex<()>,
}

impl Context {
    /// Service-less construction for in-memory tests; production always goes
    /// through [`Self::new_with_services`].
    #[cfg(test)]
    pub(crate) fn new(
        simulacrum: Simulacrum<OsRng, ForkStore>,
        checkpoint_sender: broadcast::Sender<Arc<Checkpoint>>,
    ) -> Self {
        Self {
            simulacrum: Arc::new(RwLock::new(simulacrum)),
            services: None,
            checkpoint_sender,
            checkpoint_publication_lock: Mutex::new(()),
        }
    }

    /// Build a `Context` whose Simulacrum is backed by a started [`ServiceManager`].
    ///
    /// Starts the embedded `sui-rpc-store` indexer over `checkpoint_sender`
    /// before returning, so committed local checkpoints get indexed for RPC reads.
    /// Tests use the service-less `Context::new` instead.
    pub(crate) async fn new_with_services(
        simulacrum: Simulacrum<OsRng, ForkStore>,
        mut services: ServiceManager,
        checkpoint_sender: broadcast::Sender<Arc<Checkpoint>>,
        registry: &Registry,
    ) -> Result<Self> {
        let simulacrum = Arc::new(RwLock::new(simulacrum));
        services
            .start_indexer(simulacrum.clone(), checkpoint_sender.clone(), registry)
            .await?;
        Ok(Self {
            simulacrum,
            services: Some(services),
            checkpoint_sender,
            checkpoint_publication_lock: Mutex::new(()),
        })
    }

    pub(crate) fn simulacrum(&self) -> &Arc<RwLock<ForkedSimulacrum>> {
        &self.simulacrum
    }

    /// Only tests need direct service-manager access; production reads go
    /// through the store handles created at startup.
    #[cfg(test)]
    pub(crate) fn services(&self) -> Option<&ServiceManager> {
        self.services.as_ref()
    }

    /// Resolves when the embedded rpc-store indexer stops; pends forever on
    /// service-less (in-memory) contexts. Used as a liveness watchdog by the
    /// server loop, so an indexer failure surfaces immediately instead of as
    /// a publication timeout on the next executed transaction.
    pub(crate) async fn indexer_stopped(&self) -> anyhow::Result<()> {
        match &self.services {
            Some(services) => services.indexer_stopped().await,
            None => std::future::pending().await,
        }
    }

    /// Execute `operation`, create a checkpoint afterward, and publish that
    /// checkpoint to subscribers.
    ///
    /// This is the main entry point for any execution that requires checkpoint
    /// advancement to ensure the checkpoint is either indexed and published by
    /// `sui-rpc-store`, or published directly when no service manager is
    /// attached.
    ///
    /// # Panics
    ///
    /// Panics if Simulacrum creates a checkpoint but the full checkpoint
    /// payload cannot be assembled from the same store, or if the indexer
    /// cannot index the checkpoint before publishing.
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

    /// Fallible variant of [`Self::run_with_new_checkpoint`]. If `operation`
    /// returns an error, no checkpoint is created. The publication lock is
    /// intentionally held through indexing or direct enqueueing so
    /// subscribers observe the same order that Simulacrum used to create
    /// checkpoints.
    ///
    /// # Panics
    ///
    /// Panics if Simulacrum creates a checkpoint but the full checkpoint
    /// payload cannot be assembled from the same store, or if the indexer
    /// cannot index the checkpoint before publishing.
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
        let (output, publication) = {
            let mut sim = self.simulacrum.write().await;
            let output = operation(&mut sim)?;
            let publication = Self::create_checkpoint_publication(&mut sim);
            (output, publication)
        };

        let metadata = publication.metadata;
        self.publish_checkpoint(publication)
            .await
            .unwrap_or_else(|err| {
                panic!(
                    "failed to publish checkpoint {}: {err:#}",
                    metadata.sequence_number
                )
            });

        Ok((output, metadata))
    }

    fn create_checkpoint_publication(sim: &mut ForkedSimulacrum) -> CheckpointPublication {
        let verified = sim.create_checkpoint();
        let metadata = CreatedCheckpointMetadata {
            sequence_number: verified.data().sequence_number,
            timestamp_ms: verified.data().timestamp_ms,
        };

        let payload = Self::checkpoint_payload(sim, verified).unwrap_or_else(|err| {
            panic!(
                "failed to build checkpoint {} after Simulacrum created it: {err}",
                metadata.sequence_number
            )
        });

        CheckpointPublication { metadata, payload }
    }

    fn checkpoint_payload(
        sim: &ForkedSimulacrum,
        verified: VerifiedCheckpoint,
    ) -> Result<Checkpoint> {
        let contents = sim
            .store()
            .get_checkpoint_contents_by_digest(&verified.content_digest)?
            .ok_or_else(|| {
                anyhow!(
                    "checkpoint contents for sequence {} not found",
                    verified.data().sequence_number
                )
            })?;

        Ok(sim.get_checkpoint_data(verified, contents)?)
    }

    async fn publish_checkpoint(&self, publication: CheckpointPublication) -> Result<()> {
        if let Some(services) = &self.services {
            services
                .wait_for_indexed_checkpoint(publication.metadata.sequence_number)
                .await?;
            return Ok(());
        }

        // The broadcast send is non-blocking; an error just means there are no
        // live subscribers, which is fine.
        if let Err(err) = self
            .checkpoint_sender
            .send(Arc::new(publication.payload))
            .context("failed to enqueue checkpoint to subscription service")
        {
            tracing::warn!(
                sequence_number = publication.metadata.sequence_number,
                ?err,
                "failed to publish checkpoint to subscribers"
            );
        }
        Ok(())
    }
}
