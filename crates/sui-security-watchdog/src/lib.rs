// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use clap::Parser;
use std::path::PathBuf;

mod metrics;
mod pagerduty;
mod query_runner;
pub mod scheduler;

#[derive(Parser, Clone, Debug)]
#[clap(
    name = "Sui Security Watchdog",
    about = "Watchdog service to monitor chain data.",
    rename_all = "kebab-case"
)]
pub struct SecurityWatchdogConfig {
    #[clap(long)]
    pub pd_wallet_monitoring_service_id: String,
    #[clap(long)]
    pub config: PathBuf,
    /// Host serving the ClickHouse HTTPS interface.
    #[clap(long)]
    pub ch_host: String,
    /// Port of the ClickHouse HTTPS interface.
    #[clap(long, default_value = "8443")]
    pub ch_port: u16,
    /// Database that queries in the config file are resolved against.
    #[clap(long, default_value = "ds_prod")]
    pub ch_database: String,
    /// ClickHouse user to authenticate as.
    #[clap(long)]
    pub ch_user: String,
    /// The url of the metrics client to connect to.
    #[clap(long, default_value = "127.0.0.1", global = true)]
    pub client_metric_host: String,
    /// The port of the metrics client to connect to.
    #[clap(long, default_value = "8081", global = true)]
    pub client_metric_port: u16,
}
