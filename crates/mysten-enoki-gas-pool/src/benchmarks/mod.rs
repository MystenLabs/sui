// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::rpc::client::GasPoolRpcClient;
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::time::{interval, Duration, Instant};

#[derive(Clone, Default)]
struct BenchmarkStatsPerSecond {
    pub num_requests: u64,
    pub total_latency: u128,
    pub num_errors: u64,
}

pub async fn run_benchmark(gas_station_url: String, reserve_duration_sec: u64, num_clients: u64) {
    let mut handles = vec![];
    let stats = Arc::new(RwLock::new(BenchmarkStatsPerSecond::default()));
    let client = GasPoolRpcClient::new(gas_station_url);
    for _ in 0..num_clients {
        let client = client.clone();
        let stats = stats.clone();
        let handle = tokio::spawn(async move {
            loop {
                let now = Instant::now();
                let result = client.reserve_gas(1, None, reserve_duration_sec).await;
                let mut stats_guard = stats.write();
                match result {
                    Ok(_) => {
                        stats_guard.num_requests += 1;
                        stats_guard.total_latency += now.elapsed().as_millis();
                    }
                    Err(_) => {
                        stats_guard.num_errors += 1;
                    }
                }
            }
        });
        handles.push(handle);
    }
    let handle = tokio::spawn(async move {
        let mut prev_stats = stats.read().clone();
        let mut interval = interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let cur_stats = stats.read().clone();
            let request_per_second = cur_stats.num_requests - prev_stats.num_requests;
            println!(
                "Requests per second: {}, errors per second: {}, average latency: {}ms",
                request_per_second,
                cur_stats.num_errors - prev_stats.num_errors,
                if request_per_second == 0 {
                    0
                } else {
                    (cur_stats.total_latency - prev_stats.total_latency)
                        / request_per_second as u128
                }
            );
            prev_stats = cur_stats;
        }
    });
    handle.await.unwrap();
}
