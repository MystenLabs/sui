// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use actix_web::{dev::Server, web, App, HttpRequest, HttpServer, Responder};
use anyhow::{anyhow, bail};
use serde::Deserialize;
use tracing::info;
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
    args: Vec<Vec<OsString>>,
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
        let dest = dest.join(repo_name).into_os_string();

        macro_rules! ostr {
            ($arg:expr) => {
                OsString::from($arg)
            };
        }

        let mut args = vec![];
        // Args to clone empty repository
        let cmd_args: Vec<OsString> = vec![
            ostr!("clone"),
            ostr!("-n"),
            ostr!("--depth=1"),
            ostr!("--filter=tree:0"),
            ostr!(&p.repository),
            dest.clone(),
        ];
        args.push(cmd_args);

        // Args to sparse checkout the package set
        let mut cmd_args: Vec<OsString> = vec![
            ostr!("-C"),
            dest.clone(),
            ostr!("sparse-checkout"),
            ostr!("set"),
            ostr!("--no-cone"),
        ];
        let path_args: Vec<OsString> = p.paths.iter().map(OsString::from).collect();
        cmd_args.extend_from_slice(&path_args);
        args.push(cmd_args);

        // Args to checkout the default branch.
        let cmd_args: Vec<OsString> = vec![ostr!("-C"), dest, ostr!("checkout")];
        args.push(cmd_args);

        Ok(Self {
            args,
            repo_url: p.repository.clone(),
        })
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        for args in &self.args {
            let result = Command::new("git").args(args).output().map_err(|_| {
                anyhow!(
                    "Error cloning {} with command `git {:#?}`",
                    self.repo_url,
                    args
                )
            })?;
            if !result.status.success() {
                bail!(
                    "Nonzero exit status when cloning {} with command `git {:#?}`.\
		     Stderr: {:?}",
                    self.repo_url,
                    args,
                    result.stderr
                )
            }
        }
        Ok(())
    }
}

/// Clones repositories and checks out packages as per `config` at the directory `dir`.
pub async fn clone_repositories(config: &Config, dir: &Path) -> anyhow::Result<()> {
    let mut tasks = vec![];
    for p in &config.packages {
        let command = CloneCommand::new(p, dir)?;
        info!("cloning {} to {}", &p.repository, dir.display());
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
