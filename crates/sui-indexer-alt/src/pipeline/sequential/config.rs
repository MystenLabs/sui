// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use clap::Args;

#[derive(Args, Debug, Clone)]
pub struct SequentialPipelineConfig {
    /// How often to check whether write-ahead logs related to the consistent range can be
    /// pruned.
    #[arg(
        long,
        default_value = "300",
        value_name = "SECONDS",
        value_parser = |s: &str| s.parse().map(Duration::from_secs),
    )]
    pub consistent_pruning_interval: Duration,

    /// How long to wait before honouring reader low watermarks.
    #[arg(
        long,
        default_value = "120",
        value_name = "SECONDS",
        value_parser = |s: &str| s.parse().map(Duration::from_secs),
    )]
    pub pruner_delay: Duration,

    /// Number of checkpoints to delay indexing summary tables for.
    #[clap(long)]
    pub consistent_range: Option<u64>,
}

impl SequentialPipelineConfig {
    const DEFAULT_CONSISTENT_PRUNING_INTERVAL: &'static str = "300";
    const DEFAULT_PRUNER_DELAY: &'static str = "120";

    pub fn default_consistent_pruning_interval() -> Duration {
        Self::DEFAULT_CONSISTENT_PRUNING_INTERVAL
            .parse()
            .map(Duration::from_secs)
            .unwrap()
    }

    pub fn default_pruner_delay() -> Duration {
        Self::DEFAULT_PRUNER_DELAY
            .parse()
            .map(Duration::from_secs)
            .unwrap()
    }
}
