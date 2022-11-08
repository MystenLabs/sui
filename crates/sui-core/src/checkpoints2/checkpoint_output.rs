// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::error::SuiResult;
use sui_types::messages_checkpoint::{CheckpointContents, CheckpointSummary};
use tracing::{debug, info};

pub trait CheckpointOutput: Sync + Send + 'static {
    fn checkpoint_created(
        &self,
        summary: &CheckpointSummary,
        contents: &CheckpointContents,
    ) -> SuiResult;
}

pub struct LogCheckpointOutput;

impl LogCheckpointOutput {
    pub fn boxed() -> Box<dyn CheckpointOutput> {
        Box::new(Self)
    }
}

impl CheckpointOutput for LogCheckpointOutput {
    fn checkpoint_created(
        &self,
        summary: &CheckpointSummary,
        contents: &CheckpointContents,
    ) -> SuiResult {
        debug!(
            "Including following transactions in checkpoint {}: {:?}",
            summary.sequence_number, contents
        );
        info!(
            "Creating checkpoint {:?} at sequence {}, previous digest {:?}, transactions count {}, content digest {:?}",
            summary.digest(),
            summary.sequence_number,
            summary.previous_digest,
            contents.size(),
            summary.content_digest
        );

        Ok(())
    }
}
