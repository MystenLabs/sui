// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO: Remove these when we have examples driver which uses this client
#![allow(dead_code)]
#![allow(unused_imports)]

use axum::http::HeaderValue;
use hyper::header;
use reqwest::RequestBuilder;

use crate::{
    config::{ConnectionConfig, ServiceConfig},
    extensions::query_limits_checker::LIMITS_HEADER,
    server::simple_server::start_example_server,
    utils::reset_db,
};

use super::response::GraphqlResponse;
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

    pub async fn execute_to_graphql(
        &self,
        query: String,
        get_usage: bool,
        mut headers: Vec<(header::HeaderName, header::HeaderValue)>,
    ) -> Result<GraphqlResponse, reqwest::Error> {
        if get_usage {
            headers.push((LIMITS_HEADER.clone(), HeaderValue::from_static("true")));
        }
        let body = serde_json::json!({
            "query": query,
        });

        let mut builder = self.inner.post(&self.url).json(&body);
        for (key, value) in headers {
            builder = builder.header(key, value);
        }
        Ok(GraphqlResponse::from_resp(builder.send().await?).await)
    }
}
