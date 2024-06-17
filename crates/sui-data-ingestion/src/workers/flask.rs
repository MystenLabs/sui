// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use sui_data_ingestion_core::Worker;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

pub struct FlaskWorker;

#[async_trait]
impl Worker for FlaskWorker {
    async fn process_checkpoint(&self, _: CheckpointData) -> Result<()> {
        Ok(())
    }

    async fn save_progress(&self, sequence_number: CheckpointSequenceNumber) -> Option<CheckpointSequenceNumber> {
        if sequence_number % 1000 == 0 {
            Some(sequence_number)
        } else {
            None
        }
    }
}
