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

#[cfg(feature = "pg_integration")]
#[tokio::test]
async fn test_simple_client() {
    let mut connection_config = ConnectionConfig::ci_integration_test_cfg();

    let cluster = crate::cluster::start_cluster(connection_config).await;

    // Wait for servers to start and catchup
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let query = r#"
        query {
            chainIdentifier
        }
    "#;
    let res = cluster
        .graphql_client
        .execute(query.to_string(), vec![])
        .await
        .unwrap();
    let chain_id_actual = cluster
        .validator_fullnode_handle
        .fullnode_handle
        .sui_client
        .read_api()
        .get_chain_identifier()
        .await
        .unwrap();

    let exp = format!(
        "{{\"data\":{{\"chainIdentifier\":\"{}\"}}}}",
        chain_id_actual
    );
    assert_eq!(&format!("{}", res), &exp);
}
