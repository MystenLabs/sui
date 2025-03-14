// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Idfier: Apache-2.0

use anyhow::{Context, Result};
use colored::Colorize;
use tracing::info;

use crate::cli::lib::gcp::log::logs_for_ns;
use crate::cli::pulumi::config::get_pulumi_config;

pub async fn print_logs(project: &str, namespace: &str) -> Result<()> {
    let config = get_pulumi_config()?;
    let gcp_project_id = config
        .config
        .get("gcp:project")
        .context("Failed to get gcp:project from pulumi config")?["value"]
        .as_str()
        .unwrap();
    let cluster_name = config
        .config
        .get(&format!("{project}:cluster_id"))
        .context("Failed to get cluster_id from pulumi config")?["value"]
        .as_str()
        .unwrap();
    info!(
        "Getting logs for project: {}, cluster: {}, project_id: {}, namespace: {}",
        project, cluster_name, gcp_project_id, namespace
    );
    let log_entries = logs_for_ns(gcp_project_id, cluster_name, namespace).await?;

    // // Process and display the logs
    if log_entries.entries.is_empty() {
        println!("No logs found for the service namespace.");
    } else {
        println!("Received {} log entries:", log_entries.entries.len());
        for (i, entry) in log_entries.entries.iter().enumerate() {
            let timestamp = &entry.timestamp;
            let app = entry
                .labels
                .get("k8s-pod/app")
                .cloned()
                .unwrap_or("unknown".to_string());
            let message = entry
                .text_payload
                .clone()
                .unwrap_or("No text payload".to_owned());
            println!(
                "[{}] {} ({}): {}",
                i + 1,
                timestamp.red(),
                app.bright_purple(),
                message
            );
        }
    }
    Ok(())
}
