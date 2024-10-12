// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use url::Url;

#[derive(clap::Parser, Debug, Clone)]
pub struct Args {
    /// First checkpoint to start indexing from.
    #[arg(long, default_value_t = 0)]
    pub start: u64,

    /// Remote Store to fetch CheckpointData from.
    #[arg(long)]
    pub remote_store_url: Url,
}
