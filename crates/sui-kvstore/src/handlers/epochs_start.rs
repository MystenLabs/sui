// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::full_checkpoint_content::Checkpoint;

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

        let epoch = epoch_info.epoch;
        let protocol_version = epoch_info
            .protocol_version
            .context("missing protocol_version")?;
        let start_timestamp_ms = epoch_info
            .start_timestamp_ms
            .context("missing start_timestamp_ms")?;
        let start_checkpoint = epoch_info
            .start_checkpoint
            .context("missing start_checkpoint")?;
        let reference_gas_price = epoch_info
            .reference_gas_price
            .context("missing reference_gas_price")?;
        let system_state = epoch_info
            .system_state
            .as_ref()
            .context("missing system_state")?;

        let entry = tables::make_entry(
            tables::epochs::encode_key(epoch),
            tables::epochs::encode_start(
                epoch,
                protocol_version,
                start_timestamp_ms,
                start_checkpoint,
                reference_gas_price,
                system_state,
            )?,
            Some(start_timestamp_ms),
        );

        Ok(vec![entry])
    }
}

impl BigTableProcessor for EpochStartPipeline {
    const TABLE: &'static str = tables::epochs::NAME;
    const FANOUT: usize = 100;
    const MIN_EAGER_ROWS: usize = 1;
}
