// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use async_graphql::dataloader::Loader;
use diesel::{ExpressionMethods, QueryDsl};
use sui_indexer_alt_schema::{schema::tx_digests, transactions::StoredTxDigest};

use super::reader::{ReadError, Reader};

/// Key for fetching a transaction's digest by its sequence number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TxDigestKey(pub u64);

#[async_trait::async_trait]
impl Loader<TxDigestKey> for Reader {
    type Value = StoredTxDigest;
    type Error = Arc<ReadError>;

    async fn load(
        &self,
        keys: &[TxDigestKey],
    ) -> Result<HashMap<TxDigestKey, Self::Value>, Self::Error> {
        use tx_digests::dsl as d;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await.map_err(Arc::new)?;

        let seqs: BTreeSet<_> = keys.iter().map(|d| d.0 as i64).collect();
        let stored: Vec<StoredTxDigest> = conn
            .results(d::tx_digests.filter(d::tx_sequence_number.eq_any(seqs)))
            .await
            .map_err(Arc::new)?;

        Ok(stored
            .into_iter()
            .map(|stored| {
                let key = TxDigestKey(stored.tx_sequence_number as u64);
                (key, stored)
            })
            .collect())
    }
}
