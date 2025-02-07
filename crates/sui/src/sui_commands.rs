// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::client_commands::SuiClientCommands;
use crate::console::start_console;
use crate::fire_drill::{run_fire_drill, FireDrill};
use crate::genesis_ceremony::{run, Ceremony};
use crate::keytool::KeyToolCommand;
use crate::validator_commands::SuiValidatorCommand;
use anyhow::{anyhow, bail, ensure, Context};
use clap::*;
use colored::Colorize;
use fastcrypto::traits::KeyPair;
use move_analyzer::analyzer;
use move_package::BuildConfig;
use rand::rngs::OsRng;
use std::io::{stderr, stdout, Write};
use std::net::{AddrParseError, IpAddr, Ipv4Addr, SocketAddr};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, io};
use sui_bridge::config::BridgeCommitteeConfig;
use sui_bridge::metrics::BridgeMetrics;
use sui_bridge::sui_client::SuiBridgeClient;
use sui_bridge::sui_transaction_builder::build_committee_register_transaction;
use sui_config::node::Genesis;
use sui_config::p2p::SeedPeer;
use sui_config::{
    genesis_blob_exists, sui_config_dir, Config, PersistedConfig, FULL_NODE_DB_PATH,
    SUI_CLIENT_CONFIG, SUI_FULLNODE_CONFIG, SUI_NETWORK_CONFIG,
};
use sui_config::{
    SUI_BENCHMARK_GENESIS_GAS_KEYSTORE_FILENAME, SUI_GENESIS_FILENAME, SUI_KEYSTORE_FILENAME,
};
use sui_faucet::{create_wallet_context, start_faucet, AppState, FaucetConfig, SimpleFaucet};
use sui_indexer::test_utils::{
    start_indexer_jsonrpc_for_testing, start_indexer_writer_for_testing,
};

use sui_graphql_rpc::{
    config::{ConnectionConfig, ServiceConfig},
    test_infra::cluster::start_graphql_server_with_fn_rpc,
};

use sui_keys::keypair_file::read_key;
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_move::{self, execute_move_command};
use sui_move_build::SuiPackageHooks;
use sui_sdk::sui_client_config::{SuiClientConfig, SuiEnv};
use sui_sdk::wallet_context::WalletContext;
use sui_swarm::memory::Swarm;
use sui_swarm_config::genesis_config::{GenesisConfig, DEFAULT_NUMBER_OF_AUTHORITIES};
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_swarm_config::node_config_builder::FullnodeConfigBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{SignatureScheme, SuiKeyPair, ToFromBytes};
use tempfile::tempdir;
use tracing;
use tracing::info;

const CONCURRENCY_LIMIT: usize = 30;
const DEFAULT_EPOCH_DURATION_MS: u64 = 60_000;
const DEFAULT_FAUCET_NUM_COINS: usize = 5; // 5 coins per request was the default in sui-test-validator
const DEFAULT_FAUCET_MIST_AMOUNT: u64 = 200_000_000_000; // 200 SUI
const DEFAULT_FAUCET_PORT: u16 = 9123;

const DEFAULT_GRAPHQL_PORT: u16 = 9125;

const DEFAULT_INDEXER_PORT: u16 = 9124;

#[derive(Args)]
pub struct IndexerArgs {
    /// Start an indexer with default host and port: 0.0.0.0:9124. This flag accepts also a port,
    /// a host, or both (e.g., 0.0.0.0:9124).
    /// When providing a specific value, please use the = sign between the flag and value:
    /// `--with-indexer=6124` or `--with-indexer=0.0.0.0`, or `--with-indexer=0.0.0.0:9124`
    /// The indexer will be started in writer mode and reader mode.
    #[clap(long,
            default_missing_value = "0.0.0.0:9124",
            num_args = 0..=1,
            require_equals = true,
            value_name = "INDEXER_HOST_PORT",
        )]
    with_indexer: Option<String>,

    /// Start a GraphQL server with default host and port: 0.0.0.0:9125. This flag accepts also a
    /// port, a host, or both (e.g., 0.0.0.0:9125).
    /// When providing a specific value, please use the = sign between the flag and value:
    /// `--with-graphql=6124` or `--with-graphql=0.0.0.0`, or `--with-graphql=0.0.0.0:9125`
    /// Note that GraphQL requires a running indexer, which will be enabled by default if the
    /// `--with-indexer` flag is not set.
    #[clap(
            long,
            default_missing_value = "0.0.0.0:9125",
            num_args = 0..=1,
            require_equals = true,
            value_name = "GRAPHQL_HOST_PORT"
        )]
    with_graphql: Option<String>,

    /// Port for the Indexer Postgres DB. Default port is 5432.
    #[clap(long, default_value = "5432")]
    pg_port: u16,

    /// Hostname for the Indexer Postgres DB. Default host is localhost.
    #[clap(long, default_value = "localhost")]
    pg_host: String,

    /// DB name for the Indexer Postgres DB. Default DB name is sui_indexer.
    #[clap(long, default_value = "sui_indexer")]
    pg_db_name: String,

    /// DB username for the Indexer Postgres DB. Default username is postgres.
    #[clap(long, default_value = "postgres")]
    pg_user: String,

    /// DB password for the Indexer Postgres DB. Default password is postgrespw.
    #[clap(long, default_value = "postgrespw")]
    pg_password: String,
}

impl IndexerArgs {
    pub fn for_testing() -> Self {
        Self {
            with_indexer: None,
            with_graphql: None,
            pg_port: 5432,
            pg_host: "localhost".to_string(),
            pg_db_name: "sui_indexer".to_string(),
            pg_user: "postgres".to_string(),
            pg_password: "postgrespw".to_string(),
        }
    }
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
        indexer_feature_args: IndexerArgs,

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
    /// Start Sui interactive console.
    #[clap(name = "console")]
    Console {
        /// Sets the file storing the state of our user accounts (an empty one will be created if missing)
        #[clap(long = "client.config")]
        config: Option<PathBuf>,
    },
    /// Client for interacting with the Sui network.
    #[clap(name = "client")]
    Client {
        /// Sets the file storing the state of our user accounts (an empty one will be created if missing)
        #[clap(long = "client.config")]
        config: Option<PathBuf>,
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
        /// Sets the file storing the state of our user accounts (an empty one will be created if missing)
        /// Only used when the `--dump-bytecode-as-base64` is set.
        #[clap(long = "client.config")]
        config: Option<PathBuf>,
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

                indexer_feature_args,
                fullnode_rpc_port,
                data_ingestion_dir,
                no_full_node,
                epoch_duration_ms,
                committee_size,
            } => {
                start(
                    config_dir.clone(),
                    with_faucet,
                    indexer_feature_args,
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
                let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
                cmd.execute(&mut keystore).await?.print(!json);
                Ok(())
            }
            SuiCommand::Console { config } => {
                let config = config.unwrap_or(sui_config_dir()?.join(SUI_CLIENT_CONFIG));
                prompt_if_no_config(&config, false).await?;
                let context = WalletContext::new(&config, None, None)?;
                if let Ok(client) = context.get_client().await {
                    if let Err(e) = client.check_api_version() {
                        eprintln!("{}", format!("[warning] {e}").yellow().bold());
                    }
                }
                start_console(context, &mut stdout(), &mut stderr()).await
            }
            SuiCommand::Client {
                config,
                cmd,
                json,
                accept_defaults,
            } => {
                let config_path = config.unwrap_or(sui_config_dir()?.join(SUI_CLIENT_CONFIG));
                prompt_if_no_config(&config_path, accept_defaults).await?;
                if let Some(cmd) = cmd {
                    let mut context = WalletContext::new(&config_path, None, None)?;
                    if let Ok(client) = context.get_client().await {
                        if let Err(e) = client.check_api_version() {
                            eprintln!("{}", format!("[warning] {e}").yellow().bold());
                        }
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
                let mut context = WalletContext::new(&config_path, None, None)?;
                if let Some(cmd) = cmd {
                    if let Ok(client) = context.get_client().await {
                        if let Err(e) = client.check_api_version() {
                            eprintln!("{}", format!("[warning] {e}").yellow().bold());
                        }
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
                match &mut cmd {
                    sui_move::Command::Build(build) if build.dump_bytecode_as_base64 => {
                        if build.ignore_chain {
                            build.chain_id = None;
                        } else {
                            // `sui move build` does not ordinarily require a network connection.
                            // The exception is when --dump-bytecode-as-base64 is specified: In this
                            // case, we should resolve the correct addresses for the respective chain
                            // (e.g., testnet, mainnet) from the Move.lock under automated address management.
                            let config =
                                client_config.unwrap_or(sui_config_dir()?.join(SUI_CLIENT_CONFIG));
                            prompt_if_no_config(&config, false).await?;
                            let context = WalletContext::new(&config, None, None)?;
                            if let Ok(client) = context.get_client().await {
                                if let Err(e) = client.check_api_version() {
                                    eprintln!("{}", format!("[warning] {e}").yellow().bold());
                                }
                            }
                            let client = context.get_client().await?;
                            let chain_id = client.read_api().get_chain_identifier().await.ok();
                            build.chain_id = chain_id.clone();
                        }
                    }
                    _ => (),
                };
                execute_move_command(package_path.as_deref(), build_config, cmd)
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
                let mut context = WalletContext::new(&config_path, None, None)?;
                if let Ok(client) = context.get_client().await {
                    if let Err(e) = client.check_api_version() {
                        eprintln!("{}", format!("[warning] {e}").yellow().bold());
                    }
                }
                let rgp = context.get_reference_gas_price().await?;
                let rpc_url = &context.config.get_active_env()?.rpc;
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
                    context.add_account(None, account_kp.copy());
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
                    let signed_tx = context.sign_transaction(&tx);
                    tasks.push(context.execute_transaction_must_succeed(signed_tx));
                }
                futures::future::join_all(tasks).await;
                Ok(())
            }
            SuiCommand::FireDrill { fire_drill } => run_fire_drill(fire_drill).await,
            SuiCommand::Analyzer => {
                analyzer::run();
                Ok(())
            }
        }
    }
}

/// Starts a local network with the given configuration.
async fn start(
    config: Option<PathBuf>,
    with_faucet: Option<String>,
    indexer_feature_args: IndexerArgs,
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

    let IndexerArgs {
        mut with_indexer,
        with_graphql,
        pg_port,
        pg_host,
        pg_db_name,
        pg_user,
        pg_password,
    } = indexer_feature_args;

    let pg_address = format!("postgres://{pg_user}:{pg_password}@{pg_host}:{pg_port}/{pg_db_name}");

    if with_graphql.is_some() {
        with_indexer = Some(with_indexer.unwrap_or_default());
    }

    if with_indexer.is_some() {
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
            None => NonZeroUsize::new(DEFAULT_NUMBER_OF_AUTHORITIES),
        }
        .ok_or_else(|| anyhow!("Committee size must be at least 1."))?;
        swarm_builder = swarm_builder.committee_size(committee_size);
        let genesis_config = GenesisConfig::custom_genesis(1, 100);
        swarm_builder = swarm_builder.with_genesis_config(genesis_config);
        let epoch_duration_ms = epoch_duration_ms.unwrap_or(DEFAULT_EPOCH_DURATION_MS);
        swarm_builder = swarm_builder.with_epoch_duration_ms(epoch_duration_ms);
        tempdir()?.into_path()
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
    if with_indexer.is_some() && data_ingestion_dir.is_none() {
        data_ingestion_dir = Some(tempdir()?.into_path())
    }

    if let Some(ref dir) = data_ingestion_dir {
        swarm_builder = swarm_builder.with_data_ingestion_dir(dir.clone());
    }

    let mut fullnode_url = sui_config::node::default_json_rpc_address();
    fullnode_url.set_port(fullnode_rpc_port);

    if no_full_node {
        swarm_builder = swarm_builder.with_fullnode_count(0);
    } else {
        swarm_builder = swarm_builder
            .with_fullnode_count(1)
            .with_fullnode_rpc_addr(fullnode_url);
    }

    let mut swarm = swarm_builder.build();
    swarm.launch().await?;
    // Let nodes connect to one another
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    info!("Cluster started");

    // the indexer requires a fullnode url with protocol specified
    let fullnode_url = format!("http://{}", fullnode_url);
    info!("Fullnode URL: {}", fullnode_url);

    if let Some(input) = with_indexer {
        let indexer_address = parse_host_port(input, DEFAULT_INDEXER_PORT)
            .map_err(|_| anyhow!("Invalid indexer host and port"))?;
        info!("Starting the indexer service at {indexer_address}");
        // Start in reader mode
        start_indexer_jsonrpc_for_testing(
            pg_address.clone(),
            fullnode_url.clone(),
            indexer_address.to_string(),
            None,
        )
        .await;
        info!("Indexer started in reader mode");
        start_indexer_writer_for_testing(
            pg_address.clone(),
            None,
            None,
            // We ensured above that this is set to something if --with-indexer is set
            data_ingestion_dir,
            None,
            None, /* start_checkpoint */
            None, /* end_checkpoint */
        )
        .await;
        info!("Indexer started in writer mode");
    }

    if let Some(input) = with_graphql {
        let graphql_address = parse_host_port(input, DEFAULT_GRAPHQL_PORT)
            .map_err(|_| anyhow!("Invalid graphql host and port"))?;
        tracing::info!("Starting the GraphQL service at {graphql_address}");
        let graphql_connection_config = ConnectionConfig {
            port: graphql_address.port(),
            host: graphql_address.ip().to_string(),
            db_url: pg_address,
            ..Default::default()
        };

        start_graphql_server_with_fn_rpc(
            graphql_connection_config,
            Some(fullnode_url.clone()),
            None, // it will be initialized by default
            ServiceConfig::test_defaults(),
        )
        .await;
        info!("GraphQL started");
    }

    if let Some(input) = with_faucet {
        let faucet_address = parse_host_port(input, DEFAULT_FAUCET_PORT)
            .map_err(|_| anyhow!("Invalid faucet host and port"))?;
        tracing::info!("Starting the faucet service at {faucet_address}");

        let host_ip = match faucet_address {
            SocketAddr::V4(addr) => *addr.ip(),
            _ => bail!("Faucet configuration requires an IPv4 address"),
        };

        let config = FaucetConfig {
            host_ip,
            port: faucet_address.port(),
            num_coins: DEFAULT_FAUCET_NUM_COINS,
            amount: DEFAULT_FAUCET_MIST_AMOUNT,
            ..Default::default()
        };

        let prometheus_registry = prometheus::Registry::new();
        if force_regenesis {
            let kp = swarm.config_mut().account_keys.swap_remove(0);
            let keystore_path = config_dir.join(SUI_KEYSTORE_FILENAME);
            let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
            let address: SuiAddress = kp.public().into();
            keystore.add_key(None, SuiKeyPair::Ed25519(kp)).unwrap();
            SuiClientConfig {
                keystore,
                envs: vec![SuiEnv {
                    alias: "localnet".to_string(),
                    rpc: fullnode_url,
                    ws: None,
                    basic_auth: None,
                }],
                active_address: Some(address),
                active_env: Some("localnet".to_string()),
            }
            .persisted(config_dir.join(SUI_CLIENT_CONFIG).as_path())
            .save()
            .unwrap();
        }
        let faucet_wal = config_dir.join("faucet.wal");
        let simple_faucet = SimpleFaucet::new(
            create_wallet_context(config.wallet_client_timeout_secs, config_dir)?,
            &prometheus_registry,
            faucet_wal.as_path(),
            config.clone(),
        )
        .await
        .unwrap();

        let app_state = Arc::new(AppState {
            faucet: simple_faucet,
            config,
        });

        start_faucet(app_state, CONCURRENCY_LIMIT, &prometheus_registry).await?;
    }

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(3));
    let mut unhealthy_cnt = 0;
    loop {
        for node in swarm.validator_nodes() {
            if let Err(err) = node.health_check(true).await {
                unhealthy_cnt += 1;
                if unhealthy_cnt > 3 {
                    // The network could temporarily go down during reconfiguration.
                    // If we detect a failed validator 3 times in a row, give up.
                    return Err(err.into());
                }
                // Break the inner loop so that we could retry latter.
                break;
            } else {
                unhealthy_cnt = 0;
            }
        }

        interval.tick().await;
    }
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
            let is_compatible = FileBasedKeystore::new(&keystore_path).is_ok()
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
            bail!("Cannot run genesis with non-empty Sui config directory {}, please use the --force/-f option to remove the existing configuration", sui_config_dir.to_str().unwrap());
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
                let mut keystore = FileBasedKeystore::new(&path)?;
                for gas_key in GenesisConfig::benchmark_gas_keys(ips.len()) {
                    keystore.add_key(None, gas_key)?;
                }
                keystore.save()?;

                // Make a new genesis config from the provided ip addresses.
                GenesisConfig::new_for_benchmarks(&ips)
            } else if keystore_path.exists() {
                let existing_keys = FileBasedKeystore::new(&keystore_path)?.addresses();
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
        None => NonZeroUsize::new(DEFAULT_NUMBER_OF_AUTHORITIES),
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

    let mut keystore = FileBasedKeystore::new(&keystore_path)?;
    for key in &network_config.account_keys {
        keystore.add_key(None, SuiKeyPair::Ed25519(key.copy()))?;
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
            }),
            None => {
                if accept_defaults {
                    print!("Creating config file [{:?}] with default (devnet) Full node server and ed25519 key scheme.", wallet_conf_path);
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

            let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
            let key_scheme = if accept_defaults {
                SignatureScheme::ED25519
            } else {
                println!("Select key scheme to generate keypair (0 for ed25519, 1 for secp256k1, 2: for secp256r1):");
                match SignatureScheme::from_flag(read_line()?.trim()) {
                    Ok(s) => s,
                    Err(e) => return Err(anyhow!("{e}")),
                }
            };
            let (new_address, phrase, scheme) =
                keystore.generate_and_add_new_key(key_scheme, None, None, None)?;
            let alias = keystore.get_alias_by_address(&new_address)?;
            println!(
                "Generated new keypair and alias for address with scheme {:?} [{alias}: {new_address}]",
                scheme.to_string()
            );
            println!("Secret Recovery Phrase : [{phrase}]");
            let alias = env.alias.clone();
            SuiClientConfig {
                keystore,
                envs: vec![env],
                active_address: Some(new_address),
                active_env: Some(alias),
            }
            .persisted(wallet_conf_path)
            .save()?;
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
