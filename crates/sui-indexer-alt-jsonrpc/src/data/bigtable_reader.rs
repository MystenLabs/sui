// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;
use std::future::Future;
use std::time::{Duration, Instant};

use async_graphql::dataloader::DataLoader;
use prometheus::Registry;
use sui_kvstore::BigTableClient;
use tracing::warn;

use crate::data::error::Error;

/// A reader backed by Bigtable kv store.
/// In order to use this reader, the environment variable `GOOGLE_APPLICATION_CREDENTIALS`
/// must be set to the path of the credentials file.
#[derive(Clone)]
pub struct BigtableReader(pub(crate) BigTableClient, pub(crate) Duration);

impl BigtableReader {
    pub(crate) async fn new(
        instance_id: String,
        registry: &Registry,
        threshold: Duration,
    ) -> Result<Self, Error> {
        if std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_err() {
            return Err(Error::BigtableCreate(anyhow::anyhow!(
                "Environment variable GOOGLE_APPLICATION_CREDENTIALS is not set"
            )));
        }
        let client = BigTableClient::new_remote(
            instance_id,
            true,
            None,
            "indexer-alt-jsonrpc".to_string(),
            Some(registry),
        )
        .await
        .map_err(Error::BigtableCreate)?;
        Ok(Self(client, threshold))
    }

    /// Create a data loader backed by this reader.
    pub(crate) fn as_data_loader(&self) -> DataLoader<Self> {
        DataLoader::new(self.clone(), tokio::spawn)
    }

    pub(crate) async fn timed_load<F, T, E, A: Debug>(
        &self,
        method_name: &str,
        args: &A,
        load: F,
    ) -> Result<T, E>
    where
        F: Future<Output = Result<T, E>>,
    {
        let start = Instant::now();
        let result = load.await;
        let elapsed = start.elapsed();

        if elapsed > self.1 {
            warn!(
                "BigTableClient load '{}' with args '{:?}' took {:?}, which exceeds the threshold of {:?}",
                method_name, args, elapsed, self.1
            );
        }

        result
    }
}
