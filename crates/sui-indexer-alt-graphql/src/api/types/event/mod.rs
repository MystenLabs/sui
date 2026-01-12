// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{Context, Object, connection::Connection};
use diesel::{prelude::QueryableByName, sql_types::BigInt};
use itertools::Itertools;
use serde::Deserialize;
use serde::Serialize;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_sql_macro::query;
use sui_types::base_types::SuiAddress as NativeSuiAddress;
use sui_types::digests::TransactionDigest;
use sui_types::event::Event as NativeEvent;

use crate::api::scalars::base64::Base64;
use crate::api::scalars::cursor::JsonCursor;
use crate::api::scalars::date_time::DateTime;
use crate::api::scalars::uint53::UInt53;
use crate::api::types::address::Address;
use crate::api::types::available_range::AvailableRangeKey;
use crate::api::types::checkpoint::filter::checkpoint_bounds;
use crate::api::types::event::filter::EventFilter;
use crate::api::types::lookups::CheckpointBounds as _;
use crate::api::types::lookups::ScanCursor;
use crate::api::types::lookups::ScanCursorWithEvent;
use crate::api::types::lookups::TxBoundsCursor;
use crate::api::types::move_module::MoveModule;
use crate::api::types::move_package::MovePackage;
use crate::api::types::move_type::MoveType;
use crate::api::types::move_value::MoveValue;
use crate::api::types::scan;
use crate::api::types::transaction::Transaction;
use crate::config::Limits;
use crate::error::RpcError;
use crate::error::upcast;
use crate::pagination::Page;
use crate::scope::Scope;
use crate::task::watermark::Watermarks;

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

/// Cursor for event scanning - includes checkpoint for bloom filter traversal
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, Copy)]
pub(crate) struct EventScanCursor {
    #[serde(rename = "t")]
    pub tx_sequence_number: u64,
    #[serde(rename = "e")]
    pub ev_sequence_number: u64,
    #[serde(rename = "c")]
    pub cp_sequence_number: u64,
}

pub(crate) type SCEvent = JsonCursor<EventScanCursor>;

#[derive(Clone)]
pub(crate) struct Event {
    pub(crate) scope: Scope,
    pub(crate) native: NativeEvent,
    /// Digest of the transaction that emitted this event
    pub(crate) transaction_digest: TransactionDigest,
    /// Position of this event within the transaction's events list (0-indexed)
    pub(crate) sequence_number: u64,
    /// Timestamp of the checkpoint that includes the transaction containing this event.
    pub(crate) timestamp_ms: Option<u64>,
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
    async fn event_bcs(&self) -> Option<Result<Base64, RpcError>> {
        Some(
            bcs::to_bytes(&self.native)
                .context("Failed to serialize event")
                .map(Base64)
                .map_err(RpcError::from),
        )
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
    ///
    /// `null` for simulated/executed transactions as they are not included in a checkpoint.
    async fn timestamp(&self) -> Option<Result<DateTime, RpcError>> {
        Some(DateTime::from_ms(self.timestamp_ms? as i64))
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
        filter: EventFilter,
    ) -> Result<Connection<String, Event>, RpcError> {
        let pg_reader: &PgReader = ctx.data()?;

        let watermarks: &Arc<Watermarks> = ctx.data()?;
        let available_range_key = AvailableRangeKey {
            type_: "Query".to_string(),
            field: Some("events".to_string()),
            filters: Some(filter.active_filters()),
        };
        let reader_lo = available_range_key.reader_lo(watermarks)?;

        let Some(mut query) = filter.tx_bounds(ctx, &scope, reader_lo, &page).await? else {
            return Ok(Connection::new(false, false));
        };

        #[derive(QueryableByName)]
        struct TxSequenceNumber(
            #[diesel(sql_type = BigInt, column_name = "tx_sequence_number")] i64,
        );

        query += filter.query()?;
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

        page.paginate_results(events, |(c, _)| JsonCursor::new(*c), |(_, e)| Ok(e))
    }

    /// Scan through checkpoints using bloom filtering to find events matching ALL filters.
    ///
    /// Unlike `paginate`, this method supports multiple filters simultaneously (sender, module, type).
    pub(crate) async fn scan(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<SCEvent>,
        filter: EventFilter,
    ) -> Result<Connection<String, Event>, RpcError<scan::ScanError>> {
        let limits: &Limits = ctx.data()?;
        let watermarks: &Arc<Watermarks> = ctx.data()?;
        let available_range_key = AvailableRangeKey {
            type_: "Query".to_string(),
            field: Some("events".to_string()),
            filters: Some(filter.active_filters()),
        };
        let reader_lo = available_range_key.reader_lo(watermarks).map_err(upcast)?;

        let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
            return Ok(Connection::new(false, false));
        };

        let Some(cp_bounds) = checkpoint_bounds(
            filter.after_checkpoint.map(u64::from),
            filter.at_checkpoint.map(u64::from),
            filter.before_checkpoint.map(u64::from),
            reader_lo,
            checkpoint_viewed_at,
        ) else {
            return Ok(Connection::new(false, false));
        };

        scan::events(ctx, scope, &filter, &page, cp_bounds, limits).await
    }
}

impl TxBoundsCursor for CEvent {
    fn tx_sequence_number(&self) -> u64 {
        self.tx_sequence_number
    }
}

impl ScanCursor for EventScanCursor {
    fn cp_sequence_number(&self) -> u64 {
        self.cp_sequence_number
    }

    fn tx_sequence_number(&self) -> u64 {
        self.tx_sequence_number
    }
}

impl ScanCursorWithEvent for EventScanCursor {
    fn ev_sequence_number(&self) -> u64 {
        self.ev_sequence_number
    }
}
