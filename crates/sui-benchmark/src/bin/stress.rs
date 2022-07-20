// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use clap::*;
use futures::future::try_join_all;
use futures::future::{join_all, BoxFuture};
use futures::FutureExt;
use futures::{stream::FuturesUnordered, StreamExt};
use rand::seq::IteratorRandom;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use strum_macros::EnumString;
use sui_config::NetworkConfig;
use sui_core::authority_aggregator::AuthorityAggregator;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_quorum_driver::QuorumDriverHandler;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::crypto::{get_key_pair, EmptySignInfo, KeyPair};
use sui_types::messages::{
    ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
    Transaction, TransactionEnvelope,
};
use sui_types::object::{Object, Owner};
use test_utils::authority::{
    spawn_test_authorities, test_and_configure_authority_configs, test_authority_aggregator,
};
use test_utils::messages::{
    make_counter_create_transaction, make_counter_increment_transaction,
    make_transfer_object_transaction,
};
use test_utils::objects::{
    generate_gas_object, generate_gas_objects_for_testing, generate_gas_objects_with_owner,
};
use test_utils::transaction::publish_counter_package;
use tokio::time;
use tokio::time::Instant;
use tracing::{debug, error};

#[derive(Parser)]
#[clap(name = "Stress Testing Framework")]
struct Opts {
    /// Size of the Sui committee.
    #[clap(long, default_value = "4", global = true)]
    pub committee_size: u64,
    /// Target qps
    #[clap(long, default_value = "1000", global = true)]
    pub target_qps: u64,
    /// Number of workers
    #[clap(long, default_value = "2", global = true)]
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

type Gas = (ObjectRef, Owner);
type PackageAndCounter = (ObjectRef, ObjectID);
type CounterAndGas = (PackageAndCounter, Gas);
type Transfer = (SuiAddress, SuiAddress);
type TransferObjectAndGas = (ObjectRef, Transfer, Vec<Gas>);

#[derive(Clone, Debug)]
enum Payload {
    SharedCounterTxPayload(CounterAndGas),
    TransferObjectTxPayload(TransferObjectAndGas),
}

impl Payload {
    fn make_transaction(
        &self,
        keypairs: &HashMap<SuiAddress, KeyPair>,
    ) -> TransactionEnvelope<EmptySignInfo> {
        match self {
            Payload::SharedCounterTxPayload(((package_ref, counter_id), (gas, _))) => {
                make_counter_increment_transaction(*gas, *package_ref, *counter_id)
            }
            Payload::TransferObjectTxPayload((object_ref, (from, to), gas)) => {
                let (gas_obj, _) = gas
                    .iter()
                    .find(|x| x.1.get_owner_address().unwrap() == *from)
                    .unwrap();
                make_transfer_object_transaction(
                    *object_ref,
                    *gas_obj,
                    *from,
                    keypairs.get(&*from).unwrap(),
                    *to,
                )
            }
        }
    }
    fn make_new_payload(&self, new_object: ObjectRef, new_gas: ObjectRef) -> Payload {
        match self {
            Payload::SharedCounterTxPayload(((package_ref, counter_id), (_, owner))) => {
                Payload::SharedCounterTxPayload(((*package_ref, *counter_id), (new_gas, *owner)))
            }
            Payload::TransferObjectTxPayload((_, (from, to), gas)) => {
                let new_address_and_gas: Vec<Gas> = gas
                    .iter()
                    .map(|x| {
                        if x.1.get_owner_address().unwrap() == *from {
                            (new_gas, Owner::AddressOwner(*from))
                        } else {
                            *x
                        }
                    })
                    .collect();
                let (_, recipient) = gas
                    .iter()
                    .find(|x| x.1.get_owner_address().unwrap() != *to)
                    .unwrap();
                Payload::TransferObjectTxPayload((
                    new_object,
                    (*to, recipient.get_owner_address().unwrap()),
                    new_address_and_gas,
                ))
            }
        }
    }
    fn get_object_id(&self) -> ObjectID {
        match self {
            Payload::SharedCounterTxPayload(((_, counter_id), (_, _))) => *counter_id,
            Payload::TransferObjectTxPayload((object_ref, _, _)) => object_ref.0,
        }
    }
}

#[derive(Debug)]
enum NextOp {
    Response(Option<(Instant, Payload)>),
    Retry(Box<(Transaction, Payload)>),
}

async fn init_object_transfer_benchmark(
    count: usize,
    num_accounts: usize,
    configs: NetworkConfig,
) -> (
    Vec<Payload>,
    Arc<HashMap<SuiAddress, KeyPair>>,
    AuthorityAggregator<NetworkAuthorityClient>,
) {
    // create several accounts to transfer object between
    let accounts: Arc<HashMap<SuiAddress, KeyPair>> =
        Arc::new((0..num_accounts).map(|_| get_key_pair()).collect());
    // create enough gas to do those transfers
    let gas: Vec<Vec<Object>> = (0..count)
        .map(|_| {
            accounts
                .iter()
                .map(|(owner, _)| generate_gas_objects_with_owner(1, *owner).pop().unwrap())
                .collect()
        })
        .collect();
    // choose a random owner to be the owner of transfer objects
    let owner = *accounts.keys().choose(&mut rand::thread_rng()).unwrap();
    // create transfer objects
    let mut transfer_objects = generate_gas_objects_with_owner(count, owner);
    // create a vector of gas for all accounts along with the transfer object
    let refs: Vec<(Vec<Gas>, ObjectRef)> = gas
        .iter()
        .zip(transfer_objects.iter())
        .map(|(g, t)| {
            (
                g.iter()
                    .map(|x| (x.compute_object_reference(), x.owner))
                    .collect(),
                t.compute_object_reference(),
            )
        })
        .collect();
    let mut flattened: Vec<Object> = gas.into_iter().flatten().collect();
    flattened.append(&mut transfer_objects);

    // Setup the network
    let _ = spawn_test_authorities(flattened.clone(), &configs).await;
    let clients = test_authority_aggregator(&configs);

    (
        refs.iter()
            .map(|(g, t)| {
                let from = owner;
                let (_, to) = *g
                    .iter()
                    .find(|x| x.1.get_owner_address().unwrap() != from)
                    .unwrap();
                Payload::TransferObjectTxPayload((
                    *t,
                    (from, to.get_owner_address().unwrap()),
                    g.clone(),
                ))
            })
            .collect(),
        accounts,
        clients,
    )
}

async fn init_shared_counter_benchmark(
    count: usize,
    configs: NetworkConfig,
) -> (
    Vec<Payload>,
    Arc<HashMap<SuiAddress, KeyPair>>,
    AuthorityAggregator<NetworkAuthorityClient>,
) {
    // create enough gas
    let mut gas = vec![];
    let mut counters_gas = generate_gas_objects_for_testing(count);
    let publish_module_gas = generate_gas_object();
    gas.append(&mut counters_gas);
    gas.push(publish_module_gas.clone());

    // Setup the network
    let _ = spawn_test_authorities(gas.clone(), &configs).await;
    let clients = test_authority_aggregator(&configs);
    let quorum_driver_handler = QuorumDriverHandler::new(clients.clone());

    // Publish basics package
    eprintln!("Publishing basics package");
    let package_ref = publish_counter_package(publish_module_gas, configs.validator_set()).await;
    gas.pop();

    let qd_and_gas = gas
        .into_iter()
        .map(|g| (quorum_driver_handler.clone_quorum_driver(), g));

    // create counters
    let futures = qd_and_gas.map(|(qd, gas_object)| async move {
        let tx =
            make_counter_create_transaction(gas_object.compute_object_reference(), package_ref);
        if let ExecuteTransactionResponse::EffectsCert(result) = qd
            .execute_transaction(ExecuteTransactionRequest {
                transaction: tx,
                request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
            })
            .await
            .unwrap()
        {
            let (_, effects) = *result;
            Payload::SharedCounterTxPayload((
                (package_ref, effects.effects.created[0].0 .0),
                effects.effects.gas_object,
            ))
        } else {
            panic!("Failed to create shared counter!");
        }
    });

    eprintln!("Creating shared counters, this may take a while..");
    (
        join_all(futures).await.into_iter().collect(),
        Arc::new(HashMap::new()),
        clients,
    )
}

async fn run(
    clients: AuthorityAggregator<NetworkAuthorityClient>,
    addresses: Arc<HashMap<SuiAddress, KeyPair>>,
    payloads: Vec<Payload>,
    opts: Opts,
) {
    eprintln!("Starting benchmark!");
    let payload_per_worker = payloads.len() / opts.num_workers as usize;
    let partitioned_payload: Vec<Vec<Payload>> = payloads
        .chunks(payload_per_worker)
        .map(|s| s.into())
        .collect();
    let mut tasks = Vec::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    let request_delay_micros = 1_000_000 / (opts.num_workers * opts.target_qps);
    let stat_delay_micros = 1_000_000 * opts.stat_collection_interval;
    (0..opts.num_workers).for_each(|i| {
            let mut free_pool = partitioned_payload[i as usize].clone();
            let addresses_cloned = addresses.clone();
            // Make a per worker quorum driver, otherwise they all share the same task.
            let quorum_driver_handler = QuorumDriverHandler::new(clients.clone());
            let qd = quorum_driver_handler.clone_quorum_driver();
            let tx_cloned = tx.clone();
            let mut request_interval = time::interval(Duration::from_micros(request_delay_micros));
            request_interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

            let mut stat_interval = time::interval(Duration::from_micros(stat_delay_micros));
            let runner = tokio::spawn(async move {
                let mut num_success = 0;
                let mut num_error = 0;
                let mut min_latency = Duration::MAX;
                let mut max_latency = Duration::ZERO;
                let mut num_no_gas = 0;
                let mut num_in_flight: u64 = 0;
                let mut num_submitted = 0;
                let mut futures: FuturesUnordered<BoxFuture<NextOp>> = FuturesUnordered::new();

                let mut retry_queue : VecDeque<Box<(TransactionEnvelope<EmptySignInfo>, Payload)>> = VecDeque::new();

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
                                let tx = payload.make_transaction(&addresses_cloned);
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
        });

    tasks.push(tokio::spawn(async move {
            let mut stat_collection: BTreeMap<usize, Stats> = BTreeMap::new();
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
                eprintln!("Throughput = {}, min_latency_ms = {}, max_latency_ms = {}, num_success = {}, num_error = {}, no_gas = {}, submitted = {}, in_flight = {}", total_qps, min_latency.as_millis(), max_latency.as_millis(), num_success, num_error, num_no_gas, num_submitted, num_in_flight);
            }
        }));
    let _: Vec<_> = try_join_all(tasks).await.unwrap().into_iter().collect();
}

#[tokio::main]
async fn main() {
    let mut config = telemetry_subscribers::TelemetryConfig::new("stress");
    config.log_level = Some("warn".to_string());
    config.log_file = Some("stress.log".to_string());
    let _guard = config.with_env().init();
    let opts: Opts = Opts::parse();

    // This is the maximum number of increment counter ops in flight
    let max_in_flight_ops = opts.target_qps as usize * opts.in_flight_ratio as usize;

    let configs = test_and_configure_authority_configs(opts.committee_size as usize);

    // initialize the right kind of benchmark
    let (payload, addresses, clients) = match opts.transaction_type {
        TransactionType::SharedCounter => {
            init_shared_counter_benchmark(max_in_flight_ops, configs).await
        }
        TransactionType::TransferObject => {
            init_object_transfer_benchmark(
                max_in_flight_ops,
                opts.num_transfer_accounts as usize,
                configs,
            )
            .await
        }
    };
    run(clients, addresses, payload, opts).await
}
