// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::dataloader::Loader;
use diesel::{ExpressionMethods, QueryDsl, Queryable, Selectable, SelectableHelper};
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};
use sui_indexer_alt_schema::schema::kv_transactions;
use sui_kvstore::TransactionEventsData;
use sui_types::digests::TransactionDigest;

use crate::{bigtable_reader::BigtableReader, error::Error, pg_reader::PgReader};

/// Key for fetching transaction events contents (Events, TimestampMs) by digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionEventsKey(pub TransactionDigest);

/// Partial transaction and events for when you only need transaction content for events
#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = kv_transactions)]
pub struct StoredTransactionEvents {
    pub events: Vec<u8>,
    pub timestamp_ms: i64,
}

#[async_trait::async_trait]
impl Loader<TransactionEventsKey> for PgReader {
    type Value = StoredTransactionEvents;
    type Error = Error;

    async fn load(
        &self,
        keys: &[TransactionEventsKey],
    ) -> Result<HashMap<TransactionEventsKey, Self::Value>, Error> {
        use kv_transactions::dsl as t;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await.map_err(Arc::new)?;

        let digests: BTreeSet<_> = keys.iter().map(|d| d.0.into_inner()).collect();
        let transactions: Vec<(Vec<u8>, StoredTransactionEvents)> = conn
            .results(
                t::kv_transactions
                    .select((t::tx_digest, StoredTransactionEvents::as_select()))
                    .filter(t::tx_digest.eq_any(digests)),
            )
            .await?;
        let digest_to_stored: HashMap<_, _> = transactions
            .into_iter()
            .map(|(tx_digest, stored)| (tx_digest.clone(), stored))
            .collect();

        Ok(keys
            .iter()
            .filter_map(|key| {
                let slice: &[u8] = key.0.as_ref();
                Some((*key, digest_to_stored.get(slice).cloned()?))
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl Loader<TransactionEventsKey> for BigtableReader {
    type Value = TransactionEventsData;
    type Error = Error;

    async fn load(
        &self,
        keys: &[TransactionEventsKey],
    ) -> Result<HashMap<TransactionEventsKey, Self::Value>, Self::Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let digests: Vec<_> = keys.iter().map(|k| k.0).collect();
        Ok(self
            .transactions_events(&digests)
            .await?
            .into_iter()
            .map(|(digest, events)| (TransactionEventsKey(digest), events))
            .collect())
    }
}
