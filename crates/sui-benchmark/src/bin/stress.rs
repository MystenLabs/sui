// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::{anyhow, Result};
use clap::*;
use futures::future::try_join_all;
use futures::future::BoxFuture;
use futures::FutureExt;
use futures::{stream::FuturesUnordered, StreamExt};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use strum_macros::EnumString;
use sui_benchmark::workloads::shared_counter::SharedCounterWorkload;
use sui_benchmark::workloads::transfer_object::TransferObjectWorkload;
use sui_benchmark::workloads::workload::get_latest;
use sui_benchmark::workloads::workload::Payload;
use sui_benchmark::workloads::workload::Workload;
use sui_config::Config;
use sui_config::PersistedConfig;
use sui_core::authority_aggregator::AuthAggMetrics;
use sui_core::authority_aggregator::AuthorityAggregator;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_gateway::config::GatewayConfig;
use sui_node::SuiNode;
use sui_quorum_driver::QuorumDriverHandler;
use sui_sdk::crypto::SuiKeystore;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::EncodeDecodeBase64;
use sui_types::crypto::KeypairTraits;
use sui_types::crypto::{AccountKeyPair, EmptySignInfo};
use sui_types::messages::{
    ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
    TransactionEnvelope,
};
use test_utils::authority::spawn_test_authorities;
use test_utils::authority::test_and_configure_authority_configs;
use test_utils::objects::generate_gas_objects_with_owner;
use test_utils::test_account_keys;
use tokio::runtime::Builder;
use tokio::sync::Barrier;
use tokio::sync::OnceCell;
use tokio::time;
use tokio::time::Instant;
use tracing::{debug, error};

#[derive(Parser)]
#[clap(name = "Stress Testing Framework")]
struct Opts {
    /// Si&ze of the Sui committee.
    #[clap(long, default_value = "4", global = true)]
    pub committee_size: u64,
    /// Target qps
    #[clap(long, default_value = "1000", global = true)]
    pub target_qps: u64,
    /// Number of workers
    #[clap(long, default_value = "12", global = true)]
    pub num_workers: u64,
    /// Max in-flight ratio
    #[clap(long, default_value = "5", global = true)]
    pub in_flight_ratio: u64,
    /// Num of accounts to use for transfer objects
    #[clap(long, default_value = "5", global = true)]
    pub num_transfer_accounts: u64,
    /// Stat collection interval seconds
    #[clap(long, default_value = "10", global = true)]
    pub stat_collection_interval: u64,
    /// Shared counter or transfer object
    #[clap(arg_enum, default_value = "owned", global = true, ignore_case = true)]
    pub transaction_type: TransactionType,
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
    /// Whether to run local or remote benchmark
    /// NOTE: For running remote benchmark we must have the following
    /// gateway_config_path, keypair_path and primary_gas_id
    #[clap(long, parse(try_from_str), default_value = "true", global = true)]
    pub local: bool,
}

struct Stats {
    pub id: usize,
    pub num_success: u64,
    pub num_error: u64,
    pub num_no_gas: u64,
    pub num_submitted: u64,
    pub num_in_flight: u64,
    pub min_latency: Duration,
    pub max_latency: Duration,
    pub duration: Duration,
}

#[derive(Parser, Debug, Clone, PartialEq, ArgEnum, EnumString, Eq)]
#[clap(rename_all = "kebab-case")]
enum TransactionType {
    #[clap(name = "shared")]
    SharedCounter,
    #[clap(name = "owned")]
    TransferObject,
}

type RetryType = Box<(TransactionEnvelope<EmptySignInfo>, Box<dyn Payload>)>;
enum NextOp {
    Response(Option<(Instant, Box<dyn Payload>)>),
    Retry(RetryType),
}

async fn print_start_benchmark() {
    static ONCE: OnceCell<bool> = OnceCell::const_new();
    ONCE.get_or_init(|| async move {
        eprintln!("Starting benchmark!");
        true
    })
    .await;
}

async fn run(
    clients: AuthorityAggregator<NetworkAuthorityClient>,
    workload: Box<dyn Workload<dyn Payload>>,
    num_requests_per_worker: u64,
    opts: Opts,
    barrier: Arc<Barrier>,
) {
    let mut tasks = Vec::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    let request_delay_micros = 1_000_000 / (opts.num_workers * opts.target_qps);
    let stat_delay_micros = 1_000_000 * opts.stat_collection_interval;

    for i in 0..opts.num_workers {
        eprintln!("Starting worker: {}", i);
        let mut free_pool = workload
            .make_test_payloads(num_requests_per_worker, &clients)
            .await;
        let tx_cloned = tx.clone();
        let cloned_barrier = barrier.clone();
        // Make a per worker quorum driver, otherwise they all share the same task.
        let quorum_driver_handler = QuorumDriverHandler::new(clients.clone());
        let qd = quorum_driver_handler.clone_quorum_driver();
        let runner = tokio::spawn(async move {
            cloned_barrier.wait().await;
            print_start_benchmark().await;
            let mut num_success = 0;
            let mut num_error = 0;
            let mut min_latency = Duration::MAX;
            let mut max_latency = Duration::ZERO;
            let mut num_no_gas = 0;
            let mut num_in_flight: u64 = 0;
            let mut num_submitted = 0;
            let mut request_interval = time::interval(Duration::from_micros(request_delay_micros));
            request_interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

            let mut stat_interval = time::interval(Duration::from_micros(stat_delay_micros));
            let mut futures: FuturesUnordered<BoxFuture<NextOp>> = FuturesUnordered::new();

            let mut retry_queue: VecDeque<RetryType> = VecDeque::new();

            loop {
                tokio::select! {
                    _ = stat_interval.tick() => {
                        if tx_cloned
                            .send(Stats {
                                id: i as usize,
                                num_success,
                                num_error,
                                min_latency,
                                max_latency,
                                num_no_gas,
                                num_in_flight,
                                num_submitted,
                                duration: Duration::from_micros(stat_delay_micros),
                            })
                            .await
                            .is_err()
                        {
                            debug!("Failed to update stat!");
                        }
                        num_success = 0;
                        num_error = 0;
                        num_no_gas = 0;
                        num_submitted = 0;
                        min_latency = Duration::MAX;
                        max_latency = Duration::ZERO;
                    }
                    _ = request_interval.tick() => {

                        // If a retry is available send that
                        // (sending retries here subjects them to our rate limit)
                        if let Some(b) = retry_queue.pop_front() {
                            num_submitted += 1;
                            num_error += 1;
                            let res = qd
                                .execute_transaction(ExecuteTransactionRequest {
                                    transaction: b.0.clone(),
                                    request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
                                })
                                .map(move |res| {
                                    match res {
                                        Ok(ExecuteTransactionResponse::EffectsCert(result)) => {
                                            let (_, effects) = *result;
                                            let new_version = effects.effects.mutated.iter().find(|(object_ref, _)| {
                                                object_ref.0 == b.1.get_object_id()
                                            }).map(|x| x.0).unwrap();
                                            NextOp::Response(Some((
                                                Instant::now(),
                                                b.1.make_new_payload(new_version, effects.effects.gas_object.0),
                                            ),
                                            ))
                                        }
                                        Ok(resp) => {
                                            error!("unexpected_response: {:?}", resp);
                                            NextOp::Retry(b)
                                        }
                                        Err(sui_err) => {
                                            error!("{}", sui_err);
                                            NextOp::Retry(b)
                                        }
                                    }
                                });
                            futures.push(Box::pin(res));
                            continue
                        }

                        // Otherwise send a fresh request
                        if free_pool.is_empty() {
                            num_no_gas += 1;
                        } else {
                            num_in_flight += 1;
                            num_submitted += 1;
                            let payload = free_pool.pop().unwrap();
                            let tx = payload.make_transaction();
                            let start = Instant::now();
                            let res = qd
                                .execute_transaction(ExecuteTransactionRequest {
                                    transaction: tx.clone(),
                                request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
                            })
                            .map(move |res| {
                                match res {
                                    Ok(ExecuteTransactionResponse::EffectsCert(result)) => {
                                        let (_, effects) = *result;
                                        let new_version = effects.effects.mutated.iter().find(|(object_ref, _)| {
                                            object_ref.0 == payload.get_object_id()
                                        }).map(|x| x.0).unwrap();
                                        NextOp::Response(Some((
                                            start,
                                            payload.make_new_payload(new_version, effects.effects.gas_object.0),
                                        )))
                                    }
                                    Ok(resp) => {
                                        error!("unexpected_response: {:?}", resp);
                                        NextOp::Retry(Box::new((tx, payload)))
                                    }
                                    Err(sui_err) => {
                                        error!("Retry due to error: {}", sui_err);
                                        NextOp::Retry(Box::new((tx, payload)))
                                    }
                                }
                            });
                            futures.push(Box::pin(res));
                        }
                    }
                    Some(op) = futures.next() => {
                        match op {
                            NextOp::Retry(b) => {
                                retry_queue.push_back(b);
                            }
                            NextOp::Response(Some((start, payload))) => {
                                free_pool.push(payload);
                                let latency = start.elapsed();
                                num_success += 1;
                                num_in_flight -= 1;
                                if latency > max_latency {
                                    max_latency = latency;
                                }
                                if latency < min_latency {
                                    min_latency = latency;
                                }
                            }
                            NextOp::Response(None) => {
                                // num_in_flight -= 1;
                                unreachable!();
                            }
                        }
                    }
                }
            }
        });
        tasks.push(runner);
    }

    tasks.push(tokio::spawn(async move {
            let mut stat_collection: BTreeMap<usize, Stats> = BTreeMap::new();
            let mut counter = 0;
            while let Some(s @ Stats {
                id,
                num_success: _,
                num_error: _,
                min_latency: _,
                max_latency: _,
                num_no_gas: _,
                num_in_flight: _,
                num_submitted: _,
                duration
            }) = rx.recv().await {
                stat_collection.insert(id, s);
                let mut total_qps: f32 = 0.0;
                let mut num_success: u64 = 0;
                let mut num_error: u64 = 0;
                let mut min_latency: Duration = Duration::MAX;
                let mut max_latency: Duration = Duration::ZERO;
                let mut num_in_flight: u64 = 0;
                let mut num_submitted: u64 = 0;
                let mut num_no_gas = 0;
                for (_, v) in stat_collection.iter() {
                    total_qps += v.num_success as f32 / duration.as_secs() as f32;
                    num_success += v.num_success;
                    num_error += v.num_error;
                    num_no_gas += v.num_no_gas;
                    num_submitted += v.num_submitted;
                    num_in_flight += v.num_in_flight;
                    min_latency = if v.min_latency < min_latency {
                        v.min_latency
                    } else {
                        min_latency
                    };
                    max_latency = if v.max_latency > max_latency {
                        v.max_latency
                    } else {
                        max_latency
                    };
                }
                let denom = num_success + num_error;
                let _error_rate = if denom > 0 {
                    num_error as f32 / denom as f32
                } else {
                    0.0
                };
                counter += 1;
                if counter % opts.num_workers == 0 {
                    eprintln!("Throughput = {}, min_latency_ms = {}, max_latency_ms = {}, num_success = {}, num_error = {}, no_gas = {}, submitted = {}, in_flight = {}", total_qps, min_latency.as_millis(), max_latency.as_millis(), num_success, num_error, num_no_gas, num_submitted, num_in_flight);
                }
            }
        }));
    try_join_all(tasks).await.unwrap().into_iter().collect()
}

fn make_workload(
    primary_gas_id: ObjectID,
    primary_gas_account_owner: SuiAddress,
    primary_gas_account_keypair: Arc<AccountKeyPair>,
    opts: &Opts,
) -> Box<dyn Workload<dyn Payload>> {
    match opts.transaction_type {
        TransactionType::SharedCounter => SharedCounterWorkload::new_boxed(
            primary_gas_id,
            primary_gas_account_owner,
            primary_gas_account_keypair,
            None,
        ),
        TransactionType::TransferObject => TransferObjectWorkload::new_boxed(
            opts.num_transfer_accounts,
            primary_gas_id,
            primary_gas_account_owner,
            primary_gas_account_keypair,
        ),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut config = telemetry_subscribers::TelemetryConfig::new("stress");
    config.log_string = Some("warn".to_string());
    config.log_file = Some("/tmp/stress.log".to_string());
    let _guard = config.with_env().init();
    let opts: Opts = Opts::parse();

    // This is the maximum number of increment counter ops in flight
    let max_in_flight_ops = opts.target_qps as usize * opts.in_flight_ratio as usize;

    let barrier = Arc::new(Barrier::new(2));
    let cloned_barrier = barrier.clone();
    let (primary_gas_id, owner, keypair, gateway_config) = if opts.local {
        eprintln!("Configuring local benchmark..");
        let configs = {
            let mut configs = test_and_configure_authority_configs(opts.committee_size as usize);
            configs.validator_configs.iter_mut().for_each(|config| {
                let parameters = &mut config.consensus_config.as_mut().unwrap().narwhal_config;
                parameters.batch_size = 12800;
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
                if try_join_all(handles).await.is_err() {
                    error!("Failed while waiting for nodes");
                }
            });
        });
        (primary_gas_id, owner, Arc::new(keypair), gateway_config)
    } else {
        eprintln!("Configuring remote benchmark..");
        cloned_barrier.wait().await;
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
        let committee = config.make_committee()?;
        let authority_clients = config.make_authority_clients();
        let metrics = AuthAggMetrics::new(&prometheus::Registry::new());
        let aggregator = AuthorityAggregator::new(committee, authority_clients, metrics);
        let primary_gas_id = ObjectID::from_hex_literal(&opts.primary_gas_id)?;
        let primary_gas = get_latest(primary_gas_id, &aggregator)
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
        let keystore = SuiKeystore::load_or_create(&keystore_path)?;
        let keypair = keystore
            .key_pairs()
            .iter()
            .find(|x| {
                let address: SuiAddress = Into::<SuiAddress>::into(x.public());
                address == primary_gas_account
            })
            .map(|x| x.encode_base64())
            .unwrap();
        //let contents = std::fs::read_to_string(keypair.encode_base64())?;
        (
            primary_gas_id,
            primary_gas_account,
            Arc::new(keypair.parse()?),
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
    let handle: JoinHandle<()> = std::thread::spawn(move || {
        client_runtime.block_on(async move {
            let committee = gateway_config.make_committee().unwrap();
            let authority_clients = gateway_config.make_authority_clients();
            let metrics = AuthAggMetrics::new(&prometheus::Registry::new());
            let aggregator = AuthorityAggregator::new(committee, authority_clients, metrics);
            let mut workload = make_workload(primary_gas_id, owner, keypair, &opts);
            workload.init(&aggregator).await;
            let barrier = Arc::new(Barrier::new(opts.num_workers as usize));
            run(
                aggregator,
                workload,
                max_in_flight_ops as u64 / opts.num_workers,
                opts,
                barrier,
            )
            .await
        });
    });
    if handle.join().is_err() {
        error!("Failed to join thread");
    }
    Ok(())
}
