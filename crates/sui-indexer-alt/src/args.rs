// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

use crate::{db::DbConfig, handlers::CommitterConfig, ingestion::IngestionConfig};

#[derive(clap::Parser, Debug, Clone)]
pub struct Args {
    #[command(flatten)]
    pub ingestion: IngestionConfig,

    #[command(flatten)]
    pub db: DbConfig,

    #[command(flatten)]
    pub committer: CommitterConfig,

    /// Address to serve Prometheus Metrics from.
    #[arg(long, default_value = "0.0.0.0:9184")]
    pub metrics_address: SocketAddr,
}
