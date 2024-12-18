// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data::{Db, DbConnection, QueryExecutor},
    error::Error,
};
use async_graphql::*;
use diesel::{OptionalExtension, QueryDsl};
use diesel_async::scoped_futures::ScopedFutureExt;
use sui_indexer::schema::chain_identifier;
use sui_types::{
    digests::ChainIdentifier as NativeChainIdentifier, messages_checkpoint::CheckpointDigest,
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ChainIdentifier(pub(crate) Option<NativeChainIdentifier>);

impl ChainIdentifier {
    /// Query the Chain Identifier from the DB.
    pub(crate) async fn query(db: &Db) -> Result<Option<NativeChainIdentifier>, Error> {
        use chain_identifier::dsl;

        let Some(digest_bytes) = db
            .execute(move |conn| {
                async {
                    conn.first(move || dsl::chain_identifier.select(dsl::checkpoint_digest))
                        .await
                        .optional()
                }
                .scope_boxed()
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch genesis digest: {e}")))?
        else {
            return Ok(None);
        };

        let native_identifier = Self::from_bytes(digest_bytes)?;

        Ok(Some(native_identifier))
    }

    /// Treat `bytes` as a checkpoint digest and extract a chain identifier from it.
    pub(crate) fn from_bytes(bytes: Vec<u8>) -> Result<NativeChainIdentifier, Error> {
        let genesis_digest = CheckpointDigest::try_from(bytes)
            .map_err(|e| Error::Internal(format!("Failed to deserialize genesis digest: {e}")))?;
        Ok(NativeChainIdentifier::from(genesis_digest))
    }
}

impl From<Option<NativeChainIdentifier>> for ChainIdentifier {
    fn from(chain_identifier: Option<NativeChainIdentifier>) -> Self {
        Self(chain_identifier)
    }
}

impl From<NativeChainIdentifier> for ChainIdentifier {
    fn from(chain_identifier: NativeChainIdentifier) -> Self {
        Self(Some(chain_identifier))
    }
}
