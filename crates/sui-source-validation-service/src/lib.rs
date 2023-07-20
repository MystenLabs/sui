// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::net::TcpListener;
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
use hyper::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;
use tracing::{debug, info};
use url::Url;

use move_compiler::compiled_unit::CompiledUnitEnum;
use move_core_types::account_address::AccountAddress;
use move_package::BuildConfig as MoveBuildConfig;
use move_symbol_pool::Symbol;
use sui_move::build::resolve_lock_file_path;
use sui_move_build::{BuildConfig, SuiPackageHooks};
use sui_sdk::SuiClientBuilder;
use sui_source_validation::{BytecodeSourceVerifier, SourceMode};

pub const HOST_PORT_ENV: &str = "HOST_PORT";
pub const SUI_SOURCE_VALIDATION_VERSION_HEADER: &str = "X-Sui-Source-Validation-Version";
pub const SUI_SOURCE_VALIDATION_VERSION: &str = "0.1";

pub const MAINNET_URL: &str = "https://fullnode.mainnet.sui.io:443";
pub const TESTNET_URL: &str = "https://fullnode.testnet.sui.io:443";
pub const DEVNET_URL: &str = "https://fullnode.devnet.sui.io:443";
pub const LOCALNET_URL: &str = "http://127.0.0.1:9000";

pub fn host_port() -> String {
    match option_env!("HOST_PORT") {
        Some(v) => v.to_string(),
        None => String::from("0.0.0.0:8000"),
    }
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub packages: Vec<PackageSources>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "source", content = "values")]
pub enum PackageSources {
    Repository(RepositorySource),
    Directory(DirectorySource),
}

#[derive(Clone, Deserialize, Debug)]
pub struct RepositorySource {
    pub repository: String,
    pub branch: String,
    pub paths: Vec<String>,
    pub network: Option<Network>,
}

#[derive(Clone, Deserialize, Debug)]
pub struct DirectorySource {
    pub paths: Vec<String>,
    pub network: Option<Network>,
}

#[derive(Debug)]
pub struct SourceInfo {
    pub path: PathBuf,
    // Is Some when content is hydrated from disk.
    pub source: Option<String>,
}

#[derive(Eq, PartialEq, Clone, Default, Deserialize, Debug, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    #[default]
    #[serde(alias = "Mainnet")]
    Mainnet,
    #[serde(alias = "Testnet")]
    Testnet,
    #[serde(alias = "Devnet")]
    Devnet,
    #[serde(alias = "Localnet")]
    Localnet,
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Network::Mainnet => "mainnet",
                Network::Testnet => "testnet",
                Network::Devnet => "devnet",
                Network::Localnet => "localnet",
            }
        )
    }
}

/// Map (package address, module name) tuples to verified source info.
type SourceLookup = BTreeMap<(AccountAddress, Symbol), SourceInfo>;
/// Top-level lookup that maps network to sources for corresponding on-chain networks.
pub type NetworkLookup = BTreeMap<Network, SourceLookup>;

pub async fn verify_package(
    network: &Network,
    package_path: impl AsRef<Path>,
) -> anyhow::Result<(Network, SourceLookup)> {
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

    let network_url = match network {
        Network::Mainnet => MAINNET_URL,
        Network::Testnet => TESTNET_URL,
        Network::Devnet => DEVNET_URL,
        Network::Localnet => LOCALNET_URL,
    };
    let client = SuiClientBuilder::default().build(network_url).await?;
    BytecodeSourceVerifier::new(client.read_api())
        .verify_package(
            &compiled_package,
            /* verify_deps */ false,
            SourceMode::Verify,
        )
        .await?;

    let mut map = SourceLookup::new();
    let address = compiled_package
        .published_at
        .as_ref()
        .map(|id| **id)
        .map_err(|_| anyhow!("could not resolve published-at field in package manifest"))?;
    info!("verifying {} at {address}", package_path.as_ref().display());
    for v in &compiled_package.package.root_compiled_units {
        let path = v.source_path.to_path_buf();
        let source = Some(fs::read_to_string(path.as_path())?);
        match v.unit {
            CompiledUnitEnum::Module(ref m) => {
                map.insert((address, m.name), SourceInfo { path, source })
            }
            CompiledUnitEnum::Script(ref m) => {
                map.insert((address, m.name), SourceInfo { path, source })
            }
        };
    }
    Ok((network.clone(), map))
}

pub fn parse_config(config_path: impl AsRef<Path>) -> anyhow::Result<Config> {
    let contents = fs::read_to_string(config_path)?;
    Ok(toml::from_str(&contents)?)
}

pub fn repo_name_from_url(url: &str) -> anyhow::Result<String> {
    let repo_url = Url::parse(url)?;
    let mut components = repo_url
        .path_segments()
        .ok_or_else(|| anyhow!("Could not discover repository path in url {url}"))?;
    let repo_name = components
        .next_back()
        .ok_or_else(|| anyhow!("Could not discover repository name in url {url}"))?;
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
    pub fn new(p: &RepositorySource, dest: &Path) -> anyhow::Result<CloneCommand> {
        let repo_name = repo_name_from_url(&p.repository)?;
        let network = p.network.clone().unwrap_or_default().to_string();
        let dest = dest.join(network).join(repo_name).into_os_string();

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
pub async fn clone_repositories(repos: Vec<&RepositorySource>, dir: &Path) -> anyhow::Result<()> {
    let mut tasks = vec![];
    for p in &repos {
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

pub async fn initialize(config: &Config, dir: &Path) -> anyhow::Result<NetworkLookup> {
    let mut repos = vec![];
    for s in &config.packages {
        match s {
            PackageSources::Repository(r) => repos.push(r),
            PackageSources::Directory(_) => (), /* skip cloning */
        }
    }
    clone_repositories(repos, dir).await?;
    verify_packages(config, dir).await
}

pub async fn verify_packages(config: &Config, dir: &Path) -> anyhow::Result<NetworkLookup> {
    let mut tasks = vec![];
    for p in &config.packages {
        match p {
            PackageSources::Repository(r) => {
                let repo_name = repo_name_from_url(&r.repository)?;
                let network_name = r.network.clone().unwrap_or_default().to_string();
                let packages_dir = dir.join(network_name).join(repo_name);
                for p in &r.paths {
                    let package_path = packages_dir.join(p).clone();
                    let network = r.network.clone().unwrap_or_default();
                    let t =
                        tokio::spawn(async move { verify_package(&network, package_path).await });
                    tasks.push(t)
                }
            }
            PackageSources::Directory(packages_dir) => {
                for p in &packages_dir.paths {
                    let package_path = PathBuf::from(p);
                    let network = packages_dir.network.clone().unwrap_or_default();
                    let t =
                        tokio::spawn(async move { verify_package(&network, package_path).await });
                    tasks.push(t)
                }
            }
        }
    }

    let mut mainnet_lookup = SourceLookup::new();
    let mut testnet_lookup = SourceLookup::new();
    let mut devnet_lookup = SourceLookup::new();
    let mut localnet_lookup = SourceLookup::new();
    for t in tasks {
        let (network, new_lookup) = t.await.unwrap()?;
        match network {
            Network::Mainnet => mainnet_lookup.extend(new_lookup),
            Network::Testnet => testnet_lookup.extend(new_lookup),
            Network::Devnet => devnet_lookup.extend(new_lookup),
            Network::Localnet => localnet_lookup.extend(new_lookup),
        }
    }
    let mut lookup = NetworkLookup::new();
    lookup.insert(Network::Mainnet, mainnet_lookup);
    lookup.insert(Network::Testnet, testnet_lookup);
    lookup.insert(Network::Devnet, devnet_lookup);
    lookup.insert(Network::Localnet, localnet_lookup);
    Ok(lookup)
}

pub struct AppState {
    pub sources: NetworkLookup,
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
    let listener = TcpListener::bind(host_port())?;
    Ok(Server::from_tcp(listener)?.serve(app.into_make_service()))
}

#[derive(Deserialize)]
pub struct Request {
    #[serde(default)]
    network: Network,
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
    headers: HeaderMap,
    State(app_state): State<Arc<AppState>>,
    Query(Request {
        network,
        address,
        module,
    }): Query<Request>,
) -> impl IntoResponse {
    debug!("request network={network}&address={address}&module={module}");
    let version = headers
        .get(SUI_SOURCE_VALIDATION_VERSION_HEADER)
        .as_ref()
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let mut headers = HeaderMap::new();
    headers.insert(
        SUI_SOURCE_VALIDATION_VERSION_HEADER,
        SUI_SOURCE_VALIDATION_VERSION.parse().unwrap(),
    );

    match version {
        Some(v) if v != SUI_SOURCE_VALIDATION_VERSION => {
            let error = format!(
                "Unsupported version '{v}' specified in header \
		 {SUI_SOURCE_VALIDATION_VERSION_HEADER}"
            );
            return (
                StatusCode::BAD_REQUEST,
                headers,
                Json(ErrorResponse { error }).into_response(),
            );
        }
        Some(_) => (),
        None => info!("No version set, using {SUI_SOURCE_VALIDATION_VERSION}"),
    };

    let symbol = Symbol::from(module);
    let Ok(address) = AccountAddress::from_hex_literal(&address) else {
	let error = format!("Invalid hex address {address}");
	return (
	    StatusCode::BAD_REQUEST,
	    headers,
	    Json(ErrorResponse { error }).into_response()
	)
    };

    let source_result = app_state
        .sources
        .get(&network)
        .and_then(|l| l.get(&(address, symbol)));
    if let Some(SourceInfo {
        source: Some(source),
        ..
    }) = source_result
    {
        (
            StatusCode::OK,
            headers,
            Json(SourceResponse {
                source: source.to_owned(),
            })
            .into_response(),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            headers,
            Json(ErrorResponse {
                error: format!(
                    "No source found for {symbol} at address {address} on network {network}"
                ),
            })
            .into_response(),
        )
    }
}
