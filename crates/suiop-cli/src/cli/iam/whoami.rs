// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use anyhow::Result;
use reqwest;
use tracing::debug;

const ENDPOINT: &str = "/auth/validate_access_token";

pub async fn get_identity(base_url: &str, token: &str) -> Result<String> {
    let full_url = format!("{}{}", base_url, ENDPOINT);
    debug!("full_url: {}", full_url);
    let client = reqwest::Client::new();
    let mut body = HashMap::new();
    body.insert("token", token);

    let req = client.post(full_url).json(&body);
    debug!("req: {:?}", req);

    let resp = req.send().await?;
    debug!("resp: {:?}", resp);

    if resp.status().is_success() {
        let username = resp.text().await?;
        Ok(username)
    } else {
        Err(anyhow::anyhow!(resp.text().await?))
    }
}
