// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

use crate::ingestion::IngestionConfig;

#[derive(clap::Parser, Debug, Clone)]
pub struct Args {
    #[command(flatten)]
    pub ingestion: IngestionConfig,

    /// Address to serve Prometheus Metrics from.
    #[arg(long, default_value = "0.0.0.0:9184")]
    pub metrics_address: SocketAddr,
}
