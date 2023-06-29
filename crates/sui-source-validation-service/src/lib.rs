// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{ffi::OsString, fs, path::Path, process::Command};

use anyhow::{anyhow, bail};
use axum::routing::{get, IntoMakeService};
use axum::{Router, Server};
use hyper::server::conn::AddrIncoming;
use serde::Deserialize;
use std::net::TcpListener;
use sui_sdk::SuiClient;
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
    pub branch: String,
    pub paths: Vec<String>,
}

pub async fn verify_package(
    client: &SuiClient,
    package_path: impl AsRef<Path>,
) -> anyhow::Result<()> {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    let config = resolve_lock_file_path(
        MoveBuildConfig::default(),
        Some(package_path.as_ref().to_path_buf()),
    )?;
    let build_config = BuildConfig {
        config,
        run_bytecode_verifier: false, /* no need to run verifier if code is on-chain */
        print_diags_to_stderr: false,
        lint: false,
    };
    let compiled_package = build_config
        .build(package_path.as_ref().to_path_buf())
        .unwrap();

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

pub fn repo_name_from_url(url: &str) -> anyhow::Result<String> {
    let repo_url = Url::parse(url)?;
    let Some(mut components) = repo_url.path_segments() else {
	    bail!("Could not discover repository path in url {url}")
	};
    let Some(repo_name) = components.next_back() else {
	    bail!("Could not discover repository name in url {url}")
    };

    Ok(repo_name.to_string())
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
        let repo_name = repo_name_from_url(&p.repository)?;
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
            ostr!("--no-checkout"),
            ostr!("--depth=1"), // implies --single-branch
            ostr!("--filter=tree:0"),
            ostr!(format!("--branch={}", p.branch)),
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
                    "Nonzero exit status when cloning {} with command `git {:#?}`. \
		     Stderr: {}",
                    self.repo_url,
                    args,
                    String::from_utf8_lossy(&result.stderr)
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
    verify_packages(context, config, dir).await?;
    Ok(())
}

pub async fn verify_packages(
    context: &WalletContext,
    config: &Config,
    dir: &Path,
) -> anyhow::Result<()> {
    let mut tasks = vec![];
    for p in &config.packages {
        let repo_name = repo_name_from_url(&p.repository)?;
        let packages_dir = dir.join(repo_name);
        for p in &p.paths {
            let package_path = packages_dir.join(p).clone();
            let client = context.get_client().await?;
            info!("verifying {p}");
            let t = tokio::spawn(async move { verify_package(&client, package_path).await });
            tasks.push(t)
        }
    }

    for t in tasks {
        t.await.unwrap()?;
    }
    Ok(())
}

pub fn serve() -> anyhow::Result<Server<AddrIncoming, IntoMakeService<Router>>> {
    let app = Router::new().route("/api", get(api_route));
    let listener = TcpListener::bind("0.0.0.0:8000")?;
    Ok(Server::from_tcp(listener)?.serve(app.into_make_service()))
}

async fn api_route() -> &'static str {
    "{\"source\": \"code\"}"
}
