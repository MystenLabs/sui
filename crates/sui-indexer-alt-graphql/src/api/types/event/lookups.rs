// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use anyhow::Context as _;
use async_graphql::{dataloader::DataLoader, Context};
use sui_indexer_alt_reader::{
    kv_loader::{KvLoader, TransactionEventsContents},
    pg_reader::PgReader,
    tx_digests::TxDigestKey,
};
use sui_indexer_alt_schema::transactions::StoredTxDigest;
use sui_types::{digests::TransactionDigest, event::Event as NativeEvent};

use crate::api::types::event::Event;
use crate::error::RpcError;
use crate::scope::Scope;

/// Helper struct to manage event lookups by transaction sequence numbers
pub(crate) struct EventLookup {
    tx_digests_map: HashMap<TxDigestKey, StoredTxDigest>,
    transaction_events: HashMap<TransactionDigest, TransactionEventsContents>,
}

impl EventLookup {
    /// Create a new EventLookup from transaction sequence numbers
    pub async fn from_sequence_numbers(
        ctx: &Context<'_>,
        tx_sequence_numbers: &Vec<u64>,
    ) -> Result<Self, RpcError> {
        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;
        let kv_loader: &KvLoader = ctx.data()?;

        let tx_digest_keys: Vec<TxDigestKey> = tx_sequence_numbers
            .iter()
            .map(|r| TxDigestKey(*r))
            .collect();

        let tx_digests_map = pg_loader
            .load_many(tx_digest_keys)
            .await
            .context("Failed to load transaction digests")?;

        let transaction_digests: Vec<TransactionDigest> = tx_digests_map
            .values()
            .map(|stored| TransactionDigest::try_from(stored.tx_digest.clone()))
            .collect::<Result<_, _>>()
            .context("Failed to deserialize transaction digests")?;

        let transaction_events = kv_loader
            .load_many_transaction_events(transaction_digests)
            .await
            .context("Failed to load transaction events")?;
        Ok(Self {
            tx_digests_map,
            transaction_events,
        })
    }

    /// Get events for multiple transaction sequence numbers
    pub fn events_from_tx_sequence_numbers(
        &self,
        scope: &Scope,
        tx_sequence_numbers: &Vec<u64>,
    ) -> Result<Vec<Event>, RpcError> {
        let mut events: Vec<Event> = tx_sequence_numbers
            .iter()
            .map(|tx_seq_num| self.events_from_tx_sequence_number(scope, *tx_seq_num))
            .collect::<Result<Vec<_>, RpcError>>()?
            .into_iter()
            .flatten()
            .collect();
        events.sort_by(|a, b| {
            (a.tx_sequence_number, a.sequence_number)
                .cmp(&(b.tx_sequence_number, b.sequence_number))
        });
        Ok(events)
    }

    /// Get events for a single transaction sequence number
    fn events_from_tx_sequence_number(
        &self,
        scope: &Scope,
        tx_sequence_number: u64,
    ) -> Result<Vec<Event>, RpcError> {
        let key = TxDigestKey(tx_sequence_number);
        let stored_tx_digest = self
            .tx_digests_map
            .get(&key)
            .context("Failed to get transaction digest")?;

        let tx_digest = TransactionDigest::try_from(stored_tx_digest.tx_digest.clone())
            .context("Failed to deserialize transaction digest")?;

        let contents = self
            .transaction_events
            .get(&tx_digest)
            .context("Failed to get events")?;

        let native_events: Vec<NativeEvent> = contents.events()?;

        native_events
            .into_iter()
            .enumerate()
            .map(|(idx, native)| {
                Ok(Event {
                    scope: scope.clone(),
                    native,
                    transaction_digest: contents.digest()?,
                    sequence_number: idx as u64,
                    timestamp_ms: contents.timestamp_ms(),
                    tx_sequence_number,
                })
            })
            .collect()
    }
}
