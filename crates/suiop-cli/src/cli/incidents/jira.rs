// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use serde::Deserialize;
use serde::Serialize;
use std::env;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use tracing::debug;
use tracing::info;

const BASE_URL: &str = "https://mysten.atlassian.net/";
const CREATE_ENDPOINT: &str = "rest/api/2/issue/bulk";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IssueUpdate {
    update: IssueUpdateDetails,
    fields: IssueFields,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IssueUpdateDetails {}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IssueProject {
    id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IssueFields {
    project: IssueProject,
    summary: String,
    description: String,
    #[serde(rename = "issuetype")]
    issue_type: IssueType,
    labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IssueType {
    name: String,
}

impl IssueFields {
    fn from_raw(raw_input: &str, description: &str) -> Self {
        Self {
            project: IssueProject {
                id: "10011".to_owned(),
            },
            summary: raw_input.to_owned(),
            issue_type: IssueType {
                name: "Task".to_owned(),
            },
            description: description.to_owned(),
            labels: vec!["incident-follow-up".to_owned()],
        }
    }
}

impl fmt::Display for IssueFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "{}", self.summary)?;
        write!(f, "    {}", self.description)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IssueCreationBody {
    issue_updates: Vec<IssueUpdate>,
}

impl IssueCreationBody {
    fn new(issues: Vec<IssueFields>) -> Self {
        Self {
            issue_updates: issues
                .into_iter()
                .map(|fields| IssueUpdate {
                    update: IssueUpdateDetails {},
                    fields,
                })
                .collect(),
        }
    }
}

pub async fn generate_follow_up_tasks(input_file: &PathBuf) -> Result<()> {
    let jira_email = env::var("JIRA_API_EMAIL").expect("please set the JIRA_API_EMAIL env var");
    let jira_api_key = env::var("JIRA_API_KEY").expect("please set the JIRA_API_KEY env var");

    let input = fs::read_to_string(input_file).context("couldn't read input contents")?;
    let issues = input
        .trim()
        .lines()
        .map(|entry| {
            let entry_elements = entry.split(':').collect::<Vec<_>>();
            let inc_number = entry_elements[0];
            let title = entry_elements[1].trim();
            IssueFields::from_raw(
                title,
                &format!("This is a follow up task for incident #{}, created as a result of incident review.", &inc_number),
            )
        })
        .collect();
    let body = IssueCreationBody::new(issues);
    debug!(
        "completed request body: {}",
        serde_json::value::to_value(&body)?
    );
    let client = reqwest::Client::new();
    let request = client
        .post(format!("{}{}", BASE_URL, CREATE_ENDPOINT))
        .json(&body)
        .basic_auth(jira_email, Some(jira_api_key));
    debug!("request: {:?}", request);
    info!("Planned to create the following tasks:");
    for inc in body.issue_updates {
        info!("{}", inc.fields);
    }

    inquire::Confirm::new("do you want to proceed?")
        .with_default(false)
        .prompt()?;
    let response = request.send().await?;
    info!("{:#?}", response);
    info!("{:#?}", response.text().await?);
    Ok(())
}
