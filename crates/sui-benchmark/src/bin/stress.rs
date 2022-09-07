// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::{anyhow, Result};
use clap::*;
use futures::future::join_all;
use futures::future::try_join_all;
use prometheus::Registry;
use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use strum_macros::EnumString;
use sui_benchmark::benchmark::follow;
use sui_benchmark::drivers::bench_driver::BenchDriver;
use sui_benchmark::drivers::driver::Driver;
use sui_benchmark::workloads::shared_counter::SharedCounterWorkload;
use sui_benchmark::workloads::transfer_object::TransferObjectWorkload;
use sui_benchmark::workloads::workload::get_latest;
use sui_benchmark::workloads::workload::CombinationWorkload;
use sui_benchmark::workloads::workload::Payload;
use sui_benchmark::workloads::workload::Workload;
use sui_benchmark::workloads::workload::WorkloadType;
use sui_config::gateway::GatewayConfig;
use sui_config::Config;
use sui_config::PersistedConfig;
use sui_core::authority_aggregator::AuthAggMetrics;
use sui_core::authority_aggregator::AuthorityAggregator;
use sui_core::epoch::epoch_store::EpochStore;
use sui_core::gateway_state::GatewayState;
use sui_core::safe_client::SafeClientMetrics;
use sui_node::metrics;
use sui_node::SuiNode;
use sui_sdk::crypto::FileBasedKeystore;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::crypto::EncodeDecodeBase64;
use sui_types::crypto::SuiKeyPair;

use sui_core::authority_client::NetworkAuthorityClientMetrics;
use test_utils::authority::spawn_test_authorities;
use test_utils::authority::test_and_configure_authority_configs;
use test_utils::objects::generate_gas_objects_with_owner;
use test_utils::test_account_keys;
use tokio::runtime::Builder;
use tokio::sync::Barrier;
use tracing::error;

#[derive(Parser)]
#[clap(name = "Stress Testing Framework")]
struct Opts {
    /// Si&ze of the Sui committee.
    #[clap(long, default_value = "4", global = true)]
    pub committee_size: u64,
    /// Num of accounts to use for transfer objects
    #[clap(long, default_value = "5", global = true)]
    pub num_transfer_accounts: u64,
    /// Num server threads
    #[clap(long, default_value = "24", global = true)]
    pub num_server_threads: u64,
    /// Num client threads
    /// ideally same as number of workers
    #[clap(long, default_value = "3", global = true)]
    pub num_client_threads: u64,
    /// Path where gateway config is stored when running remote benchmark
    /// This is also the path where gateway config is stored during local
    /// benchmark
    #[clap(long, default_value = "/tmp/gateway.yaml", global = true)]
    pub gateway_config_path: String,
    /// Path where keypair for primary gas account is stored. The format of
    /// this file is same as what `sui keytool generate` outputs
    #[clap(long, default_value = "", global = true)]
    pub keystore_path: String,
    /// Object id of the primary gas coin used for benchmark
    /// NOTE: THe remote network should have this coin in its genesis config
    /// with large enough gas i.e. u64::MAX
    #[clap(long, default_value = "", global = true)]
    pub primary_gas_id: String,
    #[clap(long, default_value = "5000", global = true)]
    pub primary_gas_objects: u64,
    /// Whether to run local or remote benchmark
    /// NOTE: For running remote benchmark we must have the following
    /// gateway_config_path, keypair_path and primary_gas_id
    #[clap(long, parse(try_from_str), default_value = "true", global = true)]
    pub local: bool,
    /// Default workload is 100% transfer object
    #[clap(subcommand)]
    run_spec: RunSpec,
    #[clap(long, default_value = "9091", global = true)]
    pub server_metric_port: u16,
    #[clap(long, default_value = "127.0.0.1", global = true)]
    pub client_metric_host: String,
    #[clap(long, default_value = "8081", global = true)]
    pub client_metric_port: u16,
    /// Number of followers to run. This also  stresses the follower logic in validators
    #[clap(long, default_value = "0", global = true)]
    pub num_followers: u64,
    /// Whether or no to download TXes during follow
    #[clap(long, global = true)]
    pub download_txes: bool,
}

#[derive(Debug, Clone, Parser, Eq, PartialEq, EnumString)]
#[non_exhaustive]
#[clap(rename_all = "kebab-case")]
pub enum RunSpec {
    // Allow the ability to mix shared object and
    // single owner transactions in the benchmarking
    // framework. Currently, only shared counter
    // and transfer object transaction types are
    // supported but there will be more in future. Also
    // there is no dependency between individual
    // transactions such that they can all be executed
    // and make progress in parallel. But this too
    // will likely change in future to support
    // more representative workloads.
    Bench {
        // relative weight of shared counter
        // transaction in the benchmark workload
        #[clap(long, default_value = "0")]
        shared_counter: u32,
        // relative weight of transfer object
        // transactions in the benchmark workload
        #[clap(long, default_value = "1")]
        transfer_object: u32,
        // Target qps
        #[clap(long, default_value = "1000", global = true)]
        target_qps: u64,
        // Number of workers
        #[clap(long, default_value = "12", global = true)]
        num_workers: u64,
        // Max in-flight ratio
        #[clap(long, default_value = "5", global = true)]
        in_flight_ratio: u64,
        // Stat collection interval seconds
        #[clap(long, default_value = "10", global = true)]
        stat_collection_interval: u64,
    },
}

fn make_workload(
    primary_gas_id: ObjectID,
    primary_gas_account_owner: SuiAddress,
    primary_gas_account_keypair: Arc<AccountKeyPair>,
    opts: &Opts,
) -> Box<dyn Workload<dyn Payload>> {
    let mut workloads = HashMap::<WorkloadType, (u32, Box<dyn Workload<dyn Payload>>)>::new();
    match opts.run_spec {
        RunSpec::Bench {
            shared_counter,
            transfer_object,
            ..
        } => {
            if shared_counter > 0 {
                let workload = SharedCounterWorkload::new_boxed(
                    primary_gas_id,
                    primary_gas_account_owner,
                    primary_gas_account_keypair.clone(),
                    None,
                );
                workloads
                    .entry(WorkloadType::SharedCounter)
                    .or_insert((shared_counter, workload));
            }
            if transfer_object > 0 {
                let workload = TransferObjectWorkload::new_boxed(
                    opts.num_transfer_accounts,
                    primary_gas_id,
                    primary_gas_account_owner,
                    primary_gas_account_keypair,
                );
                workloads
                    .entry(WorkloadType::TransferObject)
                    .or_insert((transfer_object, workload));
            }
        }
    }
    CombinationWorkload::new_boxed(workloads)
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut config = telemetry_subscribers::TelemetryConfig::new("stress");
    config.log_string = Some("warn".to_string());
    config.log_file = Some("/tmp/stress.log".to_string());
    let _guard = config.with_env().init();
    let opts: Opts = Opts::parse();

    let barrier = Arc::new(Barrier::new(2));
    let cloned_barrier = barrier.clone();
    let (primary_gas_id, owner, keypair, gateway_config) = if opts.local {
        eprintln!("Configuring local benchmark..");
        let configs = {
            let mut configs = test_and_configure_authority_configs(opts.committee_size as usize);
            let mut metric_port = opts.server_metric_port;
            configs.validator_configs.iter_mut().for_each(|config| {
                let parameters = &mut config.consensus_config.as_mut().unwrap().narwhal_config;
                parameters.batch_size = 12800;
                config.metrics_address = format!("127.0.0.1:{}", metric_port).parse().unwrap();
                metric_port += 1;
            });
            Arc::new(configs)
        };
        let gateway_config = GatewayConfig {
            epoch: 0,
            validator_set: configs.validator_set().to_vec(),
            send_timeout: Duration::from_secs(4),
            recv_timeout: Duration::from_secs(4),
            buffer_size: 650000,
            db_folder_path: PathBuf::from("/tmp/client_db"),
        };
        gateway_config.save(&opts.gateway_config_path)?;
        // bring up servers ..
        let (owner, keypair): (SuiAddress, AccountKeyPair) = test_account_keys().pop().unwrap();
        let primary_gas = generate_gas_objects_with_owner(1, owner);
        let primary_gas_id = primary_gas.get(0).unwrap().id();
        // Make the client runtime wait until we are done creating genesis objects
        let cloned_config = configs;
        let cloned_gas = primary_gas;
        let auth_clients = GatewayState::make_authority_clients(
            &gateway_config,
            NetworkAuthorityClientMetrics::new_for_tests(),
        );
        // spawn a thread to spin up sui nodes on the multi-threaded server runtime
        let _ = std::thread::spawn(move || {
            // create server runtime
            let server_runtime = Builder::new_multi_thread()
                .thread_stack_size(32 * 1024 * 1024)
                .worker_threads(opts.num_server_threads as usize)
                .enable_all()
                .build()
                .unwrap();
            server_runtime.block_on(async move {
                // Setup the network
                let nodes: Vec<SuiNode> = spawn_test_authorities(cloned_gas, &cloned_config).await;
                let handles: Vec<_> = nodes.into_iter().map(move |node| node.wait()).collect();
                cloned_barrier.wait().await;
                let mut follower_handles = vec![];

                // Start the followers if any
                for idx in 0..opts.num_followers {
                    // Kick off a task which follows all authorities and discards the data
                    for (name, auth_client) in auth_clients.clone() {
                        follower_handles.push(tokio::task::spawn(async move {
                            eprintln!("Starting follower {idx} for validator {}", name);
                            follow(auth_client.clone(), opts.download_txes).await
                        }))
                    }
                }

                if try_join_all(handles).await.is_err() {
                    error!("Failed while waiting for nodes");
                }
                join_all(follower_handles).await;
            });
        });
        (primary_gas_id, owner, Arc::new(keypair), gateway_config)
    } else {
        eprintln!("Configuring remote benchmark..");
        std::thread::spawn(move || {
            Builder::new_multi_thread()
                .build()
                .unwrap()
                .block_on(async move {
                    cloned_barrier.wait().await;
                });
        });
        let config_path = Some(&opts.gateway_config_path)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| {
                anyhow!(format!(
                    "Failed to find gateway config at path: {}",
                    opts.gateway_config_path
                ))
            })?;
        let config: GatewayConfig = PersistedConfig::read(&config_path)?;
        let committee = GatewayState::make_committee(&config)?;
        let authority_clients = GatewayState::make_authority_clients(
            &config,
            NetworkAuthorityClientMetrics::new_for_tests(),
        );
        let registry = prometheus::Registry::new();
        let epoch_store = Arc::new(EpochStore::new_for_testing(&committee));
        let aggregator = AuthorityAggregator::new(
            committee,
            epoch_store,
            authority_clients,
            AuthAggMetrics::new(&registry),
            SafeClientMetrics::new(&registry),
        );
        let offset = ObjectID::from_hex_literal(&opts.primary_gas_id)?;
        let ids = ObjectID::in_range(offset, opts.primary_gas_objects)?;
        let primary_gas_id = ids.choose(&mut rand::thread_rng()).unwrap();
        let primary_gas = get_latest(*primary_gas_id, &aggregator)
            .await
            .ok_or_else(|| {
                anyhow!(format!(
                    "Failed to read primary gas object with id: {}",
                    primary_gas_id
                ))
            })?;
        let primary_gas_account = primary_gas.owner.get_owner_address()?;
        let keystore_path = Some(&opts.keystore_path)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| {
                anyhow!(format!(
                    "Failed to find keypair at path: {}",
                    &opts.keystore_path
                ))
            })?;
        let keystore = FileBasedKeystore::load_or_create(&keystore_path)?;
        let keypair = keystore
            .key_pairs()
            .iter()
            .find(|x| {
                let address: SuiAddress = Into::<SuiAddress>::into(&x.public());
                address == primary_gas_account
            })
            .map(|x| x.encode_base64())
            .unwrap();
        // TODO(joyqvq): This is a hack to decode base64 keypair with added flag, ok for now since it is for benchmark use.
        // Rework to get the typed keypair directly from above.
        let ed25519_keypair = match SuiKeyPair::decode_base64(&keypair).unwrap() {
            SuiKeyPair::Ed25519SuiKeyPair(x) => x,
            _ => panic!("Unexpected keypair type"),
        };
        (
            *primary_gas_id,
            primary_gas_account,
            Arc::new(ed25519_keypair),
            config,
        )
    };
    barrier.wait().await;
    // create client runtime
    let client_runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .worker_threads(opts.num_client_threads as usize)
        .build()
        .unwrap();
    let handle = std::thread::spawn(move || {
        client_runtime.block_on(async move {
            let committee = GatewayState::make_committee(&gateway_config).unwrap();
            let authority_clients = GatewayState::make_authority_clients(
                &gateway_config,
                NetworkAuthorityClientMetrics::new_for_tests(),
            );
            let registry: Registry = metrics::start_prometheus_server(
                format!("{}:{}", opts.client_metric_host, opts.client_metric_port)
                    .parse()
                    .unwrap(),
            );
            let epoch_store = Arc::new(EpochStore::new_for_testing(&committee));
            let aggregator = AuthorityAggregator::new(
                committee,
                epoch_store,
                authority_clients,
                AuthAggMetrics::new(&registry),
                SafeClientMetrics::new(&registry),
            );
            let mut workload = make_workload(primary_gas_id, owner, keypair, &opts);
            workload.init(&aggregator).await;
            let driver = match opts.run_spec {
                RunSpec::Bench {
                    target_qps,
                    num_workers,
                    in_flight_ratio,
                    stat_collection_interval,
                    ..
                } => BenchDriver::new(
                    target_qps,
                    in_flight_ratio,
                    num_workers,
                    stat_collection_interval,
                ),
            };
            driver.run(workload, aggregator, &registry).await
        })
    });
    let joined = handle.join();
    if let Err(err) = joined {
        Err(anyhow!("Failed to join client runtime: {:?}", err))
    } else {
        joined.unwrap()
    }
}
