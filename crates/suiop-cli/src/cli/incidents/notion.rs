// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::cli::notion::ids::DatabaseId;
use crate::cli::notion::models::search::DatabaseQuery;
use crate::cli::notion::models::{ListResponse, Page};
use crate::cli::notion::NotionApi;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::str::FromStr;
use tracing::debug;

use crate::DEBUG_MODE;

use super::incident::Incident;

// incident selection db
pub static INCIDENT_DB_ID: Lazy<DatabaseId> = Lazy::new(|| {
    if *DEBUG_MODE {
        // incident selection db for testing
        DatabaseId::from_str("10e6d9dcb4e980f8ae73c4aa2da176cd").expect("Invalid Database ID")
    } else {
        // incident selection db for production
        DatabaseId::from_str("a8da55dadb524e7db202b4dfd799d9ce").expect("Invalid Database ID")
    }
});

// incident selection db names
pub static INCIDENT_DB_NAME: Lazy<String> = Lazy::new(|| {
    if *DEBUG_MODE {
        "Incident Selection (Debug)".to_owned()
    } else {
        "Incident Selection".to_owned()
    }
});

/// Macro for debugging Notion database properties.
///
/// This macro takes two arguments:
/// - `$notion`: A reference to a Notion instance.
/// - `$prop`: The name of the property to debug.
///
/// It retrieves the specified database, gets the property, and prints debug information
/// based on the property type. Supported property types include:
/// - MultiSelect
/// - People
/// - Date
/// - Title
/// - Checkbox
///
/// For unsupported property types, it prints an "Unexpected property type" message.
///
/// # Panics
///
/// This macro will panic if:
/// - It fails to get the database.
/// - The specified property does not exist in the database.
#[allow(unused_macros)]
macro_rules! debug_prop {
    ($notion:expr, $prop:expr) => {
        let db = $notion
            .client
            .get_database(INCIDENT_DB_ID.clone())
            .await
            .expect("Failed to get database");
        let prop = db.properties.get($prop).unwrap();
        match prop {
            PropertyConfiguration::MultiSelect {
                multi_select,
                id: _,
            } => {
                println!("multi select property");
                println!("{:#?}", multi_select.options);
            }
            PropertyConfiguration::People { id: _ } => {
                println!("people property");
            }
            PropertyConfiguration::Date { id: _ } => {
                println!("date property");
            }
            PropertyConfiguration::Title { id: _ } => {
                println!("title property");
            }
            PropertyConfiguration::Checkbox { id: _ } => {
                println!("checkbox property");
            }
            _ => {
                println!("Unexpected property type {:?}", prop);
            }
        }
    };
}

pub struct Notion {
    client: NotionApi,
    token: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NotionPerson {
    pub object: String,
    pub id: String,
    pub name: String,
    pub avatar_url: Option<String>,
    pub person: NotionPersonDetails,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NotionPersonDetails {
    pub email: String,
}
impl Notion {
    pub fn new() -> Self {
        let token = env::var("NOTION_API_TOKEN")
            .expect("Please set the NOTION_API_TOKEN environment variable");
        debug!("using notion token {}", token);
        let client = NotionApi::new(token.clone()).expect("Failed to create Notion API client");
        Self { client, token }
    }

    /// Get all incidents from the incident selection database
    #[allow(dead_code)]
    pub async fn get_incident_selection_incidents(&self) -> Result<ListResponse<Page>> {
        // Retrieve the db
        self.client
            .query_database(INCIDENT_DB_ID.clone(), DatabaseQuery::default())
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Get all people objects from the Notion API
    pub async fn get_all_people(&self) -> Result<Vec<NotionPerson>> {
        let url = "https://api.notion.com/v1/users";
        let client = reqwest::Client::new();

        let response = client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Notion-Version", "2022-06-28")
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send request: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Request failed with status: {}, response: {}",
                response.status(),
                response
                    .text()
                    .await
                    .unwrap_or("no response text".to_owned())
            ));
        }

        response
            .json::<serde_json::Value>()
            .await
            .map(|v| {
                serde_json::from_value::<Vec<NotionPerson>>(v["results"].clone())
                    .expect("deserializing people")
            })
            .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))
    }

    /// Get the shape of the incident selection database to understand the data model
    #[allow(dead_code)]
    pub async fn get_shape(self) -> Result<()> {
        let db = self.client.get_database(INCIDENT_DB_ID.clone()).await?;
        println!("{:#?}", db.properties);
        Ok(())
    }

    /// Insert a suiop incident into the incident selection database
    pub async fn insert_incident(&self, incident: Incident) -> Result<()> {
        let url = "https://api.notion.com/v1/pages";
        let body = json!({
            "parent": { "database_id": INCIDENT_DB_ID.to_string() },
            "properties": {
                "Name": {
                    "title": [{
                        "text": {
                            "content":format!("{}: {}", incident.number, incident.title)
                        }
                    }]
                },
                "link": {
                    "url": incident.html_url,
                },
                "PoC(s)": {
                    "people": incident.poc_users.unwrap_or_else(|| panic!("no poc users for incident {}", incident.number)).iter().map(|u| {
                        json!({
                            "object": "user",
                            "id": u.notion_user.as_ref().map(|u| u.id.clone()),
                        })
                    }).collect::<Vec<_>>(),
                },
            }
        });

        let client = reqwest::ClientBuilder::new()
            // .default_headers(headers)
            .build()
            .expect("failed to build reqwest client");
        let response = client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .header("Notion-Version", "2021-05-13")
            .json(&body)
            .send()
            .await
            .context("sending insert db row")?;

        if response.status().is_success() {
            debug!(
                "inserted incident: {:?}",
                response.text().await.context("getting response text")?
            );
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Failed to insert incident: {:?}",
                response.text().await.context("getting response text")?
            ))
        }
    }
}
