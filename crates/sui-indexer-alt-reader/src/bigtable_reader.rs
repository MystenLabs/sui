// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;
use std::future::Future;
use std::time::Duration;

use anyhow::anyhow;
use async_graphql::dataloader::DataLoader;
use prometheus::Registry;
use sui_kvstore::{BigTableClient, Checkpoint, KeyValueStoreReader, TransactionData};
use sui_types::digests::TransactionDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use tracing::warn;

use crate::error::Error;

#[derive(clap::Args, Debug, Clone, Default)]
pub struct BigtableArgs {
    /// Time spent waiting for a request to Bigtable to complete, in milliseconds.
    #[arg(long)]
    pub bigtable_statement_timeout_ms: Option<u64>,
}

/// A reader backed by BigTable KV store.
///
/// In order to use this reader, the environment variable `GOOGLE_APPLICATION_CREDENTIALS` must be
/// set to the path of the credentials file.
#[derive(Clone)]
pub struct BigtableReader(BigTableClient);

impl BigtableArgs {
    pub fn statement_timeout(&self) -> Option<Duration> {
        self.bigtable_statement_timeout_ms
            .map(Duration::from_millis)
    }
}

impl BigtableReader {
    /// Create a new reader, talking to the Bigtable instance with ID `instance_id`. The
    /// constructor assumes that the `GOOGLE_APPLICATION_CREDENTIALS` environment variable is set
    /// and points to a valid JSON credentials file.
    ///
    /// `client_name` is used as a label for metrics coming from ths underlying Bigtable client,
    /// which will be registered with the supplied prometheus `registry`.
    pub async fn new(
        instance_id: String,
        client_name: String,
        bigtable_args: BigtableArgs,
        registry: &Registry,
    ) -> Result<Self, Error> {
        if std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_err() {
            return Err(Error::BigtableCreate(anyhow!(
                "Environment variable GOOGLE_APPLICATION_CREDENTIALS is not set"
            )));
        }

        Ok(Self(
            BigTableClient::new_remote(
                instance_id,
                true,
                bigtable_args.statement_timeout(),
                client_name,
                Some(registry),
            )
            .await
            .map_err(Error::BigtableCreate)?,
        ))
    }

    /// Create a data loader backed by this reader.
    pub fn as_data_loader(&self) -> DataLoader<Self> {
        DataLoader::new(self.clone(), tokio::spawn)
    }

    /// Multi-get checkpoints by sequence number.
    pub(crate) async fn checkpoints(
        &self,
        keys: &[CheckpointSequenceNumber],
    ) -> Result<Vec<Checkpoint>, Error> {
        measure("checkpoints", &keys, self.0.clone().get_checkpoints(keys)).await
    }

    /// Multi-get transactions by transaction digest.
    pub(crate) async fn transactions(
        &self,
        keys: &[TransactionDigest],
    ) -> Result<Vec<TransactionData>, Error> {
        measure("transactions", &keys, self.0.clone().get_transactions(keys)).await
    }

    /// Multi-get objects by object ID and version.
    pub(crate) async fn objects(&self, keys: &[ObjectKey]) -> Result<Vec<Object>, Error> {
        measure("objects", &keys, self.0.clone().get_objects(keys)).await
    }
}

/// Run the `load` future, detecting a timeout, and logging a warning with the details of the
/// request if that is the case.
async fn measure<T, A: Debug>(
    method: &str,
    args: &A,
    load: impl Future<Output = anyhow::Result<T>>,
) -> Result<T, Error> {
    let result = load.await;

    if result.as_ref().is_err_and(is_timeout) {
        warn!(method, ?args, "Bigtable timeout");
    }

    result.map_err(Error::BigtableRead)
}

/// Detect a tonic timeout error in the source chain.
fn is_timeout(err: &anyhow::Error) -> bool {
    let mut source = err.source();

    while let Some(err) = source {
        if err.downcast_ref::<tonic::TimeoutExpired>().is_some() {
            return true;
        } else {
            source = err.source();
        }
    }

    false
}
