// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod jira;
mod pd;
mod slack;

use anyhow::Result;
use chrono::{Duration, Local};
use clap::Parser;
use jira::generate_follow_up_tasks;
use pd::{fetch_incidents, print_recent_incidents, review_recent_incidents};
use std::path::PathBuf;

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

pub async fn incidents_cmd(args: &IncidentsArgs) -> Result<()> {
    match &args.action {
        IncidentsAction::GetRecentIncidents {
            long,
            limit,
            days,
            with_priority,
            interactive,
        } => {
            let current_time = Local::now();
            let start_time = current_time - Duration::days(*days as i64);

            let incidents = fetch_incidents(*limit, start_time, current_time).await?;
            if *interactive {
                println!("{:?} incidents found", incidents);
                review_recent_incidents(
                    incidents
                        .into_iter()
                        // filter on priority > P3 and any slack channel association
                        .filter(|i| {
                            i.priority
                                .clone()
                                .filter(|p| !p.name.is_empty() && p.name != "P3")
                                .is_some()
                                || i.slack_channel.is_some()
                        })
                        .collect::<Vec<_>>(),
                )
                .await?
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
