// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use anyhow::Context;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_types::digests::TransactionDigest;

use crate::error::Error;
use crate::ledger_grpc_reader::ChunkedLoader;
use crate::ledger_grpc_reader::LedgerGrpcReader;

/// Key for fetching transaction events contents (Events, TimestampMs) by digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionEventsKey(pub TransactionDigest);

#[async_trait::async_trait]
impl ChunkedLoader<TransactionEventsKey> for LedgerGrpcReader {
    type Value = proto::ExecutedTransaction;
    type Error = Error;

    fn chunk_size(&self) -> usize {
        self.max_batch_get_transactions()
    }

    async fn load_chunk(
        &self,
        keys: &[TransactionEventsKey],
    ) -> Result<HashMap<TransactionEventsKey, proto::ExecutedTransaction>, Error> {
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

                Ok((TransactionEventsKey(digest), executed))
            })
            .collect::<anyhow::Result<_>>()
            .map_err(Error::from)
    }
}

#[cfg(test)]
mod tests {
    use async_graphql::dataloader::Loader;

    use super::*;
    use crate::ledger_grpc_reader::test_support::assert_chunked;
    use crate::ledger_grpc_reader::test_support::mock_reader;

    #[tokio::test]
    async fn load_chunks_oversized_batches() {
        let (reader, mock, server) = mock_reader().await;
        let limit = reader.max_batch_get_transactions();

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
