// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod init;
mod setup;

use anyhow::Result;
use clap::Parser;
use init::ProjectType;
use setup::ensure_gcloud;
use setup::ensure_pulumi_setup;

#[derive(Parser, Debug, Clone)]
pub struct PulumiArgs {
    #[command(subcommand)]
    action: PulumiAction,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum PulumiAction {
    /// initialize a new pulumi project
    #[command(name = "init", aliases=["i"])]
    InitProject {
        /// subcommand for project type
        #[command(subcommand)]
        project_type: ProjectType,

        /// use GCP KMS as encryption provider
        #[arg(short, long, group = "feature")]
        kms: bool,

        /// the name of the project to be created
        #[arg(long, aliases = ["name"])]
        project_name: Option<String>,
    },
}

pub fn pulumi_cmd(args: &PulumiArgs) -> Result<()> {
    ensure_pulumi_setup()?;
    match &args.action {
        PulumiAction::InitProject {
            project_type,
            kms,
            project_name,
        } => {
            if *kms {
                ensure_gcloud()?;
            }
            project_type.create_project(kms, project_name.clone())
        }
    }
}
