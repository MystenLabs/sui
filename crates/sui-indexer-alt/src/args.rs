// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::IndexerConfig;
use std::path::PathBuf;

#[derive(clap::Parser, Debug, Clone)]
pub struct Args {
    #[command(flatten)]
    pub indexer_config: IndexerConfig,

    #[arg(long)]
    pub config_path: Option<PathBuf>,
}
