// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use client::MetricsPushClient;
use mysten_metrics::RegistryService;
use std::time::Duration;
use sui_types::crypto::NetworkKeyPair;

mod client;

/// Starts a task to periodically push metrics to a configured endpoint if a metrics push endpoint
/// is configured.
pub fn start_metrics_push_task(
    push_interval_seconds: Option<u64>,
    push_url: String,
    metrics_key_pair: NetworkKeyPair,
    registry: RegistryService,
) {
    use fastcrypto::traits::KeyPair;

    const DEFAULT_METRICS_PUSH_INTERVAL: Duration = Duration::from_secs(60);

    let interval = push_interval_seconds
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_METRICS_PUSH_INTERVAL);
    let url = reqwest::Url::parse(&push_url).expect("unable to parse metrics push url");

    let mut client = MetricsPushClient::new(metrics_key_pair.copy());

    tokio::spawn(async move {
        tracing::info!(push_url =% url, interval =? interval, "Started Metrics Push Service");

        let mut interval = tokio::time::interval(interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut errors = 0;
        loop {
            interval.tick().await;

            if let Err(error) = client.push_metrics(&url, &registry).await {
                errors += 1;
                if errors >= 10 {
                    // If we hit 10 failures in a row, start logging errors.
                    tracing::error!("unable to push metrics: {error}; new client will be created");
                } else {
                    tracing::warn!("unable to push metrics: {error}; new client will be created");
                }
                // aggressively recreate our client connection if we hit an error
                client = MetricsPushClient::new(metrics_key_pair.copy());
            } else {
                errors = 0;
            }
        }
    });
}
