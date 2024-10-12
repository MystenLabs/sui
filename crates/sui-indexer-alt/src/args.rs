// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

use url::Url;

#[derive(clap::Parser, Debug, Clone)]
pub struct Args {
    /// First checkpoint to start indexing from.
    #[arg(long, default_value_t = 0)]
    pub start: u64,

    /// Remote Store to fetch CheckpointData from.
    #[arg(long)]
    pub remote_store_url: Url,

    /// Address to serve Prometheus Metrics from.
    #[arg(long, default_value = "0.0.0.0:9184")]
    pub metrics_address: SocketAddr,
}
