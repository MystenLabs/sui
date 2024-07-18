// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::unix_seconds_to_timestamp_string;
use anyhow::anyhow;
use base64::{engine::general_purpose, Engine};
use prometheus_http_query::Client;
use reqwest::header::{HeaderValue, AUTHORIZATION};
use tracing::debug;

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

// This will return the median value of the queried metric over the given time range.
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
        let mut samples: Vec<f64> = result
            .first()
            .unwrap()
            .samples()
            .iter()
            .filter_map(|sample| {
                let v = sample.value();
                if v.is_nan() {
                    None
                } else {
                    Some(v)
                }
            })
            .collect();
        assert!(!samples.is_empty(), "No valid samples found for {query}");

        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let median = (samples[(samples.len() - 1) / 2] + samples[samples.len() / 2]) / 2.;
        debug!(
            "Got median value {median} over time range {} - {}",
            unix_seconds_to_timestamp_string(start),
            unix_seconds_to_timestamp_string(end)
        );
        Ok(median)
    } else {
        Err(anyhow!(
            "Did not get expected response from server for {query}"
        ))
    }
}
