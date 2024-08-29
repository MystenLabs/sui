use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;
use reqwest::{header, Client};
use serde::Deserialize;

const CHANNELS_URL: &str = "https://slack.com/api/conversations.list";

pub struct Slack {
    client: Client,
    pub channels: Vec<Channel>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Channel {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize, Clone)]
struct ResponseMetadata {
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct ConversationsResponse {
    ok: bool,
    error: Option<String>,
    channels: Option<Vec<Channel>>,
    response_metadata: ResponseMetadata,
}

impl Slack {
    pub fn new() -> Self {
        let token = std::env::var("SLACK_BOT_TOKEN").expect("Please set SLACK_BOT_TOKEN env var");
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(format!("Bearer {}", token).as_str())
                .expect("failed to add Bearer token for slack client"),
        );
        let client = reqwest::ClientBuilder::new()
            .default_headers(headers)
            .build()
            .expect("failed to build reqwest client");
        Self {
            client,
            channels: vec![],
        }
    }

    pub async fn get_channels(&mut self) -> Result<Vec<String>> {
        let mut channels: Vec<Channel> = vec![];

        let mut result: ConversationsResponse = self
            .client
            .get(CHANNELS_URL)
            .send()
            .await
            .map_err(|e| anyhow!(e))?
            .json()
            .await?;
        let new_channels = result.channels.expect("Expected channels to exist").clone();
        channels.extend(new_channels.into_iter());
        while let Some(cursor) = result.response_metadata.next_cursor {
            if cursor.is_empty() {
                break;
            }
            result = self
                .client
                .get(CHANNELS_URL)
                .query(&[("cursor", cursor)])
                .send()
                .await
                .map_err(|e| anyhow!(e))?
                .json()
                .await
                .context("parsing json from channels api")?;
            let extra_channels = result.channels.expect("Expected channels to exist").clone();
            channels.extend(extra_channels.into_iter());
        }
        self.channels = channels.iter().map(|c| (*c).clone()).collect();
        let names = self.channels.iter().map(|c| c.name.clone()).collect();
        Ok(names)
    }
}
