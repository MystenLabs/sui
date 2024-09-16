// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use futures::future::Either;
use reqwest::{header, Client};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::File;
use std::path::PathBuf;
use tracing::debug;

use super::slack_api::get_channels;
use super::slack_api::get_users;
use super::slack_api::Channel;
use super::slack_api::User;

#[derive(Debug, Default)]
pub struct Slack {
    client: Client,
    pub channels: Vec<Channel>,
    pub users: Vec<User>,
}

fn get_serialize_filepath(subname: &str) -> PathBuf {
    dirs::home_dir()
        .expect("HOME env var not set")
        .join(".suiop")
        .join(subname)
}

/// Serialize the obj into ~/.suiop/{subname} so we can cache it across
/// executions
pub fn serialize_to_file<T: Serialize>(subname: &str, obj: &Vec<T>) -> Result<()> {
    let file = File::create(get_serialize_filepath(subname).as_path())?;
    serde_json::to_writer(file, obj)?;
    Ok(())
}

/// Check if the file in ~/.suiop/{subname} is less than 1 day old
/// and if so, deserialize the value from it.
///
/// Otherwise return None
pub fn deserialize_from_file<T: DeserializeOwned>(subname: &str) -> Option<Vec<T>> {
    let mut result = None;
    let file_path = get_serialize_filepath(subname);
    if let Ok(metadata) = file_path.metadata() {
        if let Ok(modified) = metadata.modified() {
            if let Ok(elapsed) = modified.elapsed() {
                // 1 day
                if elapsed.as_secs() < 24 * 60 * 60 {
                    if let Ok(file) = File::open(file_path) {
                        if let Ok(obj) = serde_json::from_reader::<_, Vec<T>>(file) {
                            debug!("Using cached {}", subname);
                            result = Some(obj);
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
        let channels = deserialize_from_file("channels")
            .map_or_else(
                || {
                    Either::Left(async {
                        let channels = get_channels(&client).await.expect("Failed to get channels");
                        serialize_to_file("channels", &channels)
                            .expect("Failed to serialize channels");
                        channels
                    })
                },
                |v| Either::Right(async { v }),
            )
            .await;
        let users = deserialize_from_file("users")
            .map_or_else(
                || {
                    Either::Left(async {
                        let users = get_users(&client).await.expect("Failed to get users");
                        serialize_to_file("users", &users).expect("Failed to serialize users");
                        users
                    })
                },
                |u| Either::Right(async { u }),
            )
            .await;
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
