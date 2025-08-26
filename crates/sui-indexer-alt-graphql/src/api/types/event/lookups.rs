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

pub(crate) async fn events_from_sequence_numbers(
    scope: &Scope,
    ctx: &Context<'_>,
    tx_sequence_numbers: &[u64],
) -> Result<Vec<Event>, RpcError> {
    let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;
    let kv_loader: &KvLoader = ctx.data()?;

    let tx_digest_keys: Vec<TxDigestKey> = tx_sequence_numbers
        .iter()
        .map(|r| TxDigestKey(*r))
        .collect();

    let sequence_to_digest = pg_loader
        .load_many(tx_digest_keys)
        .await
        .context("Failed to load transaction digests")?;

    let transaction_digests: Vec<TransactionDigest> = sequence_to_digest
        .values()
        .map(|stored| TransactionDigest::try_from(stored.tx_digest.clone()))
        .collect::<Result<_, _>>()
        .context("Failed to deserialize transaction digests")?;

    let digest_to_events = kv_loader
        .load_many_transaction_events(transaction_digests)
        .await
        .context("Failed to load transaction events")?;

    let events: Vec<Event> = tx_sequence_numbers
        .iter()
        .map(|tx_seq_num| {
            let key = TxDigestKey(*tx_seq_num);
            let stored_tx_digest = sequence_to_digest
                .get(&key)
                .context("Failed to get transaction digest")?;

            let tx_digest = TransactionDigest::try_from(stored_tx_digest.tx_digest.clone())
                .context("Failed to deserialize transaction digest")?;

            let contents = digest_to_events
                .get(&tx_digest)
                .context("Failed to get events")?;

            let native_events: Vec<NativeEvent> = contents.events()?;

            Ok::<Vec<Event>, RpcError>(
                native_events
                    .into_iter()
                    .enumerate()
                    .map(|(idx, native)| Event {
                        scope: scope.clone(),
                        native,
                        transaction_digest: tx_digest,
                        sequence_number: idx as u64,
                        timestamp_ms: contents.timestamp_ms(),
                        tx_sequence_number: *tx_seq_num,
                    })
                    .collect(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect();

    Ok(events)
}
