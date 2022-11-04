// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::{anyhow, Result};
use clap::*;
use futures::future::join_all;
use futures::future::try_join_all;
use futures::StreamExt;
use prometheus::Registry;
use rand::seq::SliceRandom;
use std::path::PathBuf;
use std::sync::Arc;
use strum_macros::EnumString;
use sui_benchmark::drivers::bench_driver::BenchDriver;
use sui_benchmark::drivers::driver::Driver;
use sui_benchmark::drivers::BenchmarkCmp;
use sui_benchmark::drivers::BenchmarkStats;
use sui_benchmark::drivers::Interval;
use sui_benchmark::util::get_ed25519_keypair_from_keystore;
use sui_benchmark::workloads::{
    make_combination_workload, make_shared_counter_workload, make_transfer_object_workload,
};
use sui_benchmark::FullNodeProxy;
use sui_benchmark::LocalValidatorAggregatorProxy;
use sui_benchmark::ValidatorProxy;
use sui_core::authority_aggregator::reconfig_from_genesis;
use sui_core::authority_aggregator::AuthorityAggregatorBuilder;
use sui_core::authority_client::AuthorityAPI;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_node::metrics;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::batch::UpdateItem;
use sui_types::crypto::AccountKeyPair;
use sui_types::messages::BatchInfoRequest;
use sui_types::messages::BatchInfoResponseItem;
use sui_types::messages::TransactionInfoRequest;
use tracing::log::info;

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
    #[clap(long, default_value = "", global = true)]
    pub log_path: String,
    /// [Required for remote benchmark]
    /// Path where genesis.blob is stored when running remote benchmark
    #[clap(long, default_value = "/tmp/genesis.blob", global = true)]
    pub genesis_blob_path: String,
    /// [Required for remote benchmark]
    /// Path where keypair for primary gas account is stored. The format of
    /// this file is same as what `sui keytool generate` outputs
    #[clap(long, default_value = "", global = true)]
    pub keystore_path: String,
    /// [Required for remote benchmark]
    /// Object id of the primary gas coin used for benchmark
    /// NOTE: THe remote network should have this coin in its genesis config
    /// with large enough gas i.e. u64::MAX
    #[clap(long, default_value = "", global = true)]
    pub primary_gas_id: String,
    #[clap(long, default_value = "5000", global = true)]
    pub primary_gas_objects: u64,
    /// Whether to run local or remote benchmark
    /// NOTE: For running remote benchmark we must have the following
    /// genesis_blob_path, keypair_path and primary_gas_id
    #[clap(long, parse(try_from_str), default_value = "true", global = true)]
    pub local: bool,
    /// If provided, use FullNodeProxy to submit transactions and read data.
    /// This param only matters when local = false, namely local runs always
    /// use a LocalValidatorAggregatorProxy.
    #[clap(long, global = true)]
    pub fullnode_rpc_address: Option<String>,
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
    /// Run in disjoint_mode when we don't want different workloads
    /// to interfere with each other. This mode is useful when
    /// we don't want backoff to penalize all workloads even if only
    /// one (or some) is slow.
    #[clap(long, parse(try_from_str), default_value = "true", global = true)]
    pub disjoint_mode: bool,
    /// Number of transactions or duration to
    /// run the benchmark for. Default set to
    /// "unbounded" i.e. benchmark runs forever
    /// until terminated with a ctrl-c. However,
    /// if we wanted to run the test for
    /// 60 seconds, this could be set as "60s".
    /// And if we wanted to run the test for
    /// 10,000 transactions we could set it to
    /// "10000"
    #[clap(long, global = true, default_value = "unbounded")]
    pub run_duration: Interval,
    /// Path where benchmark stats is stored
    #[clap(long, default_value = "/tmp/bench_result", global = true)]
    pub benchmark_stats_path: String,
    /// Path where previous benchmark stats is stored to use for comparison
    #[clap(long, default_value = "", global = true)]
    pub compare_with: String,
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
        // 100 for max hotness i.e all requests target
        // just the same shared counter, 0 for no hotness
        // i.e. all requests target a different shared
        // counter. The way total number of counters to
        // create is computed roughly as:
        // total_shared_counters = max(1, qps * (1.0 - hotness/100.0))
        #[clap(long, default_value = "50")]
        shared_counter_hotness_factor: u32,
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

pub async fn follow(authority_client: NetworkAuthorityClient, download_txes: bool) {
    let _batch_client_handle = tokio::task::spawn(async move {
        let mut start = 0;

        loop {
            let receiver = authority_client
                .handle_batch_stream(BatchInfoRequest {
                    start: Some(start),
                    length: 10_000,
                })
                .await;

            if let Err(e) = &receiver {
                error!("Listener error: {:?}", e);
                break;
            }
            let mut receiver = receiver.unwrap();

            info!("Start batch listener at sequence: {}.", start);
            while let Some(item) = receiver.next().await {
                match item {
                    Ok(BatchInfoResponseItem(UpdateItem::Transaction((_tx_seq, tx_digest)))) => {
                        if download_txes {
                            authority_client
                                .handle_transaction_info_request(TransactionInfoRequest::from(
                                    tx_digest.transaction,
                                ))
                                .await
                                .unwrap();
                            info!(
                                "Client downloaded TX with digest {:?}",
                                tx_digest.transaction
                            );
                        }
                        start = _tx_seq + 1;
                    }
                    Ok(BatchInfoResponseItem(UpdateItem::Batch(_signed_batch))) => {
                        info!(
                            "Client received batch up to sequence {}",
                            _signed_batch.data().next_sequence_number
                        );
                    }
                    Err(err) => {
                        error!("{:?}", err);
                        break;
                    }
                }
            }
        }
    });
}

/// To spin up a local cluster and direct some load
/// at it with 50/50 shared and owned traffic, use
/// it something like:
/// ```cargo run  --release  --package sui-benchmark
/// --bin stress -- --num-client-threads 12 \
/// --num-server-threads 10 \
/// --num-transfer-accounts 2 \
/// bench \
/// --target-qps 100 \
/// --in-flight-ratio 2 \
/// --shared-counter 50 \
/// --transfer-object 50```
/// To point the traffic to an already running cluster,
/// use it something like:
/// ```cargo run  --release  --package sui-benchmark --bin stress -- --num-client-threads 12 \
/// --num-server-threads 10 \
/// --num-transfer-accounts 2 \
/// --primary-gas-id 0x59931dcac57ba20d75321acaf55e8eb5a2c47e9f \
/// --gateway-config-path /tmp/gateway.yaml \
/// --keystore-path /tmp/sui.keystore bench \
/// --target-qps 100 \
/// --in-flight-ratio 2 \
/// --shared-counter 50 \
/// --transfer-object 50```
#[tokio::main]
async fn main() -> Result<()> {
    let opts: Opts = Opts::parse();
    let mut config = telemetry_subscribers::TelemetryConfig::new("stress");
    config.log_string = Some("warn".to_string());
    if !opts.log_path.is_empty() {
        config.log_file = Some(opts.log_path);
    }
    let _guard = config.with_env().init();

    let registry: Arc<Registry> = Arc::new(metrics::start_prometheus_server(
        format!("{}:{}", opts.client_metric_host, opts.client_metric_port)
            .parse()
            .unwrap(),
    ));
    let barrier = Arc::new(Barrier::new(2));
    let cloned_barrier = barrier.clone();
    let (primary_gas_id, owner, keypair, aggregator) = if opts.local {
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

        // bring up servers ..
        let (owner, keypair): (SuiAddress, AccountKeyPair) = test_account_keys().pop().unwrap();
        let primary_gas = generate_gas_objects_with_owner(1, owner);
        let primary_gas_id = primary_gas.get(0).unwrap().id();
        // Make the client runtime wait until we are done creating genesis objects
        let cloned_config = configs.clone();
        let cloned_gas = primary_gas;

        let (aggregator, auth_clients) = AuthorityAggregatorBuilder::from_network_config(&configs)
            .with_registry(registry.clone())
            .build()
            .unwrap();

        let proxy: Arc<dyn ValidatorProxy + Send + Sync> = Arc::new(
            LocalValidatorAggregatorProxy::from_auth_agg(Arc::new(aggregator)),
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
                let nodes: Vec<_> = spawn_test_authorities(cloned_gas, &cloned_config).await;
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
        (primary_gas_id, owner, Arc::new(keypair), proxy)
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

        let proxy: Arc<dyn ValidatorProxy + Send + Sync> =
            if let Some(fullnode_url) = opts.fullnode_rpc_address {
                eprintln!("Using Full node: {fullnode_url}..");
                Arc::new(FullNodeProxy::from_url(&fullnode_url).await.unwrap())
            } else {
                let genesis = sui_config::node::Genesis::new_from_file(&opts.genesis_blob_path);
                let genesis = genesis.genesis()?;
                let (aggregator, _) = AuthorityAggregatorBuilder::from_genesis(genesis)
                    .with_registry(registry.clone())
                    .build()
                    .unwrap();

                let aggregator = reconfig_from_genesis(aggregator).await?;
                Arc::new(LocalValidatorAggregatorProxy::from_auth_agg(Arc::new(
                    aggregator,
                )))
            };
        eprintln!(
            "Reconfiguration - Reconfiguration to epoch {} is done",
            proxy.get_current_epoch(),
        );

        let offset = ObjectID::from_hex_literal(&opts.primary_gas_id)?;
        let ids = ObjectID::in_range(offset, opts.primary_gas_objects)?;
        let primary_gas_id = ids.choose(&mut rand::thread_rng()).unwrap();
        let primary_gas = proxy.get_object(*primary_gas_id).await?;

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
        let ed25519_keypair =
            get_ed25519_keypair_from_keystore(keystore_path, &primary_gas_account)?;
        (
            *primary_gas_id,
            primary_gas_account,
            Arc::new(ed25519_keypair),
            proxy,
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
    let prev_benchmark_stats_path = opts.compare_with.clone();
    let curr_benchmark_stats_path = opts.benchmark_stats_path.clone();
    let arc_agg = aggregator.clone();
    let registry_clone = registry.clone();
    let handle = std::thread::spawn(move || {
        client_runtime.block_on(async move {
            match opts.run_spec {
                RunSpec::Bench {
                    target_qps,
                    num_workers,
                    in_flight_ratio,
                    stat_collection_interval,
                    shared_counter,
                    transfer_object,
                    shared_counter_hotness_factor,
                    ..
                } => {
                    let shared_counter_ratio = 1.0
                        - (std::cmp::min(shared_counter_hotness_factor as u32, 100) as f32 / 100.0);
                    let workloads = if !opts.disjoint_mode {
                        let mut combination_workload = make_combination_workload(
                            target_qps,
                            num_workers,
                            in_flight_ratio,
                            primary_gas_id,
                            owner,
                            keypair,
                            opts.num_transfer_accounts,
                            shared_counter,
                            transfer_object,
                        );
                        let max_ops = target_qps * in_flight_ratio;
                        let num_shared_counters = (max_ops as f32 * shared_counter_ratio) as u64;
                        combination_workload
                            .workload
                            .init(num_shared_counters, arc_agg.clone())
                            .await;
                        vec![combination_workload]
                    } else {
                        let mut workloads = vec![];
                        let shared_counter_weight =
                            shared_counter as f32 / (shared_counter + transfer_object) as f32;
                        let shared_counter_qps = (shared_counter_weight * target_qps as f32) as u64;
                        let shared_counter_num_workers =
                            (shared_counter_weight * num_workers as f32).ceil() as u64;
                        let shared_counter_max_ops = (shared_counter_qps * in_flight_ratio) as u64;
                        let num_shared_counters =
                            (shared_counter_max_ops as f32 * shared_counter_ratio) as u64;
                        if let Some(mut shared_counter_workload) = make_shared_counter_workload(
                            shared_counter_qps,
                            shared_counter_num_workers,
                            shared_counter_max_ops,
                            primary_gas_id,
                            owner,
                            keypair.clone(),
                        ) {
                            shared_counter_workload
                                .workload
                                .init(num_shared_counters, arc_agg.clone())
                                .await;
                            workloads.push(shared_counter_workload);
                        }
                        let transfer_object_weight = 1.0 - shared_counter_weight;
                        let transfer_object_qps = target_qps - shared_counter_qps;
                        let transfer_object_num_workers =
                            (transfer_object_weight * num_workers as f32).ceil() as u64;
                        let transfer_object_max_ops =
                            (transfer_object_qps * in_flight_ratio) as u64;
                        if let Some(mut transfer_object_workload) = make_transfer_object_workload(
                            transfer_object_qps,
                            transfer_object_num_workers,
                            transfer_object_max_ops,
                            opts.num_transfer_accounts,
                            &primary_gas_id,
                            owner,
                            keypair,
                        ) {
                            transfer_object_workload
                                .workload
                                .init(num_shared_counters, arc_agg.clone())
                                .await;
                            workloads.push(transfer_object_workload);
                        }
                        workloads
                    };
                    let interval = opts.run_duration;
                    // We only show continuous progress in stderr
                    // if benchmark is running in unbounded mode,
                    // otherwise summarized benchmark results are
                    // published in the end
                    let show_progress = interval.is_unbounded();
                    let driver = BenchDriver::new(stat_collection_interval);
                    driver
                        .run(workloads, arc_agg, &registry_clone, show_progress, interval)
                        .await
                }
            }
        })
    });
    let joined = handle.join();
    if let Err(err) = joined {
        Err(anyhow!("Failed to join client runtime: {:?}", err))
    } else {
        let stats: BenchmarkStats = joined.unwrap().unwrap();
        let table = stats.to_table();
        eprintln!("Benchmark Report:");
        eprintln!("{}", table);
        if !prev_benchmark_stats_path.is_empty() {
            let data = std::fs::read_to_string(&prev_benchmark_stats_path)?;
            let prev_stats: BenchmarkStats = serde_json::from_str(&data)?;
            let cmp = BenchmarkCmp {
                new: &stats,
                old: &prev_stats,
            };
            let cmp_table = cmp.to_table();
            eprintln!(
                "Benchmark Comparison Report[{}]:",
                prev_benchmark_stats_path
            );
            eprintln!("{}", cmp_table);
        }
        if !curr_benchmark_stats_path.is_empty() {
            let serialized = serde_json::to_string(&stats)?;
            std::fs::write(curr_benchmark_stats_path, serialized)?;
        }
        Ok(())
    }
}
