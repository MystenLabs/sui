// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod incident;
mod jira;
pub(crate) mod notion;
mod pd;
mod selection;
mod user;

use crate::cli::slack::Slack;
use anyhow::Result;
use chrono::{Duration, Local};
use clap::Parser;
use incident::Incident;
use jira::generate_follow_up_tasks;
use pd::print_recent_incidents;
use selection::review_recent_incidents;
use std::path::PathBuf;
use tracing::{debug, info};

#[derive(Parser, Debug, Clone)]
pub struct IncidentsArgs {
    #[command(subcommand)]
    action: IncidentsAction,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum IncidentsAction {
    /// show recent incident details
    #[command(name = "recent", aliases=["r", "recent_incidents"])]
    GetRecentIncidents {
        /// extended output with additional fields
        #[arg(short, long)]
        long: bool,
        /// the max number of incidents to show
        #[arg(long, default_value = "500")]
        limit: usize,
        /// the days to go back
        #[arg(short, long, default_value = "7")]
        days: usize,
        /// limit to incidents with any priority set
        #[arg(long, short = 'p', default_value = "false")]
        with_priority: bool,
        #[arg(short, long, default_value = "false")]
        interactive: bool,
    },
    /// generate Jira tasks for incident follow ups
    #[command(name = "generate follow up tasks", aliases=["g", "gen", "generate"])]
    GenerateFollowUpTasks {
        /// filename with tasks to add. should be named {incident number}.txt
        #[arg(short, long)]
        input_filename: PathBuf,
    },
}

/// - Fetch incidents from the PagerDuty API.
/// - Associate slack channels when they exist.
/// - Return the combined incident list.
async fn get_incidents(limit: &usize, days: &usize) -> Result<Vec<Incident>> {
    let current_time = Local::now();
    info!("going back {} days", days);
    let start_time = current_time - Duration::days(*days as i64);
    let slack = Slack::new().await;
    Ok(pd::fetch_incidents(*limit, start_time, current_time)
        .await?
        .into_iter()
        // Change into more robust Incident type
        .map(incident::Incident::from)
        .map(|mut incident| {
            // Add associated slack channel if it exists
            debug!("Checking if incidents list contains {}", incident.number);
            incident.slack_channel = selection::get_channel_for(&incident, &slack).cloned();
            debug!("Found channel: {:?}", incident.slack_channel);
            incident
        })
        .collect())
}

pub async fn incidents_cmd(args: &IncidentsArgs) -> Result<()> {
    match &args.action {
        IncidentsAction::GetRecentIncidents {
            long,
            limit,
            days,
            with_priority,
            interactive,
        } => {
            let incidents = get_incidents(limit, days).await?;
            if *interactive {
                review_recent_incidents(incidents).await?
            } else {
                print_recent_incidents(incidents, *long, *with_priority).await?
            }
        }
        IncidentsAction::GenerateFollowUpTasks { input_filename } => {
            generate_follow_up_tasks(input_filename).await?
        }
    }
    Ok(())
}
