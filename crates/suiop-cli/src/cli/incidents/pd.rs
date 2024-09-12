// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use chrono::Utc;
use chrono::{DateTime, Local, NaiveDateTime};
use colored::{ColoredString, Colorize};
use inquire::{Confirm, MultiSelect};
use reqwest;
use reqwest::header::HeaderMap;
use reqwest::header::ACCEPT;
use reqwest::header::AUTHORIZATION;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::env;
use strsim::normalized_damerau_levenshtein;
use tracing::debug;

use crate::cli::incidents::slack::Slack;
use crate::cli::lib::utils::day_of_week;
use crate::DEBUG_MODE;

use super::slack_api::{Channel, User};

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
pub struct Incident {
    #[serde(rename = "incident_number")]
    number: u64,
    title: String,
    created_at: Option<String>,
    resolved_at: Option<String>,
    html_url: String,
    /// The slack users responsible for reporting
    #[serde(skip_deserializing)]
    poc_users: Option<Vec<User>>,
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

    fn short_fmt(&self) -> String {
        format!(
            "• {} {} {} {}",
            if let Some(channel) = self.slack_channel.clone() {
                format!("{} (<#{}>)", self.number, channel.id)
            } else {
                self.number.to_string()
            },
            self.resolved_at
                .clone()
                .map(|c| NaiveDateTime::parse_from_str(&c, DATE_FORMAT_IN)
                    .expect("parsing closed date")
                    .format(DATE_FORMAT_OUT_SHORT)
                    .to_string())
                .unwrap_or("".to_owned()),
            self.title,
            self.poc_users.as_ref().map_or_else(
                || "".to_string(),
                |u| u
                    .iter()
                    .map(|u| { format!("<@{}>", u.id) })
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        )
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

/// Filter incidents based on whether they have <= min_priority priority or any slack
/// channel associated.
fn filter_incidents_for_review(incidents: Vec<Incident>, min_priority: &str) -> Vec<Incident> {
    let min_priority_u = min_priority
        .trim_start_matches("P")
        .parse::<u8>()
        .expect("Parsing priority");
    println!("min_priority_u: {}", min_priority_u);
    incidents
        .into_iter()
        // filter on priority <= min_priority and any slack channel association
        .filter(|i| {
            i.priority
                .clone()
                .filter(|p| {
                    println!("{} <= {}?", p.u8(), min_priority_u);
                    !p.name.is_empty() && p.u8() <= min_priority_u
                })
                .is_some()
                || i.slack_channel.is_some()
        })
        .collect()
}

fn request_pocs(slack: &Slack) -> Result<Vec<User>> {
    MultiSelect::new(
        "Please select the users who are POCs for this incident",
        slack.users.clone(),
    )
    .with_default(&[])
    .prompt()
    .map_err(|e| anyhow::anyhow!(e))
}

pub async fn review_recent_incidents(incidents: Vec<Incident>) -> Result<()> {
    let slack = Slack::new().await;
    let filtered_incidents = filter_incidents_for_review(incidents, "P2");
    let mut group_map = group_by_similar_title(filtered_incidents, 0.9);
    let mut to_review = vec![];
    let mut excluded = vec![];
    for (title, incident_group) in group_map.iter_mut() {
        let treat_as_one = if incident_group.len() > 1 {
            println!(
                "There are {} incidents with a title similar to this: {}",
                &incident_group.len(),
                title
            );
            println!("All incidents with a similar title:");
            for i in incident_group.iter() {
                i.print(false)?;
            }
            Confirm::new("Treat them as one?")
                .with_default(true)
                .prompt()
                .expect("Unexpected response")
        } else {
            false
        };
        if treat_as_one {
            let ans = Confirm::new("Keep these incidents for review?")
                .with_default(false)
                .prompt()
                .expect("Unexpected response");
            if ans {
                let poc_users = request_pocs(&slack)?;
                incident_group
                    .iter_mut()
                    .for_each(|i| i.poc_users = Some(poc_users.clone()));
                to_review.extend(incident_group.clone());
            } else {
                excluded.extend(incident_group.clone());
            }
        } else {
            for incident in incident_group.iter_mut() {
                incident.print(false)?;
                let ans = Confirm::new("Keep this incident for review?")
                    .with_default(false)
                    .prompt()
                    .expect("Unexpected response");
                if ans {
                    let poc_users = request_pocs(&slack)?;
                    incident.poc_users = Some(poc_users.clone());
                    to_review.push(incident.clone());
                } else {
                    excluded.push(incident.clone());
                }
            }
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
            .map(Incident::short_fmt)
            .collect::<Vec<_>>()
            .join("\n"),
        excluded
            .iter()
            .map(Incident::short_fmt)
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
        slack.send_message(slack_channel, &message).await?;
    }
    // post to https://slack.com/api/chat.postMessage with message
    Ok(())
}

fn group_by_similar_title(
    incidents: Vec<Incident>,
    threshold: f64,
) -> HashMap<String, Vec<Incident>> {
    if !(0.0..=1.0).contains(&threshold) {
        panic!("Threshold must be between 0.0 and 1.0");
    }

    let mut groups: HashMap<String, Vec<Incident>> = HashMap::new();

    for incident in incidents {
        // Try to find an existing title that is similar enough
        let mut found = false;
        for (existing_title, group) in groups.iter_mut() {
            if normalized_damerau_levenshtein(
                &incident.title.chars().take(20).collect::<String>(),
                &existing_title.chars().take(20).collect::<String>(),
            ) >= threshold
            {
                // If similar, add it to this group
                group.push(incident.clone());
                found = true;
                break;
            }
        }

        // If no similar title found, add a new group
        if !found {
            groups
                .entry(incident.title.clone())
                .or_default()
                .push(incident);
        }
    }

    debug!(
        "map: {:#?}",
        groups.iter().map(|(k, v)| (k, v.len())).collect::<Vec<_>>()
    );
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_by_similar_title() {
        let incidents = vec![
            Incident {
                title: "Incident 1".to_string(),
                ..Default::default()
            },
            Incident {
                title: "Incident 2".to_string(),
                ..Default::default()
            },
            Incident {
                title: "Another thing entirely".to_string(),
                ..Default::default()
            },
            Incident {
                title: "Another thing entirely 2".to_string(),
                ..Default::default()
            },
            Incident {
                title: "A third thing that doesn't look the same".to_string(),
                ..Default::default()
            },
        ];

        let groups = group_by_similar_title(incidents, 0.8);
        println!("{:#?}", groups);

        assert_eq!(groups.len(), 3);
        assert_eq!(groups.get("Incident 1").unwrap().len(), 2);
        assert!(groups.get("Incident 2").is_none());
        assert_eq!(groups.get("Another thing entirely").unwrap().len(), 2);
        assert_eq!(
            groups
                .get("A third thing that doesn't look the same")
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn test_group_by_similar_title_with_similar_titles() {
        let incidents = vec![
            Incident {
                title: "Incident 1".to_string(),
                ..Default::default()
            },
            Incident {
                title: "Incident 1".to_string(),
                ..Default::default()
            },
            Incident {
                title: "Incident 2".to_string(),
                ..Default::default()
            },
            Incident {
                title: "Incident 2".to_string(),
                ..Default::default()
            },
            Incident {
                title: "Incident 3".to_string(),
                ..Default::default()
            },
        ];

        let groups = group_by_similar_title(incidents, 0.8);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups.get("Incident 1").unwrap().len(), 5);
    }

    #[test]
    #[should_panic(expected = "Threshold must be between 0.0 and 1.0")]
    fn test_group_by_similar_title_with_invalid_threshold() {
        let incidents = vec![
            Incident {
                title: "Incident 1".to_string(),
                ..Default::default()
            },
            Incident {
                title: "Incident 2".to_string(),
                ..Default::default()
            },
        ];

        group_by_similar_title(incidents, -0.5);
    }
}
