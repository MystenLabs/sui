// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod init;
mod logs;

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
pub use init::bootstrap_service;
use init::ServiceLanguage;
use logs::get_logs;
use std::path::PathBuf;

use crate::{cache_local, get_cached_local, run_cmd};

const PULUMI_NAMESPACE_CACHE_KEY: &str = "pulumi_namespace";

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
    /// View service logs
    #[command(name = "logs", aliases=["l"])]
    ViewLogs {
        /// service namespace to view logs for
        #[arg(short, long, default_value_t=get_pulumi_namespace())]
        namespace: String,
    },
}

fn get_pulumi_namespace_from_cmd() -> String {
    run_cmd(vec!["pulumi", "stack", "output", "namespace"], None)
        .map(|cmd_output| {
            let ns = String::from_utf8(cmd_output.stdout)
                .unwrap()
                .trim()
                .to_string();
            cache_local(PULUMI_NAMESPACE_CACHE_KEY, ns.clone())
                .expect("Failed to cache pulumi namespace");
            ns
        })
        .unwrap_or_else(|_| "default".to_string())
}

fn get_pulumi_namespace() -> String {
    let cached_ns = get_cached_local::<String>(PULUMI_NAMESPACE_CACHE_KEY);

    cached_ns
        .map(|ca| {
            // check if the cached entry is older than 1 day, if so, refresh it
            if ca.metadata.modified().unwrap().elapsed().unwrap().as_secs() > 86400 {
                get_pulumi_namespace_from_cmd()
            } else {
                ca.value
            }
        })
        .unwrap_or_else(|_| get_pulumi_namespace_from_cmd())
}

pub async fn service_cmd(args: &ServiceArgs) -> Result<()> {
    match &args.action {
        ServiceAction::InitService { lang, path } => bootstrap_service(lang, path),
        ServiceAction::ViewLogs { namespace } => {
            println!("namespace: {}", namespace.bright_purple());
            println!("View logs for the entire namespace at {}", format!("https://metrics.sui.io/explore?schemaVersion=1&panes=%7B%22yo7%22:%7B%22datasource%22:%22CU1v-k2Vk%22,%22queries%22:%5B%7B%22refId%22:%22A%22,%22expr%22:%22%7Bnamespace%3D%5C%22{}%5C%22%7D%20%7C%3D%20%60%60%22,%22queryType%22:%22range%22,%22datasource%22:%7B%22type%22:%22loki%22,%22uid%22:%22CU1v-k2Vk%22%7D,%22editorMode%22:%22builder%22%7D%5D,%22range%22:%7B%22from%22:%22now-1h%22,%22to%22:%22now%22%7D%7D%7D&orgId=1", namespace).bold());
            get_logs(namespace).await
        }
    }
}
