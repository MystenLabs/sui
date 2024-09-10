// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]
mod benchmark_client;
use benchmark_client::{parse_url, url_to_multiaddr, Client, OperatingMode};
use clap::{Parser, Subcommand};
use config::{
    Committee, CommitteeBuilder, Epoch, Export, Import, Parameters, PrometheusMetricsParameters,
    WorkerCache, WorkerId, WorkerIndex, WorkerInfo,
};
use crypto::{KeyPair, NetworkKeyPair};
use eyre::{Context, Result};
use fastcrypto::traits::{EncodeDecodeBase64, KeyPair as _};
use futures::join;
use mysten_metrics::start_prometheus_server;
use narwhal_node as node;
use narwhal_node::metrics::NarwhalBenchMetrics;
use narwhal_node::primary_node::PrimaryNode;
use narwhal_node::worker_node::WorkerNode;
use network::client::NetworkClient;
use node::{
    execution_state::SimpleExecutionState,
    metrics::{primary_metrics_registry, worker_metrics_registry},
};
use prometheus::Registry;
use rand::{rngs::StdRng, SeedableRng};
use std::sync::Arc;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use storage::{CertificateStoreCacheMetrics, NodeStorage};
use sui_keys::keypair_file::{
    read_authority_keypair_from_file, read_network_keypair_from_file,
    write_authority_keypair_to_file, write_keypair_to_file,
};
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::{
    crypto::{
        get_key_pair_from_rng, AuthorityKeyPair, AuthorityPublicKey, NetworkPublicKey, SuiKeyPair,
    },
    multiaddr::Multiaddr,
};
use telemetry_subscribers::TelemetryGuards;
use tokio::sync::mpsc::channel;
use tokio::time::Duration;
use url::Url;

// TODO: remove when old benchmark code is removed
// #[cfg(feature = "benchmark")]
// use tracing::subscriber::set_global_default;
// #[cfg(feature = "benchmark")]
// use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing::{info, warn};
use worker::{LazyNarwhalClient, TrivialTransactionValidator};

#[derive(Parser)]
#[command(author, version, about)]
/// A production implementation of Narwhal and Bullshark.
struct App {
    #[arg(short, action = clap::ArgAction::Count)]
    /// Sets the level of verbosity
    verbosity: u8,
    #[command(subcommand)]
    command: Commands,
}

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
enum Commands {
    /// Generate a committee, workers and the parameters config files of all validators
    /// from a list of initial peers. This is only suitable for benchmarks as it exposes all keys.
    BenchmarkGenesis {
        #[clap(
            long,
            value_name = "ADDR",
            num_args(1..),
            value_delimiter = ',',
            help = "A list of ip addresses to generate a genesis suitable for benchmarks"
        )]
        ips: Vec<String>,
        /// The working directory where the files will be generated.
        #[clap(long, value_name = "FILE", default_value = "genesis")]
        working_directory: PathBuf,
        /// The number of workers per authority
        #[clap(long, value_name = "NUM", default_value = "1")]
        num_workers: usize,
        /// The base port
        #[clap(long, value_name = "PORT", default_value = "5000")]
        base_port: usize,
    },
    /// Save an encoded bls12381 keypair (Base64 encoded `privkey`) to file
    GenerateKeys {
        /// The file where to save the encoded authority key pair
        #[arg(long)]
        filename: PathBuf,
    },
    /// Save an encoded ed25519 network keypair (Base64 encoded `flag || privkey`) to file
    GenerateNetworkKeys {
        /// The file where to save the encoded network key pair
        #[arg(long)]
        filename: PathBuf,
    },
    /// Get the public key from a keypair file
    GetPubKey {
        /// The file where the keypair is stored
        #[arg(long)]
        filename: PathBuf,
    },
    /// Run a node
    Run {
        /// The file containing the node's primary keys
        #[arg(long)]
        primary_keys: PathBuf,
        /// The file containing the node's primary network keys
        #[arg(long)]
        primary_network_keys: PathBuf,
        /// The file containing the node's worker network keys
        #[arg(long)]
        worker_keys: PathBuf,
        /// The file containing committee information
        #[arg(long)]
        committee: String,
        /// The file containing worker information
        #[arg(long)]
        workers: String,
        /// The file containing the node parameters
        #[arg(long)]
        parameters: Option<String>,
        /// The path where to create the data store
        #[arg(long)]
        store: PathBuf,

        #[command(subcommand)]
        subcommand: NodeType,
    },
}

#[derive(Subcommand)]
enum NodeType {
    /// Run a single primary
    Primary,
    /// Run a single worker
    Worker {
        /// The worker Id
        id: WorkerId,
    },
    /// Run a primary & worker in the same process as part of benchmark
    Benchmark {
        /// The worker Id
        #[clap(long, value_name = "NUM")]
        worker_id: WorkerId,
        /// The network address of the node where to send txs. A url format is expected ex 'http://127.0.0.1:7000'
        #[clap(long, value_parser = parse_url, global = true)]
        addr: Url,
        /// The size of each transaciton in bytes
        #[clap(long, default_value = "512", global = true)]
        size: usize,
        /// The rate (txs/s) at which to send the transactions
        #[clap(long, default_value = "100", global = true)]
        rate: u64,
        /// Optional duration of the benchmark in seconds. If not provided the benchmark will run forever.
        #[clap(long, global = true)]
        duration: Option<u64>,
        /// Network addresses that must be reachable before starting the benchmark.
        #[clap(long, value_delimiter = ',', value_parser = parse_url, global = true)]
        nodes: Vec<Url>,
    },
}

#[tokio::main]
async fn main() -> Result<(), eyre::Report> {
    let app = App::parse();

    let tracing_level = match app.verbosity {
        0 => "error",
        1 => "warn",
        2 => "info",
        3 => "debug",
        _ => "trace",
    };

    // some of the network is very verbose, so we require more 'v's
    let network_tracing_level = match app.verbosity {
        0 | 1 => "error",
        2 => "warn",
        3 => "info",
        4 => "debug",
        _ => "trace",
    };

    let _guard = setup_telemetry(tracing_level, network_tracing_level, None);

    match &app.command {
        Commands::BenchmarkGenesis {
            ips,
            working_directory,
            num_workers,
            base_port,
        } => benchmark_genesis(ips, working_directory, *num_workers, *base_port)?,
        Commands::GenerateKeys { filename } => {
            let keypair: AuthorityKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
            write_authority_keypair_to_file(&keypair, filename).unwrap();
        }
        Commands::GenerateNetworkKeys { filename } => {
            let network_keypair: NetworkKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
            write_keypair_to_file(&SuiKeyPair::Ed25519(network_keypair), filename).unwrap();
        }
        Commands::GetPubKey { filename } => {
            match read_network_keypair_from_file(filename) {
                Ok(keypair) => {
                    // Network keypair file is stored as `flag || privkey`.
                    println!("{:?}", keypair.public())
                }
                Err(_) => {
                    // Authority keypair file is stored as `privkey`.
                    match read_authority_keypair_from_file(filename) {
                        Ok(kp) => println!("{:?}", kp.public()),
                        Err(e) => {
                            println!("Failed to read keypair at path {:?} err: {:?}", filename, e)
                        }
                    }
                }
            }
        }
        Commands::Run {
            primary_keys,
            primary_network_keys,
            worker_keys,
            committee,
            workers,
            parameters,
            store,
            subcommand,
        } => {
            let primary_keypair = read_authority_keypair_from_file(primary_keys)
                .expect("Failed to load the node's primary keypair");
            let primary_network_keypair = read_network_keypair_from_file(primary_network_keys)
                .expect("Failed to load the node's primary network keypair");
            let worker_keypair = read_network_keypair_from_file(worker_keys)
                .expect("Failed to load the node's worker keypair");

            let mut committee =
                Committee::import(committee).context("Failed to load the committee information")?;
            committee.load();

            let authority_id = committee
                .authority_by_key(primary_keypair.public())
                .unwrap()
                .id();

            let (primary_registry, worker_registry) = match subcommand {
                NodeType::Primary => (Some(primary_metrics_registry(authority_id)), None),
                NodeType::Worker { id } => (None, Some(worker_metrics_registry(*id, authority_id))),
                NodeType::Benchmark {
                    worker_id,
                    size: _,
                    rate: _,
                    duration: _,
                    nodes: _,
                    addr: _,
                } => (
                    Some(primary_metrics_registry(authority_id)),
                    Some(worker_metrics_registry(*worker_id, authority_id)),
                ),
            };

            // TODO: re-enable telemetry if needed, otherwise remove when old benchmark code is removed
            // In benchmarks, transactions are not deserializable => many errors at the debug level
            // Moreover, we need RFC 3339 timestamps to parse properly => we use a custom subscriber.
            // cfg_if::cfg_if! {
            //     if #[cfg(feature = "benchmark")] {
            //         setup_benchmark_telemetry(tracing_level, network_tracing_level)?;
            //     } else {
            //         let _guard = setup_telemetry(tracing_level, network_tracing_level, Some(&registry));
            //     }
            // }
            run(
                subcommand,
                workers,
                parameters.as_deref(),
                store,
                committee,
                primary_keypair,
                primary_network_keypair,
                worker_keypair,
                primary_registry,
                worker_registry,
            )
            .await?
        }
    }

    Ok(())
}

/// Generate all the genesis files required for benchmarks.
fn benchmark_genesis(
    ips: &[String],
    working_directory: &PathBuf,
    num_workers: usize,
    base_port: usize,
) -> Result<()> {
    tracing::info!("Generating benchmark genesis files");
    fs::create_dir_all(working_directory).wrap_err(format!(
        "Failed to create directory '{}'",
        working_directory.display()
    ))?;

    // Use rng seed so that runs across multiple instances generate the same configs.
    let mut rng = StdRng::seed_from_u64(0);

    // Generate primary keys
    let mut primary_names = Vec::new();
    let primary_key_files = (0..ips.len())
        .map(|i| {
            let mut path = working_directory.clone();
            path.push(format!("primary-{}-key.json", i));
            path
        })
        .collect::<Vec<_>>();

    for filename in primary_key_files {
        let keypair: AuthorityKeyPair = get_key_pair_from_rng(&mut rng).1;
        write_authority_keypair_to_file(&keypair, filename).unwrap();
        let pk = keypair.public().to_string();
        primary_names.push(pk);
    }

    // Generate primary network keys
    let mut primary_network_names = Vec::new();
    let primary_network_key_files = (0..ips.len())
        .map(|i| {
            let mut path = working_directory.clone();
            path.push(format!("primary-{}-network-key.json", i));
            path
        })
        .collect::<Vec<_>>();

    for filename in primary_network_key_files {
        let network_keypair: NetworkKeyPair = get_key_pair_from_rng(&mut rng).1;
        let pk = network_keypair.public().to_string();
        write_keypair_to_file(&SuiKeyPair::Ed25519(network_keypair), filename).unwrap();
        primary_network_names.push(pk);
    }

    // todo: add the option for remote workers and multiple workers
    let mut addresses = BTreeMap::new();
    for (pk, (network_pk, ip)) in primary_names
        .iter()
        .zip(primary_network_names.iter().zip(ips.iter()))
    {
        addresses.insert(pk.clone(), (network_pk.clone(), ip.clone()));
    }

    // Generate committee config
    let mut committee_path = working_directory.clone();
    committee_path.push(Committee::DEFAULT_FILENAME);
    let mut committee_builder = CommitteeBuilder::new(Epoch::default());
    for (i, (pk, (network_pk, ip))) in addresses.iter().enumerate() {
        let primary_address: Multiaddr = format!("/ip4/{}/udp/{}", ip, base_port + i)
            .parse()
            .unwrap();
        let protocol_key = AuthorityPublicKey::decode_base64(pk.as_str().trim())?.clone();
        let network_key = NetworkPublicKey::decode_base64(network_pk.as_str().trim())?.clone();
        committee_builder = committee_builder.add_authority(
            protocol_key,
            1, // todo: make stake configurable
            primary_address,
            network_key,
            ip.to_string(),
        );
    }
    let committee = committee_builder.build();
    tracing::info!("Generated committee file: {}", committee_path.display());

    committee
        .export(committee_path.as_path().as_os_str().to_str().unwrap())
        .expect("Failed to export committee file");

    // Generate workers keys
    let mut worker_names = Vec::new();
    // todo: add the option for remote workers and multiple workers
    let worker_key_files = (0..num_workers * ips.len())
        .map(|i| {
            let mut path = working_directory.clone();
            path.push(format!("worker-{i}-key.json"));
            path
        })
        .collect::<Vec<_>>();

    for filename in worker_key_files {
        let network_keypair: NetworkKeyPair = get_key_pair_from_rng(&mut rng).1;
        let pk = network_keypair.public().to_string();
        write_keypair_to_file(&SuiKeyPair::Ed25519(network_keypair), filename).unwrap();
        worker_names.push(pk);
    }

    // Generate workers config
    let mut workers_path = working_directory.clone();
    workers_path.push(WorkerCache::DEFAULT_FILENAME);
    let mut worker_cache = WorkerCache {
        workers: BTreeMap::new(),
        epoch: Epoch::default(),
    };
    // 2 ports used per authority so add 2 * num authorities to base port
    let mut worker_base_port = base_port + (2 * primary_names.len());

    for (i, (pk, ip)) in primary_names.iter().zip(ips.iter()).enumerate() {
        let mut workers = BTreeMap::new();
        for j in 0..num_workers {
            let worker_network_key =
                NetworkPublicKey::decode_base64(worker_names[i * num_workers + j].as_str().trim())?
                    .clone();

            let worker_info = WorkerInfo {
                name: worker_network_key,
                transactions: Multiaddr::try_from(format!("/ip4/{ip}/tcp/{worker_base_port}/http"))
                    .unwrap(),
                worker_address: Multiaddr::try_from(format!(
                    "/ip4/{ip}/udp/{}",
                    worker_base_port + 1
                ))
                .unwrap(),
            };
            worker_base_port += 2;
            workers.insert(j as WorkerId, worker_info);
        }
        let protocol_key = AuthorityPublicKey::decode_base64(pk.as_str().trim())?.clone();
        worker_cache
            .workers
            .insert(protocol_key, WorkerIndex(workers));
    }

    worker_cache
        .export(workers_path.as_path().as_os_str().to_str().unwrap())
        .expect("Failed to export workers file");

    // Generate node parameters config
    let mut parameters_path = working_directory.clone();
    parameters_path.push(Parameters::DEFAULT_FILENAME);
    let parameters = Parameters {
        prometheus_metrics: PrometheusMetricsParameters {
            socket_addr: Multiaddr::try_from(format!(
                "/ip4/0.0.0.0/tcp/{}/http",
                PrometheusMetricsParameters::DEFAULT_PORT
            ))
            .unwrap(),
        },
        ..Default::default()
    };
    parameters
        .export(parameters_path.as_path().as_os_str().to_str().unwrap())
        .expect("Failed to export parameters file");
    tracing::info!(
        "Generated (public) parameters file: {}",
        parameters_path.display()
    );

    Ok(())
}

fn setup_telemetry(
    tracing_level: &str,
    network_tracing_level: &str,
    prom_registry: Option<&Registry>,
) -> TelemetryGuards {
    let log_filter = format!("{tracing_level},h2={network_tracing_level},tower={network_tracing_level},hyper={network_tracing_level},tonic::transport={network_tracing_level},quinn={network_tracing_level}");

    let config = telemetry_subscribers::TelemetryConfig::new()
        // load env variables
        .with_env()
        // load special log filter
        .with_log_level(&log_filter);

    let config = if let Some(reg) = prom_registry {
        config.with_prom_registry(reg)
    } else {
        config
    };

    let (guard, _handle) = config.init();
    guard
}

// TODO: re-enable telemetry if needed, otherwise remove when old benchmark code is removed
// #[cfg(feature = "benchmark")]
// fn setup_benchmark_telemetry(
//     tracing_level: &str,
//     network_tracing_level: &str,
// ) -> Result<(), eyre::Report> {
//     let custom_directive = "narwhal_executor=info";
//     let filter = EnvFilter::builder()
//         .with_default_directive(LevelFilter::INFO.into())
//         .parse(format!(
//             "{tracing_level},h2={network_tracing_level},tower={network_tracing_level},hyper={network_tracing_level},tonic::transport={network_tracing_level},{custom_directive}"
//         ))?;

//     let env_filter = EnvFilter::try_from_default_env().unwrap_or(filter);

//     let timer = tracing_subscriber::fmt::time::UtcTime::rfc_3339();
//     let subscriber_builder = tracing_subscriber::fmt::Subscriber::builder()
//         .with_env_filter(env_filter)
//         .with_timer(timer)
//         .with_ansi(false);
//     let subscriber = subscriber_builder.with_writer(std::io::stderr).finish();
//     set_global_default(subscriber).expect("Failed to set subscriber");
//     Ok(())
// }

// Runs either a worker or a primary.
async fn run(
    node_type: &NodeType,
    workers: &str,
    parameters: Option<&str>,
    store_path: &Path,
    committee: Committee,
    primary_keypair: KeyPair,
    primary_network_keypair: NetworkKeyPair,
    worker_keypair: NetworkKeyPair,
    primary_registry: Option<Registry>,
    worker_registry: Option<Registry>,
) -> Result<(), eyre::Report> {
    // Read the workers and node's keypair from file.
    let worker_cache =
        WorkerCache::import(workers).context("Failed to load the worker information")?;

    // Load default parameters if none are specified.
    let parameters = match parameters {
        Some(filename) => {
            Parameters::import(filename).context("Failed to load the node's parameters")?
        }
        None => Parameters::default(),
    };

    // spin up prometheus server exporter
    let prom_address = parameters.prometheus_metrics.socket_addr.clone();
    info!(
        "Starting Prometheus HTTP metrics endpoint at {}",
        prom_address
    );
    let registry_service = start_prometheus_server(
        prom_address
            .to_socket_addr()
            .expect("failed to convert Multiaddr to SocketAddr"),
    );

    // Make the data store.
    let certificate_store_cache_metrics =
        Arc::new(CertificateStoreCacheMetrics::new(registry_service.clone()));
    let store = NodeStorage::reopen(store_path, Some(certificate_store_cache_metrics.clone()));

    let client = NetworkClient::new_from_keypair(&primary_network_keypair);

    // The channel returning the result for each transaction's execution.
    let (_tx_transaction_confirmation, _rx_transaction_confirmation) = channel(100);

    // Check whether to run a primary, a worker, or an entire benchmark cluster.
    let (primary, worker, client) = match node_type {
        NodeType::Primary => {
            let primary = PrimaryNode::new(parameters.clone(), registry_service.clone());

            primary
                .start(
                    primary_keypair,
                    primary_network_keypair,
                    committee,
                    ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown),
                    worker_cache,
                    client.clone(),
                    &store,
                    SimpleExecutionState::new(_tx_transaction_confirmation),
                )
                .await?;

            (Some(primary), None, None)
        }
        NodeType::Worker { id } => {
            let worker = WorkerNode::new(
                *id,
                ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown),
                parameters.clone(),
                registry_service.clone(),
            );

            worker
                .start(
                    primary_keypair.public().clone(),
                    worker_keypair,
                    committee,
                    worker_cache,
                    client,
                    &store,
                    TrivialTransactionValidator,
                    None,
                )
                .await?;

            (None, Some(worker), None)
        }
        NodeType::Benchmark {
            worker_id,
            size,
            rate,
            duration,
            nodes,
            addr,
        } => {
            let primary = PrimaryNode::new(parameters.clone(), registry_service.clone());

            primary
                .start(
                    primary_keypair.copy(),
                    primary_network_keypair,
                    committee.clone(),
                    ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown),
                    worker_cache.clone(),
                    client.clone(),
                    &store,
                    SimpleExecutionState::new(_tx_transaction_confirmation),
                )
                .await?;

            let worker = WorkerNode::new(
                *worker_id,
                ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown),
                parameters.clone(),
                registry_service.clone(),
            );

            let mut worker_store_path = PathBuf::new();
            if let Some(parent) = store_path.parent() {
                worker_store_path.push(parent);
            }
            if let Some(file_name) = store_path.file_name().and_then(|name| name.to_str()) {
                worker_store_path.push(format!("{}-{}", file_name, worker_id));
            } else {
                worker_store_path.push(format!("worker-db-{}", worker_id));
            }
            let worker_store =
                NodeStorage::reopen(worker_store_path, Some(certificate_store_cache_metrics));

            worker
                .start(
                    primary_keypair.public().clone(),
                    worker_keypair,
                    committee,
                    worker_cache,
                    client,
                    &worker_store,
                    TrivialTransactionValidator,
                    None,
                )
                .await?;

            let registry: Registry = registry_service.default_registry();
            mysten_metrics::init_metrics(&registry);
            let metrics = NarwhalBenchMetrics::new(&registry);

            let target = addr;
            let size = *size;
            let rate = *rate;
            let nodes = nodes.to_vec();
            let operating_mode = OperatingMode::Local;

            let duration: Option<Duration> = match duration {
                Some(d) => {
                    info!("Benchmark Duration: {d}");
                    Some(Duration::from_secs(*d))
                }
                None => None,
            };

            let client = Client {
                target: target.clone(),
                size,
                rate,
                nodes,
                duration,
                metrics,
                local_client: Arc::new(LazyNarwhalClient::new(url_to_multiaddr(addr)?)),
                operating_mode,
            };

            // Waits for all nodes to be online and synchronized and then start benchmark.
            client.start().await?;

            (Some(primary), Some(worker), Some(client))
        }
    };

    if let Some(registry) = worker_registry {
        mysten_metrics::init_metrics(&registry);
        registry_service.add(registry);
    }

    if let Some(registry) = primary_registry {
        mysten_metrics::init_metrics(&registry);
        registry_service.add(registry);
    }

    match (primary, worker, client) {
        (Some(primary), Some(worker), Some(client)) => {
            join!(primary.wait(), worker.wait(), client.wait());
        }
        (Some(primary), None, None) => {
            primary.wait().await;
        }
        (None, Some(worker), None) => {
            worker.wait().await;
        }
        (None, None, None)
        | (None, None, Some(_))
        | (None, Some(_), Some(_))
        | (Some(_), None, Some(_))
        | (Some(_), Some(_), None) => {
            warn!("No primary or worker node was started");
        }
    }

    // If this expression is reached, the program ends and all other tasks terminate.
    Ok(())
}
