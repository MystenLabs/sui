// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ParquetSchema;
use anyhow::Result;
use serde::Serialize;
use sui_types::base_types::EpochId;

pub trait AnalyticsWriter<S: Serialize + ParquetSchema>: Send + Sync + 'static {
    /// Persist given rows into a file
    fn write(&mut self, rows: Box<dyn Iterator<Item = S> + Send + Sync>) -> Result<()>;
    /// Flush the current file
    fn flush(&mut self, end_checkpoint_seq_num: u64) -> Result<bool>;
    /// Reset internal state with given epoch and checkpoint sequence number
    fn reset(&mut self, epoch_num: EpochId, start_checkpoint_seq_num: u64) -> Result<()>;
    /// Number of rows accumulated since last flush
    fn rows(&self) -> Result<usize>;
}
