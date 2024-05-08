// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs::create_dir_all;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use clap::{Parser, ValueEnum};
use include_dir::include_dir;
use include_dir::Dir;
use tracing::info;

/// include the dockerfile templates dir in the binary
static DOCKERFILES_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/dockerfile-templates");
const DEFAULT_PATH: &str = "docker/";

#[derive(Parser, Debug, Clone)]
pub struct DockerArgs {
    #[command(subcommand)]
    action: DockerAction,
}

#[derive(Parser, Debug, Clone, ValueEnum)]
pub enum DockerLanguageRuntime {
    /// Use the Rust template
    Rust,
    /// Use the Typescript template
    Ts,
}
#[derive(clap::Subcommand, Debug, Clone)]
pub enum DockerAction {
    /// Generate a new dockerfile for an existing codebase
    #[command(name = "generate", aliases=["g"])]
    Generate {
        /// language runtime to use
        #[arg(short, long, value_enum)]
        runtime: DockerLanguageRuntime,
        /// dir to put the generated dockerfile in
        #[arg(short, long, default_value = DEFAULT_PATH)]
        path: PathBuf,
    },
}

pub async fn docker_cmd(args: &DockerArgs) -> Result<()> {
    match &args.action {
        DockerAction::Generate { path, runtime } => match runtime {
            DockerLanguageRuntime::Rust => {
                todo!("Generating Dockerfile for Rust");
            }
            DockerLanguageRuntime::Ts => {
                info!("Generating Dockerfile for Typescript in {:?}", path);
                generate_ts_dockerfile(path)
            }
        },
    }
}

fn generate_ts_dockerfile(path: &Path) -> Result<()> {
    let dockerfile_template = DOCKERFILES_DIR
        .get_file("typescript.Dockerfile")
        .context("getting cargo toml file from boilerplate")?;
    create_dir_all(path).context("creating dockerfile dir")?;
    let mut main_file = File::create(path.join("Dockerfile")).context("creating dockefile")?;
    main_file
        .write_all(dockerfile_template.contents())
        .context("writing dockerfile")?;
    Ok(())
}
