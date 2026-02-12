// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::handlers::BigTableProcessor;
use crate::tables;

/// Pipeline that writes epoch end data to BigTable.
/// This is written when a new epoch starts (for the previous epoch).
pub struct EpochEndPipeline;

#[async_trait::async_trait]
impl Processor for EpochEndPipeline {
    const NAME: &'static str = "kvstore_epochs_end";
    type Value = Entry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        self.process_sync(checkpoint)
    }
}

impl BigTableProcessor for EpochEndPipeline {
    const TABLE: &'static str = tables::epochs::NAME;
    const FANOUT: usize = 100;
    const MIN_EAGER_ROWS: usize = 1;

    fn process_sync(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Entry>> {
        let Some(epoch_info) = checkpoint.epoch_info()? else {
            return Ok(vec![]);
        };

        if epoch_info.epoch == 0 {
            return Ok(vec![]);
        }

        let epoch_id = epoch_info.epoch - 1;
        let start_checkpoint = epoch_info
            .start_checkpoint
            .context("missing start_checkpoint for epoch end")?;
        let end_timestamp_ms = epoch_info
            .start_timestamp_ms
            .context("missing start_timestamp_ms for epoch end")?;
        let end_checkpoint = start_checkpoint - 1;

        let entry = tables::make_entry(
            tables::epochs::encode_key(epoch_id),
            tables::epochs::encode_end(end_timestamp_ms, end_checkpoint),
            Some(end_timestamp_ms),
        );

        Ok(vec![entry])
    }
}
