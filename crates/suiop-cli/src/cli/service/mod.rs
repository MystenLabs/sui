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
use std::{
    env::current_dir,
    path::{Path, PathBuf},
};
use tracing::debug;

use crate::{cache_local, get_cached_local, run_cmd};

const PULUMI_WORKSPACE_FILE_CACHE_KEY: &str = "pulumi_workspace_file";

#[derive(Parser, Debug, Clone)]
pub struct LogsArgs {}

#[derive(Parser, Debug, Clone)]
pub struct ServiceArgs {
    #[command(subcommand)]
    pub action: ServiceAction,
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
    ViewLogs,
}

fn get_ns_cache_key(stack: &str) -> String {
    format!("pulumi_namespace.{}", stack)
}

fn get_pulumi_namespace_from_cmd(stack: &str) -> String {
    run_cmd(vec!["pulumi", "stack", "output", "namespace"], None)
        .map(|cmd_output| {
            let ns = String::from_utf8(cmd_output.stdout)
                .unwrap()
                .trim()
                .to_string();
            cache_local(&get_ns_cache_key(stack), ns.clone())
                .expect("Failed to cache pulumi namespace");
            ns
        })
        .unwrap_or_else(|_| "default".to_string())
}

fn get_pulumi_namespace(project_name: &str) -> String {
    let stack = get_pulumi_stack(project_name);
    let cached_ns = get_cached_local::<String>(&get_ns_cache_key(&stack));

    cached_ns
        .map(|ca| {
            // check if the cached entry is older than 1 day, if so, refresh it
            if ca.is_expired() {
                get_pulumi_namespace_from_cmd(&stack)
            } else {
                ca.value
            }
        })
        .unwrap_or_else(|_| get_pulumi_namespace_from_cmd(&stack))
}

fn find_workspace_file(project_name: &str) -> PathBuf {
    // Try to find the workspace file in ~/.pulumi/workspaces
    let home = std::env::var("HOME").expect("HOME environment variable not set");
    let workspace_path = Path::new(&home).join(".pulumi").join("workspaces");
    let dir_entries =
        std::fs::read_dir(&workspace_path).expect("Failed to read workspace directory");
    let dir_entries2 =
        std::fs::read_dir(workspace_path).expect("Failed to read workspace directory");
    for dir in dir_entries2 {
        debug!("entries: {:?}", dir.unwrap().path());
    }
    let workspace_file = dir_entries.flatten().find(|entry| {
        let filename = entry.file_name();
        let captures = regex::Regex::new(r"^(.*?)-[0-9a-fA-F-]+-workspace\.json$")
            .unwrap()
            .captures_iter(filename.to_str().unwrap())
            .next()
            .expect("No captures found");
        captures.get(1).expect("No stack name found").as_str() == project_name
    });
    // read the workspace file and extract the stack name
    let workspace_file = workspace_file.expect("No workspace file found");
    cache_local(
        PULUMI_WORKSPACE_FILE_CACHE_KEY,
        workspace_file.path().to_str().unwrap().to_string(),
    )
    .expect("Failed to cache workspace file");
    workspace_file.path()
}

fn get_pulumi_stack(project_name: &str) -> String {
    let workspace_file = get_cached_local::<String>(PULUMI_WORKSPACE_FILE_CACHE_KEY)
        .map(|cached_workspace_file| {
            if cached_workspace_file.is_expired() {
                find_workspace_file(project_name)
            } else {
                PathBuf::from(cached_workspace_file.value)
            }
        })
        .unwrap_or_else(|_| find_workspace_file(project_name));
    let contents = std::fs::read_to_string(workspace_file).expect("Failed to read workspace file");
    let json: serde_json::Value =
        serde_json::from_str(&contents).expect("Failed to parse workspace file as JSON");
    let stack = json["stack"]
        .as_str()
        .expect("Stack name should be a string");

    debug!("stack: {}", stack);
    stack.to_string()
}

pub async fn service_cmd(args: &ServiceArgs) -> Result<()> {
    match &args.action {
        ServiceAction::InitService { lang, path } => bootstrap_service(lang, path),
        ServiceAction::ViewLogs => {
            // get the project name if not provided by the user
            // the current top-level dir is the project name
            let project_name = current_dir()
                .expect("Failed to get current dir")
                .file_name()
                .expect("Failed to get current dir")
                .to_string_lossy()
                .to_string();

            let namespace = get_pulumi_namespace(&project_name);
            println!("namespace: {}", namespace.bright_purple());
            println!(
                "View logs for the entire namespace at {}",
                format!(
                    "https://metrics.sui.io/explore?schemaVersion=1&panes=%7B%22yo7%22:%7B%22datasource%22:%22CU1v-k2Vk%22,%22queries%22:%5B%7B%22refId%22:%22A%22,%22expr%22:%22%7Bnamespace%3D%5C%22{}%5C%22%7D%20%7C%3D%20%60%60%22,%22queryType%22:%22range%22,%22datasource%22:%7B%22type%22:%22loki%22,%22uid%22:%22CU1v-k2Vk%22%7D,%22editorMode%22:%22builder%22%7D%5D,%22range%22:%7B%22from%22:%22now-1h%22,%22to%22:%22now%22%7D%7D%7D&orgId=1",
                    namespace
                )
                .bold()
            );
            let stack = get_pulumi_stack(&project_name);
            get_logs(&stack, &namespace).await
        }
    }
}
