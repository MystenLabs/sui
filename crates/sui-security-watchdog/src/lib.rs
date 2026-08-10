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
    /// Host serving the ClickHouse HTTP interface.
    #[clap(long, global = true)]
    pub ch_host: String,
    /// Port of the ClickHouse HTTP interface. 8443 is TLS, 8123 is plain HTTP.
    #[clap(long, default_value = "8443", global = true)]
    pub ch_port: u16,
    /// Database that queries in the config file are resolved against.
    #[clap(long, default_value = "ds_prod", global = true)]
    pub ch_database: String,
    /// ClickHouse user to authenticate as.
    #[clap(long, global = true)]
    pub ch_user: String,
    /// Connect over plain HTTP. Only for a local ClickHouse; ClickHouse Cloud requires TLS.
    #[clap(long, default_value_t = false, global = true)]
    pub ch_no_tls: bool,
    /// Per-query timeout. Jobs run on a cron, so a hung query must not outlive its interval.
    #[clap(long, default_value = "60", global = true)]
    pub ch_query_timeout_secs: u64,
    /// The url of the metrics client to connect to.
    #[clap(long, default_value = "127.0.0.1", global = true)]
    pub client_metric_host: String,
    /// The port of the metrics client to connect to.
    #[clap(long, default_value = "8081", global = true)]
    pub client_metric_port: u16,
}
