// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::anyhow;
use clap::*;
use prometheus_http_query::Client;
use std::fs::File;
use std::io::Read;
use sui_metric_checker::query::{instant_query, range_query};
use sui_metric_checker::{fails_threshold_condition, timestamp_to_unix_seconds, Config, QueryType};

#[derive(Parser)]
#[clap(name = "Prometheus Query")]
pub struct Opts {
    #[clap(long, required = true)]
    api_user: String,
    #[clap(long, required = true)]
    api_key: String,
    #[clap(long, required = true)]
    config: String,
    // URL of the Prometheus server, defaults to gateway for dev environments
    #[clap(long, default_value = "https://gateway.mimir.sui.io/prometheus")]
    url: String,
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
        let c = reqwest::Client::builder().no_proxy().build().unwrap();
        Client::from(c, &opts.url).unwrap()
    };

    let mut failed_queries = Vec::new();
    for query in config.queries {
        let queried_value = match query.query_type {
            QueryType::Instant => instant_query(&auth_header, client.clone(), &query.query).await?,
            QueryType::Range => {
                range_query(
                    &auth_header,
                    client.clone(),
                    &query.query,
                    timestamp_to_unix_seconds(
                        &query
                            .start
                            .expect("Start timestamp is required for range query"),
                    )?,
                    timestamp_to_unix_seconds(
                        &query
                            .end
                            .expect("End timestamp is required for range query"),
                    )?,
                    query.step.expect("Step is required for range query"),
                )
                .await?
            }
        };

        if let Some(validate_result) = query.validate_result {
            if fails_threshold_condition(
                queried_value,
                validate_result.threshold,
                &validate_result.condition,
            )? {
                failed_queries.push(format!(
                    "Query {} returned value of {queried_value} which is {} {}",
                    query.query, validate_result.condition, validate_result.threshold
                ));
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
