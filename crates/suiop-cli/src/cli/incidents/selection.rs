// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use inquire::{Confirm, MultiSelect};
use std::collections::HashMap;
use strsim::normalized_damerau_levenshtein;
use tracing::debug;

use crate::cli::lib::utils::day_of_week;
use crate::cli::slack::{Channel, Slack, User};
use crate::DEBUG_MODE;

use super::incident::Incident;

fn request_pocs(slack: &Slack) -> Result<Vec<User>> {
    MultiSelect::new(
        "Please select the users who are POCs for this incident",
        slack.users.clone(),
    )
    .with_default(&[])
    .prompt()
    .map_err(|e| anyhow::anyhow!(e))
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
                .filter(|p| !p.name.is_empty() && p.u8() <= min_priority_u)
                .is_some()
                || i.slack_channel.is_some()
        })
        .collect()
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

pub fn get_channel_for<'a>(incident: &Incident, slack: &'a Slack) -> Option<&'a Channel> {
    slack
        .channels
        .iter()
        .find(|c| c.name.contains(&incident.number.to_string()))
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
