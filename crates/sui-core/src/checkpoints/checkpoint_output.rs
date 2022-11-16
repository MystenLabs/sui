// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::StableSyncAuthoritySigner;
use crate::consensus_adapter::SubmitToConsensus;
use async_trait::async_trait;
use fastcrypto::encoding::{Encoding, Hex};
use sui_types::base_types::AuthorityName;
use sui_types::error::SuiResult;
use sui_types::messages::ConsensusTransaction;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSignatureMessage, CheckpointSummary,
    SignedCheckpointSummary,
};
use tracing::{debug, info};

#[async_trait]
pub trait CheckpointOutput: Sync + Send + 'static {
    async fn checkpoint_created(
        &self,
        summary: &CheckpointSummary,
        contents: &CheckpointContents,
        last_checkpoint_of_epoch: bool,
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
impl<T: SubmitToConsensus> CheckpointOutput for SubmitCheckpointToConsensus<T> {
    async fn checkpoint_created(
        &self,
        summary: &CheckpointSummary,
        contents: &CheckpointContents,
        last_checkpoint_of_epoch: bool,
    ) -> SuiResult {
        LogCheckpointOutput
            .checkpoint_created(summary, contents, last_checkpoint_of_epoch)
            .await?;
        if last_checkpoint_of_epoch {
            // Augment the checkpoint with the change epoch transaction.
        }
        let summary = SignedCheckpointSummary::new_from_summary(
            summary.clone(),
            self.authority,
            &*self.signer,
        );
        let message = CheckpointSignatureMessage { summary };
        let transaction = ConsensusTransaction::new_checkpoint_signature_message(message);
        self.sender.submit_to_consensus(&transaction).await
    }
}

#[async_trait]
impl CheckpointOutput for LogCheckpointOutput {
    async fn checkpoint_created(
        &self,
        summary: &CheckpointSummary,
        contents: &CheckpointContents,
        last_checkpoint_of_epoch: bool,
    ) -> SuiResult {
        debug!(
            "Including following transactions in checkpoint {}: {:?}",
            summary.sequence_number, contents
        );
        info!(
            "Creating checkpoint {:?} at sequence {}, previous digest {:?}, transactions count {}, content digest {:?}, last_checkpoint_of_epoch {}",
            Hex::encode(summary.digest()),
            summary.sequence_number,
            summary.previous_digest,
            contents.size(),
            Hex::encode(summary.content_digest),
            last_checkpoint_of_epoch,
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
            summary.summary.sequence_number,
            Hex::encode(summary.summary.digest())
        );
        Ok(())
    }
}
