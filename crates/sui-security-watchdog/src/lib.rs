// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use clap::Parser;
use std::path::PathBuf;

mod metrics;
mod pagerduty;
mod query_runner;
pub mod scheduler;

#[derive(Parser, Clone, Debug)]
#[command(
    name = "Sui Security Watchdog",
    about = "Watchdog service to monitor chain data.",
    rename_all = "kebab-case"
)]
pub struct SecurityWatchdogConfig {
    #[arg(long)]
    pub pd_wallet_monitoring_service_id: String,
    #[arg(long)]
    pub config: PathBuf,
    #[arg(long, default_value = None, global = true)]
    pub sf_account_identifier: Option<String>,
    #[arg(long, default_value = None, global = true)]
    pub sf_warehouse: Option<String>,
    #[arg(long, default_value = None, global = true)]
    pub sf_database: Option<String>,
    #[arg(long, default_value = None, global = true)]
    pub sf_schema: Option<String>,
    #[arg(long, default_value = None, global = true)]
    pub sf_username: Option<String>,
    #[arg(long, default_value = None, global = true)]
    pub sf_role: Option<String>,
    /// The url of the metrics client to connect to.
    #[arg(long, default_value = "127.0.0.1", global = true)]
    pub client_metric_host: String,
    /// The port of the metrics client to connect to.
    #[arg(long, default_value = "8081", global = true)]
    pub client_metric_port: u16,
}
