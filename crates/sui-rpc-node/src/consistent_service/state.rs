// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared per-service state for the [`ConsistentService`]
//! handlers.
//!
//! Holds the [`Db`] handle, the [`PaginationConfig`], and a
//! [`ConsistencyConfig`] (to surface `stride` on
//! `available_range`). Methods cover the cross-handler plumbing:
//!
//! - [`State::checkpoint`] resolves the per-request anchor
//!   checkpoint from the request metadata, defaulting to the
//!   latest available snapshot.
//! - [`State::checkpointed_response`] stamps every response with
//!   `x-sui-checkpoint-height` and
//!   `x-sui-lowest-available-checkpoint` headers so clients can
//!   paginate consistently across requests.
//! - [`State::snapshot`] looks up the [`Snapshot`] for a
//!   resolved checkpoint, returning [`Error::SnapshotMissing`]
//!   if it has been evicted in the meantime.
//!
//! [`ConsistentService`]: sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_server::ConsistentService
//! [`ConsistencyConfig`]: crate::config::ConsistencyConfig
//! [`PaginationConfig`]: crate::config::PaginationConfig

use std::sync::Arc;

use sui_consistent_store::Db;
use sui_consistent_store::Snapshot;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::CHECKPOINT_HEIGHT_METADATA;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::LOWEST_AVAILABLE_CHECKPOINT_METADATA;
use tonic::metadata::AsciiMetadataValue;

use crate::config::PaginationConfig;
use sui_rpc_store::ConsistencyConfig;

/// Shared state plumbed into every [`ConsistentService`] handler.
///
/// `Clone` is cheap (`Db` is `Arc`-backed and the configs sit
/// behind `Arc`s), so we hand out clones rather than `&` to
/// satisfy the tonic generated trait's `Sync` requirements.
///
/// [`ConsistentService`]: sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_server::ConsistentService
#[derive(Clone)]
pub(crate) struct State {
    pub(crate) db: Db,
    pub(crate) pagination: Arc<PaginationConfig>,
    pub(crate) consistency: Arc<ConsistencyConfig>,
}

impl State {
    pub(crate) fn new(
        db: Db,
        pagination: PaginationConfig,
        consistency: ConsistencyConfig,
    ) -> Self {
        Self {
            db,
            pagination: Arc::new(pagination),
            consistency: Arc::new(consistency),
        }
    }
}

/// Errors raised by [`State`] helpers that map to specific
/// `tonic::Code`s before reaching the wire.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("bad checkpoint header: {0:?}")]
    BadCheckpoint(AsciiMetadataValue),

    #[error(
        "requested checkpoint {requested} not in available range \
         [{available_lo}, {available_hi}]"
    )]
    NotInRange {
        requested: u64,
        available_lo: u64,
        available_hi: u64,
    },

    #[error("no snapshots are currently available")]
    NoSnapshots,

    #[error("snapshot for checkpoint {0} was evicted before reads completed")]
    SnapshotMissing(u64),
}

impl From<Error> for tonic::Status {
    fn from(e: Error) -> Self {
        match &e {
            Error::BadCheckpoint(_) => tonic::Status::invalid_argument(e.to_string()),
            Error::NotInRange { .. } => tonic::Status::out_of_range(e.to_string()),
            Error::NoSnapshots => tonic::Status::unavailable(e.to_string()),
            Error::SnapshotMissing(_) => tonic::Status::aborted(e.to_string()),
        }
    }
}

impl State {
    /// Resolve the per-request anchor checkpoint.
    ///
    /// If the request carries an `x-sui-checkpoint-height`
    /// metadata header, that value is parsed and validated
    /// against the current snapshot range. Otherwise, defaults
    /// to the highest available snapshot.
    pub(crate) fn checkpoint<T>(&self, request: &tonic::Request<T>) -> Result<u64, Error> {
        let range = self.db.snapshot_range().ok_or(Error::NoSnapshots)?;

        let Some(value) = request.metadata().get(CHECKPOINT_HEIGHT_METADATA) else {
            return Ok(*range.end());
        };

        let parsed: u64 = value
            .to_str()
            .map_err(|_| Error::BadCheckpoint(value.clone()))?
            .parse()
            .map_err(|_| Error::BadCheckpoint(value.clone()))?;

        if parsed < *range.start() || parsed > *range.end() {
            return Err(Error::NotInRange {
                requested: parsed,
                available_lo: *range.start(),
                available_hi: *range.end(),
            });
        }

        Ok(parsed)
    }

    /// Look up the [`Snapshot`] for `checkpoint`. Returns
    /// [`Error::SnapshotMissing`] if it was evicted between
    /// [`Self::checkpoint`]'s read and now (a rare race that
    /// can happen under aggressive eviction).
    pub(crate) fn snapshot(&self, checkpoint: u64) -> Result<Snapshot, Error> {
        self.db
            .at_snapshot(checkpoint)
            .ok_or(Error::SnapshotMissing(checkpoint))
    }

    /// Wrap `payload` in a [`tonic::Response`] (or pass-through
    /// the [`tonic::Status`]) and stamp the response metadata
    /// with the current snapshot range. Stamping the error path
    /// too lets clients paginate after an `OutOfRange` reply
    /// without re-issuing a fresh `AvailableRange` call.
    pub(crate) fn checkpointed_response<T>(
        &self,
        result: Result<T, tonic::Status>,
    ) -> Result<tonic::Response<T>, tonic::Status> {
        let mut resp = result.map(tonic::Response::new);

        let Some(range) = self.db.snapshot_range() else {
            return resp;
        };

        let meta = resp
            .as_mut()
            .map_or_else(|s| s.metadata_mut(), |r| r.metadata_mut());

        if let Ok(value) = range.start().to_string().parse() {
            meta.insert(LOWEST_AVAILABLE_CHECKPOINT_METADATA, value);
        }
        if let Ok(value) = range.end().to_string().parse() {
            meta.insert(CHECKPOINT_HEIGHT_METADATA, value);
        }

        resp
    }
}
