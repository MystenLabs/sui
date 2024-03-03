// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::command::is_binary_in_path;
use crate::{command::CommandOptions, run_cmd};
use anyhow::anyhow;
use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info};

fn ensure_docker() -> Result<()> {
    if !is_binary_in_path("docker") {
        error!("docker is not on your local PATH, please install docker");
        return Err(anyhow!("docker not installed"));
    }
    debug!("docker is installed");
    Ok(())
}

async fn start_container() -> Result<()> {
    // TODO: if the container is over some age, replace it
    let container_name = "pulumi-devcontainer";
    let container_exists = run_cmd(
        vec!["docker", "ps", "-a", "--format", "{{.Names}}"],
        Some(CommandOptions::new(true, false)),
    )?;
    if container_exists.stdout.contains(container_name.as_bytes()) {
        info!("container already exists, starting it up");
        run_cmd(
            vec!["docker", "start", container_name],
            Some(CommandOptions::new(true, false)),
        )?;
    } else {
        info!("container does not exist, creating it");
        run_cmd(
            vec![
            // TODO: also include a volume attachment to their pulumi dir
            // docker run -d --name pgo123 --env-file <(env) mysten/pulumi-go bash -c "tail -f /dev/null"
            ],
            Some(CommandOptions::new(true, false)),
        )?;
    }
    Ok(())
}

pub async fn start_shell() -> Result<()> {
    ensure_docker()?;
    start_container().await?;
    // attach_container().await?;
    // after they are done, make sure we clean up the container
    Ok(())
}
