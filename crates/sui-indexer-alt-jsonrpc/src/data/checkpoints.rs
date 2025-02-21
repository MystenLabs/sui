// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use async_graphql::dataloader::Loader;
use diesel::{ExpressionMethods, QueryDsl};
use sui_indexer_alt_schema::{checkpoints::StoredCheckpoint, schema::kv_checkpoints};

use super::reader::{ReadError, Reader};

/// Key for fetching a checkpoint's content by its sequence number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CheckpointKey(pub u64);

#[async_trait::async_trait]
impl Loader<CheckpointKey> for Reader {
    type Value = StoredCheckpoint;
    type Error = Arc<ReadError>;

    async fn load(
        &self,
        keys: &[CheckpointKey],
    ) -> Result<HashMap<CheckpointKey, Self::Value>, Self::Error> {
        use kv_checkpoints::dsl as c;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await.map_err(Arc::new)?;

        let seqs: BTreeSet<_> = keys.iter().map(|d| d.0 as i64).collect();
        let checkpoints: Vec<StoredCheckpoint> = conn
            .results(c::kv_checkpoints.filter(c::sequence_number.eq_any(seqs)))
            .await
            .map_err(Arc::new)?;

        Ok(checkpoints
            .into_iter()
            .map(|c| (CheckpointKey(c.sequence_number as u64), c))
            .collect())
    }
}
