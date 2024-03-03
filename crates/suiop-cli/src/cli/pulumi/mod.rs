// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod init;
mod setup;
mod shell;

use anyhow::Result;
use clap::Parser;
use init::ProjectType;
use setup::ensure_gcloud;
use setup::ensure_setup;
use shell::start_shell;

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
        /// initialize app project
        #[arg(short, long, group = "type")]
        app: bool,

        /// initialize barebones project (default)
        #[arg(short, long, group = "type")]
        basic: bool,

        /// initialize cronjob project
        #[arg(short, long, group = "type")]
        cronjob: bool,

        /// use GCP KMS as encryption provider
        #[arg(short, long, group = "feature")]
        kms: bool,

        /// the name of the project to be created
        #[arg(long, aliases = ["name"])]
        project_name: Option<String>,
    },
    /// create and attach to a new devcontainer shell
    ///
    /// the new environment will include everything necessary to use pulumi
    #[command(name = "shell", aliases=["sh"])]
    Shell,
}

pub async fn pulumi_cmd(args: &PulumiArgs) -> Result<()> {
    ensure_setup()?;
    match &args.action {
        PulumiAction::InitProject {
            app,
            basic,
            cronjob,
            kms,
            project_name,
        } => {
            if *kms {
                ensure_gcloud()?;
            }
            let project_type = match (app, basic, cronjob) {
                (true, false, false) => ProjectType::App,
                (false, false, true) => ProjectType::CronJob,
                (_, _, _) => ProjectType::Basic,
            };
            project_type.create_project(kms, project_name.clone())
        }
        PulumiAction::Shell => {
            let _ = start_shell().await?;
            Ok(())
        }
    }
}
