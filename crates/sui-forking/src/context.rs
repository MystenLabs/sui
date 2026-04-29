// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use rand::rngs::OsRng;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::sync::mpsc;

use simulacrum::Simulacrum;
use sui_protocol_config::Chain;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::storage::ReadStore as _;

use crate::store::DataStore;

type ForkedSimulacrum = Simulacrum<OsRng, DataStore>;

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

/// Shared context for the forked network: the simulacrum, chain identifier,
/// and the producer half of the checkpoint subscription channel.
pub struct Context {
    simulacrum: Arc<RwLock<ForkedSimulacrum>>,
    chain_identifier: Chain,
    checkpoint_sender: mpsc::Sender<Checkpoint>,
    checkpoint_publication_lock: Mutex<()>,
}

impl Context {
    pub(crate) fn new(
        simulacrum: Simulacrum<OsRng, DataStore>,
        chain_identifier: Chain,
        checkpoint_sender: mpsc::Sender<Checkpoint>,
    ) -> Self {
        Self {
            simulacrum: Arc::new(RwLock::new(simulacrum)),
            chain_identifier,
            checkpoint_sender,
            checkpoint_publication_lock: Mutex::new(()),
        }
    }

    pub(crate) fn simulacrum(&self) -> &Arc<RwLock<ForkedSimulacrum>> {
        &self.simulacrum
    }

    pub(crate) fn chain_identifier(&self) -> &Chain {
        &self.chain_identifier
    }

    /// Execute `operation`, create a checkpoint afterward, and publish that
    /// checkpoint to subscribers.
    ///
    /// This is the main entry point for any execution that requires checkpoint
    /// advancement to ensure the checkpoint is published for the subscription
    /// service.
    ///
    /// # Panics
    ///
    /// Panics if Simulacrum creates a checkpoint but the full checkpoint
    /// payload cannot be assembled from the same store.
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
    /// intentionally held through enqueueing the checkpoint so the
    /// `sui-rpc-api` subscription broker observes the same order that
    /// Simulacrum used to create checkpoints.
    ///
    /// # Panics
    ///
    /// Panics if Simulacrum creates a checkpoint but the full checkpoint
    /// payload cannot be assembled from the same store.
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
        self.publish_checkpoint(publication).await;

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

    async fn publish_checkpoint(&self, publication: CheckpointPublication) {
        if let Err(err) = self
            .checkpoint_sender
            .send(publication.payload)
            .await
            .context("failed to enqueue checkpoint to subscription service")
        {
            tracing::warn!(
                sequence_number = publication.metadata.sequence_number,
                ?err,
                "failed to publish checkpoint to subscribers"
            );
        }
    }
}
