// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeSet, HashMap};

use async_graphql::dataloader::Loader;
use diesel::{ExpressionMethods, QueryDsl};
use sui_indexer_alt_schema::{checkpoints::StoredCheckpoint, schema::kv_checkpoints};
use sui_types::{
    crypto::AuthorityQuorumSignInfo,
    messages_checkpoint::{CheckpointContents, CheckpointSummary},
};

use crate::{bigtable_reader::BigtableReader, error::Error, pg_reader::PgReader};

/// Key for fetching a checkpoint's content by its sequence number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CheckpointKey(pub u64);

#[async_trait::async_trait]
impl Loader<CheckpointKey> for PgReader {
    type Value = StoredCheckpoint;
    type Error = Error;

    async fn load(
        &self,
        keys: &[CheckpointKey],
    ) -> Result<HashMap<CheckpointKey, Self::Value>, Error> {
        use kv_checkpoints::dsl as c;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await?;

        let seqs: BTreeSet<_> = keys.iter().map(|d| d.0 as i64).collect();
        let checkpoints: Vec<StoredCheckpoint> = conn
            .results(c::kv_checkpoints.filter(c::sequence_number.eq_any(seqs)))
            .await?;

        Ok(checkpoints
            .into_iter()
            .map(|c| (CheckpointKey(c.sequence_number as u64), c))
            .collect())
    }
}

#[async_trait::async_trait]
impl Loader<CheckpointKey> for BigtableReader {
    type Value = (
        CheckpointSummary,
        CheckpointContents,
        AuthorityQuorumSignInfo<true>,
    );
    type Error = Error;

    async fn load(
        &self,
        keys: &[CheckpointKey],
    ) -> Result<HashMap<CheckpointKey, Self::Value>, Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let checkpoint_keys: Vec<_> = keys.iter().map(|k| k.0).collect();

        Ok(self
            .checkpoints(&checkpoint_keys)
            .await?
            .into_iter()
            .map(|c| {
                (
                    CheckpointKey(c.summary.sequence_number),
                    (c.summary, c.contents, c.signatures),
                )
            })
            .collect())
    }
}
