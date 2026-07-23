// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use rand::rngs::OsRng;
use tokio::sync::RwLock;

use simulacrum::Simulacrum;
use simulacrum::SimulatorStore as _;
use sui_indexer_alt_framework::ingestion::ingestion_client::CheckpointError;
use sui_indexer_alt_framework::ingestion::ingestion_client::CheckpointResult;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientTrait;
use sui_types::digests::ChainIdentifier;
use sui_types::storage::ReadStore as _;

use crate::store::ForkStore;

type ForkedSimulacrum = Simulacrum<OsRng, ForkStore>;

/// Pull-side checkpoint source for the fork's embedded `sui-rpc-store`
/// indexer.
pub(crate) struct SimulacrumIngestion {
    simulacrum: Arc<RwLock<ForkedSimulacrum>>,
    chain_identifier: ChainIdentifier,
}

impl SimulacrumIngestion {
    pub(crate) fn new(
        simulacrum: Arc<RwLock<ForkedSimulacrum>>,
        chain_identifier: ChainIdentifier,
    ) -> Self {
        Self {
            simulacrum,
            chain_identifier,
        }
    }
}

#[async_trait]
impl IngestionClientTrait for SimulacrumIngestion {
    async fn chain_id(&self) -> anyhow::Result<ChainIdentifier> {
        Ok(self.chain_identifier)
    }

    async fn checkpoint(&self, checkpoint: u64) -> CheckpointResult {
        let sim = self.simulacrum.read().await;

        // The ingestion service polls past the local tip until the next
        // checkpoint is sealed; answer those polls locally instead of routing
        // them through the ForkStore remote-fallback path every retry.
        let local_tip = sim
            .store()
            .get_highest_checkpint()
            .map(|checkpoint| checkpoint.data().sequence_number);
        if local_tip.is_some_and(|tip| checkpoint > tip) {
            return Err(CheckpointError::NotFound);
        }

        let verified = sim
            .store()
            .get_checkpoint_by_sequence_number(checkpoint)
            .map_err(|err| CheckpointError::Fetch(anyhow!("{err:#}")))?
            .ok_or(CheckpointError::NotFound)?;
        let Some(contents) = sim
            .store()
            .get_checkpoint_contents(&verified.content_digest)
        else {
            return Err(CheckpointError::Fetch(anyhow!(
                "checkpoint {checkpoint} present but contents are missing",
            )));
        };

        sim.get_checkpoint_data(verified, contents)
            .map_err(|err| CheckpointError::Fetch(anyhow!("{err:#}")))
    }

    async fn latest_checkpoint_number(&self) -> anyhow::Result<u64> {
        let sim = self.simulacrum.read().await;
        Ok(sim
            .store()
            .get_highest_checkpint()
            .map(|checkpoint| checkpoint.data().sequence_number)
            .unwrap_or_default())
    }
}
