// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::Context;
use async_graphql::Object;
use async_graphql::connection::Connection;
use diesel::prelude::QueryableByName;
use diesel::sql_types::BigInt;
use itertools::Itertools;
use serde::Deserialize;
use serde::Serialize;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_rpc_cursor::CursorKind;
use sui_rpc_cursor::CursorToken;
use sui_rpc_cursor::Position;
use sui_sql_macro::query;
use sui_types::base_types::SuiAddress as NativeSuiAddress;
use sui_types::digests::TransactionDigest;
use sui_types::event::Event as NativeEvent;

use crate::api::scalars::base64::Base64;
use crate::api::scalars::cursor::ByteCursor;
use crate::api::scalars::cursor::JsonCursor;
use crate::api::scalars::cursor::MultiCursor;
use crate::api::scalars::cursor::OpaqueCursor;
use crate::api::scalars::date_time::DateTime;
use crate::api::scalars::uint53::UInt53;
use crate::api::types::address::Address;
use crate::api::types::available_range::AvailableRangeKey;
use crate::api::types::event::filter::EventFilter;
use crate::api::types::lookups::CheckpointBounds;
use crate::api::types::lookups::TxBoundsCursor;
use crate::api::types::move_module::MoveModule;
use crate::api::types::move_package::MovePackage;
use crate::api::types::move_type::MoveType;
use crate::api::types::move_value::MoveValue;
use crate::api::types::transaction::Transaction;
use crate::error::RpcError;
use crate::extensions::query_limits;
use crate::pagination::Page;
use crate::scope::Scope;
use crate::task::watermark::Watermarks;

pub(crate) mod filter;
mod lookups;

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, PartialOrd, Ord, Copy)]
pub struct EventCursor {
    #[serde(rename = "t")]
    pub tx_sequence_number: u64,
    #[serde(rename = "e")]
    pub ev_sequence_number: u64,
}

/// Validated event cursor coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventToken {
    /// Tracks the originating `CursorToken`'s kind, so it can be reproduced on re-encode.
    kind: CursorKind,
    checkpoint: u64,
    tx_seq: u64,
    event_index: u32,
}

/// Compatibility dispatch over the on-wire cursor formats: `CursorToken` (primary) or the
/// legacy JSON cursor (secondary).
pub type CEvent = MultiCursor<OpaqueCursor<EventToken>, JsonCursor<EventCursor>>;

#[derive(Clone)]
pub(crate) struct Event {
    pub(crate) scope: Scope,
    /// Shared `Arc` so that multiple subscribers receiving events from the same
    /// streamed checkpoint avoid a deep clone per yield. Cloning an `Event` is then
    /// just an atomic refcount bump on the `native` field.
    pub(crate) native: Arc<NativeEvent>,
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
        Some(Transaction::with_digest(
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
        query_limits::rich::debit(ctx)?;
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

        page.paginate_results(
            events,
            |(c, _)| EventToken::cursor(0, c.tx_sequence_number, c.ev_sequence_number),
            |(_, e)| Ok(e),
        )
    }
}

impl EventToken {
    /// Mint the edge cursor for the event at the given coordinates.
    pub(crate) fn cursor(checkpoint: u64, tx_seq: u64, ev_sequence_number: u64) -> CEvent {
        CEvent::new(OpaqueCursor::new(Self {
            kind: CursorKind::Item,
            checkpoint,
            tx_seq,
            // Event counts per transaction are protocol-bounded, far below u32::MAX.
            event_index: ev_sequence_number
                .try_into()
                .expect("event index fits in u32"),
        }))
    }
}

impl CEvent {
    pub(crate) fn ev_sequence_number(&self) -> u64 {
        match self {
            CEvent::Primary(c) => c.event_index as u64,
            CEvent::Secondary(c) => c.ev_sequence_number,
        }
    }
}

impl ByteCursor for EventToken {
    fn decode_cursor(bytes: &[u8]) -> anyhow::Result<Self> {
        CursorToken::decode(bytes)?.try_into()
    }

    fn encode_cursor(&self) -> bytes::Bytes {
        CursorToken::from(self).encode()
    }
}

impl From<&EventToken> for CursorToken {
    fn from(token: &EventToken) -> Self {
        CursorToken {
            kind: token.kind,
            position: Position::Events {
                checkpoint: token.checkpoint,
                tx_seq: token.tx_seq,
                event_index: token.event_index,
            },
        }
    }
}

impl TryFrom<CursorToken> for EventToken {
    type Error = anyhow::Error;

    fn try_from(token: CursorToken) -> anyhow::Result<Self> {
        let Position::Events {
            checkpoint,
            tx_seq,
            event_index,
        } = token.position
        else {
            anyhow::bail!("invalid cursor");
        };
        Ok(Self {
            kind: token.kind,
            checkpoint,
            tx_seq,
            event_index,
        })
    }
}

impl Eq for CEvent {}

/// Cursors minted by different paths disagree on the checkpoint hint (and kind), so pagination
/// only compares the event coordinates.
impl PartialEq for CEvent {
    fn eq(&self, other: &Self) -> bool {
        (self.tx_sequence_number(), self.ev_sequence_number())
            == (other.tx_sequence_number(), other.ev_sequence_number())
    }
}

impl TxBoundsCursor for CEvent {
    fn tx_sequence_number(&self) -> u64 {
        match self {
            CEvent::Primary(c) => c.tx_seq,
            CEvent::Secondary(c) => c.tx_sequence_number,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::connection::CursorType;
    use fastcrypto::encoding::Base64 as B64;
    use fastcrypto::encoding::Encoding;

    /// Legacy pg-style cursor: a JSON-encoded `EventCursor`.
    fn legacy_cursor(tx_sequence_number: u64, ev_sequence_number: u64) -> CEvent {
        CEvent::Secondary(JsonCursor::new(EventCursor {
            tx_sequence_number,
            ev_sequence_number,
        }))
    }

    #[test]
    fn primary_cursor_roundtrips() {
        let cursor = EventToken::cursor(1, 2, 3);
        let decoded = CEvent::decode_cursor(&cursor.encode_cursor()).expect("valid cursor");
        assert_eq!(decoded.tx_sequence_number(), 2);
        assert_eq!(decoded.ev_sequence_number(), 3);
        assert_eq!(decoded, cursor);
    }

    /// Pagination equality keys off the event coordinates only: legacy cursors and grpc cursors
    /// minted with different checkpoint hints all compare equal at the same position.
    #[test]
    fn equality_ignores_checkpoint_hint() {
        assert_eq!(legacy_cursor(2, 3), EventToken::cursor(0, 2, 3));
        assert_eq!(EventToken::cursor(7, 2, 3), EventToken::cursor(0, 2, 3));
        assert_eq!(legacy_cursor(2, 3), EventToken::cursor(100, 2, 3));
        assert_ne!(legacy_cursor(2, 4), EventToken::cursor(0, 2, 3));
    }

    /// A token scoped to another endpoint must not decode as an event cursor.
    #[test]
    fn rejects_wrong_variant_cursor() {
        let token = CursorToken::item(Position::Transactions {
            checkpoint: 1,
            tx_seq: 2,
        });
        let encoded = B64::encode(token.encode());
        assert!(CEvent::decode_cursor(&encoded).is_err());
    }
}
