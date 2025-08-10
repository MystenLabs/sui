// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context;
use tonic::metadata::AsciiMetadataValue;

use crate::config::{ConsistencyConfig, RpcConfig};
use crate::schema::Schema;
use crate::store::Store;

use super::error::{RpcError, StatusCode};

pub(super) const CHECKPOINT_METADATA: &str = "x-sui-checkpoint";

/// State exposed to RPC service implementations.
#[derive(Clone)]
pub(crate) struct State {
    /// Access to the database.
    pub store: Store<Schema>,

    /// RPC Configuration
    pub rpc_config: Arc<RpcConfig>,

    /// Configuration for the consistent range.
    pub consistency_config: Arc<ConsistencyConfig>,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Bad checkpoint sequence number: {0:?}")]
    BadCheckpoint(AsciiMetadataValue),

    /// The store has no snapshots.
    #[error("No snapshots available")]
    NoSnapshots,
}

impl State {
    /// Extract the checkpoint to request data from, from the request metadata, or default to the
    /// latest available snapshot, if no checkpoint is provided.
    ///
    /// Fails if a valid checkpoint is not provided (the metadata exists but doesn't contain the
    /// decimal representation of a `u64`), or if a checkpoint is not provided and there are no
    /// snapshots available in the store, meaning a default (latest) checkpoint cannot be
    /// determined.
    pub(super) fn checkpoint<T>(
        &self,
        request: &tonic::Request<T>,
    ) -> Result<u64, RpcError<Error>> {
        let Some(checkpoint) = request.metadata().get(CHECKPOINT_METADATA) else {
            // If a checkpoint hasn't been supplied default to the latest snapshot.
            let range = self.store.db().snapshot_range().ok_or(Error::NoSnapshots)?;
            return Ok(*range.end());
        };

        Ok(checkpoint
            .to_str()
            .map_err(|_| Error::BadCheckpoint(checkpoint.clone()))?
            .parse()
            .map_err(|_| Error::BadCheckpoint(checkpoint.clone()))?)
    }
}

impl StatusCode for Error {
    fn code(&self) -> tonic::Code {
        match self {
            Error::BadCheckpoint(_) => tonic::Code::InvalidArgument,
            Error::NoSnapshots => tonic::Code::Unavailable,
        }
    }
}

/// Convert `content` into a `tonic::Response` annotated with the checkpoint the data came from.
pub(super) fn checkpointed_response<T>(
    checkpoint: u64,
    content: T,
) -> Result<tonic::Response<T>, RpcError<Error>> {
    let checkpoint = checkpoint
        .to_string()
        .parse()
        .with_context(|| format!("Invalid checkpoint for metadata: {checkpoint}"))?;

    let mut resp = tonic::Response::new(content);
    let meta = resp.metadata_mut();
    meta.insert(CHECKPOINT_METADATA, checkpoint);

    Ok(resp)
}
