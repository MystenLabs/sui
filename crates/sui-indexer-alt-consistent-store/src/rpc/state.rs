// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::CHECKPOINT_HEIGHT_METADATA;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::LEGACY_CHECKPOINT_METADATA;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::LOWEST_AVAILABLE_CHECKPOINT_METADATA;
use tonic::metadata::AsciiMetadataValue;

use crate::config::ConsistencyConfig;
use crate::config::RpcConfig;
use crate::rpc::error::RpcError;
use crate::rpc::error::StatusCode;
use crate::schema::Schema;
use crate::store::Store;

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
        let snapshot_range = self
            .store
            .db()
            .snapshot_range(u64::MAX)
            .ok_or(Error::NoSnapshots)?;

        let metadata = request.metadata();
        let Some(checkpoint) = metadata
            .get(CHECKPOINT_HEIGHT_METADATA)
            .or_else(|| metadata.get(LEGACY_CHECKPOINT_METADATA))
        else {
            // If a checkpoint hasn't been supplied default to the latest snapshot.
            let watermark = snapshot_range.end();
            return Ok(watermark
                .checkpoint_hi
                .checked_sub(1)
                .unwrap_or_else(|| panic!("Range end checkpoint_hi underflow {watermark:?}")));
        };

        let checkpoint = checkpoint
            .to_str()
            .map_err(|_| Error::BadCheckpoint(checkpoint.clone()))?
            .parse()
            .map_err(|_| Error::BadCheckpoint(checkpoint.clone()))?;

        if checkpoint + 1 < snapshot_range.start().checkpoint_hi
            || checkpoint >= snapshot_range.end().checkpoint_hi
        {
            return Err(RpcError::NotInRange(checkpoint));
        }

        Ok(checkpoint)
    }

    /// Convert a result into a `tonic::Response` and annotate it with checkpoint headers.
    pub(super) fn checkpointed_response<T>(
        &self,
        result: Result<T, tonic::Status>,
    ) -> Result<tonic::Response<T>, tonic::Status> {
        let mut resp = result.map(tonic::Response::new);

        let cp_hi_inclusive = u64::MAX;
        let Some(range) = self.store.db().snapshot_range(cp_hi_inclusive) else {
            return resp;
        };

        let meta = resp
            .as_mut()
            .map_or_else(|s| s.metadata_mut(), |r| r.metadata_mut());

        let start = range
            .start()
            .checkpoint_hi
            .checked_sub(1)
            .unwrap_or_else(|| {
                panic!("Range start checkpoint_hi underflow cp_hi_inclusive={cp_hi_inclusive}")
            });
        if let Ok(min) = start.to_string().parse() {
            meta.insert(LOWEST_AVAILABLE_CHECKPOINT_METADATA, min);
        }

        let end = range.end().checkpoint_hi.checked_sub(1).unwrap_or_else(|| {
            panic!("Range end checkpoint_hi underflow cp_hi_inclusive={cp_hi_inclusive}")
        });
        if let Ok(max) = end.to_string().parse() {
            let max: AsciiMetadataValue = max;
            meta.insert(CHECKPOINT_HEIGHT_METADATA, max.clone());
            meta.insert(LEGACY_CHECKPOINT_METADATA, max);
        }

        resp
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
