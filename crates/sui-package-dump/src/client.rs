// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Context, Result};
use cynic::{http::ReqwestExt, Operation, QueryBuilder};
use reqwest::IntoUrl;
use serde::{de::DeserializeOwned, Serialize};

pub(crate) struct Client {
    inner: reqwest::Client,
    url: reqwest::Url,
}

impl Client {
    /// Create a new GraphQL client, talking to a Sui GraphQL service at `url`.
    pub(crate) fn new(url: impl IntoUrl) -> Result<Self> {
        Ok(Self {
            inner: reqwest::Client::builder()
                .user_agent(concat!("sui-package-dump/", env!("CARGO_PKG_VERSION")))
                .build()
                .context("Failed to create GraphQL client")?,
            url: url.into_url().context("Invalid RPC URL")?,
        })
    }

    pub(crate) async fn query<Q, V>(&self, query: Operation<Q, V>) -> Result<Q>
    where
        V: Serialize,
        Q: DeserializeOwned + QueryBuilder<V> + 'static,
    {
        self.inner
            .post(self.url.clone())
            .run_graphql(query)
            .await
            .context("Failed to send GraphQL query")?
            .data
            .ok_or_else(|| anyhow!("Empty response to query"))
    }
}
