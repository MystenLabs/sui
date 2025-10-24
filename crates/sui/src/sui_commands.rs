// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::client_commands::{
    SuiClientCommands, USER_AGENT, implicit_deps_for_protocol_version, pkg_tree_shake,
};
use crate::fire_drill::{FireDrill, run_fire_drill};
use crate::genesis_ceremony::{Ceremony, run};
use crate::keytool::KeyToolCommand;
use crate::trace_analysis_commands::AnalyzeTraceCommand;
use crate::validator_commands::SuiValidatorCommand;
use anyhow::{Context, anyhow, bail, ensure};
use clap::*;
use colored::Colorize;
use fastcrypto::traits::KeyPair;
use futures::future;
use move_analyzer::analyzer;
use move_command_line_common::files::MOVE_COMPILED_EXTENSION;
use move_compiler::editions::Flavor;
use move_package::BuildConfig;
use mysten_common::tempdir;
use prometheus::Registry;
use rand::rngs::OsRng;
use std::collections::BTreeMap;
use std::io::{Write, stdout};
use std::net::{AddrParseError, IpAddr, Ipv4Addr, SocketAddr};
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use std::{fs, io};
use sui_bridge::config::BridgeCommitteeConfig;
use sui_bridge::metrics::BridgeMetrics;
use sui_bridge::sui_client::SuiBridgeClient;
use sui_bridge::sui_transaction_builder::build_committee_register_transaction;
use sui_config::node::Genesis;
use sui_config::p2p::SeedPeer;
use sui_config::{
    Config, FULL_NODE_DB_PATH, PersistedConfig, SUI_CLIENT_CONFIG, SUI_FULLNODE_CONFIG,
    SUI_NETWORK_CONFIG, genesis_blob_exists, sui_config_dir,
};
use sui_config::{
    SUI_BENCHMARK_GENESIS_GAS_KEYSTORE_FILENAME, SUI_GENESIS_FILENAME, SUI_KEYSTORE_FILENAME,
};
use sui_faucet::{AppState, FaucetConfig, LocalFaucet, create_wallet_context, start_faucet};
use sui_json_rpc_types::{SuiObjectDataOptions, SuiRawData};
use sui_move::summary::PackageSummaryMetadata;
use sui_sdk::SuiClient;
use sui_sdk::apis::ReadApi;
use sui_types::move_package::MovePackage;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

use move_core_types::account_address::AccountAddress;
use serde_json::json;
use sui_indexer_alt::{config::IndexerConfig, setup_indexer};
use sui_indexer_alt_consistent_store::{
    args::RpcArgs as ConsistentArgs, config::ServiceConfig as ConsistentConfig,
    start_service as start_consistent_store,
};
use sui_indexer_alt_framework::{IndexerArgs, ingestion::ClientArgs};
use sui_indexer_alt_graphql::{
    RpcArgs as GraphQlArgs, config::RpcConfig as GraphQlConfig, start_rpc as start_graphql,
};
use sui_indexer_alt_reader::{
    bigtable_reader::BigtableArgs, consistent_reader::ConsistentReaderArgs,
    fullnode_client::FullnodeArgs, system_package_task::SystemPackageTaskArgs,
};
use sui_keys::key_derive::generate_new_key;
use sui_keys::keypair_file::read_key;
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_move::manage_package::resolve_lock_file_path;
use sui_move::{self, execute_move_command};
use sui_move_build::{
    BuildConfig as SuiBuildConfig, SuiPackageHooks, check_conflicting_addresses,
    check_invalid_dependencies, check_unpublished_dependencies, implicit_deps,
};
use sui_package_management::system_package_versions::latest_system_packages;
use sui_pg_db::DbArgs;
use sui_pg_db::temp::{LocalDatabase, get_available_port};
use sui_protocol_config::Chain;
use sui_replay_2 as SR2;
use sui_sdk::sui_client_config::{SuiClientConfig, SuiEnv};
use sui_sdk::wallet_context::WalletContext;
use sui_swarm::memory::Swarm;
use sui_swarm_config::genesis_config::GenesisConfig;
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_swarm_config::node_config_builder::FullnodeConfigBuilder;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::{SignatureScheme, SuiKeyPair, ToFromBytes};
use sui_types::digests::ChainIdentifier;
use tracing::info;
use url::Url;

const DEFAULT_EPOCH_DURATION_MS: u64 = 60_000;

const DEFAULT_FAUCET_MIST_AMOUNT: u64 = 200_000_000_000; // 200 SUI
const DEFAULT_FAUCET_PORT: u16 = 9123;

const DEFAULT_CONSISTENT_STORE_PORT: u16 = 9124;
const DEFAULT_GRAPHQL_PORT: u16 = 9125;

#[derive(Args)]
pub struct RpcArgs {
    /// Start an indexer with a local PostgreSQL database.
    /// The database will be created/opened in the network's configuration directory.
    #[clap(long)]
    with_indexer: bool,

    /// Start a Consistent Store with default host and port: 0.0.0.0:9124. This flag accepts also a
    /// port, a host, or both (e.g., 0.0.0.0:9124).
    ///
    /// When providing a specific value, please use the = sign between the flag and value:
    /// `--with-consistent-store=9124` or `--with-consistent-store=0.0.0.0`, or `--with-consistent-store=0.0.0.0:9124`
    /// The Consistent Store will be automatically enabled when `--with-graphql` is set.
    #[clap(
        long,
        default_missing_value = "0.0.0.0:9124",
        num_args = 0..=1,
        require_equals = true,
        value_name = "CONSISTENT_STORE_HOST_PORT"
    )]
    with_consistent_store: Option<String>,

    /// Start a GraphQL server with default host and port: 0.0.0.0:9125. This flag accepts also a
    /// port, a host, or both (e.g., 0.0.0.0:9125).
    ///
    /// When providing a specific value, please use the = sign between the flag and value:
    /// `--with-graphql=9125` or `--with-graphql=0.0.0.0`, or `--with-graphql=0.0.0.0:9125`
    ///
    /// Note that GraphQL requires a running indexer and consistent store, which will be enabled
    /// by default even if those flags are not set.
    #[clap(
        long,
        default_missing_value = "0.0.0.0:9125",
        num_args = 0..=1,
        require_equals = true,
        value_name = "GRAPHQL_HOST_PORT"
    )]
    with_graphql: Option<String>,
}

impl RpcArgs {
    pub fn for_testing() -> Self {
        Self {
            with_indexer: false,
            with_consistent_store: None,
            with_graphql: None,
        }
    }
}

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub struct SuiEnvConfig {
    /// Sets the file storing the state of our user accounts (an empty one will be created if missing)
    #[clap(long = "client.config")]
    config: Option<PathBuf>,
    /// The Sui environment to use. This must be present in the current config file.
    #[clap(long = "client.env")]
    env: Option<String>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum SuiCommand {
    /// Start a local network in two modes: saving state between re-runs and not saving state
    /// between re-runs. Please use (--help) to see the full description.
    ///
    /// By default, sui start will start a local network from the genesis blob that exists in
    /// the Sui config default dir or in the config_dir that was passed. If the default directory
    /// does not exist and the config_dir is not passed, it will generate a new default directory,
    /// generate the genesis blob, and start the network.
    ///
    /// Note that if you want to start an indexer, Postgres DB is required.
    ///
    /// ProtocolConfig parameters can be overridden individually by setting env variables as
    /// follows:
    /// - SUI_PROTOCOL_CONFIG_OVERRIDE_ENABLE=1
    /// - Then, to configure an override, use the prefix `SUI_PROTOCOL_CONFIG_OVERRIDE_`
    ///   along with the parameter name. For example, to increase the interval between
    ///   checkpoint creation to >1/s, you might set:
    ///   SUI_PROTOCOL_CONFIG_OVERRIDE_min_checkpoint_interval_ms=1000
    ///
    /// Note that ProtocolConfig parameters must match between all nodes, or the network
    /// may break. Changing these values outside of local networks is very dangerous.
    #[clap(name = "start", verbatim_doc_comment)]
    Start {
        /// Config directory that will be used to store network config, node db, keystore
        /// sui genesis -f --with-faucet generates a genesis config that can be used to start this
        /// proces. Use with caution as the `-f` flag will overwrite the existing config directory.
        /// We can use any config dir that is generated by the `sui genesis`.
        #[clap(long = "network.config")]
        config_dir: Option<std::path::PathBuf>,

        /// A new genesis is created each time this flag is set, and state is not persisted between
        /// runs. Only use this flag when you want to start the network from scratch every time you
        /// run this command.
        ///
        /// To run with persisted state, do not pass this flag and use the `sui genesis` command
        /// to generate a genesis that can be used to start the network with.
        #[clap(long)]
        force_regenesis: bool,

        /// Start a faucet with default host and port: 0.0.0.0:9123. This flag accepts also a
        /// port, a host, or both (e.g., 0.0.0.0:9123).
        /// When providing a specific value, please use the = sign between the flag and value:
        /// `--with-faucet=6124` or `--with-faucet=0.0.0.0`, or `--with-faucet=0.0.0.0:9123`
        #[clap(
            long,
            default_missing_value = "0.0.0.0:9123",
            num_args = 0..=1,
            require_equals = true,
            value_name = "FAUCET_HOST_PORT",
        )]
        with_faucet: Option<String>,

        #[clap(flatten)]
        rpc_args: RpcArgs,

        /// Port to start the Fullnode RPC server on. Default port is 9000.
        #[clap(long, default_value = "9000")]
        fullnode_rpc_port: u16,

        /// Set the epoch duration. Can only be used when `--force-regenesis` flag is passed or if
        /// there's no genesis config and one will be auto-generated. When this flag is not set but
        /// `--force-regenesis` is set, the epoch duration will be set to 60 seconds.
        #[clap(long)]
        epoch_duration_ms: Option<u64>,

        /// Make the fullnode dump executed checkpoints as files to this directory. This is
        /// incompatible with --no-full-node.
        ///
        /// If --with-indexer is set, this defaults to a temporary directory.
        #[clap(long, value_name = "DATA_INGESTION_DIR")]
        data_ingestion_dir: Option<PathBuf>,

        /// Start the network without a fullnode
        #[clap(long = "no-full-node")]
        no_full_node: bool,
        /// Set the number of validators in the network. If a genesis was already generated with a
        /// specific number of validators, this will not override it; the user should recreate the
        /// genesis with the desired number of validators.
        #[clap(long)]
        committee_size: Option<usize>,
    },
    #[clap(name = "network")]
    Network {
        #[clap(long = "network.config")]
        config: Option<PathBuf>,
        #[clap(short, long, help = "Dump the public keys of all authorities")]
        dump_addresses: bool,
    },
    /// Bootstrap and initialize a new sui network
    #[clap(name = "genesis")]
    Genesis {
        #[clap(long, help = "Start genesis with a given config file")]
        from_config: Option<PathBuf>,
        #[clap(
            long,
            help = "Build a genesis config, write it to the specified path, and exit"
        )]
        write_config: Option<PathBuf>,
        #[clap(long)]
        working_dir: Option<PathBuf>,
        #[clap(short, long, help = "Forces overwriting existing configuration")]
        force: bool,
        #[clap(long = "epoch-duration-ms")]
        epoch_duration_ms: Option<u64>,
        #[clap(
            long,
            value_name = "ADDR",
            num_args(1..),
            value_delimiter = ',',
            help = "A list of ip addresses to generate a genesis suitable for benchmarks"
        )]
        benchmark_ips: Option<Vec<String>>,
        #[clap(
            long,
            help = "Creates an extra faucet configuration for sui persisted runs."
        )]
        with_faucet: bool,
        /// Set number of validators in the network.
        #[clap(long)]
        committee_size: Option<usize>,
    },
    GenesisCeremony(Ceremony),
    /// Sui keystore tool.
    #[clap(name = "keytool")]
    KeyTool {
        #[clap(long)]
        keystore_path: Option<PathBuf>,
        ///Return command outputs in json format
        #[clap(long, global = true)]
        json: bool,
        /// Subcommands.
        #[clap(subcommand)]
        cmd: KeyToolCommand,
    },
    /// Client for interacting with the Sui network.
    #[clap(name = "client")]
    Client {
        #[clap(flatten)]
        config: SuiEnvConfig,
        #[clap(subcommand)]
        cmd: Option<SuiClientCommands>,
        /// Return command outputs in json format.
        #[clap(long, global = true)]
        json: bool,
        #[clap(short = 'y', long = "yes")]
        accept_defaults: bool,
    },
    /// A tool for validators and validator candidates.
    #[clap(name = "validator")]
    Validator {
        /// Sets the file storing the state of our user accounts (an empty one will be created if missing)
        #[clap(long = "client.config")]
        config: Option<PathBuf>,
        #[clap(subcommand)]
        cmd: Option<SuiValidatorCommand>,
        /// Return command outputs in json format.
        #[clap(long, global = true)]
        json: bool,
        #[clap(short = 'y', long = "yes")]
        accept_defaults: bool,
    },

    /// Tool to build and test Move applications.
    #[clap(name = "move")]
    Move {
        /// Path to a package which the command should be run with respect to.
        #[clap(long = "path", short = 'p', global = true)]
        package_path: Option<PathBuf>,
        #[clap(flatten)]
        config: SuiEnvConfig,
        /// Package build options
        #[clap(flatten)]
        build_config: BuildConfig,
        /// Subcommands.
        #[clap(subcommand)]
        cmd: sui_move::Command,
    },

    /// Command to initialize the bridge committee, usually used when
    /// running local bridge cluster.
    #[clap(name = "bridge-committee-init")]
    BridgeInitialize {
        #[clap(long = "network.config")]
        network_config: Option<PathBuf>,
        #[clap(long = "client.config")]
        client_config: Option<PathBuf>,
        #[clap(long = "bridge_committee.config")]
        bridge_committee_config_path: PathBuf,
    },

    /// Tool for Fire Drill
    FireDrill {
        #[clap(subcommand)]
        fire_drill: FireDrill,
    },

    /// Invoke Sui's move-analyzer via CLI
    #[clap(name = "analyzer", hide = true)]
    Analyzer,

    /// Analyze and/or transform a trace file
    #[clap(name = "analyze-trace")]
    AnalyzeTrace {
        /// The path to the trace file to analyze
        #[arg(long, short)]
        path: PathBuf,

        /// The output directory for any generated artifacts. Defaults `<cur_dir>`
        #[arg(long, short)]
        output_dir: Option<PathBuf>,

        #[clap(subcommand)]
        command: AnalyzeTraceCommand,
    },

    #[clap(name = "replay")]
    ReplayTransaction {
        #[clap(flatten)]
        config: SuiEnvConfig,

        #[command(flatten)]
        replay_config: SR2::ReplayConfigStable,
    },
}

impl SuiCommand {
    pub async fn execute(self) -> Result<(), anyhow::Error> {
        move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
        match self {
            SuiCommand::Network {
                config,
                dump_addresses,
            } => {
                let config_path = config.unwrap_or(sui_config_dir()?.join(SUI_NETWORK_CONFIG));
                let config: NetworkConfig = PersistedConfig::read(&config_path).map_err(|err| {
                    err.context(format!(
                        "Cannot open Sui network config file at {:?}",
                        config_path
                    ))
                })?;

                if dump_addresses {
                    for validator in config.validator_configs() {
                        println!(
                            "{} - {}",
                            validator.network_address(),
                            validator.protocol_key_pair().public(),
                        );
                    }
                }
                Ok(())
            }
            SuiCommand::Start {
                config_dir,
                force_regenesis,
                with_faucet,
                rpc_args,
                fullnode_rpc_port,
                data_ingestion_dir,
                no_full_node,
                epoch_duration_ms,
                committee_size,
            } => {
                start(
                    config_dir.clone(),
                    with_faucet,
                    rpc_args,
                    force_regenesis,
                    epoch_duration_ms,
                    fullnode_rpc_port,
                    data_ingestion_dir,
                    no_full_node,
                    committee_size,
                )
                .await?;

                Ok(())
            }
            SuiCommand::Genesis {
                working_dir,
                force,
                from_config,
                write_config,
                epoch_duration_ms,
                benchmark_ips,
                with_faucet,
                committee_size,
            } => {
                genesis(
                    from_config,
                    write_config,
                    working_dir,
                    force,
                    epoch_duration_ms,
                    benchmark_ips,
                    with_faucet,
                    committee_size,
                )
                .await
            }
            SuiCommand::GenesisCeremony(cmd) => run(cmd),
            SuiCommand::KeyTool {
                keystore_path,
                json,
                cmd,
            } => {
                let keystore_path =
                    keystore_path.unwrap_or(sui_config_dir()?.join(SUI_KEYSTORE_FILENAME));
                let mut keystore =
                    Keystore::from(FileBasedKeystore::load_or_create(&keystore_path)?);
                cmd.execute(&mut keystore).await?.print(!json);
                Ok(())
            }
            SuiCommand::Client {
                config,
                cmd,
                json,
                accept_defaults,
            } => {
                let config_path = config
                    .config
                    .unwrap_or(sui_config_dir()?.join(SUI_CLIENT_CONFIG));
                prompt_if_no_config(&config_path, accept_defaults).await?;
                if let Some(cmd) = cmd {
                    let mut context = WalletContext::new(&config_path)?;
                    if let Some(env_override) = config.env {
                        context = context.with_env_override(env_override);
                    }
                    if let Ok(client) = context.get_client().await
                        && let Err(e) = client.check_api_version()
                    {
                        eprintln!("{}", format!("[warning] {e}").yellow().bold());
                    }
                    cmd.execute(&mut context).await?.print(!json);
                } else {
                    // Print help
                    let mut app: Command = SuiCommand::command();
                    app.build();
                    app.find_subcommand_mut("client").unwrap().print_help()?;
                }
                Ok(())
            }
            SuiCommand::Validator {
                config,
                cmd,
                json,
                accept_defaults,
            } => {
                let config_path = config.unwrap_or(sui_config_dir()?.join(SUI_CLIENT_CONFIG));
                prompt_if_no_config(&config_path, accept_defaults).await?;
                let mut context = WalletContext::new(&config_path)?;
                if let Some(cmd) = cmd {
                    if let Ok(client) = context.get_client().await
                        && let Err(e) = client.check_api_version()
                    {
                        eprintln!("{}", format!("[warning] {e}").yellow().bold());
                    }
                    cmd.execute(&mut context).await?.print(!json);
                } else {
                    // Print help
                    let mut app: Command = SuiCommand::command();
                    app.build();
                    app.find_subcommand_mut("validator").unwrap().print_help()?;
                }
                Ok(())
            }
            SuiCommand::Move {
                package_path,
                build_config,
                mut cmd,
                config: client_config,
            } => {
                match cmd {
                    sui_move::Command::Summary(mut s) if s.package_id.is_some() => {
                        let (_, client) = get_chain_id_and_client(
                            client_config,
                            "sui move summary --package-id <object_id>",
                        )
                        .await?;
                        let Some(client) = client else {
                            bail!(
                                "`sui move summary --package-id <object_id>` requires a configured network"
                            );
                        };

                        let read_api = client.read_api();

                        // If they didn't run with `--bytecode` correct this for them but warn them
                        // to let them know that we are changing it.
                        if !s.summary.bytecode {
                            eprintln!("{}",
                                "[warning] `sui move summary --package-id <object_id>` only supports bytecode summaries. \
                                 Falling back to producing a bytecode-based summary. To not get this warning you can run with `--bytecode`".yellow().bold()
                            );
                            s.summary.bytecode = true;
                        }
                        let root_package_id = s
                            .package_id
                            .as_ref()
                            .expect("Safe since we checked in the match statement");

                        // Create a tempdir to download the package bytes to, and then download the
                        // packages bytes there.
                        let package_bytes_location = tempdir()?;
                        let path = package_bytes_location.path();
                        let package_metadata =
                            download_package_and_deps_under(read_api, path, *root_package_id)
                                .await?;

                        // Now produce the summary, pointing at the tempdir containing the package
                        // bytes.
                        execute_move_command(
                            Some(path),
                            build_config,
                            sui_move::Command::Summary(s),
                            Some(sui_move::CommandMeta::Summary(package_metadata)),
                        )?;
                        return Ok(());
                    }
                    sui_move::Command::Build(build) if build.dump_bytecode_as_base64 => {
                        // `sui move build` does not ordinarily require a network connection.
                        // The exception is when --dump-bytecode-as-base64 is specified: In this
                        // case, we should resolve the correct addresses for the respective chain
                        // (e.g., testnet, mainnet) from the Move.lock under automated address management.
                        // In addition, tree shaking also requires a network as it needs to fetch
                        // on-chain linkage table of package dependencies.
                        let (chain_id, client) = if build.ignore_chain {
                            // for tests it's useful to ignore the chain id!
                            (None, None)
                        } else {
                            get_chain_id_and_client(
                                client_config,
                                "sui move build --dump-bytecode-as-base64",
                            )
                            .await?
                        };

                        let rerooted_path = move_cli::base::reroot_path(package_path.as_deref())?;
                        let mut build_config =
                            resolve_lock_file_path(build_config, Some(&rerooted_path))?;

                        let previous_id = if let Some(ref chain_id) = chain_id {
                            sui_package_management::set_package_id(
                                &rerooted_path,
                                build_config.install_dir.clone(),
                                chain_id,
                                AccountAddress::ZERO,
                            )?
                        } else {
                            None
                        };

                        if let Some(client) = &client {
                            let protocol_config =
                                client.read_api().get_protocol_config(None).await?;
                            build_config.implicit_dependencies =
                                implicit_deps_for_protocol_version(
                                    protocol_config.protocol_version,
                                )?;
                        } else {
                            build_config.implicit_dependencies =
                                implicit_deps(latest_system_packages());
                        }

                        let mut pkg = SuiBuildConfig {
                            config: build_config.clone(),
                            run_bytecode_verifier: true,
                            print_diags_to_stderr: true,
                            chain_id: chain_id.clone(),
                        }
                        .build(&rerooted_path)?;

                        // Restore original ID, then check result.
                        if let (Some(chain_id), Some(previous_id)) = (chain_id, previous_id) {
                            let _ = sui_package_management::set_package_id(
                                &rerooted_path,
                                build_config.install_dir.clone(),
                                &chain_id,
                                previous_id,
                            )?;
                        }

                        let with_unpublished_deps = build.with_unpublished_dependencies;

                        check_conflicting_addresses(&pkg.dependency_ids.conflicting, true)?;
                        check_invalid_dependencies(&pkg.dependency_ids.invalid)?;
                        if !with_unpublished_deps {
                            check_unpublished_dependencies(&pkg.dependency_ids.unpublished)?;
                        }

                        if let Some(client) = client {
                            pkg_tree_shake(client.read_api(), with_unpublished_deps, &mut pkg)
                                .await?;
                        }

                        println!(
                            "{}",
                            json!({
                                "modules": pkg.get_package_base64(with_unpublished_deps),
                                "dependencies": pkg.get_dependency_storage_package_ids(),
                                "digest": pkg.get_package_digest(with_unpublished_deps),
                            })
                        );
                        return Ok(());
                    }
                    _ => (),
                };

                // If a specific environment is specified for the build command we set the chain ID
                // to the one that is specified.
                if client_config.env.is_some() && matches!(cmd, sui_move::Command::Build(_)) {
                    let (chain_id, _) =
                        get_chain_id_and_client(client_config, "sui move build").await?;

                    let sui_move::Command::Build(build_config) = &mut cmd else {
                        unreachable!("We checked for Build above, so this should never happen");
                    };

                    build_config.chain_id = chain_id;
                }

                execute_move_command(package_path.as_deref(), build_config, cmd, None)
            }
            SuiCommand::BridgeInitialize {
                network_config,
                client_config,
                bridge_committee_config_path,
            } => {
                // Load the config of the Sui authority.
                let network_config_path = network_config
                    .clone()
                    .unwrap_or(sui_config_dir()?.join(SUI_NETWORK_CONFIG));
                let network_config: NetworkConfig = PersistedConfig::read(&network_config_path)
                    .map_err(|err| {
                        err.context(format!(
                            "Cannot open Sui network config file at {:?}",
                            network_config_path
                        ))
                    })?;
                let bridge_committee_config: BridgeCommitteeConfig =
                    PersistedConfig::read(&bridge_committee_config_path).map_err(|err| {
                        err.context(format!(
                            "Cannot open Bridge Committee config file at {:?}",
                            bridge_committee_config_path
                        ))
                    })?;

                let config_path =
                    client_config.unwrap_or(sui_config_dir()?.join(SUI_CLIENT_CONFIG));
                let mut context = WalletContext::new(&config_path)?;
                if let Ok(client) = context.get_client().await
                    && let Err(e) = client.check_api_version()
                {
                    eprintln!("{}", format!("[warning] {e}").yellow().bold());
                }
                let rgp = context.get_reference_gas_price().await?;
                let rpc_url = &context.get_active_env()?.rpc;
                println!("rpc_url: {}", rpc_url);
                let bridge_metrics = Arc::new(BridgeMetrics::new_for_testing());
                let sui_bridge_client = SuiBridgeClient::new(rpc_url, bridge_metrics).await?;
                let bridge_arg = sui_bridge_client
                    .get_mutable_bridge_object_arg_must_succeed()
                    .await;
                assert_eq!(
                    network_config.validator_configs().len(),
                    bridge_committee_config
                        .bridge_authority_port_and_key_path
                        .len()
                );
                for node_config in network_config.validator_configs() {
                    let account_kp = node_config.account_key_pair.keypair();
                    context.add_account(None, account_kp.copy()).await;
                }

                let context = context;
                let mut tasks = vec![];
                for (node_config, (port, key_path)) in network_config
                    .validator_configs()
                    .iter()
                    .zip(bridge_committee_config.bridge_authority_port_and_key_path)
                {
                    let account_kp = node_config.account_key_pair.keypair();
                    let sui_address = SuiAddress::from(&account_kp.public());
                    let gas_obj_ref = context
                        .get_one_gas_object_owned_by_address(sui_address)
                        .await?
                        .expect("Validator does not own any gas objects");
                    let kp = match read_key(&key_path, true)? {
                        SuiKeyPair::Secp256k1(key) => key,
                        _ => unreachable!("we required secp256k1 key in `read_key`"),
                    };

                    // build registration tx
                    let tx = build_committee_register_transaction(
                        sui_address,
                        &gas_obj_ref,
                        bridge_arg,
                        kp.public().as_bytes().to_vec(),
                        &format!("http://127.0.0.1:{port}"),
                        rgp,
                        1000000000,
                    )
                    .unwrap();
                    let signed_tx = context.sign_transaction(&tx).await;
                    tasks.push(context.execute_transaction_must_succeed(signed_tx));
                }
                future::join_all(tasks).await;
                Ok(())
            }
            SuiCommand::FireDrill { fire_drill } => run_fire_drill(fire_drill).await,
            SuiCommand::Analyzer => {
                let sui_implicit_deps = implicit_deps(latest_system_packages());
                let flavor = Flavor::Sui;
                let sui_pkg_hooks = Box::new(SuiPackageHooks);
                analyzer::run(sui_implicit_deps, Some(flavor), Some(sui_pkg_hooks));
                Ok(())
            }
            SuiCommand::AnalyzeTrace {
                path,
                output_dir,
                command,
            } => command.execute(path, output_dir).await,
            SuiCommand::ReplayTransaction {
                config,
                replay_config,
            } => {
                let config_path = config
                    .config
                    .unwrap_or(sui_config_dir()?.join(SUI_CLIENT_CONFIG));
                prompt_if_no_config(&config_path, /* accept_defaults */ false).await?;
                let mut context = WalletContext::new(&config_path)?;
                if let Some(env_override) = config.env {
                    context = context.with_env_override(env_override);
                }

                let node = get_replay_node(&context).await?;
                let file_config = SR2::load_config_file()?;
                let stable_config = SR2::merge_configs(replay_config, file_config);
                let experimental_config = SR2::ReplayConfigExperimental {
                    node,
                    ..Default::default()
                };

                let artifact_path =
                    SR2::handle_replay_config(&stable_config, &experimental_config, USER_AGENT)
                        .await?;

                if let Some(digest) = &stable_config.digest {
                    SR2::print_effects_or_fork(
                        digest,
                        &artifact_path,
                        stable_config.show_effects,
                        &mut std::io::stdout(),
                    )?;
                }

                Ok(())
            }
        }
    }
}

/// Starts a local network with the given configuration.
async fn start(
    config: Option<PathBuf>,
    with_faucet: Option<String>,
    rpc_args: RpcArgs,
    force_regenesis: bool,
    epoch_duration_ms: Option<u64>,
    fullnode_rpc_port: u16,
    mut data_ingestion_dir: Option<PathBuf>,
    no_full_node: bool,
    committee_size: Option<usize>,
) -> Result<(), anyhow::Error> {
    if force_regenesis {
        ensure!(
            config.is_none(),
            "Cannot pass `--force-regenesis` and `--network.config` at the same time."
        );
    }

    let RpcArgs {
        with_indexer,
        mut with_consistent_store,
        with_graphql,
    } = rpc_args;

    // Automatically enable consistent store if GraphQL is enabled
    if with_graphql.is_some() && with_consistent_store.is_none() {
        with_consistent_store = Some("0.0.0.0:9124".to_string());
    }

    // Automatically enable indexer if GraphQL is enabled
    let with_indexer = with_indexer || with_graphql.is_some();
    if with_indexer {
        ensure!(
            !no_full_node,
            "Cannot start the indexer without a fullnode."
        );
    }

    if epoch_duration_ms.is_some() && genesis_blob_exists(config.clone()) && !force_regenesis {
        bail!(
            "Epoch duration can only be set when passing the `--force-regenesis` flag, or when \
            there is no genesis configuration in the default Sui configuration folder or the given \
            network.config argument.",
        );
    }

    let mut swarm_builder = Swarm::builder();

    // If this is set, then no data will be persisted between runs, and a new genesis will be
    // generated each run.
    let config_dir = if force_regenesis {
        let committee_size = match committee_size {
            Some(x) => NonZeroUsize::new(x),
            None => NonZeroUsize::new(1),
        }
        .ok_or_else(|| anyhow!("Committee size must be at least 1."))?;
        swarm_builder = swarm_builder.committee_size(committee_size);
        let genesis_config = GenesisConfig::custom_genesis(1, 100);
        swarm_builder = swarm_builder.with_genesis_config(genesis_config);
        let epoch_duration_ms = epoch_duration_ms.unwrap_or(DEFAULT_EPOCH_DURATION_MS);
        swarm_builder = swarm_builder.with_epoch_duration_ms(epoch_duration_ms);
        mysten_common::tempdir()?.keep()
    } else {
        // If the config path looks like a YAML file, it is treated as if it is the network.yaml
        // overriding the network.yaml found in the sui config directry. Otherwise it is treated as
        // the sui config directory for backwards compatibility with `sui-test-validator`.
        let (network_config_path, sui_config_path) = match config {
            Some(config)
                if config.is_file()
                    && config
                        .extension()
                        .is_some_and(|e| e == "yml" || e == "yaml") =>
            {
                if committee_size.is_some() {
                    eprintln!(
                        "{}",
                        "[warning] The committee-size arg wil be ignored as a network \
                            configuration already exists. To change the committee-size, you'll \
                            have to adjust the network configuration file or regenerate a genesis \
                            with the desired committee size. See `sui genesis --help` for more \
                            information."
                            .yellow()
                            .bold()
                    );
                }
                (config, sui_config_dir()?)
            }

            Some(config) => {
                if committee_size.is_some() {
                    eprintln!(
                        "{}",
                        "[warning] The committee-size arg wil be ignored as a network \
                            configuration already exists. To change the committee-size, you'll \
                            have to adjust the network configuration file or regenerate a genesis \
                            with the desired committee size. See `sui genesis --help` for more \
                            information."
                            .yellow()
                            .bold()
                    );
                }
                (config.join(SUI_NETWORK_CONFIG), config)
            }

            None => {
                let sui_config = sui_config_dir()?;
                let network_config = sui_config.join(SUI_NETWORK_CONFIG);

                if !network_config.exists() {
                    genesis(
                        None,
                        None,
                        None,
                        false,
                        epoch_duration_ms,
                        None,
                        false,
                        committee_size,
                    )
                    .await
                    .map_err(|_| {
                        anyhow!(
                            "Cannot run genesis with non-empty Sui config directory: {}.\n\n\
                                If you are trying to run a local network without persisting the \
                                data (so a new genesis that is randomly generated and will not be \
                                saved once the network is shut down), use --force-regenesis flag.\n\
                                If you are trying to persist the network data and start from a new \
                                genesis, use sui genesis --help to see how to generate a new \
                                genesis.",
                            sui_config.display(),
                        )
                    })?;
                } else if committee_size.is_some() {
                    eprintln!(
                        "{}",
                        "[warning] The committee-size arg wil be ignored as a network \
                            configuration already exists. To change the committee-size, you'll \
                            have to adjust the network configuration file or regenerate a genesis \
                            with the desired committee size. See `sui genesis --help` for more \
                            information."
                            .yellow()
                            .bold()
                    );
                }

                (network_config, sui_config)
            }
        };

        // Load the config of the Sui authority.
        let network_config: NetworkConfig =
            PersistedConfig::read(&network_config_path).map_err(|err| {
                err.context(format!(
                    "Cannot open Sui network config file at {:?}",
                    network_config_path
                ))
            })?;

        swarm_builder = swarm_builder
            .dir(sui_config_path.clone())
            .with_network_config(network_config);

        sui_config_path
    };

    // the indexer requires to set the fullnode's data ingestion directory
    // note that this overrides the default configuration that is set when running the genesis
    // command, which sets data_ingestion_dir to None.
    if with_indexer && data_ingestion_dir.is_none() {
        data_ingestion_dir = Some(mysten_common::tempdir()?.keep())
    }

    if let Some(ref dir) = data_ingestion_dir {
        swarm_builder = swarm_builder.with_data_ingestion_dir(dir.clone());
    }

    let mut fullnode_rpc_address = sui_config::node::default_json_rpc_address();
    fullnode_rpc_address.set_port(fullnode_rpc_port);

    if no_full_node {
        swarm_builder = swarm_builder.with_fullnode_count(0);
    } else {
        let rpc_config = sui_config::RpcConfig {
            enable_indexing: Some(true),
            ..Default::default()
        };

        swarm_builder = swarm_builder
            .with_fullnode_count(1)
            .with_fullnode_rpc_addr(fullnode_rpc_address)
            .with_fullnode_rpc_config(rpc_config);
    }

    let mut swarm = swarm_builder.build();
    swarm.launch().await?;
    // Let nodes connect to one another
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    info!("Cluster started");

    let fullnode_rpc_url = format!("http://{fullnode_rpc_address}");
    info!("Fullnode RPC URL: {fullnode_rpc_url}");

    let prometheus_registry = Registry::new();
    let cancel = CancellationToken::new();
    let mut rpc_services = vec![];

    // Start a local PostgreSQL database if needed
    let database = if with_indexer {
        let pg_dir = config_dir.join("indexer");
        let port = get_available_port();

        info!("Starting local PostgreSQL database at {pg_dir:?} on port {port}");

        let db = if pg_dir.exists() {
            LocalDatabase::new(pg_dir, port).context("Failed to start local PostgreSQL database")?
        } else {
            LocalDatabase::new_initdb(pg_dir, port)
                .context("Failed to initialize and start local PostgreSQL database")?
        };

        info!("Local PostgreSQL database started at {}", db.url());
        Some(db)
    } else {
        None
    };

    // Get the database URL from the local database
    let database_url = database.as_ref().map(|db| db.url().clone());
    let pipelines = if let Some(ref db_url) = database_url {
        info!("Starting the indexer with database: {db_url}");

        let client_args = ClientArgs {
            local_ingestion_path: data_ingestion_dir.clone(),
            ..Default::default()
        };

        let indexer = setup_indexer(
            db_url.clone(),
            DbArgs::default(),
            IndexerArgs::default(),
            client_args,
            IndexerConfig::for_test(),
            None,
            &prometheus_registry,
            cancel.child_token(),
        )
        .await
        .context("Failed to setup indexer")?;

        let pipelines = indexer.pipelines().map(|s| s.to_string()).collect();
        let handle = indexer.run().await.context("Failed to start indexer")?;
        rpc_services.push(handle);

        info!("Indexer started with pipelines: {pipelines:?}");
        pipelines
    } else {
        vec![]
    };

    let consistent_store_url = if let Some(input) = with_consistent_store {
        let address = parse_host_port(input, DEFAULT_CONSISTENT_STORE_PORT)
            .context("Invalid consistent store host and port")?;

        let client_args = ClientArgs {
            local_ingestion_path: data_ingestion_dir.clone(),
            ..Default::default()
        };

        let consistent_args = ConsistentArgs {
            rpc_listen_address: address,
            ..Default::default()
        };

        let handle = start_consistent_store(
            config_dir.join("consistent_store"),
            IndexerArgs::default(),
            client_args,
            consistent_args,
            "0.0.0",
            ConsistentConfig::for_test(),
            &prometheus_registry,
            cancel.child_token(),
        )
        .await
        .context("Failed to start Consistent Store")?;
        rpc_services.push(handle);

        info!("Consistent Store started at {address}");
        Some(format!("http://{address}"))
    } else {
        None
    };

    if let Some(input) = with_graphql {
        let address = parse_host_port(input, DEFAULT_GRAPHQL_PORT)
            .context("Invalid graphql host and port")?;

        info!("Starting the GraphQL service at {address}");

        let graphql_args = GraphQlArgs {
            rpc_listen_address: address,
            no_ide: false,
        };

        let consistent_reader_args = ConsistentReaderArgs {
            consistent_store_url: consistent_store_url
                .as_ref()
                .map(|url| Url::parse(url))
                .transpose()
                .context("Failed to parse consistent store URL")?,
            ..Default::default()
        };

        let fullnode_args = FullnodeArgs {
            fullnode_rpc_url: Some(fullnode_rpc_url.clone()),
        };

        let handle = start_graphql(
            database_url.clone(),
            None,
            fullnode_args,
            DbArgs::default(),
            BigtableArgs::default(),
            consistent_reader_args,
            graphql_args,
            SystemPackageTaskArgs::default(),
            "0.0.0",
            GraphQlConfig::default(),
            pipelines,
            &prometheus_registry,
            cancel.child_token(),
        )
        .await
        .context("Failed to start GraphQL server")?;
        rpc_services.push(handle);

        info!("GraphQL started at {address}");
    }

    if let Some(input) = with_faucet {
        let faucet_address = parse_host_port(input, DEFAULT_FAUCET_PORT)
            .map_err(|_| anyhow!("Invalid faucet host and port"))?;
        info!("Starting the faucet service at {faucet_address}");

        let host_ip = match faucet_address {
            SocketAddr::V4(addr) => *addr.ip(),
            _ => bail!("Faucet configuration requires an IPv4 address"),
        };

        let config = FaucetConfig {
            host_ip,
            port: faucet_address.port(),
            amount: DEFAULT_FAUCET_MIST_AMOUNT,
            ..Default::default()
        };

        if force_regenesis {
            let kp = swarm.config_mut().account_keys.swap_remove(0);
            let keystore_path = config_dir.join(SUI_KEYSTORE_FILENAME);
            let mut keystore =
                Keystore::from(FileBasedKeystore::load_or_create(&keystore_path).unwrap());
            let address: SuiAddress = kp.public().into();
            keystore
                .import(None, SuiKeyPair::Ed25519(kp))
                .await
                .unwrap();
            SuiClientConfig {
                keystore,
                external_keys: None,
                envs: vec![SuiEnv {
                    alias: "localnet".to_string(),
                    rpc: fullnode_rpc_url,
                    ws: None,
                    basic_auth: None,
                    chain_id: None,
                }],
                active_address: Some(address),
                active_env: Some("localnet".to_string()),
            }
            .persisted(config_dir.join(SUI_CLIENT_CONFIG).as_path())
            .save()
            .unwrap();
        }

        let local_faucet = LocalFaucet::new(
            create_wallet_context(config.wallet_client_timeout_secs, config_dir.clone())?,
            config.clone(),
        )
        .await?;

        let app_state = Arc::new(AppState {
            faucet: local_faucet,
            config,
        });

        start_faucet(app_state).await?;
    }

    // Run health check loop until Ctrl+C or failure
    let mut interval = interval(Duration::from_secs(3));
    let mut unhealthy = 0;

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Received Ctrl+C, shutting down...");
                break;
            }
            _ = interval.tick() => {}
        }

        for node in swarm.validator_nodes() {
            let Err(e) = node.health_check(true).await else {
                unhealthy = 0;
                break;
            };

            unhealthy += 1;
            if unhealthy > 3 {
                return Err(e.into());
            }
        }
    }

    // Trigger cancellation to shut down all RPC services, and wait for all services to exit
    // cleanly.
    cancel.cancel();
    future::join_all(rpc_services).await;
    Ok(())
}

async fn genesis(
    from_config: Option<PathBuf>,
    write_config: Option<PathBuf>,
    working_dir: Option<PathBuf>,
    force: bool,
    epoch_duration_ms: Option<u64>,
    benchmark_ips: Option<Vec<String>>,
    with_faucet: bool,
    committee_size: Option<usize>,
) -> Result<(), anyhow::Error> {
    let sui_config_dir = &match working_dir {
        // if a directory is specified, it must exist (it
        // will not be created)
        Some(v) => v,
        // create default Sui config dir if not specified
        // on the command line and if it does not exist
        // yet
        None => {
            let config_path = sui_config_dir()?;
            fs::create_dir_all(&config_path)?;
            config_path
        }
    };

    // if Sui config dir is not empty then either clean it
    // up (if --force/-f option was specified or report an
    // error
    let dir = sui_config_dir.read_dir().map_err(|err| {
        anyhow!(err).context(format!("Cannot open Sui config dir {:?}", sui_config_dir))
    })?;
    let files = dir.collect::<Result<Vec<_>, _>>()?;

    let client_path = sui_config_dir.join(SUI_CLIENT_CONFIG);
    let keystore_path = sui_config_dir.join(SUI_KEYSTORE_FILENAME);

    if write_config.is_none() && !files.is_empty() {
        if force {
            // check old keystore and client.yaml is compatible
            let is_compatible = FileBasedKeystore::load_or_create(&keystore_path).is_ok()
                && PersistedConfig::<SuiClientConfig>::read(&client_path).is_ok();
            // Keep keystore and client.yaml if they are compatible
            if is_compatible {
                for file in files {
                    let path = file.path();
                    if path != client_path && path != keystore_path {
                        if path.is_file() {
                            fs::remove_file(path)
                        } else {
                            fs::remove_dir_all(path)
                        }
                        .map_err(|err| {
                            anyhow!(err).context(format!("Cannot remove file {:?}", file.path()))
                        })?;
                    }
                }
            } else {
                fs::remove_dir_all(sui_config_dir).map_err(|err| {
                    anyhow!(err)
                        .context(format!("Cannot remove Sui config dir {:?}", sui_config_dir))
                })?;
                fs::create_dir(sui_config_dir).map_err(|err| {
                    anyhow!(err)
                        .context(format!("Cannot create Sui config dir {:?}", sui_config_dir))
                })?;
            }
        } else if files.len() != 2 || !client_path.exists() || !keystore_path.exists() {
            bail!(
                "Cannot run genesis with non-empty Sui config directory {}, please use the --force/-f option to remove the existing configuration",
                sui_config_dir.to_str().unwrap()
            );
        }
    }

    let network_path = sui_config_dir.join(SUI_NETWORK_CONFIG);
    let genesis_path = sui_config_dir.join(SUI_GENESIS_FILENAME);

    let mut genesis_conf = match from_config {
        Some(path) => PersistedConfig::read(&path)?,
        None => {
            if let Some(ips) = benchmark_ips {
                // Make a keystore containing the key for the genesis gas object.
                let path = sui_config_dir.join(SUI_BENCHMARK_GENESIS_GAS_KEYSTORE_FILENAME);
                let mut keystore = FileBasedKeystore::load_or_create(&path)?;
                for gas_key in GenesisConfig::benchmark_gas_keys(ips.len()) {
                    keystore.import(None, gas_key).await?;
                }
                keystore.save().await?;

                // Make a new genesis config from the provided ip addresses.
                GenesisConfig::new_for_benchmarks(&ips)
            } else if keystore_path.exists() {
                let existing_keys = FileBasedKeystore::load_or_create(&keystore_path)?.addresses();
                GenesisConfig::for_local_testing_with_addresses(existing_keys)
            } else {
                GenesisConfig::for_local_testing()
            }
        }
    };

    // Adds an extra faucet account to the genesis
    if with_faucet {
        info!("Adding faucet account in genesis config...");
        genesis_conf = genesis_conf.add_faucet_account();
    }

    if let Some(path) = write_config {
        let persisted = genesis_conf.persisted(&path);
        persisted.save()?;
        return Ok(());
    }

    let validator_info = genesis_conf.validator_config_info.take();
    let ssfn_info = genesis_conf.ssfn_config_info.take();

    let builder = ConfigBuilder::new(sui_config_dir);
    if let Some(epoch_duration_ms) = epoch_duration_ms {
        genesis_conf.parameters.epoch_duration_ms = epoch_duration_ms;
    }
    let committee_size = match committee_size {
        Some(x) => NonZeroUsize::new(x),
        None => NonZeroUsize::new(1),
    }
    .ok_or_else(|| anyhow!("Committee size must be at least 1."))?;

    let mut network_config = if let Some(validators) = validator_info {
        builder
            .with_genesis_config(genesis_conf)
            .with_validators(validators)
            .build()
    } else {
        builder
            .committee_size(committee_size)
            .with_genesis_config(genesis_conf)
            .build()
    };

    let mut keystore = FileBasedKeystore::load_or_create(&keystore_path)?;
    for key in &network_config.account_keys {
        keystore
            .import(None, SuiKeyPair::Ed25519(key.copy()))
            .await?;
    }
    let active_address = keystore.addresses().pop();

    network_config.genesis.save(&genesis_path)?;
    for validator in &mut network_config.validator_configs {
        validator.genesis = sui_config::node::Genesis::new_from_file(&genesis_path);
    }

    info!("Network genesis completed.");
    network_config.save(&network_path)?;
    info!("Network config file is stored in {:?}.", network_path);

    info!("Client keystore is stored in {:?}.", keystore_path);

    let fullnode_config = FullnodeConfigBuilder::new()
        .with_config_directory(FULL_NODE_DB_PATH.into())
        .with_rpc_addr(sui_config::node::default_json_rpc_address())
        .build(&mut OsRng, &network_config);

    fullnode_config.save(sui_config_dir.join(SUI_FULLNODE_CONFIG))?;
    let mut ssfn_nodes = vec![];
    if let Some(ssfn_info) = ssfn_info {
        for (i, ssfn) in ssfn_info.into_iter().enumerate() {
            let path =
                sui_config_dir.join(sui_config::ssfn_config_file(ssfn.p2p_address.clone(), i));
            // join base fullnode config with each SsfnGenesisConfig entry
            let ssfn_config = FullnodeConfigBuilder::new()
                .with_config_directory(FULL_NODE_DB_PATH.into())
                .with_p2p_external_address(ssfn.p2p_address)
                .with_network_key_pair(ssfn.network_key_pair)
                .with_p2p_listen_address("0.0.0.0:8084".parse().unwrap())
                .with_db_path(PathBuf::from("/opt/sui/db/authorities_db/full_node_db"))
                .with_network_address("/ip4/0.0.0.0/tcp/8080/http".parse().unwrap())
                .with_metrics_address("0.0.0.0:9184".parse().unwrap())
                .with_admin_interface_port(1337)
                .with_json_rpc_address("0.0.0.0:9000".parse().unwrap())
                .with_genesis(Genesis::new_from_file("/opt/sui/config/genesis.blob"))
                .build(&mut OsRng, &network_config);
            ssfn_nodes.push(ssfn_config.clone());
            ssfn_config.save(path)?;
        }

        let ssfn_seed_peers: Vec<SeedPeer> = ssfn_nodes
            .iter()
            .map(|config| SeedPeer {
                peer_id: Some(anemo::PeerId(
                    config.network_key_pair().public().0.to_bytes(),
                )),
                address: config.p2p_config.external_address.clone().unwrap(),
            })
            .collect();

        for (i, mut validator) in network_config
            .into_validator_configs()
            .into_iter()
            .enumerate()
        {
            let path = sui_config_dir.join(sui_config::validator_config_file(
                validator.network_address.clone(),
                i,
            ));
            let mut val_p2p = validator.p2p_config.clone();
            val_p2p.seed_peers = ssfn_seed_peers.clone();
            validator.p2p_config = val_p2p;
            validator.save(path)?;
        }
    } else {
        for (i, validator) in network_config
            .into_validator_configs()
            .into_iter()
            .enumerate()
        {
            let path = sui_config_dir.join(sui_config::validator_config_file(
                validator.network_address.clone(),
                i,
            ));
            validator.save(path)?;
        }
    }

    let mut client_config = if client_path.exists() {
        PersistedConfig::read(&client_path)?
    } else {
        SuiClientConfig::new(keystore.into())
    };

    if client_config.active_address.is_none() {
        client_config.active_address = active_address;
    }

    // On windows, using 0.0.0.0 will usually yield in an networking error. This localnet ip
    // address must bind to 127.0.0.1 if the default 0.0.0.0 is used.
    let localnet_ip =
        if fullnode_config.json_rpc_address.ip() == IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)) {
            "127.0.0.1".to_string()
        } else {
            fullnode_config.json_rpc_address.ip().to_string()
        };
    client_config.add_env(SuiEnv {
        alias: "localnet".to_string(),
        rpc: format!(
            "http://{}:{}",
            localnet_ip,
            fullnode_config.json_rpc_address.port()
        ),
        ws: None,
        basic_auth: None,
        chain_id: None,
    });
    client_config.add_env(SuiEnv::devnet());

    if client_config.active_env.is_none() {
        client_config.active_env = client_config.envs.first().map(|env| env.alias.clone());
    }

    client_config.save(&client_path)?;
    info!("Client config file is stored in {:?}.", client_path);

    Ok(())
}

async fn prompt_if_no_config(
    wallet_conf_path: &Path,
    accept_defaults: bool,
) -> Result<(), anyhow::Error> {
    // Prompt user for connect to devnet fullnode if config does not exist.
    if !wallet_conf_path.exists() {
        let env = match std::env::var_os("SUI_CONFIG_WITH_RPC_URL") {
            Some(v) => Some(SuiEnv {
                alias: "custom".to_string(),
                rpc: v.into_string().unwrap(),
                ws: None,
                basic_auth: None,
                chain_id: None,
            }),
            None => {
                if accept_defaults {
                    print!(
                        "Creating config file [{:?}] with default (devnet) Full node server and ed25519 key scheme.",
                        wallet_conf_path
                    );
                } else {
                    print!(
                        "Config file [{:?}] doesn't exist, do you want to connect to a Sui Full node server [y/N]?",
                        wallet_conf_path
                    );
                }
                if accept_defaults
                    || matches!(read_line(), Ok(line) if line.trim().to_lowercase() == "y")
                {
                    let url = if accept_defaults {
                        String::new()
                    } else {
                        print!(
                            "Sui Full node server URL (Defaults to Sui Testnet if not specified) : "
                        );
                        read_line()?
                    };
                    Some(if url.trim().is_empty() {
                        SuiEnv::testnet()
                    } else {
                        print!("Environment alias for [{url}] : ");
                        let alias = read_line()?;
                        let alias = if alias.trim().is_empty() {
                            "custom".to_string()
                        } else {
                            alias
                        };
                        SuiEnv {
                            alias,
                            rpc: url,
                            ws: None,
                            basic_auth: None,
                            chain_id: None,
                        }
                    })
                } else {
                    None
                }
            }
        };

        if let Some(env) = env {
            let keystore_path = match wallet_conf_path.parent() {
                // Wallet config was created in the current directory as a relative path.
                Some(parent) if parent.as_os_str().is_empty() => {
                    std::env::current_dir().context("Couldn't find current directory")?
                }

                // Wallet config was given a path with some parent (could be relative or absolute).
                Some(parent) => parent
                    .canonicalize()
                    .context("Could not find sui config directory")?,

                // No parent component and the wallet config was the empty string, use the default
                // config.
                None if wallet_conf_path.as_os_str().is_empty() => sui_config_dir()?,

                // Wallet config was requested at the root of the file system ...for some reason.
                None => wallet_conf_path.to_owned(),
            }
            .join(SUI_KEYSTORE_FILENAME);

            let mut keystore = Keystore::from(FileBasedKeystore::load_or_create(&keystore_path)?);
            let key_scheme = if accept_defaults {
                SignatureScheme::ED25519
            } else {
                println!(
                    "Select key scheme to generate keypair (0 for ed25519, 1 for secp256k1, 2: for secp256r1):"
                );
                match SignatureScheme::from_flag(read_line()?.trim()) {
                    Ok(s) => s,
                    Err(e) => return Err(anyhow!("{e}")),
                }
            };

            let (new_address, key_pair, scheme, phrase) = generate_new_key(key_scheme, None, None)?;
            keystore.import(None, key_pair).await?;
            let alias = keystore.get_alias(&new_address)?;
            println!(
                "Generated new keypair and alias for address with scheme {:?} [{alias}: {new_address}]",
                scheme.to_string()
            );
            println!("Secret Recovery Phrase : [{phrase}]");
            let alias = env.alias.clone();
            SuiClientConfig {
                keystore,
                external_keys: None,
                envs: vec![env],
                active_address: Some(new_address),
                active_env: Some(alias),
            }
            .persisted(wallet_conf_path)
            .save()?;

            let context = WalletContext::new(wallet_conf_path)?;
            let _ = context.cache_chain_id(&context.get_client().await?).await?;
        }
    }
    Ok(())
}

fn read_line() -> Result<String, anyhow::Error> {
    let mut s = String::new();
    let _ = stdout().flush();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim_end().to_string())
}

/// Get the currently configured client, and the chain ID for that client.
async fn get_chain_id_and_client(
    client_config: SuiEnvConfig,
    command_err_string: &str,
) -> anyhow::Result<(Option<String>, Option<SuiClient>)> {
    let config = client_config
        .config
        .unwrap_or(sui_config_dir()?.join(SUI_CLIENT_CONFIG));
    prompt_if_no_config(&config, false).await?;
    let mut context = WalletContext::new(&config)?;

    if let Some(env_override) = client_config.env {
        context = context.with_env_override(env_override);
    }

    let Ok(client) = context.get_client().await else {
        bail!(
            "`{command_err_string}` requires a connection to the network. \
             Current active network is {} but failed to connect to it.",
            context.config.active_env.as_ref().unwrap()
        );
    };

    if let Err(e) = client.check_api_version() {
        eprintln!("{}", format!("[warning] {e}").yellow().bold());
    }

    Ok((
        client.read_api().get_chain_identifier().await.ok(),
        Some(client),
    ))
}

/// Try to resolve an ObjectID to a MovePackage
async fn resolve_package(reader: &ReadApi, package_id: ObjectID) -> anyhow::Result<MovePackage> {
    let object = reader
        .get_object_with_options(package_id, SuiObjectDataOptions::bcs_lossless())
        .await?
        .into_object()?;

    let Some(SuiRawData::Package(package)) = object.bcs else {
        bail!("Object {} is not a package.", package_id);
    };

    Ok(MovePackage::new(
        package.id,
        package.version,
        package.module_map,
        // This package came from on-chain and the tool runs locally, so don't worry about
        // trying to enforce the package size limit.
        u64::MAX,
        package.type_origin_table,
        package.linkage_table,
    )?)
}

/// Download the package's modules and its dependencies to the specified path.
async fn download_package_and_deps_under(
    read_api: &ReadApi,
    path: &Path,
    package_id: ObjectID,
) -> anyhow::Result<PackageSummaryMetadata> {
    let mut dependencies = BTreeMap::new();
    let mut linkage = BTreeMap::new();
    let mut type_origins = BTreeMap::new();

    let root_package = resolve_package(read_api, package_id).await?;
    for (original_id, pkg_info) in root_package.linkage_table().iter() {
        let package = resolve_package(read_api, pkg_info.upgraded_id).await?;
        let relative_package_path = package
            .id()
            .deref()
            .to_canonical_string(/* with_prefix */ true);

        let package_path = path.join(&relative_package_path);
        fs::create_dir_all(&package_path)?;
        for (m_name, module) in package.serialized_module_map() {
            let mut file = fs::File::create(
                package_path
                    .join(m_name)
                    .with_extension(MOVE_COMPILED_EXTENSION),
            )?;
            file.write_all(module)?;
        }

        dependencies.insert(*original_id, PathBuf::from(relative_package_path));
        linkage.insert(*original_id, pkg_info.clone());
        type_origins.insert(*original_id, package.type_origin_table().clone());
    }

    let package_path = path.join(
        root_package
            .id()
            .deref()
            .to_canonical_string(/* with_prefix */ true),
    );
    fs::create_dir_all(&package_path)?;
    for (m_name, module) in root_package.serialized_module_map() {
        let file_path = package_path
            .join(m_name)
            .with_extension(MOVE_COMPILED_EXTENSION);
        let mut file = fs::File::create(&file_path)?;
        file.write_all(module).with_context(|| {
            format!(
                "Unable to write module {m_name} for package {} to {}",
                root_package
                    .id()
                    .deref()
                    .to_canonical_string(/* with_prefix */ true),
                file_path.display(),
            )
        })?;
    }

    Ok(PackageSummaryMetadata {
        root_package_id: Some(root_package.id()),
        root_package_original_id: Some(root_package.original_package_id()),
        root_package_version: Some(root_package.version().value()),
        type_origins: Some(type_origins),
        dependencies: Some(dependencies),
        linkage: Some(linkage),
    })
}

/// Parse the input string into a SocketAddr, with a default port if none is provided.
pub fn parse_host_port(
    input: String,
    default_port_if_missing: u16,
) -> Result<SocketAddr, AddrParseError> {
    let default_host = "0.0.0.0";
    let mut input = input;
    if input.contains("localhost") {
        input = input.replace("localhost", "127.0.0.1");
    }
    if input.contains(':') {
        input.parse::<SocketAddr>()
    } else if input.contains('.') {
        format!("{input}:{default_port_if_missing}").parse::<SocketAddr>()
    } else if input.is_empty() {
        format!("{default_host}:{default_port_if_missing}").parse::<SocketAddr>()
    } else if !input.is_empty() {
        format!("{default_host}:{input}").parse::<SocketAddr>()
    } else {
        format!("{default_host}:{default_port_if_missing}").parse::<SocketAddr>()
    }
}

/// Get the replay node representing a specific chain (e.g., testnet, mainnet, or custom)
/// from a given wallet context contining chain identifier string.
pub async fn get_replay_node(context: &WalletContext) -> Result<SR2::Node, anyhow::Error> {
    let chain_id = context
        .get_client()
        .await?
        .read_api()
        .get_chain_identifier()
        .await?;
    let err_msg = format!(
        "'{chain_id}' chain identifier is not supported for replay -- only testnet and mainnet are supported currently"
    );
    let chain_id = ChainIdentifier::from_chain_short_id(&chain_id)
        .ok_or_else(|| anyhow::anyhow!(err_msg.clone()))?;
    Ok(match chain_id.chain() {
        Chain::Mainnet => SR2::Node::Mainnet,
        Chain::Testnet => SR2::Node::Testnet,
        Chain::Unknown => bail!(err_msg),
    })
}
