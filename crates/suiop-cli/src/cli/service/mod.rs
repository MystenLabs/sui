// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod init;

use anyhow::Result;
use clap::Parser;
pub use init::bootstrap_service;
use init::ServiceLanguage;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
pub struct ServiceArgs {
    #[command(subcommand)]
    action: ServiceAction,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum ServiceAction {
    /// initialize new service boilerplate
    #[command(name = "init", aliases=["i"])]
    InitService {
        /// service boilerplate language
        #[arg(value_enum, short, long, default_value_t = ServiceLanguage::Rust)]
        lang: ServiceLanguage,

        /// directory to create service boilerplate in
        #[arg(short, long)]
        path: PathBuf,
    },
}

pub async fn service_cmd(args: &ServiceArgs) -> Result<()> {
    match &args.action {
        ServiceAction::InitService { lang, path } => bootstrap_service(lang, path),
    }
}
