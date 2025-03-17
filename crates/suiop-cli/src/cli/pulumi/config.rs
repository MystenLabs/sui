// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::run_cmd;
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct PulumiConfig {
    #[serde(flatten)]
    pub config: HashMap<String, Value>,
}

pub fn get_pulumi_config() -> Result<PulumiConfig> {
    let cmd_output = run_cmd(vec!["pulumi", "config", "--json"], None)
        .context("Failed to run pulumi config --json")?
        .stdout;
    let config: PulumiConfig =
        serde_json::from_slice(&cmd_output.clone()).context("Failed to parse pulumi config")?;
    Ok(config)
}
