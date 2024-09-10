// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use chrono::Utc;
use chrono::{DateTime, Local, NaiveDateTime};
use colored::{ColoredString, Colorize};
use inquire::Confirm;
use reqwest;
use reqwest::header::HeaderMap;
use reqwest::header::ACCEPT;
use reqwest::header::AUTHORIZATION;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::env;
use tracing::debug;

use crate::cli::incidents::slack::Channel;
use crate::cli::lib::utils::day_of_week;
use crate::DEBUG_MODE;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Priority {
    pub name: String,
    id: String,
    color: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Incident {
    #[serde(rename = "incident_number")]
    number: u64,
    title: String,
    created_at: Option<String>,
    resolved_at: Option<String>,
    html_url: String,
    pub priority: Option<Priority>,
    pub slack_channel: Option<Channel>,
}
const DATE_FORMAT_IN: &str = "%Y-%m-%dT%H:%M:%SZ";
const DATE_FORMAT_OUT: &str = "%m/%d/%Y %H:%M";
const DATE_FORMAT_OUT_SHORT: &str = "%m/%d/%y";

impl Incident {
    fn print(&self, long_output: bool) -> Result<()> {
        let priority = self.priority();
        if long_output {
            println!(
                "Incident #: {} {}",
                self.number.to_string().bright_purple(),
                if priority.is_empty() {
                    "".to_string()
                } else {
                    format!("({})", priority)
                }
            );
            println!("Title: {}", self.title.green());
            if let Some(created_at) = self.created_at.clone() {
                println!(
                    "Created at: {}",
                    NaiveDateTime::parse_from_str(&created_at, DATE_FORMAT_IN)?
                        .format(DATE_FORMAT_OUT)
                        .to_string()
                        .yellow()
                );
            }
            if let Some(resolved_at) = self.resolved_at.clone() {
                println!(
                    "Resolved at: {}",
                    NaiveDateTime::parse_from_str(&resolved_at, DATE_FORMAT_IN)?
                        .format(DATE_FORMAT_OUT)
                        .to_string()
                        .yellow()
                );
            }
            println!("URL: {}", self.html_url.bright_purple());
            if let Some(channel) = self.slack_channel.clone() {
                println!("Predicted Slack channel: {}", channel.url().bright_purple());
            }
            println!("---");
        } else {
            let resolved_at = if let Some(resolved_at) = self.resolved_at.clone() {
                let now = Utc::now().naive_utc();

                Some(now - NaiveDateTime::parse_from_str(&resolved_at, DATE_FORMAT_IN)?)
            } else {
                None
            };
            println!(
                "{}:{}{} {} ({})",
                self.number.to_string().bright_purple(),
                resolved_at
                    .map(|v| (v.num_days().to_string() + "d").yellow())
                    .unwrap_or("".to_string().yellow()),
                if priority.is_empty() {
                    "  ".to_string()
                } else {
                    format!(" {} ", priority)
                },
                self.title.green(),
                if let Some(channel) = self.slack_channel.clone() {
                    format!("({})", channel.url().bright_magenta())
                } else {
                    self.html_url.bright_purple().to_string()
                }
            );
        }
        Ok(())
    }

    fn priority(&self) -> ColoredString {
        // println!("{}", self.priority.as_ref().unwrap_or(&"none".to_string()));
        match self.priority.clone().map(|p| p.name).as_deref() {
            Some("P0") => "P0".red(),
            Some("P1") => "P1".magenta(),
            Some("P2") => "P2".truecolor(255, 165, 0),
            Some("P3") => "P3".yellow(),
            Some("P4") => "P4".white(),
            _ => "".white(),
        }
    }
}

/// Fetch incidents from the API using the given parameters until {limit} incidents have been received.
pub async fn fetch_incidents(
    limit: usize,
    start_time: DateTime<Local>,
    _end_time: DateTime<Local>,
) -> Result<Vec<Incident>> {
    let slack = super::slack::Slack::new().await;
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
    Ok(all_incidents
        .into_iter()
        .map(serde_json::from_value)
        .filter_map(|i| i.ok())
        .map(|mut i: Incident| {
            debug!("Checking if incidents list contains {}", i.number);
            i.slack_channel = slack
                .channels
                .iter()
                .find(|c| c.name.contains(&i.number.to_string()))
                .cloned();
            debug!("Found channel: {:?}", i.slack_channel);
            i
        })
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

pub async fn review_recent_incidents(incidents: Vec<Incident>) -> Result<()> {
    let mut to_review = vec![];
    let mut excluded = vec![];
    for incident in incidents {
        incident.print(false)?;
        let ans = Confirm::new("Keep this incident for review?")
            .with_default(false)
            .prompt()
            .expect("Unexpected response");
        if ans {
            to_review.push(incident);
        } else {
            excluded.push(incident);
        }
    }
    println!(
        "Incidents marked for review: {}",
        to_review
            .iter()
            .map(|i| i.number.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );

    fn short_print(i: &Incident) -> String {
        format!(
            "• {} {} {} {}",
            if let Some(channel) = i.slack_channel.clone() {
                format!("{} ({})", i.number, channel.url())
            } else {
                i.number.to_string()
            },
            i.resolved_at
                .clone()
                .map(|c| NaiveDateTime::parse_from_str(&c, DATE_FORMAT_IN)
                    .expect("parsing closed date")
                    .format(DATE_FORMAT_OUT_SHORT)
                    .to_string())
                .unwrap_or("".to_owned()),
            i.title,
            i.slack_channel
                .clone()
                .map(|c| c.name)
                .unwrap_or("".to_string()),
        )
    }

    let message = format!(
        "
Hello everyone and happy {}!

We have selected the following incidents for review:
{}
    
and the following incidents have been excluded from review:
{}

Please comment in the thread to request an adjustment to the list.",
        day_of_week(),
        to_review
            .iter()
            .map(short_print)
            .collect::<Vec<_>>()
            .join("\n"),
        excluded
            .iter()
            .map(short_print)
            .collect::<Vec<_>>()
            .join("\n")
    );
    println!(
        "Here is the message to send in the channel: 
    {}
    ",
        message
    );
    let slack_channel = if *DEBUG_MODE {
        "test-notifications"
    } else {
        "incident-postmortems"
    };
    let ans = Confirm::new(&format!(
        "Send this message to the #{} channel?",
        slack_channel
    ))
    .with_default(false)
    .prompt()
    .expect("Unexpected response");
    if ans {
        let slack = super::slack::Slack::new().await;
        slack.send_message(slack_channel, &message).await?;
    }
    // post to https://slack.com/api/chat.postMessage with message
    Ok(())
}
