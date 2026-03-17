// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::safe_client::SafeClient;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::make_transfer_sui_transaction;
use sui_types::base_types::AuthorityName;
use sui_types::messages_grpc::SubmitTxRequest;
use test_cluster::TestClusterBuilder;
use tokio::time::{Duration, Instant};
use tracing::{info, warn};

/// Large-scale stress test: deferral ON, amplification ON
#[sim_test]
async fn test_stress_deferral_on_amplification_on() {
    run_large_scale_stress(true, true).await;
}

/// Large-scale stress test: deferral ON, amplification OFF (baseline for deferral overhead)
#[sim_test]
async fn test_stress_deferral_on_amplification_off() {
    run_large_scale_stress(true, false).await;
}

/// Large-scale stress test: deferral OFF, amplification ON
#[sim_test]
async fn test_stress_deferral_off_amplification_on() {
    run_large_scale_stress(false, true).await;
}

/// Large-scale stress test: deferral OFF, amplification OFF (pure baseline)
#[sim_test]
async fn test_stress_deferral_off_amplification_off() {
    run_large_scale_stress(false, false).await;
}

async fn run_large_scale_stress(enable_deferral: bool, enable_amplification: bool) {
    telemetry_subscribers::init_for_testing();

    let _guard = ProtocolConfig::apply_overrides_for_testing(move |_, mut config| {
        config.set_defer_unpaid_amplification_for_testing(enable_deferral);
        config
    });

    info!(
        "Running stress test: deferral={}, amplification={}",
        enable_deferral, enable_amplification
    );

    let num_validators = 30;
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(num_validators)
        .build()
        .await;

    let clients: Vec<(AuthorityName, Arc<SafeClient<NetworkAuthorityClient>>)> = test_cluster
        .authority_aggregator()
        .authority_clients
        .iter()
        .map(|(name, client)| (*name, client.clone()))
        .collect();

    info!("Test with {} validators", clients.len());

    let batches = 10;
    let txns_per_batch = 100;
    let amplification_factor = if enable_amplification { num_validators } else { 1 };

    let mut all_submission_times = Vec::new();
    let mut total_failures = 0;
    let mut batch_metrics = Vec::new();

    for batch in 0..batches {
        let batch_start = Instant::now();
        let mut handles = Vec::new();
        let mut batch_failures = 0;

        for i in 0..txns_per_batch {
            let tx = make_transfer_sui_transaction(&test_cluster.wallet, None, None).await;
            let digest = *tx.digest();
            let clients_clone: Vec<_> = clients.iter().map(|(n, c)| (*n, c.clone())).collect();

            let handle = tokio::spawn(async move {
                let submit_start = Instant::now();
                let request = SubmitTxRequest::new_transaction(tx);
                let mut successes = 0;
                let mut failures = 0;

                let mut submit_handles = Vec::new();
                for (_, client) in clients_clone.iter().take(amplification_factor) {
                    let req = request.clone();
                    let c = client.clone();
                    submit_handles.push(tokio::spawn(async move {
                        c.submit_transaction(req, None).await
                    }));
                }

                for h in submit_handles {
                    match h.await {
                        Ok(Ok(_)) => successes += 1,
                        _ => failures += 1,
                    }
                }

                (i, digest, submit_start.elapsed(), successes, failures)
            });
            handles.push(handle);
        }

        for handle in handles {
            let (_, _, latency, _, failures) = handle.await.unwrap();
            all_submission_times.push(latency);
            batch_failures += failures;
        }

        let batch_time = batch_start.elapsed();
        total_failures += batch_failures;

        info!(
            "Batch {}/{}: {:?}, {} failures",
            batch + 1,
            batches,
            batch_time,
            batch_failures
        );

        batch_metrics.push((batch_time, batch_failures));
    }

    let total_txns = batches * txns_per_batch;
    let total_submissions = total_txns * amplification_factor;

    info!(
        "=== Stress Test Results (deferral={}, amplification={}) ===",
        enable_deferral, enable_amplification
    );
    info!("Validators: {}", num_validators);
    info!("Total transactions: {}", total_txns);
    info!("Total validator submissions: {}", total_submissions);
    info!("Total failures: {}", total_failures);

    if batch_metrics.len() >= 2 {
        let first_batch_time = batch_metrics[0].0;
        let last_batch_time = batch_metrics[batch_metrics.len() - 1].0;
        let degradation_ratio = last_batch_time.as_micros() as f64 / first_batch_time.as_micros().max(1) as f64;

        info!("First batch time: {:?}", first_batch_time);
        info!("Last batch time: {:?}", last_batch_time);
        info!("Degradation ratio: {:.2}x", degradation_ratio);

        if degradation_ratio > 2.0 {
            warn!(
                "DEGRADATION DETECTED: Last batch took {:.2}x longer than first batch",
                degradation_ratio
            );
        }
    }

    let failure_rate = total_failures as f64 / total_submissions as f64;

    let total_batch_time: Duration = batch_metrics.iter().map(|(t, _)| *t).sum();
    let result_line = format!(
        "deferral={:<5} amplification={:<5} time={:>6.1}s txns={} submissions={} failures={} failure_rate={:.4}%\n",
        enable_deferral,
        enable_amplification,
        total_batch_time.as_secs_f64(),
        total_txns,
        total_submissions,
        total_failures,
        failure_rate * 100.0
    );

    use std::fs::OpenOptions;
    use std::io::Write;
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/amplification_test_results.txt")
    {
        let _ = file.write_all(result_line.as_bytes());
    }

    if failure_rate > 0.05 {
        warn!(
            "HIGH FAILURE RATE: {:.2}% of submissions failed",
            failure_rate * 100.0
        );
    }
}
