// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module implements the request loader, which is used to load
/// the JSON RPC requests from a jsonl file.
use anyhow::{Context, Result};
use serde::Deserialize;
use std::{
    fs::File,
    io::{BufRead, BufReader},
};

#[derive(Clone, Debug, Deserialize)]
pub struct JsonRpcRequestLine {
    pub method: String,
    #[serde(rename = "body")]
    pub body_json: serde_json::Value,
}

pub fn load_json_rpc_requests(file_path: &str) -> Result<Vec<JsonRpcRequestLine>> {
    let file = File::open(file_path)
        .with_context(|| format!("Could not open JSON RPC file at {}", file_path))?;
    let reader = BufReader::new(file);

    let mut requests = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let request_line: JsonRpcRequestLine =
            serde_json::from_str(&line).with_context(|| "Failed to parse JSON RPC line")?;
        requests.push(request_line);
    }

    Ok(requests)
}
