// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::IndexerConfig;

#[derive(clap::Parser, Debug, Clone)]
pub struct Args {
    #[command(flatten)]
    pub indexer_config: IndexerConfig,
}
