// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use async_graphql::dataloader::Loader;
use diesel::{ExpressionMethods, QueryDsl};
use sui_indexer_alt_schema::{schema::kv_transactions, transactions::StoredTransaction};
use sui_types::digests::TransactionDigest;

use super::reader::{ReadError, Reader};

/// Key for fetching transaction contents (TransactionData, Effects, and Events) by digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TransactionKey(pub TransactionDigest);

#[async_trait::async_trait]
impl Loader<TransactionKey> for Reader {
    type Value = StoredTransaction;
    type Error = Arc<ReadError>;

    async fn load(
        &self,
        keys: &[TransactionKey],
    ) -> Result<HashMap<TransactionKey, Self::Value>, Self::Error> {
        use kv_transactions::dsl as t;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await.map_err(Arc::new)?;

        let digests: BTreeSet<_> = keys.iter().map(|d| d.0.into_inner()).collect();
        let transactions: Vec<StoredTransaction> = conn
            .results(t::kv_transactions.filter(t::tx_digest.eq_any(digests)))
            .await
            .map_err(Arc::new)?;

        let digest_to_stored: HashMap<_, _> = transactions
            .into_iter()
            .map(|stored| (stored.tx_digest.clone(), stored))
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
