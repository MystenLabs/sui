// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{error::Error, types::digest::Digest};
use diesel::{ExpressionMethods, OptionalExtension, PgConnection, QueryDsl, RunQueryDsl};
use std::str::FromStr;
use sui_indexer::{
    indexer_reader::IndexerReader,
    models_v2::{
        checkpoints::StoredCheckpoint, epoch::StoredEpochInfo, transactions::StoredTransaction,
    },
    schema_v2::{checkpoints, epochs, transactions},
    PgConnectionPoolConfig,
};

pub(crate) struct PgManager {
    pub inner: IndexerReader,
}

impl PgManager {
    pub(crate) fn new<T: Into<String>>(
        db_url: T,
        config: Option<PgConnectionPoolConfig>,
    ) -> Result<Self, Error> {
        // TODO (wlmyng): support config
        let mut config = config.unwrap_or(PgConnectionPoolConfig::default());
        config.set_pool_size(30);
        let inner = IndexerReader::new_with_config(db_url, config)
            .map_err(|e| Error::Internal(e.to_string()))?;

        Ok(Self { inner })
    }

    pub async fn run_query_async<T, E, F>(&self, query: F) -> Result<T, Error>
    where
        F: FnOnce(&mut PgConnection) -> Result<T, E> + Send + 'static,
        E: From<diesel::result::Error> + std::error::Error + Send + 'static,
        T: Send + 'static,
    {
        self.inner
            .run_query_async(query)
            .await
            .map_err(|e| Error::Internal(e.to_string()))
    }

    pub(crate) async fn fetch_tx(&self, digest: &str) -> Result<Option<StoredTransaction>, Error> {
        let digest = Digest::from_str(digest)?.into_vec();

        self.run_query_async(|conn| {
            transactions::dsl::transactions
                .filter(transactions::dsl::transaction_digest.eq(digest))
                .get_result::<StoredTransaction>(conn) // Expect exactly 0 to 1 result
                .optional()
        })
        .await
    }

    pub(crate) async fn fetch_latest_epoch(&self) -> Result<StoredEpochInfo, Error> {
        self.run_query_async(|conn| {
            epochs::dsl::epochs
                .order_by(epochs::dsl::epoch.desc())
                .limit(1)
                .first::<StoredEpochInfo>(conn)
        })
        .await
    }

    pub(crate) async fn fetch_epoch(
        &self,
        epoch_id: u64,
    ) -> Result<Option<StoredEpochInfo>, Error> {
        let epoch_id = i64::try_from(epoch_id)
            .map_err(|_| Error::Internal("Failed to convert epoch id to i64".to_string()))?;
        self.run_query_async(move |conn| {
            epochs::dsl::epochs
                .filter(epochs::dsl::epoch.eq(epoch_id))
                .get_result::<StoredEpochInfo>(conn) // Expect exactly 0 to 1 result
                .optional()
        })
        .await
    }

    pub(crate) async fn fetch_epoch_strict(&self, epoch_id: u64) -> Result<StoredEpochInfo, Error> {
        let result = self.fetch_epoch(epoch_id).await?;
        match result {
            Some(epoch) => Ok(epoch),
            None => Err(Error::Internal(format!("Epoch {} not found", epoch_id))),
        }
    }

    pub(crate) async fn fetch_latest_checkpoint(&self) -> Result<StoredCheckpoint, Error> {
        self.run_query_async(|conn| {
            checkpoints::dsl::checkpoints
                .order_by(checkpoints::dsl::sequence_number.desc())
                .limit(1)
                .first::<StoredCheckpoint>(conn)
        })
        .await
    }

    pub(crate) async fn fetch_checkpoint(
        &self,
        digest: Option<&str>,
        sequence_number: Option<u64>,
    ) -> Result<Option<StoredCheckpoint>, Error> {
        let mut query = checkpoints::dsl::checkpoints.into_boxed();

        match (digest, sequence_number) {
            (Some(digest), None) => {
                let digest = Digest::from_str(digest)?.into_vec();
                query = query.filter(checkpoints::dsl::checkpoint_digest.eq(digest));
            }
            (None, Some(sequence_number)) => {
                query = query.filter(checkpoints::dsl::sequence_number.eq(sequence_number as i64));
            }
            _ => (), // No-op if invalid input
        }

        self.run_query_async(|conn| query.get_result::<StoredCheckpoint>(conn).optional())
            .await
    }
}
