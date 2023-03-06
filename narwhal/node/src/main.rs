// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

use clap::{crate_name, crate_version, App, AppSettings, ArgMatches, SubCommand};
use config::{Committee, Import, Parameters, WorkerCache, WorkerId};
use crypto::{KeyPair, NetworkKeyPair};
use eyre::Context;
use fastcrypto::traits::KeyPair as _;
use narwhal_node as node;
use narwhal_node::primary_node::PrimaryNode;
use narwhal_node::worker_node::WorkerNode;
use network::client::NetworkClient;
use node::{
    execution_state::SimpleExecutionState,
    keypair_file::{
        get_key_pair_from_rng, read_authority_keypair_from_file, read_network_keypair_from_file,
        write_authority_keypair_to_file, write_keypair_to_file, AuthorityKeyPair,
    },
};
use std::sync::Arc;
use storage::NodeStorage;
use telemetry_subscribers::TelemetryGuards;
use tokio::sync::mpsc::channel;
use tracing::{info, warn};
use worker::TrivialTransactionValidator;

#[tokio::main]
async fn main() -> Result<(), eyre::Report> {
    let matches = App::new(crate_name!())
        .version(crate_version!())
        .about("A research implementation of Narwhal and Tusk.")
        .args_from_usage("-v... 'Sets the level of verbosity'")
        .subcommand(
            SubCommand::with_name("generate_keys")
                .about("Save an encoded bls12381 keypair (Base64 encoded `privkey`) to file")
                .args_from_usage("--filename=<FILE> 'The file where to save the encoded authority key pair'"),
        )
        .subcommand(
            SubCommand::with_name("generate_network_keys")
            .about("Save an encoded ed25519 network keypair (Base64 encoded `flag || privkey`) to file")
            .args_from_usage("--filename=<FILE> 'The file where to save the encoded network key pair'"),
        )
        .subcommand(
            SubCommand::with_name("get_pub_key")
                .about("Get the public key from a keypair file")
                .args_from_usage("--filename=<FILE> 'The file where the keypair is stored'"),
        )
        .subcommand(
            SubCommand::with_name("run")
                .about("Run a node")
                .args_from_usage("--primary-keys=<FILE> 'The file containing the node's primary keys'")
                .args_from_usage("--primary-network-keys=<FILE> 'The file containing the node's primary network keys'")
                .args_from_usage("--worker-keys=<FILE> 'The file containing the node's worker keys'")
                .args_from_usage("--committee=<FILE> 'The file containing committee information'")
                .args_from_usage("--workers=<FILE> 'The file containing worker information'")
                .args_from_usage("--parameters=[FILE] 'The file containing the node parameters'")
                .args_from_usage("--store=<PATH> 'The path where to create the data store'")
                .subcommand(SubCommand::with_name("primary")
                    .about("Run a single primary")
                    .args_from_usage("-d, --consensus-disabled 'Provide this flag to run a primary node without Tusk'")
                )
                .subcommand(
                    SubCommand::with_name("worker")
                        .about("Run a single worker")
                        .args_from_usage("--id=<INT> 'The worker id'"),
                )
                .setting(AppSettings::SubcommandRequiredElseHelp),
        )
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .get_matches();

    let tracing_level = match matches.occurrences_of("v") {
        0 => "error",
        1 => "warn",
        2 => "info",
        3 => "debug",
        _ => "trace",
    };

    // some of the network is very verbose, so we require more 'v's
    let network_tracing_level = match matches.occurrences_of("v") {
        0 | 1 => "error",
        2 => "warn",
        3 => "info",
        4 => "debug",
        _ => "trace",
    };

    match matches.subcommand() {
        ("generate_keys", Some(sub_matches)) => {
            let _guard = setup_telemetry(tracing_level, network_tracing_level);
            let key_file = sub_matches.value_of("filename").unwrap();
            let keypair: AuthorityKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng);
            write_authority_keypair_to_file(&keypair, key_file).unwrap();
        }
        ("generate_network_keys", Some(sub_matches)) => {
            let _guard = setup_telemetry(tracing_level, network_tracing_level);
            let network_key_file = sub_matches.value_of("filename").unwrap();
            let network_keypair: NetworkKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng);
            write_keypair_to_file(&network_keypair, network_key_file).unwrap();
        }
        ("get_pub_key", Some(sub_matches)) => {
            let _guard = setup_telemetry(tracing_level, network_tracing_level);
            let file = sub_matches.value_of("filename").unwrap();
            match read_network_keypair_from_file(file) {
                Ok(keypair) => {
                    // Network keypair file is stored as `flag || privkey`.
                    println!("{:?}", keypair.public())
                }
                Err(_) => {
                    // Authority keypair file is stored as `privkey`.
                    match read_authority_keypair_from_file(file) {
                        Ok(kp) => println!("{:?}", kp.public()),
                        Err(e) => {
                            println!("Failed to read keypair at path {:?} err: {:?}", file, e)
                        }
                    }
                }
            }
        }
        ("run", Some(sub_matches)) => {
            let primary_key_file = sub_matches.value_of("primary-keys").unwrap();
            let primary_keypair = read_authority_keypair_from_file(primary_key_file)
                .expect("Failed to load the node's primary keypair");
            let primary_network_key_file = sub_matches.value_of("primary-network-keys").unwrap();
            let primary_network_keypair = read_network_keypair_from_file(primary_network_key_file)
                .expect("Failed to load the node's primary network keypair");
            let worker_key_file = sub_matches.value_of("worker-keys").unwrap();
            let worker_keypair = read_network_keypair_from_file(worker_key_file)
                .expect("Failed to load the node's worker keypair");

            let _guard = setup_telemetry(tracing_level, network_tracing_level);

            run(
                sub_matches,
                committee,
                primary_keypair,
                primary_network_keypair,
                worker_keypair,
            )
            .await?
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn setup_telemetry(tracing_level: &str, network_tracing_level: &str) -> TelemetryGuards {
    let log_filter = format!("{tracing_level},h2={network_tracing_level},tower={network_tracing_level},hyper={network_tracing_level},tonic::transport={network_tracing_level},quinn={network_tracing_level}");

    let config = telemetry_subscribers::TelemetryConfig::new()
        // load env variables
        .with_env()
        // load special log filter
        .with_log_level(&log_filter);

    let (guard, _handle) = config.init();
    guard
}

// Runs either a worker or a primary.
async fn run(
    matches: &ArgMatches<'_>,
    committee: Committee,
    primary_keypair: KeyPair,
    primary_network_keypair: NetworkKeyPair,
    worker_keypair: NetworkKeyPair,
) -> Result<(), eyre::Report> {
    // Only enabled if failpoints feature flag is set
    let _failpoints_scenario: fail::FailScenario<'_>;
    if fail::has_failpoints() {
        warn!("Failpoints are enabled");
        _failpoints_scenario = fail::FailScenario::setup();
    } else {
        info!("Failpoints are not enabled");
    }

    let workers_file = matches.value_of("workers").unwrap();
    let parameters_file = matches.value_of("parameters");
    let store_path = matches.value_of("store").unwrap();

    // Read the workers and node's keypair from file.
    let worker_cache =
        WorkerCache::import(workers_file).context("Failed to load the worker information")?;

    // Load default parameters if none are specified.
    let parameters = match parameters_file {
        Some(filename) => {
            Parameters::import(filename).context("Failed to load the node's parameters")?
        }
        None => Parameters::default(),
    };

    // Make the data store.
    let registry_service = RegistryService::new(Registry::new());
    let certificate_store_cache_metrics =
        CertificateStoreCacheMetrics::new(&registry_service.default_registry());

    let store = NodeStorage::reopen(store_path, Some(certificate_store_cache_metrics));

    let client = NetworkClient::new_from_keypair(&primary_network_keypair);

    // The channel returning the result for each transaction's execution.
    let (_tx_transaction_confirmation, _rx_transaction_confirmation) = channel(100);

    // Check whether to run a primary, a worker, or an entire authority.
    let (primary, worker) = match matches.subcommand() {
        // Spawn the primary and consensus core.
        ("primary", Some(sub_matches)) => {
            let primary = PrimaryNode::new(
                parameters.clone(),
                !sub_matches.is_present("consensus-disabled"),
            );

            primary
                .start(
                    primary_keypair,
                    primary_network_keypair,
                    committee,
                    worker_cache,
                    client.clone(),
                    &store,
                    Arc::new(SimpleExecutionState::new(_tx_transaction_confirmation)),
                )
                .await?;

            (Some(primary), None)
        }

        // Spawn a single worker.
        ("worker", Some(sub_matches)) => {
            let id = sub_matches
                .value_of("id")
                .unwrap()
                .parse::<WorkerId>()
                .context("The worker id must be a positive integer")?;

            let worker = WorkerNode::new(id, parameters.clone());

            worker
                .start(
                    primary_keypair.public().clone(),
                    worker_keypair,
                    committee,
                    worker_cache,
                    client,
                    &store,
                    TrivialTransactionValidator::default(),
                )
                .await?;

            (None, Some(worker))
        }
        _ => unreachable!(),
    };

    // spin up prometheus server exporter
    let prom_address = parameters.prometheus_metrics.socket_addr;
    info!(
        "Starting Prometheus HTTP metrics endpoint at {}",
        prom_address
    );

    if let Some(primary) = primary {
        primary.wait().await;
    } else if let Some(worker) = worker {
        worker.wait().await;
    }

    // If this expression is reached, the program ends and all other tasks terminate.
    Ok(())
}
