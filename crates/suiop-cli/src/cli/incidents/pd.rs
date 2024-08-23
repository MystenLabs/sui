// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use chrono::Utc;
use chrono::{DateTime, Duration, Local, NaiveDateTime};
use colored::Colorize;
use reqwest;
use reqwest::header::HeaderMap;
use reqwest::header::ACCEPT;
use reqwest::header::AUTHORIZATION;
use serde_json::Value as JsonValue;
use std::env;

/// Fetch incidents from the API using the given parameters until {limit} incidents have been received.
async fn fetch_incidents(
    limit: usize,
    start_time: DateTime<Local>,
    _end_time: DateTime<Local>,
) -> Result<Vec<JsonValue>> {
    let url = "https://api.pagerduty.com/incidents";

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        format!(
            "Token token={}",
            env::var("PD_API_KEY").expect("please set the PD_API_KEY env var")
        )
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
    Ok(all_incidents)
}

pub async fn print_recent_incidents(
    long: bool,
    limit: usize,
    days: usize,
    with_priority: bool,
) -> Result<()> {
    let current_time = Local::now();
    let start_time = current_time - Duration::days(days as i64);
    let date_format_in = "%Y-%m-%dT%H:%M:%SZ";
    let date_format_out = "%m/%d/%Y %H:%M";

    let incidents = fetch_incidents(limit, start_time, current_time).await?;
    for incident in incidents {
        if long {
            println!(
                "Incident #: {}",
                incident["incident_number"]
                    .as_u64()
                    .expect("incident_number as_u64")
                    .to_string()
                    .bright_purple()
            );
            println!(
                "Title: {}",
                incident["title"].as_str().expect("title as_str").green()
            );
            if let JsonValue::String(created_at) = incident["created_at"].clone() {
                println!(
                    "Created at: {}",
                    NaiveDateTime::parse_from_str(&created_at, date_format_in)?
                        .format(date_format_out)
                        .to_string()
                        .yellow()
                );
            }
            if let JsonValue::String(resolved_at) = incident["resolved_at"].clone() {
                println!(
                    "Resolved at: {}",
                    NaiveDateTime::parse_from_str(&resolved_at, date_format_in)?
                        .format(date_format_out)
                        .to_string()
                        .yellow()
                );
            }
            println!(
                "URL: {}",
                incident["html_url"]
                    .as_str()
                    .expect("html_url as string")
                    .bright_purple()
            );
            println!("---");
        } else {
            let resolved_at =
                if let JsonValue::String(resolved_at) = incident["resolved_at"].clone() {
                    let now = Utc::now().naive_utc();

                    Some(now - NaiveDateTime::parse_from_str(&resolved_at, date_format_in)?)
                } else {
                    None
                };
            let priority = match incident["priority"]["name"].as_str() {
                Some("P0") => "P0".red(),
                Some("P1") => "P1".magenta(),
                Some("P2") => "P2".truecolor(255, 165, 0),
                Some("P3") => "P3".yellow(),
                Some("P4") => "P4".white(),
                _ => "  ".white(),
            };
            if with_priority && priority == "  ".white() {
                // skip incidents without priority
                continue;
            }
            println!(
                "{}: ({}) {} {} ({})",
                incident["incident_number"]
                    .as_u64()
                    .expect("incident_number as_u64")
                    .to_string()
                    .bright_purple(),
                resolved_at
                    .map(|v| (v.num_days().to_string() + "d").yellow())
                    .unwrap_or("".to_string().yellow()),
                priority,
                incident["title"].as_str().expect("title").green(),
                incident["html_url"]
                    .as_str()
                    .expect("html_url")
                    .bright_purple()
            )
        }
    }
    Ok(())
}
