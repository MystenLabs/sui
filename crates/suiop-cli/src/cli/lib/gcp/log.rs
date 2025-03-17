// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;

// Structs to serialize/deserialize the Logging API request/response
#[derive(Serialize)]
pub struct LogRequest<'a> {
    pub resource_names: Vec<String>,
    pub filter: String,
    pub order_by: String,
    pub page_size: i32,
    pub page_token: Option<&'a str>,
}

#[derive(Deserialize, Debug)]
pub struct LogEntry {
    #[serde(rename = "textPayload")]
    pub text_payload: Option<String>,
    pub timestamp: String,
    pub labels: HashMap<String, String>,
}

#[derive(Deserialize, Debug)]
pub struct LogResponse {
    pub entries: Vec<LogEntry>,
}

// Function to get an access token using gcloud
fn get_gcp_access_token() -> Result<String> {
    let output = Command::new("gcloud")
        .args(["auth", "print-access-token"])
        .output()?;

    if output.status.success() {
        let token = String::from_utf8(output.stdout)?.trim().to_string();
        Ok(token)
    } else {
        let error = String::from_utf8(output.stderr)?;
        Err(anyhow::anyhow!("Failed to get access token: {}", error))
    }
}

pub async fn logs_for_ns(
    gcp_project_id: &str,
    cluster_name: &str,
    namespace: &str,
) -> Result<LogResponse> {
    // Get an access token using gcloud
    let access_token = get_gcp_access_token()?;

    // Build the filter for GKE logs
    let filter = format!(
        r#"resource.type="k8s_container"
         resource.labels.cluster_name="{}"
         resource.labels.namespace_name="{}""#,
        cluster_name, namespace
    );

    // Define the request body for the Logging API
    let request_body = LogRequest {
        resource_names: vec![format!("projects/{}", gcp_project_id)],
        filter,
        order_by: "timestamp desc".to_string(),
        page_size: 100,
        page_token: None,
    };

    // Set up the HTTP client
    let client = Client::new();
    let url = "https://logging.googleapis.com/v2/entries:list".to_owned();

    // Send the request to the Logging API
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&request_body)
        .send()
        .await?;

    // Parse the response
    let log_entries: LogResponse = response.json().await?;
    Ok(log_entries)
}
