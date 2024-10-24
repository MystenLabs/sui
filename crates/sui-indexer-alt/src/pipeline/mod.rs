// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use crate::{handlers::Handler, models::watermarks::CommitterWatermark};

pub mod concurrent;
mod processor;

/// Extra buffer added to channels between tasks in a pipeline. There does not need to be a huge
/// capacity here because tasks already buffer rows to insert internally.
const COMMITTER_BUFFER: usize = 5;

#[derive(clap::Args, Debug, Clone)]
pub struct PipelineConfig {
    /// The collector will check for pending data at least this often
    #[arg(
        long,
        default_value = "500",
        value_name = "MILLISECONDS",
        value_parser = |s: &str| s.parse().map(Duration::from_millis),
    )]
    collect_interval: Duration,

    /// Watermark task will check for pending watermarks this often
    #[arg(
        long,
        default_value = "500",
        value_name = "MILLISECONDS",
        value_parser = |s: &str| s.parse().map(Duration::from_millis),
    )]
    watermark_interval: Duration,

    /// Avoid writing to the watermark table
    #[arg(long)]
    skip_watermark: bool,
}

/// A batch of processed values associated with a single checkpoint. This is an internal type used
/// to communicate between the handler and the committer parts of the pipeline.
struct Indexed<H: Handler> {
    /// Epoch this data is from
    epoch: u64,
    /// Checkpoint this data is from
    cp_sequence_number: u64,
    /// Max (exclusive) transaction sequence number in this batch
    tx_hi: u64,
    /// Values to be inserted into the database from this checkpoint
    values: Vec<H::Value>,
}

impl<H: Handler> Indexed<H> {
    /// Split apart the information in this indexed checkpoint into its watermark and the values to
    /// add to the database.
    fn into_batch(self) -> (CommitterWatermark<'static>, Vec<H::Value>) {
        let watermark = CommitterWatermark {
            pipeline: H::NAME.into(),
            epoch_hi_inclusive: self.epoch as i64,
            checkpoint_hi_inclusive: self.cp_sequence_number as i64,
            tx_hi: self.tx_hi as i64,
        };

        (watermark, self.values)
    }
}
