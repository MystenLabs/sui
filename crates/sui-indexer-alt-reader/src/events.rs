// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::collections::HashMap;

use anyhow::Context;
use anyhow::anyhow;
use async_graphql::dataloader::Loader;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel::Queryable;
use diesel::Selectable;
use diesel::SelectableHelper;
use prost_types::FieldMask;
use sui_indexer_alt_schema::schema::kv_transactions;
use sui_kvstore::TransactionEventsData;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::proto_to_timestamp_ms;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEvents;

use crate::error::Error;
use crate::ledger_grpc_reader::ChunkedLoader;
use crate::ledger_grpc_reader::LedgerGrpcReader;
use crate::ledger_grpc_reader::MAX_BATCH_GET_TRANSACTIONS;
use crate::pg_reader::PgReader;

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
impl ChunkedLoader<TransactionEventsKey> for LedgerGrpcReader {
    type Value = TransactionEventsData;
    type Error = Error;

    fn chunk_size(&self) -> usize {
        MAX_BATCH_GET_TRANSACTIONS
    }

    async fn load_chunk(
        &self,
        keys: &[TransactionEventsKey],
    ) -> Result<HashMap<TransactionEventsKey, TransactionEventsData>, Error> {
        let digests = keys.iter().map(|key| key.0.to_string()).collect();

        let mut request = proto::BatchGetTransactionsRequest::default();
        request.digests = digests;
        request.read_mask = Some(FieldMask::from_paths(["digest", "events.bcs", "timestamp"]));

        let batch_response = self.batch_get_transactions(request).await?;

        batch_response
            .transactions
            .into_iter()
            .filter_map(|tx_result| match tx_result.result {
                Some(proto::get_transaction_result::Result::Transaction(executed)) => {
                    Some(executed)
                }
                _ => None,
            })
            .map(|executed| {
                let digest: TransactionDigest = executed
                    .digest
                    .as_ref()
                    .context("Missing transaction digest")?
                    .parse()
                    .context("Failed to parse transaction digest")?;

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

                Ok((
                    TransactionEventsKey(digest),
                    TransactionEventsData {
                        events,
                        timestamp_ms,
                    },
                ))
            })
            .collect::<anyhow::Result<_>>()
            .map_err(Error::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger_grpc_reader::test_support::assert_chunked;
    use crate::ledger_grpc_reader::test_support::mock_reader;

    #[tokio::test]
    async fn load_chunks_oversized_batches() {
        let (reader, mock, server) = mock_reader().await;
        let limit = MAX_BATCH_GET_TRANSACTIONS;

        let keys: Vec<TransactionEventsKey> = (0..limit + 50)
            .map(|_| TransactionEventsKey(TransactionDigest::random()))
            .collect();

        let result = reader.load(&keys).await.expect("load should succeed");
        assert!(result.is_empty());

        let expected: Vec<String> = keys.iter().map(|key| key.0.to_string()).collect();
        assert_chunked(mock.transaction_batches(), limit, &expected);

        server.abort();
    }
}
