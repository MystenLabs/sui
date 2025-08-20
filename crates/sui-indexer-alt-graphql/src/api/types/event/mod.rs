// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashSet, sync::Arc};

use super::{
    address::Address, checkpoint::filter::checkpoint_bounds, transaction::filter::tx_bounds,
    transaction::Transaction,
};
use crate::{
    api::scalars::{base64::Base64, cursor::JsonCursor, date_time::DateTime, uint53::UInt53},
    error::RpcError,
    pagination::Page,
    scope::Scope,
    task::watermark::Watermarks,
};

use anyhow::Context as _;
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    Context, Object,
};
use diesel::{prelude::QueryableByName, sql_types::BigInt};
use serde::{Deserialize, Serialize};
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_sql_macro::query;
use sui_types::{
    base_types::SuiAddress as NativeSuiAddress, digests::TransactionDigest,
    event::Event as NativeEvent,
};

pub(crate) mod filter;
mod lookups;

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, PartialOrd, Ord)]
pub(crate) struct EventCursor {
    #[serde(rename = "t")]
    pub tx_sequence_number: u64,
    #[serde(rename = "e")]
    pub ev_sequence_number: u64,
}

pub(crate) type CEvent = JsonCursor<EventCursor>;

#[derive(QueryableByName)]
struct TxSequenceNumber(#[diesel(sql_type = BigInt, column_name = "tx_sequence_number")] i64);

#[derive(Clone, Debug)]
pub(crate) struct Event {
    pub(crate) scope: Scope,
    pub(crate) native: NativeEvent,
    /// Digest of the transaction that emitted this event
    pub(crate) transaction_digest: TransactionDigest,
    /// Position of this event within the transaction's events list (0-indexed)
    pub(crate) sequence_number: u64,
    /// Timestamp when the transaction containing this event was finalized (checkpoint time)
    pub(crate) timestamp_ms: u64,
    /// The transaction sequence number this event belongs to
    pub(crate) tx_sequence_number: u64,
}

// TODO(DVX-1200): Support sendingModule - MoveModule
// TODO(DVX-1203): contents - MoveValue
#[Object]
impl Event {
    /// The Base64 encoded BCS serialized bytes of the entire Event structure from sui-types.
    /// This includes: package_id, transaction_module, sender, type, and contents (which itself contains the BCS-serialized Move struct data).
    async fn event_bcs(&self) -> Result<Option<Base64>, RpcError> {
        let bcs_bytes = bcs::to_bytes(&self.native).context("Failed to serialize event")?;
        Ok(Some(Base64(bcs_bytes)))
    }

    /// Address of the sender of the transaction that emitted this event.
    async fn sender(&self) -> Option<Address> {
        if self.native.sender == NativeSuiAddress::ZERO {
            return None;
        }

        Some(Address::with_address(
            self.scope.clone(),
            self.native.sender,
        ))
    }

    /// The position of the event among the events from the same transaction.
    async fn sequence_number(&self) -> UInt53 {
        UInt53::from(self.sequence_number)
    }

    /// Timestamp corresponding to the checkpoint this event's transaction was finalized in.
    /// All events from the same transaction share the same timestamp.
    async fn timestamp(&self) -> Result<Option<DateTime>, RpcError> {
        Ok(Some(DateTime::from_ms(self.timestamp_ms as i64)?))
    }

    /// The transaction that emitted this event. This information is only available for events from indexed transactions, and not from transactions that have just been executed or dry-run.
    async fn transaction(&self) -> Option<Transaction> {
        Some(Transaction::with_id(
            self.scope.clone(),
            self.transaction_digest,
        ))
    }
}

impl Event {
    /// Paginates events based on the provided filters and page parameters.
    pub(crate) async fn paginate(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CEvent>,
        filter: filter::EventFilter,
    ) -> Result<Connection<String, Event>, RpcError> {
        let mut c = Connection::new(false, false);

        let pg_reader: &PgReader = ctx.data()?;
        let watermarks: &Arc<Watermarks> = ctx.data()?;

        // TODO: (henry) Use watermarks once we have a strategy for kv pruning.
        let reader_lo = 0;
        let global_tx_hi = watermarks.high_watermark().transaction();

        let cp_bounds = checkpoint_bounds(
            filter.after_checkpoint.map(u64::from),
            filter.at_checkpoint.map(u64::from),
            filter.before_checkpoint.map(u64::from),
            reader_lo,
            scope.checkpoint_viewed_at(),
        )
        .context("Failed to calculate checkpoint bounds")?;

        let tx_bounds = tx_bounds(ctx, &cp_bounds, global_tx_hi).await?;
        let pg_tx_bounds = pg_tx_bounds(&page, tx_bounds);

        let query = query!(
            r#"
            SELECT
                tx_sequence_number
            FROM
                ev_struct_inst
            WHERE
                tx_sequence_number >= {BigInt}
                AND tx_sequence_number < {BigInt}
            ORDER BY
                tx_sequence_number {}
            LIMIT {BigInt}
            "#,
            pg_tx_bounds.start as i64,
            pg_tx_bounds.end as i64,
            if page.is_from_front() {
                query!("ASC")
            } else {
                query!("DESC")
            },
            page.limit_with_overhead() as i64,
        );

        let mut conn = pg_reader
            .connect()
            .await
            .context("Failed to connect to database")?;

        let tx_sequence_numbers: Vec<u64> = conn
            .results(query)
            .await
            .context("Failed to execute query")?
            .into_iter()
            .map(|tx_seq: TxSequenceNumber| tx_seq.0 as u64)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        let ev_lookup =
            lookups::EventLookup::from_sequence_numbers(ctx, &tx_sequence_numbers).await?;
        let events = tx_events(&scope, &tx_sequence_numbers, &ev_lookup, &page)?;

        let (has_prev, has_next, edges) = page.paginate_results(events, |e| {
            JsonCursor::new(EventCursor {
                tx_sequence_number: e.tx_sequence_number,
                ev_sequence_number: e.sequence_number,
            })
        });

        // Set pagination state
        c.has_previous_page = has_prev;
        c.has_next_page = has_next;

        for (cursor, event) in edges {
            c.edges.push(Edge::new(cursor.encode_cursor(), event));
        }

        Ok(c)
    }
}

/// The events from the given transaction sequence numbers with
/// filtered by the cursor bounds inclusively.
fn tx_events(
    scope: &Scope,
    tx_sequence_numbers: &Vec<u64>,
    ev_lookup: &lookups::EventLookup,
    page: &Page<CEvent>,
) -> Result<Vec<Event>, RpcError> {
    let events = ev_lookup
        .events_from_tx_sequence_numbers(scope, tx_sequence_numbers)?
        .into_iter()
        .filter(|e| matches_cursor_bounds(e, page));

    if page.is_from_front() {
        Ok(events.take(page.limit_with_overhead()).collect())
    } else {
        // Graphql cursor syntax expects events to be in ascending order,
        // so we take the last N events and reorder them in ascending order.
        let mut events: Vec<Event> = events.rev().take(page.limit_with_overhead()).collect();
        events.reverse();
        Ok(events)
    }
}

fn matches_cursor_bounds(event: &Event, page: &Page<CEvent>) -> bool {
    let event_cursor = EventCursor {
        tx_sequence_number: event.tx_sequence_number,
        ev_sequence_number: event.sequence_number,
    };

    page.after().map_or(true, |after| event_cursor >= **after)
        && page
            .before()
            .map_or(true, |before| event_cursor <= **before)
}

/// The transaction sequence number bounds with pagination cursors applied inclusively.
fn pg_tx_bounds(page: &Page<CEvent>, tx_bounds: std::ops::Range<u64>) -> std::ops::Range<u64> {
    let pg_lo = page
        .after()
        .map(|c| c.tx_sequence_number)
        .map_or(tx_bounds.start, |tx_lo| tx_lo.max(tx_bounds.start));

    let pg_hi = page
        .before()
        .map(|c| c.tx_sequence_number.saturating_add(1))
        .map_or(tx_bounds.end, |tx_hi| tx_hi.min(tx_bounds.end));

    pg_lo..pg_hi
}
