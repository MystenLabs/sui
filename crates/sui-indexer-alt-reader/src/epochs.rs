// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};

use async_graphql::dataloader::Loader;
use diesel::{
    sql_types::{Array, BigInt},
    ExpressionMethods, QueryDsl,
};
use sui_indexer_alt_schema::{
    epochs::{StoredEpochEnd, StoredEpochStart},
    schema::{kv_epoch_ends, kv_epoch_starts},
};

use crate::{error::Error, pg_reader::PgReader};

/// Key for fetching information about the start of an epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EpochStartKey(pub u64);

/// Key for fetching information about the latest epoch to have started as of a given checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CheckpointBoundedEpochStartKey(pub u64);

/// Key for fetching information about the end of an epoch (which must already be finished).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EpochEndKey(pub u64);

#[async_trait::async_trait]
impl Loader<EpochStartKey> for PgReader {
    type Value = StoredEpochStart;
    type Error = Error;

    async fn load(
        &self,
        keys: &[EpochStartKey],
    ) -> Result<HashMap<EpochStartKey, Self::Value>, Error> {
        use kv_epoch_starts::dsl as s;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await?;

        let ids: Vec<_> = keys.iter().map(|e| e.0 as i64).collect();
        let epochs: Vec<StoredEpochStart> = conn
            .results(s::kv_epoch_starts.filter(s::epoch.eq_any(ids)))
            .await?;

        Ok(epochs
            .into_iter()
            .map(|e| (EpochStartKey(e.epoch as u64), e))
            .collect())
    }
}

#[async_trait::async_trait]
impl Loader<CheckpointBoundedEpochStartKey> for PgReader {
    type Value = StoredEpochStart;
    type Error = Error;

    async fn load(
        &self,
        keys: &[CheckpointBoundedEpochStartKey],
    ) -> Result<HashMap<CheckpointBoundedEpochStartKey, Self::Value>, Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await?;

        let cps: Vec<_> = keys.iter().map(|e| e.0 as i64).collect();
        let query = diesel::sql_query(
            r#"
                SELECT
                    v.*
                FROM (
                    SELECT UNNEST($1) cp_sequence_number
                ) k
                CROSS JOIN LATERAL (
                    SELECT
                        epoch,
                        protocol_version,
                        cp_lo,
                        start_timestamp_ms,
                        reference_gas_price,
                        system_state
                    FROM
                        kv_epoch_starts
                    WHERE
                        kv_epoch_starts.cp_lo <= k.cp_sequence_number
                    ORDER BY
                        kv_epoch_starts.cp_lo DESC
                    LIMIT
                        1
                ) v
            "#,
        )
        .bind::<Array<BigInt>, _>(cps);

        let stored_epochs: Vec<StoredEpochStart> = conn.results(query).await?;

        // A single data loader request may contain multiple keys for the same epoch. Store them in
        // an ordered map, so that we can find the latest version for each key.
        let cp_to_stored: BTreeMap<_, _> = stored_epochs
            .into_iter()
            .map(|epoch| (epoch.cp_lo as u64, epoch))
            .collect();

        Ok(keys
            .iter()
            .filter_map(|key| {
                let stored = cp_to_stored.range(..=key.0).last()?.1;
                Some((*key, stored.clone()))
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl Loader<EpochEndKey> for PgReader {
    type Value = StoredEpochEnd;
    type Error = Error;

    async fn load(&self, keys: &[EpochEndKey]) -> Result<HashMap<EpochEndKey, Self::Value>, Error> {
        use kv_epoch_ends::dsl as e;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await?;

        let ids: Vec<_> = keys.iter().map(|e| e.0 as i64).collect();
        let epochs: Vec<StoredEpochEnd> = conn
            .results(e::kv_epoch_ends.filter(e::epoch.eq_any(ids)))
            .await?;

        Ok(epochs
            .into_iter()
            .map(|e| (EpochEndKey(e.epoch as u64), e))
            .collect())
    }
}
