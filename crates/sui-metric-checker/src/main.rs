// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use clap::*;
use once_cell::sync::Lazy;
use prometheus_http_query::Client;
use std::fs::File;
use std::io::Read;
use std::sync::Mutex;
use std::time::Duration;
use sui_metric_checker::query::{instant_query_with_retries, range_query_with_retries};
use sui_metric_checker::{
    fails_threshold_condition, timestamp_string_to_unix_seconds, Config, NowProvider, QueryType,
};

#[derive(Parser)]
pub struct Opts {
    #[clap(long, required = true)]
    api_user: String,
    #[clap(long, required = true)]
    api_key: String,
    // Path to the config file
    #[clap(long, required = true)]
    config: String,
    // URL of the Prometheus server
    #[clap(long, required = true)]
    url: String,
}

// This allows us to use the same value for now() for all queries checked during
// the duration of tool.
struct UtcNowOnceProvider {}

impl NowProvider for UtcNowOnceProvider {
    fn now() -> DateTime<Utc> {
        static NOW: Lazy<Mutex<Option<DateTime<Utc>>>> = Lazy::new(|| Mutex::new(None));

        let mut now = NOW.lock().unwrap();
        if let Some(value) = *now {
            value
        } else {
            let value = Utc::now();
            *now = Some(value);
            value
        }
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

    let mut failed_queries = Vec::new();
    for query in config.queries {
        let queried_result = match query.query_type {
            QueryType::Instant => {
                instant_query_with_retries(&auth_header, client.clone(), &query.query, 3).await
            }
            QueryType::Range => {
                range_query_with_retries(
                    &auth_header,
                    client.clone(),
                    &query.query,
                    timestamp_string_to_unix_seconds::<UtcNowOnceProvider>(
                        &query
                            .start
                            .expect("Start timestamp is required for range query"),
                    )?,
                    timestamp_string_to_unix_seconds::<UtcNowOnceProvider>(
                        &query
                            .end
                            .expect("End timestamp is required for range query"),
                    )?,
                    query.step.expect("Step is required for range query"),
                    3,
                )
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
