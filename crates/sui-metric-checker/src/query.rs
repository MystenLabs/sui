// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::unix_seconds_to_timestamp_string;
use anyhow::anyhow;
use base64::{engine::general_purpose, Engine};
use prometheus_http_query::Client;
use reqwest::header::{HeaderValue, AUTHORIZATION};
use std::time::Duration;
use tokio::time::sleep;
use tracing::debug;

const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(5);

pub async fn instant_query_with_retries(
    auth_header: &str,
    client: Client,
    query: &str,
    max_retries: u32,
) -> Result<f64, anyhow::Error> {
    let mut retries = 0;
    let mut backoff = INITIAL_BACKOFF;

    loop {
        match instant_query(auth_header, client.clone(), query).await {
            Ok(value) => return Ok(value),
            Err(error) => {
                if retries >= max_retries {
                    return Err(anyhow!("After {max_retries} retry attempts - {error}"));
                }
                retries += 1;
                debug!("Query \"{query}\" failed, retry attempt {retries}");
                sleep(backoff).await;
                backoff *= 2;
                if backoff > MAX_BACKOFF {
                    backoff = MAX_BACKOFF;
                }
            }
        }
    }
}

pub async fn range_query_with_retries(
    auth_header: &str,
    client: Client,
    query: &str,
    start: i64,
    end: i64,
    step: f64,
    max_retries: u32,
) -> Result<f64, anyhow::Error> {
    let mut retries = 0;
    let mut backoff = INITIAL_BACKOFF;

    loop {
        match range_query(auth_header, client.clone(), query, start, end, step).await {
            Ok(value) => return Ok(value),
            Err(error) => {
                if retries >= max_retries {
                    return Err(anyhow!("After {max_retries} retry attempts - {error}"));
                }
                retries += 1;
                debug!("Query \"{query}\" failed, retry attempt {retries}");
                sleep(backoff).await;
                backoff *= 2;
                if backoff > MAX_BACKOFF {
                    backoff = MAX_BACKOFF;
                }
            }
        }
    }
}

pub async fn instant_query(
    auth_header: &str,
    client: Client,
    query: &str,
) -> Result<f64, anyhow::Error> {
    debug!("Executing {query}");
    let response = client
        .query(query)
        .header(
            AUTHORIZATION,
            HeaderValue::from_str(&format!(
                "Basic {}",
                general_purpose::STANDARD.encode(auth_header)
            ))?,
        )
        .get()
        .await?;

    let result = response
        .data()
        .as_vector()
        .unwrap_or_else(|| panic!("Expected result of type vector for {query}"));

    if !result.is_empty() {
        let first = result.first().unwrap();
        debug!("Got value {}", first.sample().value());
        Ok(first.sample().value())
    } else {
        Err(anyhow!(
            "Did not get expected response from server for {query}"
        ))
    }
}

// This will return the average value of the queried metric over the given time range.
pub async fn range_query(
    auth_header: &str,
    client: Client,
    query: &str,
    start: i64,
    end: i64,
    step: f64,
) -> Result<f64, anyhow::Error> {
    debug!("Executing {query}");
    let response = client
        .query_range(query, start, end, step)
        .header(
            AUTHORIZATION,
            HeaderValue::from_str(&format!(
                "Basic {}",
                general_purpose::STANDARD.encode(auth_header)
            ))?,
        )
        .get()
        .await?;

    let result = response
        .data()
        .as_matrix()
        .unwrap_or_else(|| panic!("Expected result of type matrix for {query}"));

    if !result.is_empty() {
        let samples = result.first().unwrap().samples();
        let sum: f64 = samples.iter().map(|sample| sample.value()).sum();
        let count = samples.len();

        let avg = if count > 0 { sum / count as f64 } else { 0.0 };
        debug!(
            "Got average value {avg} over time range {} - {}",
            unix_seconds_to_timestamp_string(start),
            unix_seconds_to_timestamp_string(end)
        );
        Ok(avg)
    } else {
        Err(anyhow!(
            "Did not get expected response from server for {query}"
        ))
    }
}
