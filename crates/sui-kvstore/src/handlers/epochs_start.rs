// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::EpochStartData;
use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::handlers::BigTableProcessor;
use crate::tables;

/// Pipeline that writes epoch start data to BigTable.
pub struct EpochStartPipeline;

#[async_trait::async_trait]
impl Processor for EpochStartPipeline {
    const NAME: &'static str = "kvstore_epochs_start";
    type Value = Entry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let Some(epoch_info) = checkpoint.epoch_info()? else {
            return Ok(vec![]);
        };

        let epoch_start = EpochStartData::from(&epoch_info);
        let entry = tables::make_entry(
            tables::epochs::encode_key(epoch_start.epoch),
            tables::epochs::encode_start(&epoch_start)?,
            epoch_start.start_timestamp_ms,
        );

        Ok(vec![entry])
    }
}

impl BigTableProcessor for EpochStartPipeline {
    const TABLE: &'static str = tables::epochs::NAME;
    const MIN_EAGER_ROWS: usize = 1;
}
