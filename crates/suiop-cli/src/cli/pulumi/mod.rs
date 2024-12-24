// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod deps;
mod init;
mod setup;

use std::path::PathBuf;

use anyhow::Result;
use clap::arg;
use clap::Parser;
use clap::ValueEnum;
use deps::update_deps_cmd;
use init::ProjectType;
use setup::ensure_gcloud;
use setup::ensure_pulumi_setup;

fn validate_runtime(s: &str) -> Result<String, String> {
    match s.to_lowercase().as_str() {
        "typescript" | "go" | "python" => Ok(s.to_lowercase()),
        _ => Err(String::from("Runtime must be typescript, go, or python")),
    }
}

#[derive(ValueEnum, PartialEq, Clone, Debug)]
pub enum PulumiProjectRuntime {
    #[clap(alias = "golang")]
    Go,
    #[clap(alias = "ts")]
    Typescript,
}

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

        /// the runtime to use for the project
        #[arg(long, default_value = "go")]
        runtime: PulumiProjectRuntime,
    },
    /// update dependencies for pulumi programs in a given directory
    #[command(name = "update-deps", aliases = ["u"])]
    UpdateDeps {
        /// Starting directory path
        #[arg(required = true)]
        filepath: PathBuf,

        /// Optional runtime filter (typescript, go, python)
        #[arg(value_parser = validate_runtime)]
        runtime: Option<String>,
    },
}

pub fn pulumi_cmd(args: &PulumiArgs) -> Result<()> {
    ensure_pulumi_setup()?;
    match &args.action {
        PulumiAction::InitProject {
            project_type,
            kms,
            project_name,
            runtime,
        } => {
            if *kms {
                ensure_gcloud()?;
            }
            project_type.create_project(kms, project_name.clone(), runtime)
        }
        PulumiAction::UpdateDeps { filepath, runtime } => {
            update_deps_cmd(filepath.clone(), runtime.clone())
        }
    }
}
