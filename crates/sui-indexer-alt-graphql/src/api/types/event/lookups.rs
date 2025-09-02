// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::{iter::Rev, ops::Range, sync::Arc};

use anyhow::Context as _;
use async_graphql::{dataloader::DataLoader, Context};
use itertools::Either;
use sui_indexer_alt_reader::kv_loader::TransactionEventsContents;
use sui_indexer_alt_reader::{kv_loader::KvLoader, pg_reader::PgReader, tx_digests::TxDigestKey};
use sui_indexer_alt_schema::transactions::StoredTxDigest;
use sui_types::{digests::TransactionDigest, event::Event as NativeEvent};

use crate::{error::RpcError, pagination::Page, scope::Scope};

use super::{
    filter::{tx_ev_bounds, EventFilter},
    CEvent, Event, EventCursor,
};

/// The page of Event cursors and Events emitted from transactions with cursors and limits with overhead applied.
///
/// Note: tx_sequence_numbers are ordered ASC or DESC depending on the page direction, while events in
///       each transactions are returned in ASC sequence number from the store.
pub(crate) async fn events_from_sequence_numbers(
    scope: &Scope,
    ctx: &Context<'_>,
    page: &Page<CEvent>,
    tx_sequence_numbers: &[u64],
    filter: &EventFilter,
) -> Result<Vec<(EventCursor, Event)>, RpcError> {
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

    let mut results = tx_events_paginated(
        scope,
        page,
        tx_sequence_numbers,
        &sequence_to_digest,
        &digest_to_events,
        filter,
    )?;

    if !page.is_from_front() {
        results.reverse();
    }

    Ok(results)
}

/// Helper function to map sequence numbers to a page of Event and Event Cursors.
fn tx_events_paginated(
    scope: &Scope,
    page: &Page<CEvent>,
    tx_sequence_numbers: &[u64],
    sequence_to_digest: &HashMap<TxDigestKey, StoredTxDigest>,
    digest_to_events: &HashMap<TransactionDigest, TransactionEventsContents>,
    filter: &EventFilter,
) -> Result<Vec<(EventCursor, Event)>, RpcError> {
    let mut results = Vec::new();
    let limit = page.limit_with_overhead();

    for &tx_seq_num in tx_sequence_numbers {
        let key = TxDigestKey(tx_seq_num);
        let stored_tx_digest = sequence_to_digest
            .get(&key)
            .context("Failed to get transaction digest")?;

        let tx_digest = TransactionDigest::try_from(stored_tx_digest.tx_digest.clone())
            .context("Failed to deserialize transaction digest")?;

        let contents = digest_to_events
            .get(&tx_digest)
            .context("Failed to get events")?;

        let native_events: Vec<NativeEvent> = contents.events()?;
        let event_bounds: Either<Range<usize>, Rev<Range<usize>>> = if page.is_from_front() {
            Either::Left(tx_ev_bounds(page, tx_seq_num, native_events.len()))
        } else {
            Either::Right(tx_ev_bounds(page, tx_seq_num, native_events.len()).rev())
        };

        for ev_seq_num in event_bounds {
            let event_cursor = EventCursor {
                tx_sequence_number: tx_seq_num,
                ev_sequence_number: ev_seq_num as u64,
            };

            if !filter.matches(&native_events[ev_seq_num]) {
                continue;
            }

            let event = Event {
                scope: scope.clone(),
                native: native_events[ev_seq_num].clone(),
                transaction_digest: tx_digest,
                sequence_number: ev_seq_num as u64,
                timestamp_ms: contents.timestamp_ms(),
            };

            results.push((event_cursor, event));

            if results.len() >= limit {
                return Ok(results);
            }
        }
    }

    Ok(results)
}
