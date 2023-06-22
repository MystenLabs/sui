// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use actix_web::{dev::Server, web, App, HttpRequest, HttpServer, Responder};
use anyhow::{anyhow, bail};
use serde::Deserialize;
use url::Url;

use move_package::BuildConfig as MoveBuildConfig;
use sui_move::build::resolve_lock_file_path;
use sui_move_build::{BuildConfig, SuiPackageHooks};
use sui_sdk::wallet_context::WalletContext;
use sui_source_validation::{BytecodeSourceVerifier, SourceMode};

#[derive(Deserialize, Debug)]
pub struct Config {
    pub packages: Vec<Packages>,
}

#[derive(Clone, Deserialize, Debug)]
pub struct Packages {
    pub repository: String,
    pub paths: Vec<String>,
}

pub async fn verify_package(
    context: &WalletContext,
    package_path: impl AsRef<Path>,
) -> anyhow::Result<()> {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    let config = resolve_lock_file_path(
        MoveBuildConfig::default(),
        Some(package_path.as_ref().to_path_buf()),
    )
    .unwrap();
    let build_config = BuildConfig {
        config,
        run_bytecode_verifier: false, /* no need to run verifier if code is on-chain */
        print_diags_to_stderr: false,
    };
    let compiled_package = build_config
        .build(package_path.as_ref().to_path_buf())
        .unwrap();

    let client = context.get_client().await?;
    BytecodeSourceVerifier::new(client.read_api())
        .verify_package(
            &compiled_package,
            /* verify_deps */ false,
            SourceMode::Verify,
        )
        .await
        .map_err(anyhow::Error::from)
}

pub fn parse_config(config_path: impl AsRef<Path>) -> anyhow::Result<Config> {
    let contents = fs::read_to_string(config_path)?;
    Ok(toml::from_str(&contents)?)
}

#[derive(Debug)]
/// Represents a sequence of git commands to clone a repository and sparsely checkout Move packages within.
pub struct CloneCommand {
    /// git args
    args: Vec<Vec<String>>,
    /// report repository url in error messages
    repo_url: String,
}

impl CloneCommand {
    pub fn new(p: &Packages, dest: &Path) -> anyhow::Result<CloneCommand> {
        let repo_url = Url::parse(&p.repository)?;
        let Some(components) = repo_url.path_segments().map(|c| c.collect::<Vec<_>>()) else {
	    bail!("Could not discover repository path in url {}", &p.repository)
	};
        let Some(repo_name) = components.last() else {
	    bail!("Could not discover repository name in url {}", &p.repository)
	};
        let dest = dest
            .join(repo_name)
            .into_os_string()
            .into_string()
            .map_err(|_| {
                anyhow!(
                    "Could not create path to clone repsository {}",
                    &p.repository
                )
            })?;

        let mut args = vec![];
        // Args to clone empty repository
        args.push(
            [
                "clone",
                "-n",
                "--depth=1",
                "--filter=tree:0",
                &p.repository,
                &dest,
            ]
            .iter()
            .map(|&s| s.into())
            .collect(),
        );

        // Args to sparse checkout the package set
        let mut prefix: Vec<String> = ["-C", &dest, "sparse-checkout", "set", "--no-cone"]
            .iter()
            .map(|&s| s.into())
            .collect();
        prefix.extend_from_slice(&p.paths);
        args.push(prefix);

        // Args to checkout the default branch.
        args.push(
            ["-C", &dest, "checkout"]
                .iter()
                .map(|&s| s.into())
                .collect(),
        );

        Ok(Self {
            args,
            repo_url: p.repository.clone(),
        })
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        for args in &self.args {
            Command::new("git").args(args).output().map_err(|_| {
                anyhow!(
                    "Error cloning package(s) for {} with command git {:#?}",
                    self.repo_url,
                    args
                )
            })?;
        }
        Ok(())
    }
}

/// Clones repositories and checks out packages as per `config` at the directory `dir`.
pub async fn clone_repositories(config: &Config, dir: &Path) -> anyhow::Result<()> {
    let mut tasks = vec![];
    for p in &config.packages {
        let command = CloneCommand::new(p, dir)?;
        let t = tokio::spawn(async move { command.run().await });
        tasks.push(t);
    }

    for t in tasks {
        t.await.unwrap()?;
    }
    Ok(())
}

pub async fn initialize(
    context: &WalletContext,
    config: &Config,
    dir: &Path,
) -> anyhow::Result<()> {
    clone_repositories(config, dir).await?;
    verify_packages(context, vec![]).await?;
    Ok(())
}

pub async fn verify_packages(
    context: &WalletContext,
    package_paths: Vec<PathBuf>,
) -> anyhow::Result<()> {
    for p in package_paths {
        verify_package(context, p).await?
    }
    Ok(())
}

pub fn serve() -> anyhow::Result<Server> {
    Ok(
        HttpServer::new(|| App::new().route("/api", web::get().to(api_route)))
            .bind("0.0.0.0:8000")?
            .run(),
    )
}

async fn api_route(_request: HttpRequest) -> impl Responder {
    "{\"source\": \"code\"}"
}
