// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::state_sync::metrics::Metrics;
use anemo::async_trait;
use anyhow::{Context, anyhow};
use sui_data_ingestion_core::Worker;
use sui_storage::verify_checkpoint;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CertifiedCheckpointSummary;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::messages_checkpoint::{FullCheckpointContents, VerifiedCheckpointContents};
use sui_types::storage::WriteStore;

pub(crate) struct StateSyncWorker<S>(pub(crate) S, pub(crate) Metrics);

#[async_trait]
impl<S: WriteStore + Clone + Send + Sync + 'static> Worker for StateSyncWorker<S> {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> anyhow::Result<()> {
        let verified_checkpoint = get_or_insert_verified_checkpoint(
            &self.0,
            checkpoint.checkpoint_summary.clone(),
            true,
        )?;
        let full_contents = FullCheckpointContents::from_contents_and_execution_data(
            checkpoint.checkpoint_contents.clone(),
            checkpoint.transactions.iter().map(|t| t.execution_data()),
        );
        full_contents.verify_digests(verified_checkpoint.content_digest)?;
        let verified_contents = VerifiedCheckpointContents::new_unchecked(full_contents);
        self.0
            .insert_checkpoint_contents(&verified_checkpoint, verified_contents)?;
        self.0
            .update_highest_synced_checkpoint(&verified_checkpoint)?;
        self.1.update_checkpoints_synced_from_archive();
        Ok(())
    }
}

pub fn get_or_insert_verified_checkpoint<S>(
    store: &S,
    certified_checkpoint: CertifiedCheckpointSummary,
    verify: bool,
) -> anyhow::Result<VerifiedCheckpoint>
where
    S: WriteStore + Clone,
{
    store
        .get_checkpoint_by_sequence_number(certified_checkpoint.sequence_number)
        .map(Ok::<VerifiedCheckpoint, anyhow::Error>)
        .unwrap_or_else(|| {
            let verified_checkpoint = if verify {
                // Verify checkpoint summary
                let prev_checkpoint_seq_num = certified_checkpoint
                    .sequence_number
                    .checked_sub(1)
                    .context("Checkpoint seq num underflow")?;
                let prev_checkpoint = store
                    .get_checkpoint_by_sequence_number(prev_checkpoint_seq_num)
                    .context(format!(
                        "Missing previous checkpoint {} in store",
                        prev_checkpoint_seq_num
                    ))?;

                verify_checkpoint(&prev_checkpoint, store, certified_checkpoint)
                    .map_err(|_| anyhow!("Checkpoint verification failed"))?
            } else {
                VerifiedCheckpoint::new_unchecked(certified_checkpoint)
            };
            // Insert checkpoint summary
            store
                .insert_checkpoint(&verified_checkpoint)
                .map_err(|e| anyhow!("Failed to insert checkpoint: {e}"))?;
            // Update highest verified checkpoint watermark
            store
                .update_highest_verified_checkpoint(&verified_checkpoint)
                .expect("store operation should not fail");
            Ok::<VerifiedCheckpoint, anyhow::Error>(verified_checkpoint)
        })
        .map_err(|e| anyhow!("Failed to get verified checkpoint: {:?}", e))
}
