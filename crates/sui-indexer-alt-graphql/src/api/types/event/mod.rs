// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    Context, Object,
};
use diesel::{prelude::QueryableByName, sql_types::BigInt};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_sql_macro::query;
use sui_types::{
    base_types::SuiAddress as NativeSuiAddress, digests::TransactionDigest,
    event::Event as NativeEvent,
};

use crate::{
    api::scalars::{base64::Base64, cursor::JsonCursor, date_time::DateTime, uint53::UInt53},
    error::RpcError,
    pagination::Page,
    scope::Scope,
    task::watermark::Watermarks,
};

use super::{
    address::Address, checkpoint::filter::checkpoint_bounds, lookups::tx_bounds,
    move_module::MoveModule, move_package::MovePackage, move_type::MoveType, move_value::MoveValue,
    transaction::Transaction,
};

pub(crate) mod filter;
mod lookups;

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, PartialOrd, Ord, Copy)]
pub(crate) struct EventCursor {
    #[serde(rename = "t")]
    pub tx_sequence_number: u64,
    #[serde(rename = "e")]
    pub ev_sequence_number: u64,
}

pub(crate) type CEvent = JsonCursor<EventCursor>;

#[derive(Clone)]
pub(crate) struct Event {
    pub(crate) scope: Scope,
    pub(crate) native: NativeEvent,
    /// Digest of the transaction that emitted this event
    pub(crate) transaction_digest: TransactionDigest,
    /// Position of this event within the transaction's events list (0-indexed)
    pub(crate) sequence_number: u64,
    /// Timestamp when the transaction containing this event was finalized (checkpoint time)
    pub(crate) timestamp_ms: u64,
}

#[Object]
impl Event {
    /// The Move value emitted for this event.
    async fn contents(&self) -> Option<MoveValue> {
        let type_ = MoveType::from_native(self.native.type_.clone().into(), self.scope.clone());
        Some(MoveValue::new(type_, self.native.contents.clone()))
    }

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

    /// The module containing the function that was called in the programmable transaction, that resulted in this event being emitted.
    async fn transaction_module(&self) -> Option<MoveModule> {
        let package = MovePackage::with_address(self.scope.clone(), self.native.package_id.into());
        Some(MoveModule::with_fq_name(
            package,
            self.native.transaction_module.to_string(),
        ))
    }
}

impl Event {
    /// Paginates events based on the provided filters and page parameters.
    ///
    /// Returns empty results when no checkpoint is set in scope (e.g. execution scope).
    pub(crate) async fn paginate(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CEvent>,
        filter: filter::EventFilter,
    ) -> Result<Connection<String, Event>, RpcError> {
        let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
            return Ok(Connection::new(false, false));
        };

        let mut c = Connection::new(false, false);
        let pg_reader: &PgReader = ctx.data()?;
        let watermarks: &Arc<Watermarks> = ctx.data()?;

        // TODO: (henry) Use watermarks once we have a strategy for kv pruning.
        let reader_lo = 0;
        let global_tx_hi = watermarks.high_watermark().transaction();

        let Some(cp_bounds) = checkpoint_bounds(
            filter.after_checkpoint.map(u64::from),
            filter.at_checkpoint.map(u64::from),
            filter.before_checkpoint.map(u64::from),
            reader_lo,
            checkpoint_viewed_at,
        ) else {
            return Ok(Connection::new(false, false));
        };

        let tx_bounds = tx_bounds(ctx, &cp_bounds, global_tx_hi, &page, |c| {
            c.tx_sequence_number
        })
        .await?;

        #[derive(QueryableByName)]
        struct TxSequenceNumber(
            #[diesel(sql_type = BigInt, column_name = "tx_sequence_number")] i64,
        );

        let mut query = filter.query(tx_bounds)?;
        query += query!(
            r#" ORDER BY tx_sequence_number {} LIMIT {BigInt}"#,
            page.order_by_direction(),
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
            .unique()
            .collect();

        let events = lookups::events_from_sequence_numbers(
            &scope,
            ctx,
            &page,
            &tx_sequence_numbers,
            &filter,
        )
        .await?;

        let (has_prev, has_next, edges) =
            page.paginate_results(events, |(cursor, _)| JsonCursor::new(*cursor));

        // Set pagination state
        c.has_previous_page = has_prev;
        c.has_next_page = has_next;

        for (cursor, (_, event)) in edges {
            c.edges.push(Edge::new(cursor.encode_cursor(), event));
        }

        Ok(c)
    }
}
