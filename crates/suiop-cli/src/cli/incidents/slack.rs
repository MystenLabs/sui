// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use futures::future::Either;
use once_cell::sync::Lazy;
use reqwest::{header, Client};
use std::fs::File;
use std::path::PathBuf;
use tracing::debug;

use super::slack_api::get_channels;
use super::slack_api::get_users;
use super::slack_api::Channel;
use super::slack_api::User;

static CHANNELS_FILEPATH: Lazy<PathBuf> = Lazy::new(|| {
    dirs::home_dir()
        .expect("HOME env var not set")
        .join(".suiop")
        .join("channels")
});

#[derive(Debug, Default)]
pub struct Slack {
    client: Client,
    pub channels: Vec<Channel>,
    pub users: Vec<User>,
}

pub async fn serialize_channels(channels: &Vec<Channel>) -> Result<()> {
    let file = File::create(CHANNELS_FILEPATH.as_path())?;
    serde_json::to_writer(file, channels)?;
    Ok(())
}

pub async fn deserialize_channels() -> Option<Vec<Channel>> {
    let mut result = None;
    let file_path = CHANNELS_FILEPATH.as_path();
    if let Ok(metadata) = file_path.metadata() {
        if let Ok(modified) = metadata.modified() {
            if let Ok(elapsed) = modified.elapsed() {
                // 1 day
                if elapsed.as_secs() < 24 * 60 * 60 {
                    if let Ok(file) = File::open(file_path) {
                        if let Ok(channels) = serde_json::from_reader::<_, Vec<Channel>>(file) {
                            debug!("Using cached channels");
                            result = Some(channels);
                        }
                    }
                }
            }
        }
    }
    result
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
        let channels = deserialize_channels()
            .await
            .map_or_else(
                || {
                    Either::Left(async {
                        let channels = get_channels(&client).await.expect("Failed to get channels");
                        serialize_channels(&channels)
                            .await
                            .expect("Failed to serialize channels");
                        channels
                    })
                },
                |v| Either::Right(async { v }),
            )
            .await;
        let users = get_users(&client).await.expect("Failed to get users");
        Self {
            client,
            channels,
            users,
        }
    }

    pub async fn send_message(self, channel: &str, message: &str) -> Result<()> {
        super::slack_api::send_message(&self.client, channel, message).await
    }
}

impl Channel {
    pub fn url(self) -> String {
        format!("https://mysten-labs.slack.com/archives/{}", self.id)
    }
}
