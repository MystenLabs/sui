// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data::{Db, DbConnection, QueryExecutor},
    error::Error,
};
use async_graphql::*;
use diesel::{ExpressionMethods, QueryDsl};
use sui_indexer::schema::checkpoints;
use sui_types::{
    digests::ChainIdentifier as NativeChainIdentifier, messages_checkpoint::CheckpointDigest,
};

pub(crate) struct ChainIdentifier;

impl ChainIdentifier {
    /// Query the Chain Identifier from the DB.
    pub(crate) async fn query(db: &Db) -> Result<NativeChainIdentifier, Error> {
        use checkpoints::dsl;

        let digest_bytes = db
            .execute(move |conn| {
                conn.first(move || {
                    dsl::checkpoints
                        .select(dsl::checkpoint_digest)
                        .order_by(dsl::sequence_number.asc())
                })
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch genesis digest: {e}")))?;

        Self::from_bytes(digest_bytes)
    }

    /// Treat `bytes` as a checkpoint digest and extract a chain identifier from it.
    pub(crate) fn from_bytes(bytes: Vec<u8>) -> Result<NativeChainIdentifier, Error> {
        let genesis_digest = CheckpointDigest::try_from(bytes)
            .map_err(|e| Error::Internal(format!("Failed to deserialize genesis digest: {e}")))?;
        Ok(NativeChainIdentifier::from(genesis_digest))
    }
}
