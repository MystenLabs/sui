// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::middleware::{self, Next};
use std::collections::BTreeMap;
use std::fmt;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use std::{ffi::OsString, fs, path::Path, process::Command};
use tokio::sync::oneshot::Sender;

use anyhow::{anyhow, bail};
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Extension;
use axum::{Json, Router};
use hyper::http::{HeaderName, HeaderValue, Method};
use hyper::{HeaderMap, StatusCode};
use mysten_metrics::RegistryService;
use prometheus::{register_int_counter_with_registry, IntCounter, Registry};
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;
use tracing::{debug, info};
use url::Url;

use move_core_types::account_address::AccountAddress;
use move_package::{BuildConfig as MoveBuildConfig, LintFlag};
use move_symbol_pool::Symbol;
use sui_move::manage_package::resolve_lock_file_path;
use sui_move_build::{BuildConfig, SuiPackageHooks};
use sui_sdk::rpc_types::SuiTransactionBlockEffects;
use sui_sdk::types::base_types::ObjectID;
use sui_sdk::SuiClientBuilder;
use sui_source_validation::{BytecodeSourceVerifier, ValidationMode};

pub const HOST_PORT_ENV: &str = "HOST_PORT";
pub const SUI_SOURCE_VALIDATION_VERSION_HEADER: &str = "x-sui-source-validation-version";
pub const SUI_SOURCE_VALIDATION_VERSION: &str = "0.1";

pub const MAINNET_URL: &str = "https://fullnode.mainnet.sui.io:443";
pub const TESTNET_URL: &str = "https://fullnode.testnet.sui.io:443";
pub const DEVNET_URL: &str = "https://fullnode.devnet.sui.io:443";
pub const LOCALNET_URL: &str = "http://127.0.0.1:9000";

pub const MAINNET_WS_URL: &str = "wss://rpc.mainnet.sui.io:443";
pub const TESTNET_WS_URL: &str = "wss://rpc.testnet.sui.io:443";
pub const DEVNET_WS_URL: &str = "wss://rpc.devnet.sui.io:443";
pub const LOCALNET_WS_URL: &str = "ws://127.0.0.1:9000";

pub const WS_PING_INTERVAL: Duration = Duration::from_millis(20_000);

pub const METRICS_ROUTE: &str = "/metrics";
pub const METRICS_HOST_PORT: &str = "0.0.0.0:9184";

pub fn host_port() -> String {
    match option_env!("HOST_PORT") {
        Some(v) => v.to_string(),
        None => String::from("0.0.0.0:8000"),
    }
}

#[derive(Clone, Deserialize, Debug)]
pub struct Config {
    pub packages: Vec<PackageSource>,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(tag = "source", content = "values")]
pub enum PackageSource {
    Repository(RepositorySource),
    Directory(DirectorySource),
}

#[derive(Clone, Deserialize, Debug)]
pub struct RepositorySource {
    pub repository: String,
    pub network: Option<Network>,
    pub branches: Vec<Branch>,
}

#[derive(Clone, Deserialize, Debug)]
pub struct Branch {
    pub branch: String,
    pub paths: Vec<Package>,
}

#[derive(Clone, Deserialize, Debug)]
pub struct DirectorySource {
    pub paths: Vec<Package>,
    pub network: Option<Network>,
}

#[derive(Clone, Deserialize, Debug)]
pub struct Package {
    pub path: String,
    /// Optional object ID to watch for upgrades. For framework packages, this is an address like 0x2.
    /// For non-framework packages this is an upgrade cap (possibly wrapped).
    pub watch: Option<ObjectID>,
}

#[derive(Clone, Serialize, Debug)]
pub struct SourceInfo {
    pub path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    // Is Some when content is hydrated from disk.
    pub source: Option<String>,
}

#[derive(Eq, PartialEq, Clone, Default, Serialize, Deserialize, Debug, Ord, PartialOrd)]
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

/// Map module name to verified source info.
pub type SourceLookup = BTreeMap<Symbol, SourceInfo>;
/// Map addresses to module names and sources.
pub type AddressLookup = BTreeMap<AccountAddress, SourceLookup>;
/// Top-level lookup that maps network to sources for corresponding on-chain networks.
pub type NetworkLookup = BTreeMap<Network, AddressLookup>;

pub async fn verify_package(
    network: &Network,
    package_path: impl AsRef<Path>,
) -> anyhow::Result<(Network, AddressLookup)> {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    // TODO(rvantonder): use config RPC URL instead of hardcoded URLs
    let network_url = match network {
        Network::Mainnet => MAINNET_URL,
        Network::Testnet => TESTNET_URL,
        Network::Devnet => DEVNET_URL,
        Network::Localnet => LOCALNET_URL,
    };
    let client = SuiClientBuilder::default().build(network_url).await?;
    let chain_id = client.read_api().get_chain_identifier().await?;
    let mut config =
        resolve_lock_file_path(MoveBuildConfig::default(), Some(package_path.as_ref()))?;
    config.lint_flag = LintFlag::LEVEL_NONE;
    config.silence_warnings = true;
    let build_config = BuildConfig {
        config,
        run_bytecode_verifier: false, /* no need to run verifier if code is on-chain */
        print_diags_to_stderr: false,
        chain_id: Some(chain_id),
    };
    let compiled_package = build_config.build(package_path.as_ref())?;

    BytecodeSourceVerifier::new(client.read_api())
        .verify(&compiled_package, ValidationMode::root())
        .await
        .map_err(|e| anyhow!("Network {network}: {e}"))?;

    let mut address_map = AddressLookup::new();
    let address = compiled_package
        .published_at
        .as_ref()
        .map(|id| **id)
        .map_err(|_| anyhow!("could not resolve published-at field in package manifest"))?;
    info!("verifying {} at {address}", package_path.as_ref().display());
    for v in &compiled_package.package.root_compiled_units {
        let path = v.source_path.to_path_buf();
        let source = Some(fs::read_to_string(path.as_path())?);
        let name = v.unit.name;
        if let Some(existing) = address_map.get_mut(&address) {
            existing.insert(name, SourceInfo { path, source });
        } else {
            let mut source_map = SourceLookup::new();
            source_map.insert(name, SourceInfo { path, source });
            address_map.insert(address, source_map);
        }
    }
    Ok((network.clone(), address_map))
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
    pub fn new(p: &RepositorySource, b: &Branch, dest: &Path) -> anyhow::Result<CloneCommand> {
        let repo_name = repo_name_from_url(&p.repository)?;
        let network = p.network.clone().unwrap_or_default().to_string();
        let sanitized_branch = b.branch.replace('/', "__");
        let dest = dest
            .join(network)
            .join(format!("{repo_name}__{sanitized_branch}"))
            .into_os_string();

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
            ostr!(format!("--branch={}", b.branch)),
            ostr!(&p.repository),
            ostr!(dest.clone()),
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
        let path_args: Vec<OsString> = b
            .paths
            .iter()
            .map(|p| OsString::from(p.path.clone()))
            .collect();
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
        for b in &p.branches {
            let command = CloneCommand::new(p, b, dir)?;
            info!(
                "cloning {}:{} to {}",
                &p.repository,
                &b.branch,
                dir.display()
            );
            let t = tokio::spawn(async move { command.run().await });
            tasks.push(t);
        }
    }

    for t in tasks {
        t.await.unwrap()?;
    }
    Ok(())
}

pub async fn initialize(
    config: &Config,
    dir: &Path,
) -> anyhow::Result<(NetworkLookup, NetworkLookup)> {
    let mut repos = vec![];
    for s in &config.packages {
        match s {
            PackageSource::Repository(r) => repos.push(r),
            PackageSource::Directory(_) => (), /* skip cloning */
        }
    }
    clone_repositories(repos, dir).await?;
    let sources = verify_packages(config, dir).await?;
    let sources_list = sources_list(&sources).await;
    Ok((sources, sources_list))
}

pub async fn sources_list(sources: &NetworkLookup) -> NetworkLookup {
    let mut sources_list = NetworkLookup::new();
    for (network, addresses) in sources {
        let mut address_map = AddressLookup::new();
        for (address, symbols) in addresses {
            let mut symbol_map = SourceLookup::new();
            for (symbol, source_info) in symbols {
                symbol_map.insert(
                    *symbol,
                    SourceInfo {
                        path: source_info.path.file_name().unwrap().into(),
                        source: None,
                    },
                );
            }
            address_map.insert(*address, symbol_map);
        }
        sources_list.insert(network.clone(), address_map);
    }
    sources_list
}

pub async fn verify_packages(config: &Config, dir: &Path) -> anyhow::Result<NetworkLookup> {
    let mut tasks = vec![];
    for p in &config.packages {
        match p {
            PackageSource::Repository(r) => {
                let repo_name = repo_name_from_url(&r.repository)?;
                let network_name = r.network.clone().unwrap_or_default().to_string();
                for b in &r.branches {
                    for p in &b.paths {
                        let sanitized_branch = b.branch.replace('/', "__");
                        let package_path = dir
                            .join(network_name.clone())
                            .join(format!("{repo_name}__{sanitized_branch}"))
                            .join(p.path.clone())
                            .clone();
                        let network = r.network.clone().unwrap_or_default();
                        let t =
                            tokio::spawn(
                                async move { verify_package(&network, package_path).await },
                            );
                        tasks.push(t)
                    }
                }
            }
            PackageSource::Directory(packages_dir) => {
                for p in &packages_dir.paths {
                    let package_path = PathBuf::from(p.path.clone());
                    let network = packages_dir.network.clone().unwrap_or_default();
                    let t =
                        tokio::spawn(async move { verify_package(&network, package_path).await });
                    tasks.push(t)
                }
            }
        }
    }

    let mut mainnet_lookup = AddressLookup::new();
    let mut testnet_lookup = AddressLookup::new();
    let mut devnet_lookup = AddressLookup::new();
    let mut localnet_lookup = AddressLookup::new();
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

// A thread that monitors on-chain transactions for package upgrades. `config` specifies which packages
// to watch. `app_state` contains the map of sources returned by the server. In particular, `watch_for_upgrades`
// invalidates (i.e., clears) the sources returned by the serve when we observe a package upgrade, so that we do not
// falsely report outdated sources for a package. Pass an optional `channel` to observe the upgrade transaction(s).
// The `channel` parameter exists for testing.
pub async fn watch_for_upgrades(
    _packages: Vec<PackageSource>,
    _app_state: Arc<RwLock<AppState>>,
    _network: Network,
    _channel: Option<Sender<SuiTransactionBlockEffects>>,
) -> anyhow::Result<()> {
    Err(anyhow!("Fatal: JsonRPC Subscriptions no longer supported. Reimplement without using subscriptions."))
}

pub struct AppState {
    pub sources: NetworkLookup,
    pub metrics: Option<SourceServiceMetrics>,
    pub sources_list: NetworkLookup,
}

pub async fn serve(app_state: Arc<RwLock<AppState>>) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/api", get(api_route))
        .route("/api/list", get(list_route))
        .layer(
            ServiceBuilder::new()
                .layer(
                    tower_http::cors::CorsLayer::new()
                        .allow_methods([Method::GET])
                        .allow_origin(tower_http::cors::Any),
                )
                .layer(middleware::from_fn(check_version_header)),
        )
        .with_state(app_state);
    let listener = TcpListener::bind(host_port())?;
    listener.set_nonblocking(true).unwrap();
    let listener = tokio::net::TcpListener::from_std(listener)?;
    axum::serve(listener, app).await?;
    Ok(())
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
    State(app_state): State<Arc<RwLock<AppState>>>,
    Query(Request {
        network,
        address,
        module,
    }): Query<Request>,
) -> impl IntoResponse {
    debug!("request network={network}&address={address}&module={module}");
    let symbol = Symbol::from(module);
    let Ok(address) = AccountAddress::from_hex_literal(&address) else {
        let error = format!("Invalid hex address {address}");
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error }).into_response(),
        );
    };

    let app_state = app_state.read().unwrap();
    if let Some(metrics) = &app_state.metrics {
        metrics.total_requests_received.inc();
    }
    let source_result = app_state
        .sources
        .get(&network)
        .and_then(|n| n.get(&address))
        .and_then(|a| a.get(&symbol));
    if let Some(SourceInfo {
        source: Some(source),
        ..
    }) = source_result
    {
        (
            StatusCode::OK,
            Json(SourceResponse {
                source: source.to_owned(),
            })
            .into_response(),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!(
                    "No source found for {symbol} at address {address} on network {network}"
                ),
            })
            .into_response(),
        )
    }
}

async fn check_version_header(
    headers: HeaderMap,
    req: hyper::Request<axum::body::Body>,
    next: Next,
) -> Response {
    let version = headers
        .get(SUI_SOURCE_VALIDATION_VERSION_HEADER)
        .as_ref()
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    match version {
        Some(v) if v != SUI_SOURCE_VALIDATION_VERSION => {
            let error = format!(
                "Unsupported version '{v}' specified in header \
		 {SUI_SOURCE_VALIDATION_VERSION_HEADER}"
            );
            let mut headers = HeaderMap::new();
            headers.insert(
                HeaderName::from_static(SUI_SOURCE_VALIDATION_VERSION_HEADER),
                HeaderValue::from_static(SUI_SOURCE_VALIDATION_VERSION),
            );
            return (
                StatusCode::BAD_REQUEST,
                headers,
                Json(ErrorResponse { error }).into_response(),
            )
                .into_response();
        }
        _ => (),
    }
    let mut response = next.run(req).await;
    response.headers_mut().insert(
        HeaderName::from_static(SUI_SOURCE_VALIDATION_VERSION_HEADER),
        HeaderValue::from_static(SUI_SOURCE_VALIDATION_VERSION),
    );
    response
}

async fn list_route(State(app_state): State<Arc<RwLock<AppState>>>) -> impl IntoResponse {
    let app_state = app_state.read().unwrap();
    (
        StatusCode::OK,
        Json(app_state.sources_list.clone()).into_response(),
    )
}

pub struct SourceServiceMetrics {
    pub total_requests_received: IntCounter,
}

impl SourceServiceMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_requests_received: register_int_counter_with_registry!(
                "total_requests",
                "Total number of requests received by Source Service",
                registry
            )
            .unwrap(),
        }
    }
}

pub fn start_prometheus_server(listener: TcpListener) -> RegistryService {
    let registry = Registry::new();

    let registry_service = RegistryService::new(registry);

    let app = Router::new()
        .route(METRICS_ROUTE, get(mysten_metrics::metrics))
        .layer(Extension(registry_service.clone()));

    tokio::spawn(async move {
        listener.set_nonblocking(true).unwrap();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        axum::serve(listener, app).await.unwrap();
    });

    registry_service
}
