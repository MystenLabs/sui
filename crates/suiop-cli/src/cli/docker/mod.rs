// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use tracing::info;

#[derive(Parser, Debug, Clone)]
pub struct DockerArgs {
    #[command(subcommand)]
    action: DockerAction,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum DockerAction {
    #[command(name = "generate", aliases=["g"])]
    Generate {},
}

pub async fn docker_cmd(args: &DockerArgs) -> Result<()> {
    match &args.action {
        DockerAction::Generate {} => {
            info!("Generating Dockerfile");
            Ok(())
        }
    }
}
