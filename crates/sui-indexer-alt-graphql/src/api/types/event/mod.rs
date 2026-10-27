// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::RangeInclusive;
use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::Context;
use async_graphql::Object;
use bytes::Bytes;
use prost_types::FieldMask;
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcReader;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2;
use sui_sdk_types::Event as SdkEvent;
use sui_types::base_types::SuiAddress as NativeSuiAddress;
use sui_types::digests::TransactionDigest;
use sui_types::event::Event as NativeEvent;
use tokio::sync::OnceCell;

use crate::api::scalars::base64::Base64;
use crate::api::scalars::cursor::ByteCursor;
use crate::api::scalars::cursor::OpaqueCursor;
use crate::api::scalars::date_time::DateTime;
use crate::api::scalars::uint53::UInt53;
use crate::api::types::address::Address;
use crate::api::types::checkpoint::filter::checkpoint_bounds;
use crate::api::types::event::filter::EventFilter;
use crate::api::types::ledger_cursor::Envelope;
use crate::api::types::ledger_cursor::EnvelopeRef;
use crate::api::types::ledger_cursor::EventKey;
use crate::api::types::ledger_cursor::OwnedCursor;
use crate::api::types::ledger_cursor::Point;
use crate::api::types::ledger_cursor::legacy::LegacyPosition;
use crate::api::types::ledger_cursor::legacy::LegacyToken;
use crate::api::types::ledger_cursor::scan::LedgerPage;
use crate::api::types::ledger_cursor::scan::RequestBound;
use crate::api::types::ledger_cursor::scan::ScanCursor;
use crate::api::types::ledger_cursor::scan::scan;
use crate::api::types::lookups::CheckpointBounds;
use crate::api::types::move_module::MoveModule;
use crate::api::types::move_package::MovePackage;
use crate::api::types::move_type::MoveType;
use crate::api::types::move_value::MoveValue;
use crate::api::types::transaction::Transaction;
use crate::error::RpcError;
use crate::extensions::query_limits;
use crate::pagination::Page;
use crate::pagination::StreamConnection;
use crate::scope::Scope;

pub(crate) mod filter;

/// A decoded event cursor: in GraphQL's own format, or in the legacy format (see
/// [`crate::api::types::ledger_cursor::legacy`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventToken {
    Owned(OwnedCursor<EventKey>),
    Legacy(LegacyEventToken),
}

/// An event cursor in the legacy format: the bytes as received, and the coordinates they hold.
/// Cursors at the same event are equal whatever their kind or checkpoint label.
#[derive(Clone, Debug)]
pub struct LegacyEventToken {
    bytes: Bytes,
    checkpoint: u64,
    tx_seq: u64,
    event_index: u32,
}

/// Compatibility dispatch over the on-wire cursor formats.
pub type CEvent = OpaqueCursor<EventToken>;

#[derive(Clone)]
pub(crate) struct Event {
    pub(crate) scope: Scope,
    /// Shared `Arc` so that multiple subscribers receiving events from the same streamed checkpoint
    /// avoid a deep clone per yield. Cloning an `Event` is then just an atomic refcount bump on the
    /// `native` field.
    pub(crate) native: Arc<NativeEvent>,
    /// Digest of the transaction that emitted this event.
    pub(crate) transaction_digest: TransactionDigest,
    /// Position of this event within the transaction's events list (0-indexed).
    pub(crate) sequence_number: u64,
    /// Timestamp of the checkpoint that includes the transaction containing this event. Seeded at
    /// construction when known (`None` when the transaction is not part of a checkpoint — simulated
    /// or just-executed transactions), or left empty.
    pub(crate) timestamp_ms: OnceCell<Option<u64>>,
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
            let timestamp_ms = self
                .timestamp_ms
                .get_or_try_init(async || {
                    let kv_loader: &KvLoader = ctx.data()?;
                    Ok::<_, RpcError>(
                        kv_loader
                            .load_one_transaction_timestamp(self.transaction_digest)
                            .await
                            .context("Failed to fetch transaction timestamp")?,
                    )
                })
                .await?;

            let Some(timestamp_ms) = *timestamp_ms else {
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

impl Event {
    /// Paginates events based on the provided filters and page parameters, served by the ledger
    /// gRPC List APIs. Pages may be partially filled, with valid cursors if there are more pages to
    /// paginate through.
    ///
    /// Returns empty results when no checkpoint is set in scope (e.g. execution scope).
    pub(crate) async fn paginate(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CEvent>,
        filter: EventFilter,
    ) -> Result<StreamConnection<Event>, RpcError> {
        query_limits::rich::debit(ctx)?;

        if page.limit() == 0 {
            return Ok(StreamConnection::empty());
        }

        // Consistency upper bound; empty when scope has no checkpoint set.
        let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
            return Ok(StreamConnection::empty());
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
            return Ok(StreamConnection::empty());
        };

        let reader: &LedgerGrpcReader = ctx.data()?;
        let result = Self::scan_grpc(
            reader,
            cp_bounds,
            &page,
            page.limit_with_lookahead(),
            &filter,
        )
        .await?;

        build_grpc_connection(scope, &page, result)
    }

    /// Scan up to `limit` events from `page`'s cursors over the checkpoint range `cp_bounds` via the
    /// streaming gRPC List API. Computing checkpoint bounds is the caller's responsibility. Items
    /// are returned in scan order (descending when paginating from the back).
    pub(crate) async fn scan_grpc(
        reader: &LedgerGrpcReader,
        cp_bounds: RangeInclusive<u64>,
        page: &Page<CEvent>,
        limit: usize,
        filter: &EventFilter,
    ) -> Result<LedgerPage<v2::Event, CEvent>, RpcError> {
        let filter = filter.to_grpc_filter()?;
        scan(page, cp_bounds, limit, event_key, |scan| async move {
            let mut request = v2::ListEventsRequest::default();
            // Everything the GraphQL node needs rides on the stream item: the event envelope and
            // its place (the timestamp resolves lazily via a timestamp-only KV lookup).
            request.read_mask = Some(FieldMask::from_paths([
                "contents",
                "event_type",
                "package_id",
                "module",
                "sender",
                "transaction_digest",
                "event_index",
                "transaction_index",
                "checkpoint",
            ]));
            request.start_checkpoint = Some(scan.start_checkpoint);
            request.end_checkpoint = Some(scan.end_checkpoint);
            request.filter = filter;
            request.options = Some(scan.options);

            reader
                .list_events(request)
                .await
                .context("Failed to list events")
        })
        .await
    }
}

impl EventToken {
    /// The cursor of the event at `key`, minted from data GraphQL holds.
    pub(crate) fn at(key: EventKey) -> CEvent {
        OpaqueCursor::new(Self::Owned(OwnedCursor::Point(Point::At(key))))
    }

    /// The cursor at the start of `checkpoint`: a page after it starts with the checkpoint's first
    /// event.
    pub(crate) fn checkpoint_start(checkpoint: u64) -> CEvent {
        OpaqueCursor::new(Self::Owned(OwnedCursor::Point(Point::checkpoint_start(
            checkpoint,
        ))))
    }

    /// Where GraphQL places this cursor: `None` for a legacy cursor, or a ledger page boundary
    /// that nothing GraphQL saw bounds.
    pub fn point(&self) -> Option<Point<EventKey>> {
        match self {
            Self::Owned(cursor) => cursor.point(),
            Self::Legacy(_) => None,
        }
    }

    /// The last checkpoint this cursor shows the client has seen any of: the one its place has
    /// reached, or a legacy cursor's checkpoint label. `None` if it shows none, as for a legacy
    /// cursor without a label.
    pub(crate) fn last_checkpoint(&self) -> Option<u64> {
        match self {
            Self::Owned(cursor) => cursor.point().and_then(|point| point.last_checkpoint()),
            Self::Legacy(legacy) => (legacy.checkpoint != 0).then_some(legacy.checkpoint),
        }
    }
}

impl ByteCursor for EventToken {
    fn decode_cursor(bytes: &[u8]) -> anyhow::Result<Self> {
        let Some(envelope) = Envelope::decode(bytes) else {
            return Ok(Self::Legacy(LegacyEventToken::decode_cursor(bytes)?));
        };

        match envelope? {
            Envelope::Event(cursor) => Ok(Self::Owned(cursor)),
            _ => anyhow::bail!("Not an event cursor"),
        }
    }

    fn encode_cursor(&self) -> Bytes {
        match self {
            Self::Owned(cursor) => EnvelopeRef::Event(cursor).encode(),
            Self::Legacy(legacy) => legacy.encode_cursor(),
        }
    }
}

impl ByteCursor for LegacyEventToken {
    fn decode_cursor(bytes: &[u8]) -> anyhow::Result<Self> {
        let token = LegacyToken::decode(bytes)?;
        let LegacyPosition::Events {
            checkpoint,
            tx_seq,
            event_index,
        } = token.position
        else {
            anyhow::bail!("Not an event cursor");
        };

        Ok(Self {
            bytes: Bytes::copy_from_slice(bytes),
            checkpoint,
            tx_seq,
            event_index,
        })
    }

    fn encode_cursor(&self) -> Bytes {
        self.bytes.clone()
    }
}

impl ScanCursor for CEvent {
    type Key = EventKey;

    fn bound(&self) -> RequestBound<EventKey> {
        match &**self {
            EventToken::Owned(cursor) => RequestBound {
                ledger: cursor.ledger_bytes().cloned(),
                point: cursor.point(),
            },

            EventToken::Legacy(legacy) => RequestBound {
                ledger: Some(legacy.bytes.clone()),
                point: None,
            },
        }
    }

    fn from_ledger(bytes: Bytes, point: Option<Point<EventKey>>) -> Self {
        OpaqueCursor::new(EventToken::Owned(OwnedCursor::Ledger { bytes, point }))
    }

    fn from_point(point: Point<EventKey>) -> Self {
        OpaqueCursor::new(EventToken::Owned(OwnedCursor::Point(point)))
    }
}

impl Eq for LegacyEventToken {}
impl PartialEq for LegacyEventToken {
    fn eq(&self, other: &Self) -> bool {
        (self.tx_seq, self.event_index) == (other.tx_seq, other.event_index)
    }
}

/// Where a `ListEvents` item sits: its transaction's checkpoint and index within it, and its index
/// among the transaction's events.
fn event_key(payload: &v2::Event) -> anyhow::Result<EventKey> {
    Ok(EventKey {
        checkpoint: payload
            .checkpoint
            .context("ListEvents item missing checkpoint")?,
        transaction_index: payload
            .transaction_index
            .context("ListEvents item missing transaction_index")?,
        event_index: payload
            .event_index
            .context("ListEvents item missing event_index")?,
    })
}

/// Hydrate an `Event` node from a `ListEvents` stream item. The read mask requests everything the
/// node needs — the event envelope and its position — so no KV lookup is required (the timestamp
/// resolves lazily); a missing field is an internal inconsistency.
pub(crate) fn event_from_stream_item(scope: Scope, payload: &v2::Event) -> Result<Event, RpcError> {
    // TODO: can we consolidate to using sui_sdk type? To explore, captured in DVX-2189
    let transaction_digest: TransactionDigest = payload
        .transaction_digest
        .as_deref()
        .context("ListEvents item missing transaction digest")?
        .parse()
        .context("Failed to parse transaction digest from ListEvents")?;

    let event_index = payload
        .event_index
        .context("ListEvents item missing event index")?;

    let native: NativeEvent = SdkEvent::try_from(payload)
        .context("Failed to convert ListEvents item")?
        .try_into()
        .context("Failed to convert event to native representation")?;

    Ok(Event {
        scope,
        native: Arc::new(native),
        transaction_digest,
        sequence_number: event_index as u64,
        timestamp_ms: OnceCell::new(),
    })
}

/// Build a `StreamConnection<Event>` from a page of a `ListEvents` stream, hydrating each edge's
/// event from the stream item itself.
///
/// Edges are returned in ascending order.
fn build_grpc_connection(
    scope: Scope,
    page: &Page<CEvent>,
    result: LedgerPage<v2::Event, CEvent>,
) -> Result<StreamConnection<Event>, RpcError> {
    page.paginate_stream_results(result, |payload| {
        event_from_stream_item(scope.clone(), payload)
    })
}

#[cfg(test)]
mod tests {
    use async_graphql::connection::CursorType;
    use fastcrypto::encoding::Base58;
    use fastcrypto::encoding::Base64 as B64;
    use fastcrypto::encoding::Encoding;
    use sui_indexer_alt_reader::ledger_grpc_reader::PageItem;
    use sui_indexer_alt_reader::ledger_grpc_reader::StreamPage;
    use sui_rpc::proto::sui::rpc::v2::QueryEndReason as End;
    use sui_types::base_types::ObjectID;
    use sui_types::parse_sui_struct_tag;

    use crate::api::types::ledger_cursor::legacy::LegacyKind;
    use crate::api::types::ledger_cursor::scan::ScanPlan;
    use crate::api::types::transaction::TransactionToken;
    use crate::pagination::PageLimits;

    use super::*;

    fn key(checkpoint: u64, transaction_index: u64, event_index: u32) -> EventKey {
        EventKey {
            checkpoint,
            transaction_index,
            event_index,
        }
    }

    /// A legacy protobuf event cursor.
    fn legacy_bytes(checkpoint: u64, tx_seq: u64, event_index: u32) -> Bytes {
        LegacyToken {
            kind: LegacyKind::Item,
            position: LegacyPosition::Events {
                checkpoint,
                tx_seq,
                event_index,
            },
        }
        .encode()
    }

    fn legacy_cursor(checkpoint: u64, tx_seq: u64, event_index: u32) -> CEvent {
        CEvent::decode_cursor(&B64::encode(legacy_bytes(checkpoint, tx_seq, event_index)))
            .expect("legacy cursor")
    }

    /// A `ListEvents` item at `key`, from the zero-digest transaction, carrying `cursor` as its
    /// ledger cursor.
    fn ev_item(cursor: &'static [u8], key: EventKey) -> PageItem<v2::Event> {
        let mut contents = v2::Bcs::default();
        contents.value = Some(Default::default());

        let mut payload = v2::Event::default();
        payload.transaction_digest = Some(Base58::encode(TransactionDigest::default().inner()));
        payload.checkpoint = Some(key.checkpoint);
        payload.transaction_index = Some(key.transaction_index);
        payload.event_index = Some(key.event_index);
        payload.package_id = Some(ObjectID::ZERO.to_canonical_string(true));
        payload.module = Some("m".to_string());
        payload.sender = Some(NativeSuiAddress::ZERO.to_string());
        payload.event_type = Some("0x0::m::T".to_string());
        payload.contents = Some(contents);
        PageItem {
            payload,
            cursor: Bytes::from_static(cursor),
        }
    }

    fn page(
        first: Option<u64>,
        after: Option<CEvent>,
        last: Option<u64>,
        before: Option<CEvent>,
    ) -> Page<CEvent> {
        let limits = PageLimits {
            default: 10,
            max: 100,
        };
        Page::from_params(&limits, first, after, last, before).expect("valid page")
    }

    fn connection(
        page: &Page<CEvent>,
        items: Vec<PageItem<v2::Event>>,
        end: End,
    ) -> Result<StreamConnection<Event>, RpcError> {
        let stream = StreamPage::for_test(items, None, None, Some(end));
        let result = ScanPlan::new(page, 0..=100).page(stream, event_key)?;
        build_grpc_connection(Scope::for_tests(), page, result)
    }

    /// The place of each edge, read back from its cursor.
    fn edge_keys(conn: &StreamConnection<Event>) -> Vec<EventKey> {
        conn.edges
            .iter()
            .map(|edge| {
                match CEvent::decode_cursor(&edge.cursor)
                    .expect("valid cursor")
                    .point()
                {
                    Some(Point::At(key)) => key,
                    point => panic!("expected an item cursor, got {point:?}"),
                }
            })
            .collect()
    }

    #[test]
    fn owned_cursor_roundtrips() {
        let cursor = EventToken::at(key(1, 2, 3));
        let decoded = CEvent::decode_cursor(&cursor.encode_cursor()).expect("valid cursor");
        assert_eq!(decoded.point(), Some(Point::At(key(1, 2, 3))));
    }

    /// A cursor for another type must not decode as an event cursor, in either format.
    #[test]
    fn rejects_other_cursor_types() {
        let legacy = LegacyToken {
            kind: LegacyKind::Item,
            position: LegacyPosition::Transactions {
                checkpoint: 1,
                tx_seq: 2,
            },
        };
        assert!(CEvent::decode_cursor(&B64::encode(legacy.encode())).is_err());

        let owned = TransactionToken::checkpoint_start(1).encode_cursor();
        assert!(CEvent::decode_cursor(&owned).is_err());
    }

    #[test]
    fn non_empty_page_uses_edge_cursors_and_hydrates_nodes() {
        let page = page(Some(10), None, None, None);
        let items = vec![
            ev_item(b"e0", key(1, 4, 0)),
            ev_item(b"e1", key(1, 4, 1)),
            ev_item(b"e2", key(1, 4, 2)),
        ];
        let conn = connection(&page, items, End::CheckpointBound).expect("connection built");
        assert_eq!(edge_keys(&conn), [key(1, 4, 0), key(1, 4, 1), key(1, 4, 2)]);
        // `CheckpointBound` means the range was exhausted: no forward continuation.
        assert!(!conn.page_info.has_next_page);
        assert_eq!(
            conn.page_info.start_cursor.as_deref(),
            Some(conn.edges[0].cursor.as_str())
        );
        assert_eq!(
            conn.page_info.end_cursor.as_deref(),
            Some(conn.edges[2].cursor.as_str())
        );

        // Nodes carry the hydrated position and envelope; timestamps resolve lazily.
        for (i, edge) in conn.edges.iter().enumerate() {
            assert_eq!(edge.node.sequence_number, i as u64);
            assert_eq!(edge.node.transaction_digest, TransactionDigest::default());
            assert_eq!(edge.node.native.package_id, ObjectID::ZERO);
            assert_eq!(
                edge.node.native.type_,
                parse_sui_struct_tag("0x0::m::T").unwrap()
            );
            assert!(edge.node.timestamp_ms.get().is_none());
        }
    }

    /// The read mask requests the full event envelope and its place, so an item missing one of
    /// them is an internal inconsistency, not an empty result.
    #[test]
    fn missing_payload_field_errors() {
        let page = page(Some(10), None, None, None);
        let missing: [fn(&mut v2::Event); 3] = [
            |event| event.contents = None,
            |event| event.sender = None,
            |event| event.transaction_index = None,
        ];

        for strip in missing {
            let mut item = ev_item(b"e0", key(1, 1, 0));
            strip(&mut item.payload);
            assert!(connection(&page, vec![item], End::CheckpointBound).is_err());
        }
    }

    /// A descending page is reversed into ascending edges.
    #[test]
    fn descending_page_reverses_to_ascending_edges() {
        let page = page(None, None, Some(10), None);
        let items = vec![
            ev_item(b"e2", key(1, 1, 2)),
            ev_item(b"e1", key(1, 1, 1)),
            ev_item(b"e0", key(1, 1, 0)),
        ];
        let conn = connection(&page, items, End::CheckpointBound).expect("connection built");
        assert_eq!(edge_keys(&conn), [key(1, 1, 0), key(1, 1, 1), key(1, 1, 2)]);
        assert_eq!(
            conn.edges
                .iter()
                .map(|e| e.node.sequence_number)
                .collect::<Vec<_>>(),
            [0, 1, 2],
        );
    }

    /// A live cursor with no ledger bytes places events within a transaction: the scan starts at
    /// its checkpoint and drops the events at or before it.
    #[test]
    fn point_cursor_drops_earlier_events() {
        let page = page(Some(10), Some(EventToken::at(key(3, 2, 1))), None, None);
        let items = vec![
            ev_item(b"a", key(3, 1, 5)),
            ev_item(b"b", key(3, 2, 1)),
            ev_item(b"c", key(3, 2, 2)),
            ev_item(b"d", key(3, 3, 0)),
        ];
        let conn = connection(&page, items, End::CheckpointBound).expect("connection built");
        assert_eq!(edge_keys(&conn), [key(3, 2, 2), key(3, 3, 0)]);
    }
}
