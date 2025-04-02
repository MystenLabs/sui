// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;
use std::future::Future;
use std::time::{Duration, Instant};

use anyhow::anyhow;
use async_graphql::dataloader::DataLoader;
use prometheus::Registry;
use sui_kvstore::{BigTableClient, Checkpoint, KeyValueStoreReader, TransactionData};
use sui_types::digests::TransactionDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use tracing::warn;

use crate::data::error::Error;

/// A reader backed by BigTable KV store.
///
/// In order to use this reader, the environment variable `GOOGLE_APPLICATION_CREDENTIALS` must be
/// set to the path of the credentials file.
#[derive(Clone)]
pub struct BigtableReader {
    client: BigTableClient,

    /// Requests to BigTable that take longer than this threshold will be logged.
    slow_request_threshold: Duration,
}

impl BigtableReader {
    pub(crate) async fn new(
        instance_id: String,
        registry: &Registry,
        slow_request_threshold: Duration,
    ) -> Result<Self, Error> {
        if std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_err() {
            return Err(Error::BigtableCreate(anyhow!(
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

        Ok(Self {
            client,
            slow_request_threshold,
        })
    }

    /// Create a data loader backed by this reader.
    pub(crate) fn as_data_loader(&self) -> DataLoader<Self> {
        DataLoader::new(self.clone(), tokio::spawn)
    }

    /// Multi-get checkpoints by sequence number.
    pub(crate) async fn checkpoints(
        &self,
        keys: &[CheckpointSequenceNumber],
    ) -> Result<Vec<Checkpoint>, Error> {
        measure(
            self.slow_request_threshold,
            "checkpoints",
            &keys,
            self.client.clone().get_checkpoints(keys),
        )
        .await
    }

    /// Multi-get transactions by transaction digest.
    pub(crate) async fn transactions(
        &self,
        keys: &[TransactionDigest],
    ) -> Result<Vec<TransactionData>, Error> {
        measure(
            self.slow_request_threshold,
            "transactions",
            &keys,
            self.client.clone().get_transactions(keys),
        )
        .await
    }

    /// Multi-get objects by object ID and version.
    pub(crate) async fn objects(&self, keys: &[ObjectKey]) -> Result<Vec<Object>, Error> {
        measure(
            self.slow_request_threshold,
            "objects",
            &keys,
            self.client.clone().get_objects(keys),
        )
        .await
    }
}

/// Run the `load` future, measuring how long it takes. If it takes longer than
/// `slow_request_threshold`, log a warning with the details of the request.
async fn measure<T, A: Debug>(
    slow_request_threshold: Duration,
    method: &str,
    args: &A,
    load: impl Future<Output = anyhow::Result<T>>,
) -> Result<T, Error> {
    let start = Instant::now();
    let result = load.await;
    let elapsed = start.elapsed();

    if elapsed > slow_request_threshold {
        warn!(
            elapsed_ms = elapsed.as_millis(),
            threshold_ms = slow_request_threshold.as_millis(),
            method,
            ?args,
            "Slow Bigtable request"
        );
    }

    result.map_err(Error::BigtableRead)
}
