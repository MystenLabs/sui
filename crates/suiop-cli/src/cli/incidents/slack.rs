// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;
use once_cell::sync::Lazy;
use reqwest::{header, Client};
use serde::Deserialize;
use serde::Serialize;
use std::fs::File;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::debug;

const CHANNELS_URL: &str = "https://slack.com/api/conversations.list";
static CHANNELS_FILEPATH: Lazy<PathBuf> = Lazy::new(|| {
    dirs::home_dir()
        .expect("HOME env var not set")
        .join(".suiop")
        .join("channels")
});

pub struct Slack {
    client: Client,
    pub channels: Vec<Channel>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Channel {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ResponseMetadata {
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ConversationsResponse {
    ok: bool,
    error: Option<String>,
    channels: Option<Vec<Channel>>,
    response_metadata: ResponseMetadata,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct SendMessageBody {
    channel: String,
    text: String,
    ts: String,
    mrkdwn: bool,
}

impl Slack {
    pub async fn new() -> Self {
        let token = std::env::var("SLACK_BOT_TOKEN").expect(
            "Please set SLACK_BOT_TOKEN env var ('slack bot token (incidentbot)' in 1password)",
        );
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
        let mut s = Self {
            client,
            channels: vec![],
        };
        s.get_channels().await.expect("Failed to get channels");
        s
    }

    pub async fn serialize_channels(&self) -> Result<()> {
        let file = File::create(CHANNELS_FILEPATH.as_path())?;
        serde_json::to_writer(file, &self.channels)?;
        Ok(())
    }

    pub async fn get_channels(&mut self) -> Result<Vec<String>> {
        let mut channels: Vec<Channel> = vec![];
        let file_path = CHANNELS_FILEPATH.as_path();
        if let Ok(metadata) = file_path.metadata() {
            if let Ok(modified) = metadata.modified() {
                if let Ok(elapsed) = modified.elapsed() {
                    // 1 day
                    if elapsed.as_secs() < 24 * 60 * 60 {
                        if let Ok(file) = File::open(file_path) {
                            if let Ok(channels) = serde_json::from_reader::<_, Vec<Channel>>(file) {
                                debug!("Using cached channels");
                                self.channels = channels;
                                return Ok(self.channels.iter().map(|c| c.name.clone()).collect());
                            }
                        }
                    }
                }
            }
        }

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
        self.serialize_channels().await?;
        let names = self.channels.iter().map(|c| c.name.clone()).collect();
        Ok(names)
    }

    pub async fn send_message(&self, channel: &str, message: &str) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();
        let message_body = SendMessageBody {
            channel: channel.to_owned(),
            text: message.to_owned(),
            ts: timestamp.to_string(),
            mrkdwn: true,
        };
        let url = "https://slack.com/api/chat.postMessage";
        let response = self.client.post(url).json(&message_body).send().await?;
        let response = response.json::<serde_json::Value>().await?;
        if response["ok"].as_bool().expect("ok was not a bool") {
            Ok(())
        } else {
            Err(anyhow!("Failed to send message: {}", response))
        }
    }
}

impl Channel {
    pub fn url(self) -> String {
        format!("https://mysten-labs.slack.com/archives/{}", self.id)
    }
}
