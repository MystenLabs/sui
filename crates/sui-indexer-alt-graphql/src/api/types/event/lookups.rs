// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{iter::Rev, ops::Range};

use anyhow::Context as _;
use async_graphql::Context;
use itertools::Either;
use sui_indexer_alt_reader::kv_loader::{KvLoader, TransactionEventsContents};
use sui_types::digests::TransactionDigest;

use crate::{api::types::transaction::tx_digests, error::RpcError, pagination::Page, scope::Scope};

use super::{
    filter::{tx_ev_bounds, EventFilter},
    CEvent, Event, EventCursor,
};

/// The page of Event cursors and Events emitted from transactions with cursors and limits with
/// overhead applied.
///
/// Note: tx_sequence_numbers are ordered ASC or DESC depending on the page direction, while events
/// in each transactions are returned in ASC sequence number from the store.
pub(crate) async fn events_from_sequence_numbers(
    scope: &Scope,
    ctx: &Context<'_>,
    page: &Page<CEvent>,
    tx_sequence_numbers: &[u64],
    filter: &EventFilter,
) -> Result<Vec<(EventCursor, Event)>, RpcError> {
    let kv_loader: &KvLoader = ctx.data()?;

    let digests = tx_digests(ctx, tx_sequence_numbers).await?;

    let digest_to_events = kv_loader
        .load_many_transaction_events(digests.iter().map(|d| d.1).collect())
        .await
        .context("Failed to load transaction events")?;

    let events = digests.into_iter().map(|(tx_sequence_number, digest)| {
        let events = digest_to_events
            .get(&digest)
            .context("Failed to get events")?;

        Ok((tx_sequence_number, digest, events))
    });

    let results = tx_events_paginated(scope, page, events, filter)?;

    Ok(results)
}

/// Helper function to map sequence numbers to a page of Event and Event Cursors.
fn tx_events_paginated<'e>(
    scope: &Scope,
    page: &Page<CEvent>,
    contents: impl Iterator<
        Item = anyhow::Result<(u64, TransactionDigest, &'e TransactionEventsContents)>,
    >,
    filter: &EventFilter,
) -> Result<Vec<(EventCursor, Event)>, RpcError> {
    let mut results = Vec::new();
    let limit = page.limit_with_overhead();

    'outer: for events in contents {
        let (tx_sequence_number, transaction_digest, contents) = events?;
        let events = contents.events()?;

        let bounds: Either<Range<usize>, Rev<Range<usize>>> = if page.is_from_front() {
            Either::Left(tx_ev_bounds(page, tx_sequence_number, events.len()))
        } else {
            Either::Right(tx_ev_bounds(page, tx_sequence_number, events.len()).rev())
        };

        for ev_sequence_number in bounds {
            let event_cursor = EventCursor {
                tx_sequence_number,
                ev_sequence_number: ev_sequence_number as u64,
            };

            let native = &events[ev_sequence_number];
            if !filter.matches(native) {
                continue;
            }

            let event = Event {
                scope: scope.clone(),
                native: native.clone(),
                transaction_digest,
                sequence_number: ev_sequence_number as u64,
                timestamp_ms: contents.timestamp_ms(),
            };

            results.push((event_cursor, event));

            if results.len() >= limit {
                break 'outer;
            }
        }
    }

    if !page.is_from_front() {
        results.reverse();
    }

    Ok(results)
}
