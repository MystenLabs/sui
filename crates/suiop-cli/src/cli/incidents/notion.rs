use anyhow::{Context, Result};
use notion::ids::DatabaseId;
use notion::models::search::DatabaseQuery;
use notion::models::{ListResponse, Page};
use notion::NotionApi;
use once_cell::sync::Lazy;
use serde_json::json;
use std::env;
use std::str::FromStr;

static AUTH_KEYS: Lazy<String> = Lazy::new(|| env::var("AUTH_KEYS").unwrap());
// incident selection db
static INCIDENT_DB_ID: Lazy<DatabaseId> = Lazy::new(|| {
    DatabaseId::from_str("a8da55dadb524e7db202b4dfd799d9ce").expect("Invalid Database ID")
});

pub struct Notion {
    client: NotionApi,
    token: String,
}

impl Notion {
    pub fn new() -> Self {
        let token = env::var("NOTION_API_TOKEN")
            .expect("Please set the NOTION_API_TOKEN environment variable");
        let client = NotionApi::new(token.clone()).expect("Failed to create Notion API client");
        Self { client, token }
    }

    pub async fn get_incident_selection_incidents(self) -> Result<ListResponse<Page>> {
        // Retrieve the db
        self.client
            .query_database(INCIDENT_DB_ID.clone(), DatabaseQuery::default())
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn insert_incident(self) -> Result<()> {
        let url = format!("https://api.notion.com/v1/pages");
        let body = json!({
            "parent": { "database_id": INCIDENT_DB_ID.to_string() },
            "properties": {
                "Name": {
                    "title": [{
                        "text": {
                            "content": "00000 [test] New Incident"
                        }
                    }]
                },
                // "Description": {
                //     "rich_text": [{
                //         "text": {
                //             "content": "Description of the new incident"
                //         }
                //     }]
                // }
            }
        });

        let client = reqwest::ClientBuilder::new()
            // .default_headers(headers)
            .build()
            .expect("failed to build reqwest client");
        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .header("Notion-Version", "2021-05-13")
            .json(&body)
            .send()
            .await
            .context("sending insert db row")?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Failed to insert incident: {:?}",
                response.text().await.context("getting response text")?
            ))
        }
    }
}
