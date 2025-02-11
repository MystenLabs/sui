// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module implements the request loader, which is used to load and deserialize
/// the JSON RPC requests from a jsonl file.
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs::File,
    io::{BufRead, BufReader},
};

mod timestamp {
    use chrono::{DateTime, Utc};
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        DateTime::parse_from_rfc3339(&s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct JsonRpcRequestLine {
    pub method: String,
    #[serde(rename = "body")]
    pub body_json: Value,
    #[serde(with = "timestamp")]
    pub timestamp: DateTime<Utc>,
}

pub fn load_json_rpc_requests(file_path: &str) -> Result<Vec<JsonRpcRequestLine>> {
    let file = File::open(file_path)
        .with_context(|| format!("Could not open JSON RPC file at {}", file_path))?;
    let reader = BufReader::new(file);

    let mut requests = Vec::new();
    // the jsonl file is sorted by timestamp already, so no need to sort the lines
    for line in reader.lines() {
        let line = line?;
        let request_line: JsonRpcRequestLine =
            serde_json::from_str(&line).with_context(|| "Failed to parse JSON RPC line")?;
        requests.push(request_line);
    }

    Ok(requests)
}
