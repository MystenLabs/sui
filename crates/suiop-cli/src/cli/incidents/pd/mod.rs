// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use chrono::{DateTime, Local};
use colored::Colorize;
use reqwest;
use reqwest::header::HeaderMap;
use reqwest::header::ACCEPT;
use reqwest::header::AUTHORIZATION;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::env;
use tracing::debug;

use super::incident::Incident;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Priority {
    pub name: String,
    id: String,
    color: String,
}

impl Priority {
    pub fn u8(&self) -> u8 {
        self.name
            .trim_start_matches("P")
            .parse()
            .expect("Parsing priority")
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub(crate) struct PagerDutyIncident {
    #[serde(rename = "incident_number")]
    pub number: u64,
    pub title: String,
    pub created_at: Option<String>,
    pub resolved_at: Option<String>,
    pub html_url: String,
    pub priority: Option<Priority>,
}

/// Fetch incidents from the API using the given parameters until {limit} incidents have been received.
pub async fn fetch_incidents(
    limit: usize,
    start_time: DateTime<Local>,
    _end_time: DateTime<Local>,
) -> Result<Vec<PagerDutyIncident>> {
    let url = "https://api.pagerduty.com/incidents";

    let api_key = env::var("PD_API_KEY").expect("please set the PD_API_KEY env var");
    if api_key.is_empty() {
        panic!("PD_API_KEY is not set");
    }

    debug!("fetching incidents from pagerduty with {}", api_key);
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        format!("Token token={}", api_key)
            .parse()
            .expect("header parsing"),
    );
    headers.insert(
        ACCEPT,
        "application/vnd.pagerduty+json;version=2"
            .parse()
            .expect("header parsing"),
    );

    let mut more_records = true;
    let mut all_incidents = vec![];
    let mut offset = 0;
    while more_records {
        let params = [
            ("offset", offset.to_string()),
            ("limit", limit.to_string()),
            ("sort_by", "resolved_at:desc".to_owned()),
            ("date_range", "all".to_owned()),
            ("statuses[]", "resolved".to_owned()),
        ];
        let client = reqwest::Client::new();
        let response = client
            .get(url)
            .headers(headers.clone())
            .query(&params)
            .send()
            .await?;
        let response = &response.json::<JsonValue>().await?;
        let incidents_received: Vec<JsonValue> =
            serde_json::from_value(response["incidents"].clone())?;
        let count_received = incidents_received.len();

        offset += count_received;
        more_records = response["more"].as_bool().expect("'more' was not a bool");

        let truncated_incidents_received: Vec<_> = incidents_received
            .clone()
            .into_iter()
            .take_while(|i| {
                let latest_resolved_at: DateTime<Local> =
                    serde_json::from_value(i["resolved_at"].clone()).unwrap();
                latest_resolved_at > start_time
            })
            .collect();
        let num_truncated_incidents = truncated_incidents_received.len();
        all_incidents.extend(truncated_incidents_received);
        if all_incidents.len() >= limit {
            // don't need any more incidents.
            all_incidents.truncate(limit);
            break;
        }
        if num_truncated_incidents < incidents_received.len() {
            // we already got all incidents that were resolved in the given time
            break;
        }
    }
    Ok(all_incidents
        .into_iter()
        .map(serde_json::from_value)
        .filter_map(|i| i.ok())
        .collect())
}

pub async fn print_recent_incidents(
    incidents: Vec<Incident>,
    long_output: bool,
    with_priority: bool,
) -> Result<()> {
    for incident in &incidents {
        if with_priority && incident.priority() == "  ".white() {
            // skip incidents without priority
            continue;
        }
        incident.print(long_output)?;
    }
    Ok(())
}
