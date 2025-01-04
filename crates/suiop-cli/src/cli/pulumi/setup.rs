// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{command::CommandOptions, run_cmd};
use anyhow::anyhow;
use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info};

const PULUMI: &str = "pulumi";
const GO: &str = "go";

fn get_current_time() -> std::time::Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
}

fn ensure_pulumi_authed() -> Result<()> {
    let home = env::var("HOME").unwrap();
    let creds_filepath = PathBuf::from(format!("{}/.pulumi/credentials.json", home));
    if !creds_filepath.exists() {
        info!(
            "{}",
            "not logged into pulumi, trying to log you in...".red()
        );
        run_cmd(
            vec!["bash", "-c", "pulumi login"],
            Some(CommandOptions::new(true, false)),
        )?;
        info!("logged in successfully")
    } else {
        debug!("authed");
    }
    Ok(())
}

fn is_binary_in_path(binary: &str) -> bool {
    if let Ok(path) = env::var("PATH") {
        path.split(':').any(|p| {
            let p_str = format!("{}/{}", p, binary);
            fs::metadata(p_str).is_ok()
        })
    } else {
        false
    }
}

fn ensure_prereqs() -> Result<()> {
    let binaries = [PULUMI, GO];
    let mut missing_binaries = vec![];
    for binary in binaries.iter() {
        if !is_binary_in_path(binary) {
            missing_binaries.push(binary)
        }
    }

    let install_guide = HashMap::from([
        (PULUMI, "https://www.pulumi.com/docs/install/"),
        (GO, "`brew install go`"),
    ]);

    if missing_binaries.is_empty() {
        info!("All prerequisites are installed");
        Ok(())
    } else {
        for missing in missing_binaries.iter() {
            error!(
                "Missing prerequisite: {} - Please follow {} to install",
                (*missing).bright_purple(),
                install_guide[*missing]
            );
        }
        Err(anyhow!("Missing prerequisites"))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct AccountInfo<'a> {
    account: &'a str,
    status: &'a str,
}
fn ensure_gcloud_logged_in() -> Result<()> {
    let output = run_cmd(vec!["bash", "-c", "gcloud auth list --format json"], None)?;
    let stdout_str = String::from_utf8(output.stdout)?;
    let accounts: Vec<AccountInfo> = serde_json::from_str(&stdout_str)?;
    for account in accounts {
        let name = account.account;
        if account.status == "ACTIVE" {
            if str::ends_with(name, "@mystenlabs.com") {
                return Ok(());
            } else {
                error!(
                    "Please select your @mystenlabs.com profile: {}",
                    "gcloud config set account `ACCOUNT`".bright_yellow()
                );
                return Err(anyhow!("Incorret account selected."));
            }
        }
    }
    error!(
        "No gcloud credentials found! Please log into your @mystenlabs.com account via: {}",
        "gcloud auth login".bright_yellow()
    );
    Err(anyhow!("Missing gcloud credentials"))
}

fn ensure_gcloud_adc_logged_in() -> Result<()> {
    match run_cmd(
        vec![
            "bash",
            "-c",
            "gcloud auth application-default print-access-token",
        ],
        None,
    ) {
        Ok(_) => Ok(()),
        Err(_) => {
            error!(
                "No gcloud ADC (Application Default Credentials) found! Please log into your @mystenlabs.com account via: {}",
                "gcloud auth application-default login".bright_yellow()
            );
            Err(anyhow!("Missing gcloud ADC credentials"))
        }
    }
}

pub fn ensure_gcloud() -> Result<()> {
    let is_gcloud_cli_installed = is_binary_in_path("gcloud");
    let gcp_proj_id: String = env::var("GCP_PROJ_ID")
        .expect("Missing GCP_PROJ_ID env var. Please set it to the desired GCP project ID");
    if is_gcloud_cli_installed {
        ensure_gcloud_logged_in()?;
        ensure_gcloud_adc_logged_in()?;
        run_cmd(
            vec![
                "bash",
                "-c",
                &format!("gcloud config set project {}", gcp_proj_id),
            ],
            Some(CommandOptions::new(true, false)),
        )?;
        Ok(())
    } else {
        error!(
            "gcloud CLI is not installed, please follow the installation guide: {}",
            "https://cloud.google.com/sdk/docs/install-sdk"
        );
        Err(anyhow!("Missing gcloud CLI"))
    }
}

pub fn ensure_pulumi_setup() -> Result<()> {
    let home = env::var("HOME").unwrap();
    // check for marker file
    let setup_marker = PathBuf::from(format!("{}/.suiop/pulumi_setup", home));
    if setup_marker.exists() {
        // our work here is done, it's set up!
        Ok(())
    } else {
        ensure_prereqs()?; // make sure golang and pulumi are installed
        ensure_pulumi_authed()?;
        // create marker file
        let prefix = setup_marker.parent().unwrap();
        fs::create_dir_all(prefix).unwrap();
        let timestamp = get_current_time();
        fs::write(&setup_marker, timestamp.as_secs().to_string())
            .context("failed to write setup file")?;
        Ok(())
    }
}
