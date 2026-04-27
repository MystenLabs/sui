// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{anyhow, Context as _, Result};
use rand::rngs::OsRng;
use tokio::sync::mpsc;
use tokio::sync::RwLock;

use simulacrum::Simulacrum;
use sui_protocol_config::Chain;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::storage::ReadStore as _;

use crate::store::DataStore;

/// Shared context for the forked network: the simulacrum, chain identifier,
/// and the producer half of the checkpoint subscription channel.
pub struct Context {
    simulacrum: Arc<RwLock<Simulacrum<OsRng, DataStore>>>,
    chain_identifier: Chain,
    checkpoint_sender: mpsc::Sender<Checkpoint>,
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
        }
    }

    pub(crate) fn simulacrum(&self) -> &Arc<RwLock<Simulacrum<OsRng, DataStore>>> {
        &self.simulacrum
    }

    pub(crate) fn chain_identifier(&self) -> &Chain {
        &self.chain_identifier
    }

    /// Fetch the checkpoint at `sequence` from the store, assemble a full
    /// `Checkpoint`, and push it to subscribers via the broker. Subscribers
    /// must observe checkpoints in monotonically increasing sequence order —
    /// the broker panics otherwise.
    pub(crate) async fn publish_checkpoint(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> Result<()> {
        let checkpoint = {
            let sim = self.simulacrum.read().await;
            let store = sim.store();

            let verified = store
                .get_checkpoint_by_sequence_number(sequence)?
                .ok_or_else(|| anyhow!("checkpoint summary {sequence} not found"))?;
            let contents = store
                .get_checkpoint_contents_by_digest(&verified.content_digest)?
                .ok_or_else(|| anyhow!("checkpoint contents for sequence {sequence} not found"))?;

            sim.get_checkpoint_data(verified, contents)?
        };

        self.checkpoint_sender
            .send(checkpoint)
            .await
            .context("failed to enqueue checkpoint to subscription service")
    }
}
