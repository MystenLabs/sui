// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::anyhow;
use backoff::{future::retry, ExponentialBackoff};
use chrono::{DateTime, Utc};
use clap::*;
use once_cell::sync::Lazy;
use prometheus_http_query::Client;
use std::fs::File;
use std::io::Read;
use std::time::Duration;
use sui_metric_checker::query::{instant_query, range_query};
use sui_metric_checker::{
    fails_threshold_condition, timestamp_string_to_unix_seconds, Config, NowProvider, QueryType,
};

#[derive(Parser)]
pub struct Opts {
    #[arg(long, required = true)]
    api_user: String,
    #[arg(long, required = true)]
    api_key: String,
    // Path to the config file
    #[arg(long, required = true)]
    config: String,
    // URL of the Prometheus server
    #[arg(long, required = true)]
    url: String,
}

// This allows us to use the same value for now() for all queries checked during
// the duration of tool.
struct UtcNowOnceProvider {}

impl NowProvider for UtcNowOnceProvider {
    fn now() -> DateTime<Utc> {
        static NOW: Lazy<DateTime<Utc>> = Lazy::new(Utc::now);
        *NOW
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opts: Opts = Opts::parse();
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let auth_header = format!("{}:{}", opts.api_user, opts.api_key);

    let mut file = File::open(opts.config)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let config: Config = serde_yaml::from_str(&contents)?;

    let client = {
        let c = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        Client::from(c, &opts.url).unwrap()
    };

    let backoff = ExponentialBackoff {
        max_elapsed_time: Some(Duration::from_secs(5)),
        ..ExponentialBackoff::default()
    };

    let mut failed_queries = Vec::new();
    for query in config.queries {
        let queried_result = match query.query_type {
            QueryType::Instant => {
                retry(backoff.clone(), || async {
                    instant_query(&auth_header, client.clone(), &query.query)
                        .await
                        .map_err(backoff::Error::transient)
                })
                .await
            }
            QueryType::Range {
                start,
                end,
                step,
                percentile,
            } => {
                retry(backoff.clone(), || async {
                    range_query(
                        &auth_header,
                        client.clone(),
                        &query.query,
                        timestamp_string_to_unix_seconds::<UtcNowOnceProvider>(&start)?,
                        timestamp_string_to_unix_seconds::<UtcNowOnceProvider>(&end)?,
                        step,
                        percentile,
                    )
                    .await
                    .map_err(backoff::Error::transient)
                })
                .await
            }
        };

        if let Some(validate_result) = query.validate_result {
            match queried_result {
                Ok(queried_value) => {
                    if fails_threshold_condition(
                        queried_value,
                        validate_result.threshold,
                        &validate_result.failure_condition,
                    ) {
                        failed_queries.push(format!(
                            "Query \"{}\" returned value of {queried_value} which is {} {}",
                            query.query,
                            validate_result.failure_condition,
                            validate_result.threshold
                        ));
                    }
                }
                Err(error) => {
                    failed_queries.push(error.to_string());
                    continue;
                }
            }
        }
    }

    if !failed_queries.is_empty() {
        return Err(anyhow!(
            "Following queries failed to meet threshold conditions: {failed_queries:#?}"
        ));
    }

    Ok(())
}
