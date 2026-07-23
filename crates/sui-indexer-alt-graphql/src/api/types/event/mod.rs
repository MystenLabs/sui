// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::Context;
use async_graphql::Object;
use async_graphql::connection::Connection;
use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use async_graphql::connection::EmptyFields;
use async_graphql::connection::PageInfo;
use diesel::prelude::QueryableByName;
use diesel::sql_types::BigInt;
use itertools::Itertools;
use move_core_types::identifier::Identifier;
use prost_types::FieldMask;
use serde::Deserialize;
use serde::Serialize;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::AlphaLedgerGrpcReader;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::StreamPage;
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2;
use sui_rpc_cursor::CursorKind;
use sui_rpc_cursor::CursorToken;
use sui_rpc_cursor::Position;
use sui_sql_macro::query;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress as NativeSuiAddress;
use sui_types::digests::TransactionDigest;
use sui_types::event::Event as NativeEvent;
use sui_types::parse_sui_struct_tag;

use crate::api::scalars::base64::Base64;
use crate::api::scalars::cursor::ByteCursor;
use crate::api::scalars::cursor::JsonCursor;
use crate::api::scalars::cursor::MultiCursor;
use crate::api::scalars::cursor::OpaqueCursor;
use crate::api::scalars::date_time::DateTime;
use crate::api::scalars::uint53::UInt53;
use crate::api::types::address::Address;
use crate::api::types::available_range::AvailableRangeKey;
use crate::api::types::checkpoint::filter::checkpoint_bounds;
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
    /// `u32` matches the primary format's `event_index` bound: legitimate values are event array
    /// indices, capped by `max_num_event_emit` in the protocol config.
    #[serde(rename = "e")]
    pub ev_sequence_number: u32,
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
    pub(crate) timestamp: EventTimestamp,
}

/// Where the timestamp of the checkpoint containing the event's transaction comes from.
#[derive(Clone, Copy)]
pub(crate) enum EventTimestamp {
    /// Timestamp known at construction time. `None` when the transaction is not part of a
    /// checkpoint (simulated or just-executed transactions).
    Known(Option<u64>),
    /// Only the containing checkpoint is known (events hydrated from the gRPC scan stream); its
    /// summary is loaded on demand if the timestamp is requested.
    Checkpoint(u64),
}

/// Custom `Connection` for events to support partially-filled pages.
pub(crate) struct EventConnection {
    pub edges: Vec<Edge<String, Event, EmptyFields>>,
    pub page_info: PageInfo,
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
    async fn timestamp(&self, ctx: &Context<'_>) -> Option<Result<DateTime, RpcError>> {
        async {
            let timestamp_ms = match self.timestamp {
                EventTimestamp::Known(timestamp_ms) => timestamp_ms,
                EventTimestamp::Checkpoint(sequence_number) => {
                    let kv_loader: &KvLoader = ctx.data()?;
                    kv_loader
                        .load_one_checkpoint(sequence_number)
                        .await
                        .context("Failed to fetch checkpoint summary")?
                        .map(|(summary, _, _)| summary.timestamp_ms)
                }
            };

            let Some(timestamp_ms) = timestamp_ms else {
                return Ok(None);
            };

            Ok(Some(DateTime::from_ms(timestamp_ms as i64)?))
        }
        .await
        .transpose()
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

#[Object]
impl EventConnection {
    /// Information to aid in pagination.
    async fn page_info(&self) -> &PageInfo {
        &self.page_info
    }

    /// A list of edges.
    async fn edges(&self) -> &[Edge<String, Event, EmptyFields>] {
        &self.edges
    }

    /// A list of nodes.
    async fn nodes(&self) -> Vec<&Event> {
        self.edges.iter().map(|e| &e.node).collect()
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
    ) -> Result<EventConnection, RpcError> {
        query_limits::rich::debit(ctx)?;

        if let Some(reader) = ctx.data_opt::<AlphaLedgerGrpcReader>() {
            return Self::paginate_grpc(reader, scope, page, filter).await;
        }

        let pg_reader: &PgReader = ctx.data()?;

        let watermarks: &Arc<Watermarks> = ctx.data()?;
        let available_range_key = AvailableRangeKey {
            type_: "Query".to_string(),
            field: Some("events".to_string()),
            filters: Some(filter.active_filters()),
        };
        let reader_lo = available_range_key.reader_lo(watermarks)?;

        let Some(mut query) = filter.tx_bounds(ctx, &scope, reader_lo, &page).await? else {
            return Ok(EventConnection::empty());
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
        .map(Into::into)
    }

    /// Serve event pagination by streaming gRPC. Returns pages that may be partially filled,
    /// with valid cursors if there are more pages to paginate through.
    async fn paginate_grpc(
        reader: &AlphaLedgerGrpcReader,
        scope: Scope,
        page: Page<CEvent>,
        filter: EventFilter,
    ) -> Result<EventConnection, RpcError> {
        if page.limit() == 0 {
            return Ok(EventConnection::empty());
        }

        // Consistency upper bound; empty when scope has no checkpoint set.
        let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
            return Ok(EventConnection::empty());
        };

        // TODO: LedgerService expose available checkpoint range for `reader_lo`.
        let reader_lo = 0;

        let Some(cp_bounds) = checkpoint_bounds(
            filter.after_checkpoint().map(u64::from),
            filter.at_checkpoint().map(u64::from),
            filter.before_checkpoint().map(u64::from),
            reader_lo,
            checkpoint_viewed_at,
        ) else {
            return Ok(EventConnection::empty());
        };

        // Extract the cursor and pass through to grpc.
        let after = page.after().map(|c| CursorToken::from(&c.token()).encode());
        // Pg-minted cursors set checkpoint as 0 (as do legacy JSON cursors, which carry no
        // checkpoint at all). Substitute the checkpoint with u64::max on the `before` bound to
        // avoid collapsing the checkpoint window.
        let before = page.before().map(|c| {
            let mut token = c.token();
            if token.checkpoint == 0 && (token.tx_seq != 0 || token.event_index != 0) {
                token.checkpoint = u64::MAX;
            }
            CursorToken::from(&token).encode()
        });

        let mut options = v2::QueryOptions::default();
        options.limit = Some(page.limit() as u32);
        options.after = after;
        options.before = before;
        options.ordering = Some(if page.is_from_front() {
            v2::Ordering::Ascending as i32
        } else {
            v2::Ordering::Descending as i32
        });

        let mut request = v2::ListEventsRequest::default();
        // Everything the GraphQL node needs rides on the stream item: the event envelope, its
        // position, and its containing checkpoint (the timestamp resolves lazily from the
        // checkpoint's summary).
        request.read_mask = Some(FieldMask::from_paths([
            "contents",
            "package_id",
            "module",
            "sender",
            "transaction_digest",
            "event_index",
            "checkpoint",
        ]));
        request.start_checkpoint = Some(*cp_bounds.start());
        // `cp_bounds` end is inclusive; the request bound is exclusive.
        request.end_checkpoint = Some(cp_bounds.end().saturating_add(1));
        request.filter = filter.to_grpc_filter()?;
        request.options = Some(options);

        let result = reader
            .list_events(request)
            .await
            .context("Failed to list events")?;

        build_grpc_connection(scope, &page, result)
    }
}

impl EventToken {
    /// Mint the edge cursor for the event at the given coordinates.
    pub(crate) fn cursor(checkpoint: u64, tx_seq: u64, event_index: u32) -> CEvent {
        CEvent::new(OpaqueCursor::new(Self {
            kind: CursorKind::Item,
            checkpoint,
            tx_seq,
            event_index,
        }))
    }
}

impl CEvent {
    pub(crate) fn ev_sequence_number(&self) -> u32 {
        match self {
            CEvent::Primary(c) => c.event_index,
            CEvent::Secondary(c) => c.ev_sequence_number,
        }
    }

    /// View the cursor as validated event coordinates, regardless of wire format. Legacy JSON
    /// cursors carry no checkpoint, so their hint defaults to 0 (unknown).
    fn token(&self) -> EventToken {
        match self {
            CEvent::Primary(c) => (**c).clone(),
            CEvent::Secondary(c) => EventToken {
                kind: CursorKind::Item,
                checkpoint: 0,
                tx_seq: c.tx_sequence_number,
                event_index: c.ev_sequence_number,
            },
        }
    }
}

impl EventConnection {
    fn empty() -> Self {
        Self {
            edges: vec![],
            page_info: PageInfo {
                has_previous_page: false,
                has_next_page: false,
                start_cursor: None,
                end_cursor: None,
            },
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

impl From<Connection<String, Event>> for EventConnection {
    /// Convert a stock async-graphql `Connection` (as produced by the PG path's
    /// `Page::paginate_results`) into the custom shape. Cursors are derived from edges, matching
    /// stock semantics.
    fn from(conn: Connection<String, Event>) -> Self {
        let start_cursor = conn.edges.first().map(|e| e.cursor.clone());
        let end_cursor = conn.edges.last().map(|e| e.cursor.clone());
        Self {
            edges: conn.edges,
            page_info: PageInfo {
                has_previous_page: conn.has_previous_page,
                has_next_page: conn.has_next_page,
                start_cursor,
                end_cursor,
            },
        }
    }
}

/// Hydrate an `Event` node from a `ListEvents` stream item. The read mask requests everything the
/// node needs — the event envelope, its position, and its containing checkpoint — so no KV lookup
/// is required; a missing field is an internal inconsistency.
fn event_from_stream_item(scope: Scope, payload: &v2::Event) -> Result<Event, RpcError> {
    let transaction_digest = payload
        .transaction_digest
        .as_deref()
        .context("ListEvents item missing transaction digest")?
        .parse::<TransactionDigest>()
        .context("Failed to parse transaction digest from ListEvents")?;

    let event_index = payload
        .event_index
        .context("ListEvents item missing event index")?;

    let checkpoint = payload
        .checkpoint
        .context("ListEvents item missing checkpoint")?;

    let package_id = payload
        .package_id
        .as_deref()
        .context("ListEvents item missing package ID")?
        .parse::<ObjectID>()
        .context("Failed to parse package ID from ListEvents")?;

    let transaction_module = Identifier::new(
        payload
            .module
            .as_deref()
            .context("ListEvents item missing module")?,
    )
    .context("Failed to parse module from ListEvents")?;

    let sender = payload
        .sender
        .as_deref()
        .context("ListEvents item missing sender")?
        .parse::<NativeSuiAddress>()
        .context("Failed to parse sender from ListEvents")?;

    let contents = payload
        .contents
        .as_ref()
        .context("ListEvents item missing contents")?;

    // Both servers render event contents via the SDK's `Event` merge, which sets the `Bcs.name`
    // to the event's canonical type string.
    let type_ = parse_sui_struct_tag(
        contents
            .name
            .as_deref()
            .context("ListEvents item contents missing type name")?,
    )
    .context("Failed to parse event type from ListEvents")?;

    let native = NativeEvent {
        package_id,
        transaction_module,
        sender,
        type_,
        contents: contents
            .value
            .as_ref()
            .context("ListEvents item contents missing value")?
            .to_vec(),
    };

    Ok(Event {
        scope,
        native: Arc::new(native),
        transaction_digest,
        sequence_number: event_index as u64,
        timestamp: EventTimestamp::Checkpoint(checkpoint),
    })
}

/// Build an `EventConnection` from draining a bitmap-scan page, hydrating each edge's event from
/// the stream item itself.
///
/// Edges are returned in ascending order.
fn build_grpc_connection(
    scope: Scope,
    page: &Page<CEvent>,
    result: StreamPage<v2::Event>,
) -> Result<EventConnection, RpcError> {
    // TODO: This and transaction::build_grpc_connection can eventually be refactored. A closure
    // that translates from the PageItem to Node is the only difference. Cursor encoding is covered
    // as both TransactionToken and EventToken implement ByteCursor + TryFrom<CursorToken>. However,
    // this refactor work should wait until the grpc migration is complete to avoid premature
    // abstractions.
    let more = result.has_more();
    let start = result.first_cursor().cloned();
    let end = result.last_cursor().cloned();
    let mut items = result.items;

    let (has_previous_page, has_next_page, start, end) = if page.is_from_front() {
        (page.after().is_some(), more, start, end)
    } else {
        items.reverse();
        (more, page.before().is_some(), end, start)
    };

    let mut edges = Vec::with_capacity(items.len());
    for item in items {
        let event = event_from_stream_item(scope.clone(), &item.payload)?;
        edges.push(Edge::new(encode_grpc_cursor(&item.cursor)?, event));
    }

    let start_cursor = start.map(|b| encode_grpc_cursor(&b)).transpose()?;
    let end_cursor = end.map(|b| encode_grpc_cursor(&b)).transpose()?;

    Ok(EventConnection {
        edges,
        page_info: PageInfo {
            has_previous_page,
            has_next_page,
            start_cursor,
            end_cursor,
        },
    })
}

/// Re-encode a server-minted cursor (raw encoded `CursorToken` bytes from the gRPC stream) as a
/// GraphQL cursor string.
fn encode_grpc_cursor(bytes: &[u8]) -> Result<String, RpcError> {
    let token = CursorToken::decode(bytes).context("Failed to decode ListEvents cursor")?;
    let token: EventToken = token
        .try_into()
        .context("Unexpected position in ListEvents cursor")?;
    Ok(CEvent::new(OpaqueCursor::new(token)).encode_cursor())
}

#[cfg(test)]
mod tests {
    use super::*;

    use async_graphql::connection::CursorType;
    use fastcrypto::encoding::Base58;
    use fastcrypto::encoding::Base64 as B64;
    use fastcrypto::encoding::Encoding;
    use move_core_types::identifier::Identifier;
    use move_core_types::language_storage::StructTag;
    use sui_indexer_alt_reader::alpha_ledger_grpc_reader::PageItem;
    use sui_types::base_types::ObjectID;

    use crate::pagination::PageLimits;

    /// Legacy pg-style cursor: a JSON-encoded `EventCursor`.
    fn legacy_cursor(tx_sequence_number: u64, ev_sequence_number: u32) -> CEvent {
        CEvent::Secondary(JsonCursor::new(EventCursor {
            tx_sequence_number,
            ev_sequence_number,
        }))
    }

    fn ev_position(checkpoint: u64, tx_seq: u64, event_index: u32) -> Position {
        Position::Events {
            checkpoint,
            tx_seq,
            event_index,
        }
    }

    /// Build a synthetic `PageItem` pointing at `event_index` of the zero-digest
    /// transaction, with the provided resume cursor.
    fn ev_item(event_index: u32, cursor: CursorToken) -> PageItem<v2::Event> {
        let mut payload = v2::Event::default();
        payload.transaction_digest = Some(Base58::encode(TransactionDigest::default().inner()));
        payload.event_index = Some(event_index);
        PageItem {
            payload,
            cursor: cursor.encode(),
        }
    }

    fn native_event() -> NativeEvent {
        NativeEvent {
            package_id: ObjectID::ZERO,
            transaction_module: Identifier::new("m").unwrap(),
            sender: NativeSuiAddress::ZERO,
            type_: StructTag {
                address: ObjectID::ZERO.into(),
                module: Identifier::new("m").unwrap(),
                name: Identifier::new("T").unwrap(),
                type_params: vec![],
            },
            contents: vec![],
        }
    }

    /// Hydrated contents for the zero-digest transaction with `n` events.
    fn contents_for(n: usize) -> HashMap<TransactionDigest, TxEventContents> {
        HashMap::from([(
            TransactionDigest::default(),
            TxEventContents {
                events: (0..n).map(|_| native_event()).collect(),
                timestamp_ms: Some(1_234),
            },
        )])
    }

    /// The GraphQL cursor string that `build_grpc_connection` mints for raw server cursor
    /// bytes.
    fn graphql_cursor(token: CursorToken) -> String {
        let token: EventToken = token.try_into().expect("events cursor");
        CEvent::new(OpaqueCursor::new(token)).encode_cursor()
    }

    fn page_limits(limit: u64) -> PageLimits {
        PageLimits {
            default: limit as u32,
            max: limit as u32,
        }
    }

    /// Build a `Page<CEvent>` going forwards (`first: N`, no `after`/`before`).
    fn forward_page(limit: u64) -> Page<CEvent> {
        Page::from_params(&page_limits(limit), Some(limit), None, None, None)
            .expect("constructing forward Page<CEvent>")
    }

    /// Build a `Page<CEvent>` going backwards (`last: N`, no `after`/`before`).
    fn backward_page(limit: u64) -> Page<CEvent> {
        Page::from_params(&page_limits(limit), None, None, Some(limit), None)
            .expect("constructing backward Page<CEvent>")
    }

    /// Forward page opened from an `after` cursor (`first: N, after: <cursor>`).
    fn forward_page_after(limit: u64, after: CEvent) -> Page<CEvent> {
        Page::from_params(&page_limits(limit), Some(limit), Some(after), None, None)
            .expect("constructing forward Page with after")
    }

    /// Backward page opened from a `before` cursor (`last: N, before: <cursor>`).
    fn backward_page_before(limit: u64, before: CEvent) -> Page<CEvent> {
        Page::from_params(&page_limits(limit), None, None, Some(limit), Some(before))
            .expect("constructing backward Page with before")
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

    /// Legacy cursors carrying an event index beyond `u32` were never minted by a server (indices
    /// are capped by `max_num_event_emit`) and must fail to parse.
    #[test]
    fn rejects_oversized_legacy_event_index() {
        let bytes = format!(r#"{{"t":2,"e":{}}}"#, u64::MAX);
        let encoded = B64::encode(bytes.as_bytes());
        assert!(CEvent::decode_cursor(&encoded).is_err());
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

    /// Legacy JSON cursors carry no checkpoint; the coordinate view defaults the hint to 0.
    #[test]
    fn legacy_cursor_token_defaults_checkpoint_zero() {
        let token = legacy_cursor(2, 3).token();
        assert_eq!(token.kind, CursorKind::Item);
        assert_eq!(token.checkpoint, 0);
        assert_eq!(token.tx_seq, 2);
        assert_eq!(token.event_index, 3);
    }

    /// Empty connection surfaces cursors if provided by the streamed page.
    #[test]
    fn empty_page_surfaces_boundary_cursors() {
        let scope = Scope::for_tests();
        let page = forward_page(10);
        let result = StreamPage::<v2::Event>::for_test(
            Vec::new(),
            Some(CursorToken::boundary(ev_position(1, 10, 0)).encode()),
            Some(CursorToken::boundary(ev_position(2, 20, 0)).encode()),
            None,
        );

        let conn =
            build_grpc_connection(scope, &page, result, &HashMap::new()).expect("connection built");
        assert!(conn.edges.is_empty());
        assert!(!conn.page_info.has_previous_page);
        assert!(conn.page_info.has_next_page);

        /// Checkpoint stamped on every synthetic stream item.
        const ITEM_CHECKPOINT: u64 = 1;

        fn ev_position(checkpoint: u64, tx_seq: u64, event_index: u32) -> Position {
            Position::Events {
                checkpoint,
                tx_seq,
                event_index,
            }
        }

        /// Build a synthetic, fully-populated `PageItem` pointing at `event_index` of the
        /// zero-digest transaction, with the provided resume cursor.
        fn ev_item(event_index: u32, cursor: CursorToken) -> PageItem<v2::Event> {
            let mut contents = v2::Bcs::default();
            contents.name = Some("0x0::m::T".to_string());
            contents.value = Some(Default::default());

            let mut payload = v2::Event::default();
            payload.transaction_digest = Some(Base58::encode(TransactionDigest::default().inner()));
            payload.event_index = Some(event_index);
            payload.checkpoint = Some(ITEM_CHECKPOINT);
            payload.package_id = Some(ObjectID::ZERO.to_canonical_string(true));
            payload.module = Some("m".to_string());
            payload.sender = Some(NativeSuiAddress::ZERO.to_string());
            payload.contents = Some(contents);
            PageItem {
                payload,
                cursor: cursor.encode(),
            }
        }

        /// The GraphQL cursor string that `build_grpc_connection` mints for raw server cursor
        /// bytes.
        fn graphql_cursor(token: CursorToken) -> String {
            let token: EventToken = token.try_into().expect("events cursor");
            CEvent::new(OpaqueCursor::new(token)).encode_cursor()
        }

        let conn = build_grpc_connection(scope, &page, result, &contents_for(3))
            .expect("connection built");
        assert_eq!(conn.edges.len(), 3);
        // `CheckpointBound` means the range was exhausted — no forward continuation.
        assert!(!conn.page_info.has_next_page);

        let start = conn.page_info.start_cursor.expect("start set");
        let end = conn.page_info.end_cursor.expect("end set");
        assert_eq!(
            start, conn.edges[0].cursor,
            "non-empty page should anchor start_cursor on first edge, not stream watermark"
        );
        assert_eq!(
            end, conn.edges[2].cursor,
            "non-empty page should anchor end_cursor on last edge, not stream watermark"
        );

        /// Build a `Page<CEvent>` going backwards (`last: N`, no `after`/`before`).
        fn backward_page(limit: u64) -> Page<CEvent> {
            Page::from_params(&page_limits(limit), None, None, Some(limit), None)
                .expect("constructing backward Page<CEvent>")
        }

        /// Forward page opened from an `after` cursor (`first: N, after: <cursor>`).
        fn forward_page_after(limit: u64, after: CEvent) -> Page<CEvent> {
            Page::from_params(&page_limits(limit), Some(limit), Some(after), None, None)
                .expect("constructing forward Page with after")
        }

        /// Backward page opened from a `before` cursor (`last: N, before: <cursor>`).
        fn backward_page_before(limit: u64, before: CEvent) -> Page<CEvent> {
            Page::from_params(&page_limits(limit), None, None, Some(limit), Some(before))
                .expect("constructing backward Page with before")
        }

        /// Empty connection surfaces cursors if provided by the streamed page.
        #[test]
        fn empty_page_surfaces_boundary_cursors() {
            let scope = Scope::for_tests();
            let page = forward_page(10);
            let result = StreamPage::<v2::Event>::for_test(
                Vec::new(),
                Some(CursorToken::boundary(ev_position(1, 10, 0)).encode()),
                Some(CursorToken::boundary(ev_position(2, 20, 0)).encode()),
                None,
            );

            let conn = build_grpc_connection(scope, &page, result).expect("connection built");
            assert!(conn.edges.is_empty());
            assert!(!conn.page_info.has_previous_page);
            assert!(conn.page_info.has_next_page);

            let start = conn.page_info.start_cursor.expect("start cursor set");
            let end = conn.page_info.end_cursor.expect("end cursor set");
            assert_ne!(start, end, "start and end cursors should be different");
        }

        /// Order of cursors on connection should be swapped from streamed page.
        #[test]
        fn empty_page_backward_correct_cursors() {
            let scope = Scope::for_tests();
            let page = backward_page(10);
            // Descending stream: the first watermark the stream reports is the high end.
            let result = StreamPage::<v2::Event>::for_test(
                Vec::new(),
                Some(CursorToken::boundary(ev_position(2, 20, 0)).encode()),
                Some(CursorToken::boundary(ev_position(1, 10, 0)).encode()),
                None,
            );

            let conn = build_grpc_connection(scope, &page, result).expect("connection built");
            assert!(conn.edges.is_empty());
            assert!(conn.page_info.has_previous_page);
            assert!(!conn.page_info.has_next_page);

            let start = conn.page_info.start_cursor.expect("start cursor set");
            let end = conn.page_info.end_cursor.expect("end cursor set");
            assert_eq!(
                start,
                graphql_cursor(CursorToken::boundary(ev_position(1, 10, 0)))
            );
            assert_eq!(
                end,
                graphql_cursor(CursorToken::boundary(ev_position(2, 20, 0)))
            );
        }

        #[test]
        fn non_empty_page_uses_edge_cursors_and_hydrates_nodes() {
            let scope = Scope::for_tests();
            let page = forward_page(10);
            let result = StreamPage::<v2::Event>::for_test(
                vec![
                    ev_item(0, CursorToken::item(ev_position(1, 1, 0))),
                    ev_item(1, CursorToken::item(ev_position(1, 1, 1))),
                    ev_item(2, CursorToken::item(ev_position(1, 1, 2))),
                ],
                None,
                None,
                Some(v2::QueryEndReason::CheckpointBound),
            );

            let conn = build_grpc_connection(scope, &page, result).expect("connection built");
            assert_eq!(conn.edges.len(), 3);
            // `CheckpointBound` means the range was exhausted — no forward continuation.
            assert!(!conn.page_info.has_next_page);

            let start = conn.page_info.start_cursor.expect("start set");
            let end = conn.page_info.end_cursor.expect("end set");
            assert_eq!(
                start, conn.edges[0].cursor,
                "non-empty page should anchor start_cursor on first edge, not stream watermark"
            );
            assert_eq!(
                end, conn.edges[2].cursor,
                "non-empty page should anchor end_cursor on last edge, not stream watermark"
            );

            // Nodes carry the hydrated position, envelope, and checkpoint for lazy timestamps.
            for (i, edge) in conn.edges.iter().enumerate() {
                assert_eq!(edge.node.sequence_number, i as u64);
                assert_eq!(edge.node.transaction_digest, TransactionDigest::default());
                assert_eq!(edge.node.native.package_id, ObjectID::ZERO);
                assert_eq!(
                    edge.node.native.type_,
                    parse_sui_struct_tag("0x0::m::T").unwrap()
                );
                assert!(matches!(
                    edge.node.timestamp,
                    EventTimestamp::Checkpoint(ITEM_CHECKPOINT)
                ));
            }
        }

        #[test]
        fn full_page_at_item_limit_signals_more() {
            let scope = Scope::for_tests();
            let page = forward_page(2);
            let result = StreamPage::<v2::Event>::for_test(
                vec![
                    ev_item(0, CursorToken::item(ev_position(1, 1, 0))),
                    ev_item(1, CursorToken::item(ev_position(1, 1, 1))),
                ],
                None,
                None,
                Some(v2::QueryEndReason::ItemLimit),
            );

            let conn = build_grpc_connection(scope, &page, result).expect("connection built");
            assert_eq!(conn.edges.len(), 2);
            assert!(
                conn.page_info.has_next_page,
                "full page + ItemLimit must report hasNextPage: true (has_more() is true)"
            );
        }

        #[test]
        fn descending_page_reverses_to_ascending_edges() {
            let scope = Scope::for_tests();
            let page = backward_page(10);
            // Descending stream order: event indices 2, 1, 0 (highest position first).
            let result = StreamPage::<v2::Event>::for_test(
                vec![
                    ev_item(2, CursorToken::item(ev_position(1, 1, 2))),
                    ev_item(1, CursorToken::item(ev_position(1, 1, 1))),
                    ev_item(0, CursorToken::item(ev_position(1, 1, 0))),
                ],
                None,
                None,
                Some(v2::QueryEndReason::CheckpointBound),
            );

            let conn = build_grpc_connection(scope, &page, result).expect("connection built");
            assert_eq!(conn.edges.len(), 3);
            // After reversal, the *first* edge corresponds to the *lowest* position from the
            // stream — i.e. the last item the stream emitted (event index 0).
            let start = conn.page_info.start_cursor.expect("start set");
            let end = conn.page_info.end_cursor.expect("end set");
            assert_eq!(start, conn.edges[0].cursor);
            assert_eq!(
                start,
                graphql_cursor(CursorToken::item(ev_position(1, 1, 0)))
            );
            assert_eq!(end, conn.edges[2].cursor);
            assert_eq!(end, graphql_cursor(CursorToken::item(ev_position(1, 1, 2))));
            assert_eq!(
                conn.edges
                    .iter()
                    .map(|e| e.node.sequence_number)
                    .collect::<Vec<_>>(),
                [0, 1, 2],
            );
        }

        /// A forward page opened from an `after` cursor reports `hasPreviousPage: true`
        /// (`page.after().is_some()`). `CheckpointBound` makes `has_more()` false, so the only
        /// source of a `true` flag is the input cursor — not the stream.
        #[test]
        fn forward_after_signals_previous_page() {
            let scope = Scope::for_tests();
            let page = forward_page_after(10, EventToken::cursor(1, 1, 0));
            let result = StreamPage::<v2::Event>::for_test(
                vec![ev_item(1, CursorToken::item(ev_position(1, 1, 1)))],
                None,
                None,
                Some(v2::QueryEndReason::CheckpointBound),
            );

            let conn = build_grpc_connection(scope, &page, result).expect("connection built");
            assert!(
                conn.page_info.has_previous_page,
                "after cursor set → hasPreviousPage"
            );
            assert!(
                !conn.page_info.has_next_page,
                "CheckpointBound → no hasNextPage"
            );
        }

        /// A backward page opened from a `before` cursor reports `hasNextPage: true`
        /// (`page.before().is_some()`). `CheckpointBound` makes `has_more()` false, so the only
        /// source of a `true` flag is the input cursor — not the stream.
        #[test]
        fn backward_before_signals_next_page() {
            let scope = Scope::for_tests();
            let page = backward_page_before(10, EventToken::cursor(1, 1, 2));
            let result = StreamPage::<v2::Event>::for_test(
                vec![
                    ev_item(1, CursorToken::item(ev_position(1, 1, 1))),
                    ev_item(0, CursorToken::item(ev_position(1, 1, 0))),
                ],
                None,
                None,
                Some(v2::QueryEndReason::CheckpointBound),
            );

            let conn = build_grpc_connection(scope, &page, result).expect("connection built");
            assert!(
                conn.page_info.has_next_page,
                "before cursor set → hasNextPage"
            );
            assert!(
                !conn.page_info.has_previous_page,
                "CheckpointBound → no hasPreviousPage"
            );
        }

        /// The read mask requests the full event envelope, so an item missing one of its fields
        /// is an internal inconsistency, not an empty result.
        #[test]
        fn missing_payload_field_errors() {
            let scope = Scope::for_tests();
            let page = forward_page(10);

            let mut item = ev_item(0, CursorToken::item(ev_position(1, 1, 0)));
            item.payload.contents = None;
            let result = StreamPage::<v2::Event>::for_test(
                vec![item],
                None,
                None,
                Some(v2::QueryEndReason::CheckpointBound),
            );
            assert!(
                build_grpc_connection(scope.clone(), &page, result).is_err(),
                "missing event contents should error"
            );

            let mut item = ev_item(0, CursorToken::item(ev_position(1, 1, 0)));
            item.payload.checkpoint = None;
            let result = StreamPage::<v2::Event>::for_test(
                vec![item],
                None,
                None,
                Some(v2::QueryEndReason::CheckpointBound),
            );
            assert!(
                build_grpc_connection(scope, &page, result).is_err(),
                "missing checkpoint should error"
            );
        }
    }

    #[test]
    fn full_page_at_item_limit_signals_more() {
        let scope = Scope::for_tests();
        let page = forward_page(2);
        let result = StreamPage::<v2::Event>::for_test(
            vec![
                ev_item(0, CursorToken::item(ev_position(1, 1, 0))),
                ev_item(1, CursorToken::item(ev_position(1, 1, 1))),
            ],
            None,
            None,
            Some(v2::QueryEndReason::ItemLimit),
        );

        let conn = build_grpc_connection(scope, &page, result, &contents_for(2))
            .expect("connection built");
        assert_eq!(conn.edges.len(), 2);
        assert!(
            conn.page_info.has_next_page,
            "full page + ItemLimit must report hasNextPage: true (has_more() is true)"
        );
    }

    #[test]
    fn descending_page_reverses_to_ascending_edges() {
        let scope = Scope::for_tests();
        let page = backward_page(10);
        // Descending stream order: event indices 2, 1, 0 (highest position first).
        let result = StreamPage::<v2::Event>::for_test(
            vec![
                ev_item(2, CursorToken::item(ev_position(1, 1, 2))),
                ev_item(1, CursorToken::item(ev_position(1, 1, 1))),
                ev_item(0, CursorToken::item(ev_position(1, 1, 0))),
            ],
            None,
            None,
            Some(v2::QueryEndReason::CheckpointBound),
        );

        let conn = build_grpc_connection(scope, &page, result, &contents_for(3))
            .expect("connection built");
        assert_eq!(conn.edges.len(), 3);
        // After reversal, the *first* edge corresponds to the *lowest* position from the
        // stream — i.e. the last item the stream emitted (event index 0).
        let start = conn.page_info.start_cursor.expect("start set");
        let end = conn.page_info.end_cursor.expect("end set");
        assert_eq!(start, conn.edges[0].cursor);
        assert_eq!(
            start,
            graphql_cursor(CursorToken::item(ev_position(1, 1, 0)))
        );
        assert_eq!(end, conn.edges[2].cursor);
        assert_eq!(end, graphql_cursor(CursorToken::item(ev_position(1, 1, 2))));
        assert_eq!(
            conn.edges
                .iter()
                .map(|e| e.node.sequence_number)
                .collect::<Vec<_>>(),
            [0, 1, 2],
        );
    }

    /// A forward page opened from an `after` cursor reports `hasPreviousPage: true`
    /// (`page.after().is_some()`). `CheckpointBound` makes `has_more()` false, so the only
    /// source of a `true` flag is the input cursor — not the stream.
    #[test]
    fn forward_after_signals_previous_page() {
        let scope = Scope::for_tests();
        let page = forward_page_after(10, EventToken::cursor(1, 1, 0));
        let result = StreamPage::<v2::Event>::for_test(
            vec![ev_item(1, CursorToken::item(ev_position(1, 1, 1)))],
            None,
            None,
            Some(v2::QueryEndReason::CheckpointBound),
        );

        let conn = build_grpc_connection(scope, &page, result, &contents_for(2))
            .expect("connection built");
        assert!(
            conn.page_info.has_previous_page,
            "after cursor set → hasPreviousPage"
        );
        assert!(
            !conn.page_info.has_next_page,
            "CheckpointBound → no hasNextPage"
        );
    }

    /// A backward page opened from a `before` cursor reports `hasNextPage: true`
    /// (`page.before().is_some()`). `CheckpointBound` makes `has_more()` false, so the only
    /// source of a `true` flag is the input cursor — not the stream.
    #[test]
    fn backward_before_signals_next_page() {
        let scope = Scope::for_tests();
        let page = backward_page_before(10, EventToken::cursor(1, 1, 2));
        let result = StreamPage::<v2::Event>::for_test(
            vec![
                ev_item(1, CursorToken::item(ev_position(1, 1, 1))),
                ev_item(0, CursorToken::item(ev_position(1, 1, 0))),
            ],
            None,
            None,
            Some(v2::QueryEndReason::CheckpointBound),
        );

        let conn = build_grpc_connection(scope, &page, result, &contents_for(2))
            .expect("connection built");
        assert!(
            conn.page_info.has_next_page,
            "before cursor set → hasNextPage"
        );
        assert!(
            !conn.page_info.has_previous_page,
            "CheckpointBound → no hasPreviousPage"
        );
    }

    /// An item pointing at a transaction the KV store did not return (or an out-of-bounds
    /// event index) is an internal inconsistency, not an empty result.
    #[test]
    fn missing_contents_errors() {
        let scope = Scope::for_tests();
        let page = forward_page(10);
        let result = StreamPage::<v2::Event>::for_test(
            vec![ev_item(0, CursorToken::item(ev_position(1, 1, 0)))],
            None,
            None,
            Some(v2::QueryEndReason::CheckpointBound),
        );
        assert!(
            build_grpc_connection(scope.clone(), &page, result, &HashMap::new()).is_err(),
            "missing transaction contents should error"
        );

        let result = StreamPage::<v2::Event>::for_test(
            vec![ev_item(5, CursorToken::item(ev_position(1, 1, 5)))],
            None,
            None,
            Some(v2::QueryEndReason::CheckpointBound),
        );
        assert!(
            build_grpc_connection(scope, &page, result, &contents_for(2)).is_err(),
            "out-of-bounds event index should error"
        );
    }
}
