// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::StableSyncAuthoritySigner;
use crate::consensus_adapter::SubmitToConsensus;
use crate::epoch::reconfiguration::ReconfigurationInitiator;
use async_trait::async_trait;
use std::sync::Arc;
use sui_types::base_types::AuthorityName;
use sui_types::error::SuiResult;
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSignatureMessage, CheckpointSummary,
    SignedCheckpointSummary, VerifiedCheckpoint,
};
use sui_types::messages_consensus::ConsensusTransaction;
use tracing::{debug, info, trace};

use super::CheckpointMetrics;

#[async_trait]
pub trait CheckpointOutput: Sync + Send + 'static {
    async fn checkpoint_created(
        &self,
        summary: &CheckpointSummary,
        contents: &CheckpointContents,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult;
}

#[async_trait]
pub trait CertifiedCheckpointOutput: Sync + Send + 'static {
    async fn certified_checkpoint_created(&self, summary: &CertifiedCheckpointSummary)
        -> SuiResult;
}

pub struct SubmitCheckpointToConsensus<T> {
    pub sender: T,
    pub signer: StableSyncAuthoritySigner,
    pub authority: AuthorityName,
    pub next_reconfiguration_timestamp_ms: u64,
    pub metrics: Arc<CheckpointMetrics>,
}

pub struct LogCheckpointOutput;

impl LogCheckpointOutput {
    pub fn boxed() -> Box<dyn CheckpointOutput> {
        Box::new(Self)
    }

    pub fn boxed_certified() -> Box<dyn CertifiedCheckpointOutput> {
        Box::new(Self)
    }
}

#[async_trait]
impl<T: SubmitToConsensus + ReconfigurationInitiator> CheckpointOutput
    for SubmitCheckpointToConsensus<T>
{
    async fn checkpoint_created(
        &self,
        summary: &CheckpointSummary,
        contents: &CheckpointContents,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        let checkpoint_seq = summary.sequence_number;
        let checkpoint_timestamp = summary.timestamp_ms;
        self.metrics.checkpoint_creation_latency_ms.observe(
            summary
                .timestamp()
                .elapsed()
                .unwrap_or_default()
                .as_millis() as u64,
        );
        debug!(
            "Sending checkpoint signature at sequence {checkpoint_seq} to consensus, timestamp {checkpoint_timestamp}.
            {}ms left till end of epoch at timestamp {}",
            self.next_reconfiguration_timestamp_ms.saturating_sub(checkpoint_timestamp), self.next_reconfiguration_timestamp_ms
        );
        LogCheckpointOutput
            .checkpoint_created(summary, contents, epoch_store)
            .await?;

        let summary = SignedCheckpointSummary::new(
            epoch_store.epoch(),
            summary.clone(),
            &*self.signer,
            self.authority,
        );

        let message = CheckpointSignatureMessage { summary };
        let transaction = ConsensusTransaction::new_checkpoint_signature_message(message);
        self.sender
            .submit_to_consensus(&transaction, epoch_store)
            .await?;
        self.metrics
            .last_sent_checkpoint_signature
            .set(checkpoint_seq as i64);
        if checkpoint_timestamp >= self.next_reconfiguration_timestamp_ms {
            // close_epoch is ok if called multiple times
            self.sender.close_epoch(epoch_store);
        }
        Ok(())
    }
}

#[async_trait]
impl CheckpointOutput for LogCheckpointOutput {
    async fn checkpoint_created(
        &self,
        summary: &CheckpointSummary,
        contents: &CheckpointContents,
        _epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        trace!(
            "Including following transactions in checkpoint {}: {:?}",
            summary.sequence_number,
            contents
        );
        info!(
            "Creating checkpoint {:?} at epoch {}, sequence {}, previous digest {:?}, transactions count {}, content digest {:?}, end_of_epoch_data {:?}",
            summary.digest(),
            summary.epoch,
            summary.sequence_number,
            summary.previous_digest,
            contents.size(),
            summary.content_digest,
            summary.end_of_epoch_data,
        );

        Ok(())
    }
}

#[async_trait]
impl CertifiedCheckpointOutput for LogCheckpointOutput {
    async fn certified_checkpoint_created(
        &self,
        summary: &CertifiedCheckpointSummary,
    ) -> SuiResult {
        info!(
            "Certified checkpoint with sequence {} and digest {}",
            summary.sequence_number,
            summary.digest()
        );
        Ok(())
    }
}

pub struct SendCheckpointToStateSync {
    handle: sui_network::state_sync::Handle,
}

impl SendCheckpointToStateSync {
    pub fn new(handle: sui_network::state_sync::Handle) -> Self {
        Self { handle }
    }
}

#[async_trait]
impl CertifiedCheckpointOutput for SendCheckpointToStateSync {
    async fn certified_checkpoint_created(
        &self,
        summary: &CertifiedCheckpointSummary,
    ) -> SuiResult {
        info!(
            "Certified checkpoint with sequence {} and digest {}",
            summary.sequence_number,
            summary.digest()
        );
        self.handle
            .send_checkpoint(VerifiedCheckpoint::new_unchecked(summary.to_owned()))
            .await;

        Ok(())
    }
}
