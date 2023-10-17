// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO: Remove these when we have examples driver which uses this client
#![allow(dead_code)]
#![allow(unused_imports)]

use hyper::header;
use reqwest::RequestBuilder;

use crate::{
    config::{ConnectionConfig, ServiceConfig},
    server::simple_server::start_example_server,
    utils::reset_db,
};

#[derive(Clone)]
pub struct SimpleClient {
    inner: reqwest::Client,
    url: String,
}

impl SimpleClient {
    pub fn new<S: Into<String>>(base_url: S) -> Self {
        Self {
            inner: reqwest::Client::new(),
            url: base_url.into(),
        }
    }

    pub async fn execute(
        &self,
        query: String,
        headers: Vec<(header::HeaderName, header::HeaderValue)>,
    ) -> Result<serde_json::Value, reqwest::Error> {
        let body = serde_json::json!({
            "query": query,
        });

        let mut builder = self.inner.post(&self.url).json(&body);
        for (key, value) in headers {
            builder = builder.header(key, value);
        }
        let res = builder.send().await?;
        res.json().await
    }
}
