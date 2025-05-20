// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Args;
use sui_data_ingestion_core::end_of_epoch_data;
use tracing::info;

#[derive(Clone, Debug)]
pub(crate) struct ArchivalCheckpointInfo {
    pub next_checkpoint_after_epoch: u64,
}

impl ArchivalCheckpointInfo {
    /// Reads checkpoint information from archival storage to determine,
    /// specifically the next checkpoint number after `start_epoch` for watermarking.
    pub async fn read_archival_checkpoint_info(args: &Args) -> anyhow::Result<Self> {
        let checkpoints = end_of_epoch_data(args.archive_url.clone(), vec![], 5).await?;
        let next_checkpoint_after_epoch = checkpoints[args.start_epoch as usize] + 1;
        info!(
            epoch = args.start_epoch,
            checkpoint = next_checkpoint_after_epoch,
            "Next checkpoint after epoch",
        );
        Ok(Self {
            next_checkpoint_after_epoch,
        })
    }
}
