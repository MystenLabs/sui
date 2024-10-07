// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use chrono::NaiveDateTime;
use chrono::Utc;
use colored::{ColoredString, Colorize};
use serde::{Deserialize, Serialize};

use super::pd::PagerDutyIncident;
use super::pd::Priority;
use super::user::User;
use crate::cli::slack::Channel;

const DATE_FORMAT_IN: &str = "%Y-%m-%dT%H:%M:%SZ";
const DATE_FORMAT_OUT: &str = "%m/%d/%Y %H:%M";
const DATE_FORMAT_OUT_SHORT: &str = "%m/%d/%y";

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Incident {
    pub number: u64,
    pub title: String,
    pub created_at: Option<String>,
    pub resolved_at: Option<String>,
    pub html_url: String,
    /// The users responsible for reporting
    #[serde(skip_deserializing)]
    pub poc_users: Option<Vec<User>>,
    pub priority: Option<Priority>,
    pub slack_channel: Option<Channel>,
}

impl From<PagerDutyIncident> for Incident {
    fn from(p: PagerDutyIncident) -> Self {
        Self {
            number: p.number,
            title: p.title,
            created_at: p.created_at,
            resolved_at: p.resolved_at,
            html_url: p.html_url,
            poc_users: None,
            priority: p.priority,
            slack_channel: None,
        }
    }
}

impl Incident {
    pub fn print(&self, long_output: bool) -> Result<()> {
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

    pub fn priority(&self) -> ColoredString {
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

    pub fn short_fmt(&self) -> String {
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
                    .map(|u| {
                        u.slack_user
                            .as_ref()
                            .map_or("".to_owned(), |su| format!("<@{}>", su.id))
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        )
    }
}
