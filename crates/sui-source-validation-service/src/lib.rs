// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::{ffi::OsString, fs, path::Path, process::Command};

use anyhow::{anyhow, bail};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, IntoMakeService};
use axum::{Json, Router, Server};
use hyper::http::Method;
use hyper::server::conn::AddrIncoming;
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use std::net::TcpListener;
use sui_sdk::SuiClient;
use tower::ServiceBuilder;
use tracing::info;
use url::Url;

use move_compiler::compiled_unit::CompiledUnitEnum;
use move_core_types::account_address::AccountAddress;
use move_package::BuildConfig as MoveBuildConfig;
use move_symbol_pool::Symbol;
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

#[derive(Debug)]
pub struct SourceInfo {
    pub path: PathBuf,
    // Is Some when content is hydrated from disk.
    pub source: Option<String>,
}

/// Map (package address, module name) tuples to verified source info.
type SourceLookup = BTreeMap<(AccountAddress, Symbol), SourceInfo>;

pub async fn verify_package(
    client: &SuiClient,
    package_path: impl AsRef<Path>,
) -> anyhow::Result<SourceLookup> {
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
    let compiled_package = build_config.build(package_path.as_ref().to_path_buf())?;

    BytecodeSourceVerifier::new(client.read_api())
        .verify_package(
            &compiled_package,
            /* verify_deps */ false,
            SourceMode::Verify,
        )
        .await?;

    let mut map = SourceLookup::new();
    let Ok(address) = compiled_package.published_at.as_ref().map(|id| **id) else { bail!("could not resolve published-at field in package manifest")};
    for v in &compiled_package.package.root_compiled_units {
        match v.unit {
            CompiledUnitEnum::Module(ref m) => {
                let path = v.source_path.to_path_buf();
                let source = Some(fs::read_to_string(path.as_path())?);
                map.insert((address, m.name), SourceInfo { path, source })
            }
            CompiledUnitEnum::Script(ref m) => {
                let path = v.source_path.to_path_buf();
                let source = Some(fs::read_to_string(path.as_path())?);
                map.insert((address, m.name), SourceInfo { path, source })
            }
        };
    }
    Ok(map)
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
) -> anyhow::Result<SourceLookup> {
    clone_repositories(config, dir).await?;
    verify_packages(context, config, dir).await
}

pub async fn verify_packages(
    context: &WalletContext,
    config: &Config,
    dir: &Path,
) -> anyhow::Result<SourceLookup> {
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

    let mut lookup = BTreeMap::new();
    for t in tasks {
        let new_lookup = t.await.unwrap()?;
        lookup.extend(new_lookup);
    }
    Ok(lookup)
}

pub struct AppState {
    pub sources: SourceLookup,
}

pub fn serve(app_state: AppState) -> anyhow::Result<Server<AddrIncoming, IntoMakeService<Router>>> {
    let app = Router::new()
        .route("/api", get(api_route).with_state(Arc::new(app_state)))
        .layer(
            ServiceBuilder::new().layer(
                tower_http::cors::CorsLayer::new()
                    .allow_methods([Method::GET])
                    .allow_origin(tower_http::cors::Any),
            ),
        );
    let listener = TcpListener::bind("0.0.0.0:8000")?;
    Ok(Server::from_tcp(listener)?.serve(app.into_make_service()))
}

#[derive(Deserialize)]
pub struct Request {
    address: String,
    module: String,
}

#[derive(Serialize, Deserialize)]
pub struct SourceResponse {
    pub source: String,
}

#[derive(Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

async fn api_route(
    State(app_state): State<Arc<AppState>>,
    Query(Request { address, module }): Query<Request>,
) -> impl IntoResponse {
    let symbol = Symbol::from(module);
    let Ok(address) = AccountAddress::from_hex_literal(&address) else {
	let error = format!("Invalid hex address {address}");
	return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error }).into_response())
    };
    let Some(SourceInfo {source : Some(source), ..}) = app_state.sources.get(&(address, symbol)) else {
	let error = format!("No source found for {symbol} at address {address}" );
	return (StatusCode::NOT_FOUND, Json(ErrorResponse { error }).into_response())
    };
    (
        StatusCode::OK,
        Json(SourceResponse {
            source: source.to_owned(),
        })
        .into_response(),
    )
}
