// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use reqwest;
use tracing::debug;

const ENDPOINT: &str = "/auth/validate_access_token";

pub async fn get_identity(base_url: &str, token: &str) -> Result<String> {
    let full_url = format!("{}{}", base_url, ENDPOINT);
    debug!("full_url: {}", full_url);
    let client = reqwest::Client::new();

    let req = client
        .get(full_url)
        .header("Authorization", format!("Bearer {}", token));
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
