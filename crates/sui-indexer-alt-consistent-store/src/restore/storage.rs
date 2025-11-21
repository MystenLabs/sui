// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::Context as _;
use bytes::Bytes;
use object_store::{ClientOptions, ObjectStore, path::Path};
use url::Url;

/// Interface implemented by storage backends that store byte blobs at paths.
#[async_trait::async_trait]
pub(super) trait Storage {
    /// Fetch the object at `path`.
    async fn get(&self, path: Path) -> anyhow::Result<Bytes>;
}

#[derive(clap::Args, Clone, Debug, Default)]
pub struct StorageConnectionArgs {
    /// How long to wait for a snapshot file to be downloaded (default to no timeout).
    #[arg(long)]
    snapshot_timeout_ms: Option<u64>,

    /// How long to wait while establishing a connection to the snapshot store (defaults to no
    /// timeout).
    #[arg(long)]
    snapshot_connection_timeout_ms: Option<u64>,
}

/// A generic client that fetches objects over HTTP.
pub(super) struct HttpStorage {
    endpoint: Url,
    client: reqwest::Client,
}

impl HttpStorage {
    pub fn new(endpoint: Url, args: StorageConnectionArgs) -> anyhow::Result<Self> {
        let mut builder = reqwest::ClientBuilder::new().https_only(false);

        if let Some(timeout) = args.snapshot_timeout_ms {
            builder = builder.timeout(Duration::from_millis(timeout));
        }

        if let Some(timeout) = args.snapshot_connection_timeout_ms {
            builder = builder.connect_timeout(Duration::from_millis(timeout));
        }

        Ok(Self {
            endpoint,
            client: builder.build()?,
        })
    }
}

#[async_trait::async_trait]
impl Storage for HttpStorage {
    async fn get(&self, path: Path) -> anyhow::Result<Bytes> {
        let url = self
            .endpoint
            .join(path.as_ref())
            .with_context(|| format!("Bad Path: {path}"))?;

        self.client
            .get(url)
            .send()
            .await
            .with_context(|| format!("Failed to fetch: {path}"))?
            .error_for_status()
            .with_context(|| format!("Failed to fetch: {path}"))?
            .bytes()
            .await
            .with_context(|| format!("Failed to read bytes from: {path}"))
    }
}

#[async_trait::async_trait]
impl<S: ObjectStore> Storage for S {
    async fn get(&self, path: Path) -> anyhow::Result<Bytes> {
        self.get(&path)
            .await
            .with_context(|| format!("Failed to fetch: {path}"))?
            .bytes()
            .await
            .with_context(|| format!("Failed to read bytes from: {path}"))
    }
}

impl From<StorageConnectionArgs> for ClientOptions {
    fn from(args: StorageConnectionArgs) -> ClientOptions {
        let mut opts = ClientOptions::new();
        opts = if let Some(timeout) = args.snapshot_timeout_ms {
            opts.with_timeout(Duration::from_millis(timeout))
        } else {
            opts.with_timeout_disabled()
        };

        opts = if let Some(timeout) = args.snapshot_connection_timeout_ms {
            opts.with_connect_timeout(Duration::from_millis(timeout))
        } else {
            opts.with_connect_timeout_disabled()
        };

        opts
    }
}
