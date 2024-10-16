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
}

#[derive(Clone, Debug)]
pub struct IndexerProgress {
    pub checkpoint: CheckpointSequenceNumber,
    pub network_total_transactions: u64,
}
