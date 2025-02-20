// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use async_graphql::dataloader::Loader;
use diesel::{ExpressionMethods, JoinOnDsl, QueryDsl, SelectableHelper};
use sui_indexer_alt_schema::{
    schema::{tx_balance_changes, tx_digests},
    transactions::StoredTxBalanceChange,
};
use sui_types::digests::TransactionDigest;

use super::reader::{ReadError, Reader};

/// Key for fetching a transaction's balance changes by digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TxBalanceChangeKey(pub TransactionDigest);

#[async_trait::async_trait]
impl Loader<TxBalanceChangeKey> for Reader {
    type Value = StoredTxBalanceChange;
    type Error = Arc<ReadError>;

    async fn load(
        &self,
        keys: &[TxBalanceChangeKey],
    ) -> Result<HashMap<TxBalanceChangeKey, Self::Value>, Self::Error> {
        use tx_balance_changes::dsl as b;
        use tx_digests::dsl as t;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await.map_err(Arc::new)?;

        let digests: BTreeSet<_> = keys.iter().map(|d| d.0.into_inner()).collect();
        let balance_changes: Vec<(Vec<u8>, StoredTxBalanceChange)> = conn
            .results(
                b::tx_balance_changes
                    .inner_join(t::tx_digests.on(b::tx_sequence_number.eq(t::tx_sequence_number)))
                    .select((t::tx_digest, StoredTxBalanceChange::as_select()))
                    .filter(t::tx_digest.eq_any(digests)),
            )
            .await
            .map_err(Arc::new)?;

        let digest_to_balance_changes: HashMap<_, _> = balance_changes.into_iter().collect();

        Ok(keys
            .iter()
            .filter_map(|key| {
                let slice: &[u8] = key.0.as_ref();
                Some((*key, digest_to_balance_changes.get(slice).cloned()?))
            })
            .collect())
    }
}
