// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

#[derive(Serialize, Deserialize, Debug)]
pub struct Service {
    pub id: String,
    #[serde(rename = "type")]
    pub r#type: String,
}

impl Default for Service {
    fn default() -> Self {
        Service {
            id: "".to_string(),
            r#type: "service_reference".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Body {
    #[serde(rename = "type")]
    pub r#type: String,
    pub details: String,
}

impl Default for Body {
    fn default() -> Self {
        Body {
            r#type: "incident_body".to_string(),
            details: "".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Incident {
    pub incident_key: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub title: String,
    pub service: Service,
    pub body: Body,
}

impl Default for Incident {
    fn default() -> Self {
        Incident {
            incident_key: "".to_string(),
            r#type: "incident".to_string(),
            title: "".to_string(),
            service: Service::default(),
            body: Body::default(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct CreateIncident {
    pub incident: Incident,
}

#[derive(Clone)]
pub struct Pagerduty {
    pub client: Arc<reqwest::Client>,
    pub api_key: String,
}

impl Pagerduty {
    pub fn new(api_key: String) -> Self {
        Pagerduty {
            client: Arc::new(reqwest::Client::new()),
            api_key,
        }
    }

    pub async fn create_incident(
        &self,
        from: &str,
        incident: CreateIncident,
    ) -> anyhow::Result<()> {
        let token = format!("Token token={}", self.api_key);

        let response = self
            .client
            .post("https://api.pagerduty.com/incidents")
            .header("Authorization", token)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header("From", from)
            .json(&incident)
            .send()
            .await?;
        // Check if the status code is in the range of 200-299
        if response.status().is_success() {
            info!(
                "Created incident with key: {:?}",
                incident.incident.incident_key
            );
            Ok(())
        } else {
            let status = response.status();
            let text = response.text().await?;
            if status.is_client_error()
                && text.contains(
                    "Open incident with matching dedup key already exists on this service",
                )
            {
                info!(
                    "Incident already exists with key: {}",
                    incident.incident.incident_key
                );
                Ok(())
            } else {
                Err(anyhow!("Failed to create incident: {}", text))
            }
        }
    }
}
