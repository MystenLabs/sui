// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use clap::*;
use futures::future::try_join_all;
use futures::future::{join_all, BoxFuture};
use futures::FutureExt;
use futures::{stream::FuturesUnordered, StreamExt};
use std::collections::BTreeMap;
use std::time::Duration;
use sui_quorum_driver::QuorumDriverHandler;
use sui_types::base_types::{ObjectID, ObjectRef};
use sui_types::messages::{
    ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
    Transaction,
};
use sui_types::object::Owner;
use test_utils::authority::{
    spawn_test_authorities, test_and_configure_authority_configs, test_authority_aggregator,
};
use test_utils::messages::{make_counter_create_transaction, make_counter_increment_transaction};
use test_utils::objects::{generate_gas_object, generate_gas_objects};
use test_utils::transaction::publish_counter_package;
use tokio::time;
use tokio::time::Instant;
use tracing::subscriber::set_global_default;
use tracing::{debug, error};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[clap(name = "Shared Objects Benchmark")]
struct Opts {
    /// Size of the Sui committee.
    #[clap(long, default_value = "4", global = true)]
    pub committee_size: usize,
    /// Target qps
    #[clap(long, default_value = "100", global = true)]
    pub target_qps: u64,
    /// Number of workers
    #[clap(long, default_value = "1", global = true)]
    pub num_workers: u64,
    /// Max in-flight ratio
    #[clap(long, default_value = "5", global = true)]
    pub in_flight_ratio: usize,
    /// Stat collection interval seconds
    #[clap(long, default_value = "10", global = true)]
    pub stat_collection_interval: u64,
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

type CounterAndGas = (ObjectID, (ObjectRef, Owner));

#[derive(Debug)]
enum NextOp {
    Response(Option<(Instant, CounterAndGas)>),
    Retry(Box<(Transaction, ObjectID, Owner)>),
}

#[tokio::main]
async fn main() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let subscriber_builder =
        tracing_subscriber::fmt::Subscriber::builder().with_env_filter(env_filter);
    let subscriber = subscriber_builder.with_writer(std::io::stderr).finish();
    set_global_default(subscriber).expect("Failed to set subscriber");
    let opts: Opts = Opts::parse();

    // This is the maximum number of increment counter ops in flight
    let max_in_flight_ops = opts.target_qps as usize * opts.in_flight_ratio;

    // We will create as many counters as the number of ops
    let num_shared_counters = max_in_flight_ops;

    // Create enough gas objects to cover for creating and incrementing counters
    let mut gas = vec![];
    let mut counters_gas = generate_gas_objects(num_shared_counters);
    let publish_module_gas = generate_gas_object();
    gas.append(&mut counters_gas);
    gas.push(publish_module_gas.clone());

    // Setup the network
    let configs = test_and_configure_authority_configs(opts.committee_size);
    let _ = spawn_test_authorities(gas.clone(), &configs).await;
    let clients = test_authority_aggregator(&configs);
    let quorum_driver_handler = QuorumDriverHandler::new(clients.clone());

    // publish package
    write("Publishing basics package".to_string());
    let package_ref = publish_counter_package(publish_module_gas, configs.validator_set()).await;
    gas.pop();

    let qd_and_gas = gas
        .into_iter()
        .map(|g| (quorum_driver_handler.clone_quorum_driver(), g));

    write(format!(
        "Number of shared counters: {}",
        num_shared_counters
    ));
    write("Creating shared counters, this may take a while..".to_string());
    // create counters
    let futures = qd_and_gas.map(|(qd, gas_object)| async move {
        let tx =
            make_counter_create_transaction(gas_object.compute_object_reference(), package_ref);
        let (counter_id, new_gas_ref) = if let ExecuteTransactionResponse::EffectsCert(result) = qd
            .execute_transaction(ExecuteTransactionRequest {
                transaction: tx,
                request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
            })
            .await
            .unwrap()
        {
            let (_, effects) = *result;
            (effects.effects.created[0].0 .0, effects.effects.gas_object)
        } else {
            unreachable!();
        };
        (counter_id, new_gas_ref)
    });

    let counter_and_gas: Vec<CounterAndGas> = join_all(futures).await.into_iter().collect();

    write(format!("Done creating {} counters!", counter_and_gas.len()));
    write("Starting benchmark!".to_string());
    let gas_per_worker = counter_and_gas.len() / opts.num_workers as usize;
    let gas: Vec<Vec<(ObjectID, (ObjectRef, Owner))>> = counter_and_gas
        .chunks(gas_per_worker)
        .map(|s| s.into())
        .collect();
    let mut tasks = Vec::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    let request_delay_micros = 1_000_000 / (opts.num_workers * opts.target_qps);
    let stat_delay_micros = 1_000_000 * opts.stat_collection_interval;

    (0..opts.num_workers).for_each(|i| {
        let mut free_pool = gas[i as usize].clone();

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
                        println!("Queue size: {}", futures.len());
                    }
                    _ = request_interval.tick() => {
                        if free_pool.is_empty() {
                            num_no_gas += 1;
                        } else {
                            num_in_flight += 1;
                            num_submitted += 1;
                            let gas = free_pool.pop().unwrap();
                            let counter_id = gas.0;
                            let owner = gas.1 .1;
                            let tx = make_counter_increment_transaction(gas.1 .0, package_ref, counter_id);
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
                                        NextOp::Response(Some((
                                            start,
                                            (counter_id, (effects.effects.gas_object.0, owner)),
                                        )))
                                    }
                                    Ok(resp) => {
                                        error!("unexpected_response: {:?}", resp);
                                        NextOp::Retry(Box::new((tx, counter_id, owner)))
                                    }
                                    Err(sui_err) => {
                                        error!("Retry due to error: {}", sui_err);
                                        NextOp::Retry(Box::new((tx, counter_id, owner)))
                                    }
                                }
                            });
                            futures.push(Box::pin(res));
                        }
                    }
                    Some(op) = futures.next() => {
                        match op {
                            NextOp::Retry(b) => {
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
                                                NextOp::Response(Some((
                                                    Instant::now(),
                                                    (b.1, (effects.effects.gas_object.0, b.2)),
                                                )))
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
            write(format!("Throughput = {}, min_latency_ms = {}, max_latency_ms = {}, num_success = {}, num_error = {}, no_gas = {}, submitted = {}, in_flight = {}", total_qps, min_latency.as_millis(), max_latency.as_millis(), num_success, num_error, num_no_gas, num_submitted, num_in_flight));
        }
    }));
    let _: Vec<_> = try_join_all(tasks).await.unwrap().into_iter().collect();
}

fn write(str: String) {
    eprintln!("{}", str);
}
