// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

pub mod benchmark;
mod synthetic_ingestion;
mod tps_tracker;

#[derive(Clone, Debug)]
pub struct SyntheticIngestionConfig {
    /// Directory to write the ingestion data to.
    pub ingestion_dir: PathBuf,
    /// Number of transactions in a checkpoint.
    pub checkpoint_size: u64,
    /// Total number of synthetic checkpoints to generate.
    pub num_checkpoints: u64,
    /// Customize the first checkpoint sequence number to be committed.
    /// This is useful if we want to benchmark on a non-empty database.
    /// Note that this must be > 0, because the genesis checkpoint is always 0.
    pub starting_checkpoint: CheckpointSequenceNumber,
}

#[derive(Clone, Debug)]
pub struct IndexerProgress {
    pub checkpoint: CheckpointSequenceNumber,
    pub network_total_transactions: u64,
}
