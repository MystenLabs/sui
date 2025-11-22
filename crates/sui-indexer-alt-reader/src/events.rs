// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::dataloader::Loader;

use anyhow::{Context, anyhow};
use diesel::{ExpressionMethods, QueryDsl, Queryable, Selectable, SelectableHelper};
use prost_types::FieldMask;
use std::collections::{BTreeSet, HashMap};
use sui_indexer_alt_schema::schema::kv_transactions;
use sui_kvstore::TransactionEventsData;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_rpc::{field::FieldMaskUtil, proto::proto_to_timestamp_ms};
use sui_types::{digests::TransactionDigest, effects::TransactionEvents};

use crate::ledger_grpc_reader::LedgerGrpcReader;
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

        let mut conn = self.connect().await?;

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

#[async_trait::async_trait]
impl Loader<TransactionEventsKey> for LedgerGrpcReader {
    type Value = TransactionEventsData;
    type Error = Error;

    async fn load(
        &self,
        keys: &[TransactionEventsKey],
    ) -> Result<HashMap<TransactionEventsKey, Self::Value>, Self::Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut results = HashMap::new();
        for key in keys {
            let request = proto::GetTransactionRequest::new(&key.0.into())
                .with_read_mask(FieldMask::from_paths(["events.bcs", "timestamp"]));

            match self.0.clone().get_transaction(request).await {
                Ok(response) => {
                    let executed = response
                        .into_inner()
                        .transaction
                        .context("No transaction returned")?;

                    let events = executed
                        .events
                        .as_ref()
                        .and_then(|e| e.bcs.as_ref())
                        .map(|bcs| -> anyhow::Result<_> {
                            let tx_events: TransactionEvents = bcs
                                .deserialize()
                                .context("Failed to deserialize transaction events")?;
                            Ok(tx_events.data)
                        })
                        .transpose()?
                        .unwrap_or_default();

                    let timestamp_ms = executed
                        .timestamp
                        .map(proto_to_timestamp_ms)
                        .transpose()
                        .map_err(|e| anyhow!("Failed to parse timestamp: {}", e))?
                        .unwrap_or(0);

                    results.insert(
                        *key,
                        TransactionEventsData {
                            events,
                            timestamp_ms,
                        },
                    );
                }
                Err(status) if status.code() == tonic::Code::NotFound => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(results)
    }
}
