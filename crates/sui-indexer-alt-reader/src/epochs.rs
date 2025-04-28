// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use async_graphql::dataloader::Loader;
use diesel::{ExpressionMethods, QueryDsl};
use sui_indexer_alt_schema::{
    epochs::{StoredEpochEnd, StoredEpochStart},
    schema::{kv_epoch_ends, kv_epoch_starts},
};

use crate::{error::Error, pg_reader::PgReader};

/// Key for fetching information about the start of an epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EpochStartKey(pub u64);

/// Key for fetching information about the end of an epoch (which must already be finished).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EpochEndKey(pub u64);

#[async_trait::async_trait]
impl Loader<EpochStartKey> for PgReader {
    type Value = StoredEpochStart;
    type Error = Arc<Error>;

    async fn load(
        &self,
        keys: &[EpochStartKey],
    ) -> Result<HashMap<EpochStartKey, Self::Value>, Self::Error> {
        use kv_epoch_starts::dsl as s;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await.map_err(Arc::new)?;

        let ids: Vec<_> = keys.iter().map(|e| e.0 as i64).collect();
        let epochs: Vec<StoredEpochStart> = conn
            .results(s::kv_epoch_starts.filter(s::epoch.eq_any(ids)))
            .await
            .map_err(Arc::new)?;

        Ok(epochs
            .into_iter()
            .map(|e| (EpochStartKey(e.epoch as u64), e))
            .collect())
    }
}

#[async_trait::async_trait]
impl Loader<EpochEndKey> for PgReader {
    type Value = StoredEpochEnd;
    type Error = Arc<Error>;

    async fn load(
        &self,
        keys: &[EpochEndKey],
    ) -> Result<HashMap<EpochEndKey, Self::Value>, Self::Error> {
        use kv_epoch_ends::dsl as e;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await.map_err(Arc::new)?;

        let ids: Vec<_> = keys.iter().map(|e| e.0 as i64).collect();
        let epochs: Vec<StoredEpochEnd> = conn
            .results(e::kv_epoch_ends.filter(e::epoch.eq_any(ids)))
            .await
            .map_err(Arc::new)?;

        Ok(epochs
            .into_iter()
            .map(|e| (EpochEndKey(e.epoch as u64), e))
            .collect())
    }
}
