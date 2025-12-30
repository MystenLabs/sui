// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{Context, Object, connection::Connection};
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
    api::{
        scalars::{base64::Base64, cursor::JsonCursor, date_time::DateTime, uint53::UInt53},
        types::{
            event::filter::EventFilter,
            lookups::{CheckpointBounds, TxBoundsCursor},
        },
    },
    error::RpcError,
    pagination::Page,
    scope::Scope,
    task::watermark::Watermarks,
};

use super::{
    address::Address, available_range::AvailableRangeKey, move_module::MoveModule,
    move_package::MovePackage, move_type::MoveType, move_value::MoveValue,
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
}

impl TxBoundsCursor for CEvent {
    fn tx_sequence_number(&self) -> u64 {
        self.tx_sequence_number
    }
}
