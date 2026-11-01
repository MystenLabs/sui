// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Resume property of the List endpoints: a client cut off after any frame
//! that resumes from the last cursor it received ends up with exactly the
//! items an uncut client pages through, in order, with nothing missing and
//! nothing duplicated. Checked for every endpoint, both orderings, and every
//! reachable terminal reason, on the handlers themselves over the mock
//! BigTable, cutting the first response of each scenario and, when it ends
//! on a limit, the second. Both cut and uncut paging must return exactly the
//! items the ledger was built with, within the request's bounds.
//!
//! Cursors also resume in the other direction: every cursor a paging returns,
//! sent back with the opposite ordering, yields exactly the items its position
//! and kind admit. These scenarios do not pace `ReadRows` (see below), so
//! their cursors come mostly from page ends rather than progress beacons.
//!
//! Every response is also checked against the ledger: each item frame's
//! cursor and the item's own checkpoint field name the checkpoint the ledger
//! put the item in, and each frame's checkpoint coverage claim is sound (every
//! item of the request in the claimed checkpoints has been delivered by that
//! frame or by earlier pages) and never moves backwards within the response.
//!
//! Filtered scans run over a sparse synthetic ledger: bitmap rows spread over
//! many buckets (so a small bucket budget ends in `SCAN_LIMIT`), and
//! `tx_seq_digest` rows at every position a watermark lookup can ask about.
//! There, every transaction's checkpoint is known, so the checkpoint of each
//! beacon and scan-limit cursor is checked too, as is the exact coverage
//! claim of a cursor bound deep in a stretch without matches. Unfiltered
//! scans assume every transaction has a row, so they run over a dense ledger
//! built from real checkpoints.
//!
//! In the cut-and-resume scenarios, every `ReadRows` returns its rows one
//! message at a time, `READ_LATENCY` apart, so scans advance row by row. The
//! bitmap-query evaluator paces progress watermarks by bucket rows charged to
//! the scan budget, and the filtered scenarios set that threshold to one
//! bucket: a watermark follows every bucket row charged without an item, so
//! progress beacons appear throughout the filtered streams and cuts after
//! beacons are exercised as well as cuts after items. Some filtered response
//! of each endpoint must carry at least two beacons at strictly advancing
//! positions, so beacons after the scan's first watermark reach the client.
//! The clock is real: a paused clock jumps to the next timer whenever the
//! runtime waits on the mock's socket, which bunches rows together and hides
//! the gaps between them.
//!
//! Every frame's `Watermark.cursor_position` is checked against the ledger as
//! well: an item frame's is the boundary right after its item ascending and
//! the item's own key descending; every item-free frame carries a Boundary
//! cursor and a position naming the boundary that cursor names. Positions
//! never move backwards within a response, and each frame's position splits
//! its paging's items into those delivered by that frame and those delivered
//! after it. Every cut also resumes as a position-aware client does: with the
//! frame's position as the request's start fields (ascending) or end fields
//! (descending), sent along with its cursor, under the same requirement of
//! nothing missing and nothing duplicated.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures::TryStreamExt;
use move_core_types::ident_str;
use mysten_common::ZipDebugEqIteratorExt;
use prometheus::Registry;
use roaring::RoaringBitmap;
use sui_inverted_index::IndexDimension;
use sui_inverted_index::encode_dimension_key;
use sui_inverted_index::event_seq;
use sui_kvstore::BigTableClient as InnerBigTableClient;
use sui_kvstore::tables;
use sui_kvstore::testing::MockBigtableServer;
use sui_kvstore::testing::insert_checkpoint_rows;
use sui_package_resolver::PackageStore;
use sui_package_resolver::Resolver;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_rpc_api::ledger_history::ledger_point::LedgerPoint;
use sui_rpc_cursor::CursorKind;
use sui_rpc_cursor::CursorToken;
use sui_rpc_cursor::Position;
use sui_types::balance_change::derive_balance_changes_2;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::digests::TransactionDigest;
use sui_types::event::Event;
use sui_types::gas_coin::GAS;
use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

use super::list_checkpoints::list_checkpoints;
use super::list_events::list_events;
use super::list_transactions::list_transactions;
use crate::KvRpcMetrics;
use crate::LedgerHistoryConfig;
use crate::LedgerHistoryMethodConfig;
use crate::StagesConfig;
use crate::bigtable_client::BigTableClient;
use crate::operation::QueryContext;
use crate::package_store::BigTablePackageStore;

/// The last 100,000 transactions match nothing, so scans of either bitmap
/// index cross whole buckets without items at one end of the ledger.
const SPARSE_CHECKPOINTS: u64 = 40;
const SPARSE_TXS_PER_CHECKPOINT: u64 = 10_000;
/// Matching transactions of the sparse ledger: both sides of event-bucket
/// (4,096), tx-bucket (65,536), and checkpoint (10,000) edges, small clusters,
/// and lone matches far apart.
const SPARSE_MATCHES: &[u64] = &[
    12, 13, 4_095, 4_096, 9_999, 10_000, 25_000, 65_535, 65_536, 70_000, 131_071, 131_072, 140_000,
    200_000, 262_143, 262_144, 270_000, 299_999,
];

/// A far cursor bound of the sparse ledger inside checkpoint 19, between the
/// matches at 140,000 and 200,000.
const MID_CHECKPOINT_BOUND_TX: u64 = 195_000;

const DENSE_CHECKPOINTS: u64 = 8;
/// Transactions per checkpoint of the dense ledger.
const DENSE_CHECKPOINT_TXS: [u64; DENSE_CHECKPOINTS as usize] = [1, 2, 3, 1, 2, 3, 1, 2];
/// Events per transaction of the dense ledger, in ledger order. Both ends
/// have few events, so the unfiltered event scan's per-request transaction
/// cap stops it before its item limit in either direction.
const DENSE_EVENTS: [usize; 15] = [0, 0, 2, 1, 0, 1, 2, 0, 1, 0, 2, 0, 1, 0, 0];

const EVENT_BUCKET: u64 = tables::event_bitmap_index::BUCKET_SIZE;
const TX_BUCKET: u64 = tables::transaction_bitmap_index::BUCKET_SIZE;
/// Delay before each row of every mock `ReadRows` response.
const READ_LATENCY: Duration = Duration::from_millis(1);

fn match_sender() -> SuiAddress {
    TestCheckpointBuilder::derive_address(7)
}

/// Two literals whose rows cover every bucket of the sparse ledger but
/// intersect only at its matches: scans filtered by both walk the ledger
/// bucket by bucket through long stretches without items.
fn and_senders() -> [SuiAddress; 2] {
    [
        TestCheckpointBuilder::derive_address(8),
        TestCheckpointBuilder::derive_address(9),
    ]
}

fn match_event() -> Event {
    Event::new(
        &ObjectID::ZERO,
        ident_str!("resume"),
        match_sender(),
        GAS::type_(),
        vec![],
    )
}

/// Events emitted by the `i`-th matching transaction of the sparse ledger.
fn sparse_event_count(i: usize) -> usize {
    1 + i % 2
}

fn serialized(bitmap: &RoaringBitmap) -> Bytes {
    let mut bytes = Vec::with_capacity(bitmap.serialized_size());
    bitmap.serialize_into(&mut bytes).unwrap();
    Bytes::from(bytes)
}

async fn sparse_ledger(mock: &MockBigtableServer) {
    // Distinct real transactions to serve as the matching rows.
    let mut builder = TestCheckpointBuilder::new(0);
    for i in 0..SPARSE_MATCHES.len() {
        builder = builder
            .start_transaction((i % 4) as u8)
            .with_events(vec![match_event(); sparse_event_count(i)])
            .finish_transaction();
    }
    let source = builder.build_checkpoint();

    for checkpoint in 0..SPARSE_CHECKPOINTS {
        let mut summary = source.summary.data().clone();
        summary.sequence_number = checkpoint;
        summary.network_total_transactions = (checkpoint + 1) * SPARSE_TXS_PER_CHECKPOINT;
        summary.timestamp_ms = 1_000 * checkpoint;
        let row =
            tables::checkpoints::encode(&summary, source.summary.auth_sig(), &source.contents)
                .unwrap();
        mock.insert_row(
            tables::checkpoints::NAME,
            tables::checkpoints::encode_key(checkpoint),
            row,
        )
        .await;
    }

    // Watermark lookups read the checkpoint of a frontier's neighboring
    // transaction. Frontiers sit on bucket and checkpoint edges, and on the
    // neighbors of matches when a cursor bounds the scan.
    let total_txs = SPARSE_CHECKPOINTS * SPARSE_TXS_PER_CHECKPOINT;
    let txs_per_event_bucket = EVENT_BUCKET >> event_seq::EVENT_BITS;
    let mut lookup_positions = BTreeSet::new();
    let edges = (0..=total_txs)
        .step_by(txs_per_event_bucket as usize)
        .chain((0..=total_txs).step_by(SPARSE_TXS_PER_CHECKPOINT as usize));
    for edge in edges {
        lookup_positions.extend([edge.saturating_sub(1), edge]);
    }
    for &tx in SPARSE_MATCHES {
        lookup_positions.extend([tx.saturating_sub(1), tx + 1]);
    }
    // A scan watermark clamped to the far cursor bound inside a checkpoint is
    // placed by the transaction on its covered side, on either side of the bound.
    lookup_positions.extend([MID_CHECKPOINT_BOUND_TX - 1, MID_CHECKPOINT_BOUND_TX]);
    let placeholder = TransactionDigest::new([0xEE; 32]);
    for tx in lookup_positions {
        if tx >= total_txs || SPARSE_MATCHES.contains(&tx) {
            continue;
        }
        let row = tables::tx_seq_digest::encode(
            &placeholder,
            0,
            (tx % SPARSE_TXS_PER_CHECKPOINT) as u32,
            tx / SPARSE_TXS_PER_CHECKPOINT,
        );
        mock.insert_row(
            tables::tx_seq_digest::NAME,
            tables::tx_seq_digest::encode_key(tx),
            row,
        )
        .await;
    }

    let mut tx_bitmaps: BTreeMap<u64, RoaringBitmap> = BTreeMap::new();
    let mut event_bitmaps: BTreeMap<u64, RoaringBitmap> = BTreeMap::new();
    for (i, &tx_seq) in SPARSE_MATCHES.iter().enumerate() {
        let tx = &source.transactions[i];
        let digest = tx.transaction.digest();
        let checkpoint = tx_seq / SPARSE_TXS_PER_CHECKPOINT;
        let event_count = sparse_event_count(i) as u32;
        let row = tables::tx_seq_digest::encode(
            &digest,
            event_count,
            (tx_seq % SPARSE_TXS_PER_CHECKPOINT) as u32,
            checkpoint,
        );
        mock.insert_row(
            tables::tx_seq_digest::NAME,
            tables::tx_seq_digest::encode_key(tx_seq),
            row,
        )
        .await;
        let row = tables::transactions::encode(
            &tx.transaction,
            &tx.signatures,
            &tx.effects,
            &tx.events,
            checkpoint,
            1_000 * checkpoint,
            &derive_balance_changes_2(&tx.effects, &source.object_set),
            &tx.unchanged_loaded_runtime_objects,
        )
        .unwrap();
        mock.insert_row(
            tables::transactions::NAME,
            tables::transactions::encode_key(&digest),
            row,
        )
        .await;

        tx_bitmaps
            .entry(tx_seq / TX_BUCKET)
            .or_default()
            .insert((tx_seq % TX_BUCKET) as u32);
        for index in 0..event_count {
            let packed = event_seq::encode_event_seq(tx_seq, index);
            event_bitmaps
                .entry(packed / EVENT_BUCKET)
                .or_default()
                .insert((packed % EVENT_BUCKET) as u32);
        }
    }
    write_bitmaps(mock, match_sender(), &tx_bitmaps, &event_bitmaps).await;

    // Each AND literal adds one non-matching transaction (event index 0) to
    // every bucket, at an offset the other literal never uses.
    for (sender, offset) in and_senders().into_iter().zip_debug_eq([1_000, 2_000]) {
        let mut tx_bitmaps = tx_bitmaps.clone();
        let mut event_bitmaps = event_bitmaps.clone();
        let tx_bucket_starts = (0..total_txs).step_by(TX_BUCKET as usize);
        for tx_seq in tx_bucket_starts.map(|start| start + offset) {
            if tx_seq >= total_txs {
                continue;
            }
            assert!(!SPARSE_MATCHES.contains(&tx_seq));
            tx_bitmaps
                .entry(tx_seq / TX_BUCKET)
                .or_default()
                .insert((tx_seq % TX_BUCKET) as u32);
        }
        let event_bucket_starts = (0..total_txs).step_by(txs_per_event_bucket as usize);
        for tx_seq in event_bucket_starts.map(|start| start + offset) {
            if tx_seq >= total_txs {
                continue;
            }
            assert!(!SPARSE_MATCHES.contains(&tx_seq));
            let packed = event_seq::encode_event_seq(tx_seq, 0);
            event_bitmaps
                .entry(packed / EVENT_BUCKET)
                .or_default()
                .insert((packed % EVENT_BUCKET) as u32);
        }
        write_bitmaps(mock, sender, &tx_bitmaps, &event_bitmaps).await;
    }
}

/// Bitmap index rows for one sender dimension, in both indexes.
async fn write_bitmaps(
    mock: &MockBigtableServer,
    sender: SuiAddress,
    tx_bitmaps: &BTreeMap<u64, RoaringBitmap>,
    event_bitmaps: &BTreeMap<u64, RoaringBitmap>,
) {
    let dimension_key = encode_dimension_key(IndexDimension::Sender, sender.as_ref());
    for (bucket, bitmap) in tx_bitmaps {
        let key = tables::transaction_bitmap_index::encode_row_key(
            tables::transaction_bitmap_index::SCHEMA_VERSION,
            &dimension_key,
            *bucket,
        );
        mock.insert_row(
            tables::transaction_bitmap_index::NAME,
            key,
            [(
                tables::transaction_bitmap_index::col::BITMAP,
                serialized(bitmap),
            )],
        )
        .await;
    }
    for (bucket, bitmap) in event_bitmaps {
        let key = tables::event_bitmap_index::encode_row_key(
            tables::event_bitmap_index::SCHEMA_VERSION,
            &dimension_key,
            *bucket,
        );
        mock.insert_row(
            tables::event_bitmap_index::NAME,
            key,
            [(tables::event_bitmap_index::col::BITMAP, serialized(bitmap))],
        )
        .await;
    }
}

/// Real checkpoints of one to three transactions, emitting zero to two
/// events each. The builder must record the checkpoints `dense_checkpoint`
/// assigns the transactions to, since item checkpoints are checked against it.
async fn dense_ledger(mock: &MockBigtableServer) {
    let mut builder = TestCheckpointBuilder::new(0);
    let mut events = DENSE_EVENTS.iter();
    let mut network_total_transactions = 0;
    for (sequence_number, tx_count) in DENSE_CHECKPOINT_TXS.into_iter().enumerate() {
        for tx in 0..tx_count {
            builder = builder.start_transaction(tx as u8);
            let count = *events.next().expect("one event count per transaction");
            if count > 0 {
                builder = builder.with_events(vec![match_event(); count]);
            }
            builder = builder.finish_transaction();
        }
        let checkpoint = builder.build_checkpoint();
        network_total_transactions += tx_count;
        let summary = checkpoint.summary.data();
        assert_eq!(summary.sequence_number, sequence_number as u64);
        assert_eq!(
            summary.network_total_transactions,
            network_total_transactions
        );
        for tx in network_total_transactions - tx_count..network_total_transactions {
            assert_eq!(dense_checkpoint(tx), summary.sequence_number);
        }
        insert_checkpoint_rows(mock, &checkpoint).await;
    }
}

struct Limits {
    bucket_budget: u64,
    max_limit_items: u32,
    /// Scan work, in bucket rows, without an item or beacon before the
    /// evaluator emits its newest watermark. One emits a watermark after
    /// every such bucket row, so progress beacons appear throughout the
    /// streams and cuts after them are exercised.
    beacon_buckets: NonZeroU64,
}

async fn context(
    mock: &MockBigtableServer,
    checkpoint_hi_exclusive: u64,
    method: &'static str,
    limits: Limits,
) -> (QueryContext, tokio::task::JoinHandle<()>) {
    let (addr, server) = mock.start().await.expect("start mock BigTable");
    let inner = InnerBigTableClient::new_local(addr.to_string(), "test".to_string())
        .await
        .expect("connect to mock BigTable");
    let registry = Registry::new();
    let metrics = KvRpcMetrics::new(&registry);
    let client = BigTableClient::new(inner.clone(), 50, metrics.bigtable_limiter.clone(), method);
    let package_store: Arc<dyn PackageStore> = Arc::new(BigTablePackageStore::new(inner));
    let method_config = LedgerHistoryMethodConfig {
        timeout_ms: None,
        default_limit_items: Some(limits.max_limit_items),
        max_limit_items: Some(limits.max_limit_items),
        render_ahead: Some(2),
    };
    let ledger_history = LedgerHistoryConfig {
        list_transactions: Some(method_config.clone()),
        list_events: Some(method_config.clone()),
        list_checkpoints: Some(method_config),
        bitmap_bucket_budget_tx: Some(limits.bucket_budget),
        bitmap_bucket_budget_event: Some(limits.bucket_budget),
        bitmap_drain_probe_rows: None,
        watermark_beacon_buckets: Some(limits.beacon_buckets),
        max_bitmap_filter_literals: Some(2),
    };
    let ctx = QueryContext::new(
        client,
        Arc::new(Resolver::new(package_store)),
        metrics,
        method,
        checkpoint_hi_exclusive,
        ledger_history,
        StagesConfig::default(),
    );
    (ctx, server)
}

#[derive(Clone, Copy, Debug)]
enum Endpoint {
    Events,
    Transactions,
    Checkpoints,
}

impl Endpoint {
    fn method(self) -> &'static str {
        match self {
            Self::Events => "list_events",
            Self::Transactions => "list_transactions",
            Self::Checkpoints => "list_checkpoints",
        }
    }
}

/// The filter of a request: none (dense ledger), the matching sender, or
/// the two AND-ed senders that intersect only at the matches (sparse ledger).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Filter {
    None,
    Sender,
    AndSenders,
}

impl Filter {
    fn senders(self) -> Vec<SuiAddress> {
        match self {
            Self::None => vec![],
            Self::Sender => vec![match_sender()],
            Self::AndSenders => and_senders().to_vec(),
        }
    }
}

#[derive(Clone, Debug)]
struct Request {
    endpoint: Endpoint,
    filter: Filter,
    ascending: bool,
    start_checkpoint: Option<u64>,
    end_checkpoint: Option<u64>,
    limit: u32,
    /// The cursor bound on the far side of the scan: `before` ascending,
    /// `after` descending. The near side carries the resume cursor.
    far_bound: Option<Bytes>,
}

/// One response frame: the scan position of its item (if it carries one),
/// its cursor, its `cursor_position`, its checkpoint coverage claim, and its
/// terminal reason (if it ends the stream).
#[derive(Clone, Debug)]
struct Frame {
    item: Option<(u64, u32)>,
    cursor: Bytes,
    /// `Watermark.cursor_position`, an unset index counting as 0.
    position: Option<LedgerPoint>,
    covered: Option<u64>,
    end: Option<QueryEndReason>,
}

/// How a client continues after a frame: with its cursor alone, or as a
/// position-aware client does, with its `cursor_position` as the request's
/// start fields (ascending) or end fields (descending) along with its cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResumeMode {
    Cursor,
    Position,
}

/// Where a request continues from: a cursor, sent as `after` ascending and
/// `before` descending, and a position for its start fields (ascending) or
/// end fields (descending).
#[derive(Clone, Debug, Default)]
struct Resume {
    cursor: Option<Bytes>,
    position: Option<LedgerPoint>,
}

impl Resume {
    fn from_cursor(cursor: Bytes) -> Self {
        Self {
            cursor: Some(cursor),
            position: None,
        }
    }

    /// Continue after `frame` in `mode`.
    fn after(frame: &Frame, mode: ResumeMode) -> Self {
        let position = match mode {
            ResumeMode::Cursor => None,
            ResumeMode::Position => Some(
                frame
                    .position
                    .expect("a frame to resume from carries a cursor_position"),
            ),
        };
        Self {
            cursor: Some(frame.cursor.clone()),
            position,
        }
    }
}

/// The checkpoint and index fields of a request.
#[derive(Clone, Copy, Debug, Default)]
struct RequestFields {
    start_checkpoint: Option<u64>,
    start_transaction_index: Option<u64>,
    start_event_index: Option<u32>,
    end_checkpoint: Option<u64>,
    end_transaction_index: Option<u64>,
    end_event_index: Option<u32>,
}

/// The fields of `request` continued at `position`, mapped to request fields
/// as the ledger position contract says: ascending, `position` is the start;
/// descending, the end, a whole checkpoint's end as `end_checkpoint` alone and
/// any other as indexes into the checkpoint before `end_checkpoint`.
fn request_fields(request: &Request, position: Option<LedgerPoint>) -> RequestFields {
    let mut fields = RequestFields {
        start_checkpoint: request.start_checkpoint,
        end_checkpoint: request.end_checkpoint,
        ..RequestFields::default()
    };
    let Some(LedgerPoint {
        checkpoint,
        transaction_index,
        event_index,
    }) = position
    else {
        return fields;
    };
    let checkpoint_start = transaction_index == 0 && event_index == 0;
    if request.ascending {
        fields.start_checkpoint = Some(checkpoint);
        if !checkpoint_start {
            fields.start_transaction_index = Some(transaction_index);
            fields.start_event_index = (event_index != 0).then_some(event_index);
        }
    } else if checkpoint_start {
        fields.end_checkpoint = Some(checkpoint);
    } else {
        fields.end_checkpoint = Some(checkpoint + 1);
        fields.end_transaction_index = Some(transaction_index);
        fields.end_event_index = (event_index != 0).then_some(event_index);
    }
    fields
}

fn options(request: &Request, resume: Option<&Resume>) -> proto::QueryOptions {
    let cursor = resume.and_then(|resume| resume.cursor.clone());
    let mut options = proto::QueryOptions::default();
    options.limit = Some(request.limit);
    if request.ascending {
        options.ordering = Some(proto::Ordering::Ascending as i32);
        options.after = cursor;
        options.before = request.far_bound.clone();
    } else {
        options.ordering = Some(proto::Ordering::Descending as i32);
        options.before = cursor;
        options.after = request.far_bound.clone();
    }
    options
}

fn frame(
    endpoint: Endpoint,
    item: Option<Position>,
    watermark: Option<proto::Watermark>,
    end: Option<proto::QueryEnd>,
) -> Frame {
    let covered = watermark
        .as_ref()
        .and_then(|watermark| watermark.checkpoint);
    let position = watermark
        .as_ref()
        .and_then(|watermark| watermark.cursor_position.as_ref())
        .map(|position| ledger_point(endpoint, position));
    let cursor = watermark
        .and_then(|watermark| watermark.cursor)
        .expect("every frame carries a resume cursor");
    let item = item.map(scan_key);
    Frame {
        item,
        cursor,
        position,
        covered,
        end: end.map(|end| end.reason()),
    }
}

/// A `cursor_position` in ledger coordinates, an unset index counting as 0.
/// It sets exactly the fields of the keys `endpoint` lists.
fn ledger_point(endpoint: Endpoint, position: &proto::LedgerPosition) -> LedgerPoint {
    let key_fields = match endpoint {
        Endpoint::Checkpoints => (true, false, false),
        Endpoint::Transactions => (true, true, false),
        Endpoint::Events => (true, true, true),
    };
    assert_eq!(
        (
            position.checkpoint.is_some(),
            position.transaction_index.is_some(),
            position.event_index.is_some()
        ),
        key_fields,
        "fields of {endpoint:?} position {position:?}"
    );
    LedgerPoint::event(
        position.checkpoint.unwrap_or(0),
        position.transaction_index.unwrap_or(0),
        position.event_index.unwrap_or(0),
    )
}

/// The scan-order key of a position: `(transaction, event index)` for
/// events, `(transaction, 0)` for transactions, `(checkpoint, 0)` for
/// checkpoints. Keys grow in ascending scan order.
fn scan_key(position: Position) -> (u64, u32) {
    match position {
        Position::Events {
            tx_seq,
            event_index,
            ..
        } => (tx_seq, event_index),
        Position::Transactions { tx_seq, .. } => (tx_seq, 0),
        Position::Checkpoints { checkpoint } => (checkpoint, 0),
    }
}

/// Check an item frame against the ledger: the item is one the ledger holds
/// for the request's filter, and both its cursor and its own checkpoint field
/// name the checkpoint the ledger put it in.
fn assert_ledger_checkpoint(
    ledger: &BTreeMap<(u64, u32), u64>,
    position: Position,
    item_checkpoint: Option<u64>,
    request: &Request,
) {
    let key = scan_key(position);
    let checkpoint = *ledger
        .get(&key)
        .unwrap_or_else(|| panic!("item {key:?} is not in the ledger: {request:?}"));
    assert_eq!(
        position.checkpoint(),
        checkpoint,
        "cursor checkpoint of item {key:?}: {request:?}"
    );
    assert_eq!(
        item_checkpoint,
        Some(checkpoint),
        "checkpoint field of item {key:?}: {request:?}"
    );
}

/// The position named by an item frame's own cursor.
fn item_position(watermark: Option<&proto::Watermark>) -> Position {
    let cursor = watermark
        .and_then(|watermark| watermark.cursor.as_ref())
        .expect("item frame carries a cursor");
    CursorToken::decode(cursor)
        .expect("item cursor decodes")
        .position
}

fn transaction_filter(filter: Filter) -> Option<proto::TransactionFilter> {
    let literals: Vec<_> = filter
        .senders()
        .into_iter()
        .map(|address| {
            let mut sender = proto::SenderFilter::default();
            sender.address = Some(address.to_string());
            let mut literal = proto::TransactionLiteral::default();
            literal.predicate = Some(proto::transaction_literal::Predicate::Sender(sender));
            literal
        })
        .collect();
    (!literals.is_empty()).then(|| {
        let mut term = proto::TransactionTerm::default();
        term.literals = literals;
        let mut filter = proto::TransactionFilter::default();
        filter.terms = vec![term];
        filter
    })
}

fn event_filter(filter: Filter) -> Option<proto::EventFilter> {
    let literals: Vec<_> = filter
        .senders()
        .into_iter()
        .map(|address| {
            let mut sender = proto::SenderFilter::default();
            sender.address = Some(address.to_string());
            let mut literal = proto::EventLiteral::default();
            literal.predicate = Some(proto::event_literal::Predicate::Sender(sender));
            literal
        })
        .collect();
    (!literals.is_empty()).then(|| {
        let mut term = proto::EventTerm::default();
        term.literals = literals;
        let mut filter = proto::EventFilter::default();
        filter.terms = vec![term];
        filter
    })
}

/// Run one request to the end of its stream, continuing from `resume`, and
/// check every item frame's checkpoints against the ledger.
async fn fetch(ctx: &QueryContext, request: &Request, resume: Option<&Resume>) -> Vec<Frame> {
    let options = Some(options(request, resume));
    let fields = request_fields(request, resume.and_then(|resume| resume.position));
    let ledger = ledger_checkpoints(request);
    let endpoint = request.endpoint;
    match endpoint {
        Endpoint::Events => {
            let mut proto_request = proto::ListEventsRequest::default();
            proto_request.start_checkpoint = fields.start_checkpoint;
            proto_request.start_transaction_index = fields.start_transaction_index;
            proto_request.start_event_index = fields.start_event_index;
            proto_request.end_checkpoint = fields.end_checkpoint;
            proto_request.end_transaction_index = fields.end_transaction_index;
            proto_request.end_event_index = fields.end_event_index;
            proto_request.options = options;
            proto_request.read_mask = Some(FieldMask::from_paths([
                "checkpoint",
                "transaction_digest",
                "event_index",
            ]));
            proto_request.filter = event_filter(request.filter);
            let responses: Vec<_> = list_events(ctx.clone(), proto_request)
                .await
                .expect("request accepted")
                .try_collect()
                .await
                .expect("stream completes");
            responses
                .into_iter()
                .map(|response| {
                    let item = response.event.as_ref().map(|event| {
                        let position = item_position(response.watermark.as_ref());
                        let Position::Events { event_index, .. } = position else {
                            panic!("event cursor has an event position");
                        };
                        assert_ledger_checkpoint(&ledger, position, event.checkpoint, request);
                        assert_eq!(event.event_index, Some(event_index));
                        position
                    });
                    frame(endpoint, item, response.watermark, response.end)
                })
                .collect()
        }
        Endpoint::Transactions => {
            let mut proto_request = proto::ListTransactionsRequest::default();
            proto_request.start_checkpoint = fields.start_checkpoint;
            proto_request.start_transaction_index = fields.start_transaction_index;
            proto_request.end_checkpoint = fields.end_checkpoint;
            proto_request.end_transaction_index = fields.end_transaction_index;
            proto_request.options = options;
            proto_request.read_mask = Some(FieldMask::from_paths(["digest", "checkpoint"]));
            proto_request.filter = transaction_filter(request.filter);
            let responses: Vec<_> = list_transactions(ctx.clone(), proto_request)
                .await
                .expect("request accepted")
                .try_collect()
                .await
                .expect("stream completes");
            responses
                .into_iter()
                .map(|response| {
                    let item = response.transaction.as_ref().map(|transaction| {
                        let position = item_position(response.watermark.as_ref());
                        assert_ledger_checkpoint(
                            &ledger,
                            position,
                            transaction.checkpoint,
                            request,
                        );
                        position
                    });
                    frame(endpoint, item, response.watermark, response.end)
                })
                .collect()
        }
        Endpoint::Checkpoints => {
            let mut proto_request = proto::ListCheckpointsRequest::default();
            proto_request.start_checkpoint = fields.start_checkpoint;
            proto_request.end_checkpoint = fields.end_checkpoint;
            proto_request.options = options;
            proto_request.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
            proto_request.filter = transaction_filter(request.filter);
            let responses: Vec<_> = list_checkpoints(ctx.clone(), proto_request)
                .await
                .expect("request accepted")
                .try_collect()
                .await
                .expect("stream completes");
            responses
                .into_iter()
                .map(|response| {
                    let item = response.checkpoint.as_ref().map(|checkpoint| {
                        let position = item_position(response.watermark.as_ref());
                        assert_ledger_checkpoint(
                            &ledger,
                            position,
                            checkpoint.sequence_number,
                            request,
                        );
                        position
                    });
                    frame(endpoint, item, response.watermark, response.end)
                })
                .collect()
        }
    }
}

/// [`fetch`] one response, and check the `cursor_position` of each of its
/// frames (see [`assert_response_positions`]).
async fn run(ctx: &QueryContext, request: &Request, resume: Option<&Resume>) -> Vec<Frame> {
    let frames = fetch(ctx, request, resume).await;
    assert_response_positions(request, &frames);
    frames
}

fn items(frames: &[Frame]) -> Vec<(u64, u32)> {
    frames.iter().filter_map(|frame| frame.item).collect()
}

/// Checkpoint of dense-ledger transaction `tx`, and its index in that
/// checkpoint.
fn dense_place(tx: u64) -> (u64, u64) {
    let mut first_tx = 0;
    for (checkpoint, tx_count) in DENSE_CHECKPOINT_TXS.iter().enumerate() {
        if tx < first_tx + tx_count {
            return (checkpoint as u64, tx - first_tx);
        }
        first_tx += tx_count;
    }
    panic!("transaction {tx} is past the dense ledger");
}

/// Checkpoint of dense-ledger transaction `tx`.
fn dense_checkpoint(tx: u64) -> u64 {
    dense_place(tx).0
}

/// Every item a request with `filter` can return, known from how the ledgers
/// were built, in ascending scan order, each with its checkpoint.
fn ledger_items(endpoint: Endpoint, filter: Filter) -> Vec<((u64, u32), u64)> {
    // (transaction, event count, checkpoint)
    let txs: Vec<(u64, u32, u64)> = if filter == Filter::None {
        let tx_count: u64 = DENSE_CHECKPOINT_TXS.iter().sum();
        (0..tx_count)
            .map(|tx| (tx, DENSE_EVENTS[tx as usize] as u32, dense_checkpoint(tx)))
            .collect()
    } else {
        SPARSE_MATCHES
            .iter()
            .enumerate()
            .map(|(i, &tx)| {
                let checkpoint = tx / SPARSE_TXS_PER_CHECKPOINT;
                (tx, sparse_event_count(i) as u32, checkpoint)
            })
            .collect()
    };
    match endpoint {
        Endpoint::Events => txs
            .iter()
            .flat_map(|&(tx, events, checkpoint)| {
                (0..events).map(move |index| ((tx, index), checkpoint))
            })
            .collect(),
        Endpoint::Transactions => txs
            .iter()
            .map(|&(tx, _, checkpoint)| ((tx, 0), checkpoint))
            .collect(),
        Endpoint::Checkpoints => {
            let checkpoints: BTreeSet<u64> = txs.iter().map(|&(_, _, cp)| cp).collect();
            checkpoints.into_iter().map(|cp| ((cp, 0), cp)).collect()
        }
    }
}

/// The ledger checkpoint of every item `request` can return.
fn ledger_checkpoints(request: &Request) -> BTreeMap<(u64, u32), u64> {
    ledger_items(request.endpoint, request.filter)
        .into_iter()
        .collect()
}

/// The items paging through `request` must yield, in scan order, ignoring
/// its far cursor bound.
fn expected_items(request: &Request, checkpoint_hi_exclusive: u64) -> Vec<(u64, u32)> {
    let start = request.start_checkpoint.unwrap_or(0);
    let end = request
        .end_checkpoint
        .map_or(checkpoint_hi_exclusive, |end| {
            end.min(checkpoint_hi_exclusive)
        });
    let mut items: Vec<_> = ledger_items(request.endpoint, request.filter)
        .into_iter()
        .filter(|&(_, checkpoint)| start <= checkpoint && checkpoint < end)
        .map(|(position, _)| position)
        .collect();
    if !request.ascending {
        items.reverse();
    }
    items
}

/// The checkpoint coordinate a boundary cursor at `position` carries on the
/// sparse ledger: the checkpoint of the last transaction the scan covered,
/// plus one descending, where a boundary's checkpoint is an exclusive upper
/// bound. `None` for checkpoint positions, which this does not model.
fn sparse_boundary_checkpoint(position: Position, ascending: bool) -> Option<u64> {
    let last_covered_tx = match position {
        Position::Events {
            tx_seq,
            event_index,
            ..
        } if ascending && event_index > 0 => Some(tx_seq),
        Position::Events { tx_seq, .. } | Position::Transactions { tx_seq, .. } => {
            if ascending {
                tx_seq.checked_sub(1)
            } else {
                Some(tx_seq)
            }
        }
        Position::Checkpoints { .. } => return None,
    };
    Some(match last_covered_tx {
        Some(tx) if ascending => tx / SPARSE_TXS_PER_CHECKPOINT,
        Some(tx) => tx / SPARSE_TXS_PER_CHECKPOINT + 1,
        None => 0,
    })
}

/// The number of checkpoints of the ledger a request with `filter` runs over.
fn ledger_checkpoint_count(filter: Filter) -> u64 {
    if filter == Filter::None {
        DENSE_CHECKPOINTS
    } else {
        SPARSE_CHECKPOINTS
    }
}

/// The number of transactions of `checkpoint` in the ledger a request with
/// `filter` runs over.
fn checkpoint_transaction_count(filter: Filter, checkpoint: u64) -> u64 {
    if filter == Filter::None {
        DENSE_CHECKPOINT_TXS[checkpoint as usize]
    } else {
        SPARSE_TXS_PER_CHECKPOINT
    }
}

/// The number of transactions of the ledger a request with `filter` runs over.
fn ledger_transaction_count(filter: Filter) -> u64 {
    (0..ledger_checkpoint_count(filter))
        .map(|checkpoint| checkpoint_transaction_count(filter, checkpoint))
        .sum()
}

/// The checkpoint of transaction `tx`, and its index in that checkpoint, in
/// the ledger a request with `filter` runs over.
fn transaction_place(filter: Filter, tx: u64) -> (u64, u64) {
    if filter == Filter::None {
        dense_place(tx)
    } else {
        (
            tx / SPARSE_TXS_PER_CHECKPOINT,
            tx % SPARSE_TXS_PER_CHECKPOINT,
        )
    }
}

/// The key in ledger coordinates of an item of `request`, given by its
/// `scan_key`.
fn item_key(request: &Request, item: (u64, u32)) -> LedgerPoint {
    match request.endpoint {
        Endpoint::Checkpoints => LedgerPoint::checkpoint_start(item.0),
        Endpoint::Transactions | Endpoint::Events => {
            let (checkpoint, transaction_index) = transaction_place(request.filter, item.0);
            LedgerPoint::event(checkpoint, transaction_index, item.1)
        }
    }
}

/// The boundary right after the item keyed `key`: the key with its finest
/// index incremented.
fn above(endpoint: Endpoint, key: LedgerPoint) -> LedgerPoint {
    match endpoint {
        Endpoint::Checkpoints => LedgerPoint::checkpoint_start(key.checkpoint + 1),
        Endpoint::Transactions => {
            LedgerPoint::transaction(key.checkpoint, key.transaction_index + 1)
        }
        Endpoint::Events => {
            LedgerPoint::event(key.checkpoint, key.transaction_index, key.event_index + 1)
        }
    }
}

/// The boundary a Boundary cursor at `position` names, in ledger coordinates:
/// right before the checkpoint, transaction, or event at `position`, or the
/// end of the ledger past its last transaction.
fn cursor_boundary(request: &Request, position: Position) -> LedgerPoint {
    let (tx_seq, event_index) = match position {
        Position::Checkpoints { checkpoint } => return LedgerPoint::checkpoint_start(checkpoint),
        Position::Transactions { tx_seq, .. } => (tx_seq, 0),
        Position::Events {
            tx_seq,
            event_index,
            ..
        } => (tx_seq, event_index),
    };
    if tx_seq >= ledger_transaction_count(request.filter) {
        return LedgerPoint::checkpoint_start(ledger_checkpoint_count(request.filter));
    }
    let (checkpoint, transaction_index) = transaction_place(request.filter, tx_seq);
    LedgerPoint::event(checkpoint, transaction_index, event_index)
}

/// The first key at or after `point` among all items of the request's kind in
/// its ledger, matching its filter or not; `None` past the last. Two points
/// name the same boundary exactly when this is the same for both, whichever
/// of them carries an index past the end of a transaction or checkpoint.
fn first_key_at_or_after(request: &Request, point: LedgerPoint) -> Option<LedgerPoint> {
    let checkpoints = ledger_checkpoint_count(request.filter);
    match request.endpoint {
        Endpoint::Checkpoints => {
            // A point inside a checkpoint lies past that checkpoint's key.
            let inside = point.transaction_index != 0 || point.event_index != 0;
            let checkpoint = point.checkpoint + u64::from(inside);
            (checkpoint < checkpoints).then(|| LedgerPoint::checkpoint_start(checkpoint))
        }
        Endpoint::Transactions => {
            // A point inside a transaction lies past that transaction's key.
            let mut checkpoint = point.checkpoint;
            let mut transaction_index = point.transaction_index + u64::from(point.event_index != 0);
            while checkpoint < checkpoints {
                if transaction_index < checkpoint_transaction_count(request.filter, checkpoint) {
                    return Some(LedgerPoint::transaction(checkpoint, transaction_index));
                }
                checkpoint += 1;
                transaction_index = 0;
            }
            None
        }
        Endpoint::Events => ledger_items(Endpoint::Events, request.filter)
            .into_iter()
            .map(|(item, _)| item_key(request, item))
            .find(|key| *key >= point),
    }
}

fn names_same_boundary(request: &Request, a: LedgerPoint, b: LedgerPoint) -> bool {
    first_key_at_or_after(request, a) == first_key_at_or_after(request, b)
}

/// The `cursor_position` of a response's terminal frame.
fn terminal_position(response: &[Frame]) -> Option<LedgerPoint> {
    response.last().and_then(|frame| frame.position)
}

/// Check the `cursor_position` of every frame of one response against the
/// ledger. An item frame carries an Item cursor and, ascending, the boundary
/// right after its item, descending, the item's key. An item-free frame
/// carries a Boundary cursor and a position naming the boundary that cursor
/// names. Positions never move backwards, and split the response's items as
/// [`assert_scan_positions`] says.
fn assert_response_positions(request: &Request, response: &[Frame]) {
    let mut previous: Option<LedgerPoint> = None;
    for (index, frame) in response.iter().enumerate() {
        let token = CursorToken::decode(&frame.cursor).expect("cursor decodes");
        let position = frame.position.unwrap_or_else(|| {
            panic!("frame {index} carries no cursor_position: {response:#?} {request:?}")
        });
        match frame.item {
            Some(item) => {
                assert_eq!(token.kind, CursorKind::Item, "frame {index}: {request:?}");
                let key = item_key(request, item);
                let expected = if request.ascending {
                    above(request.endpoint, key)
                } else {
                    key
                };
                assert_eq!(
                    position, expected,
                    "position of item {item:?} in frame {index}: {request:?}"
                );
            }
            None => {
                assert_eq!(
                    token.kind,
                    CursorKind::Boundary,
                    "item-free frame {index}: {response:#?} {request:?}"
                );
                let named = cursor_boundary(request, token.position);
                assert!(
                    names_same_boundary(request, position, named),
                    "position {position:?} of frame {index} names the boundary of its cursor \
                     {:?} ({named:?}): {response:#?} {request:?}",
                    token.position
                );
            }
        }
        if let Some(previous) = previous {
            let regresses = if request.ascending {
                position < previous
            } else {
                position > previous
            };
            assert!(
                !regresses,
                "position {position:?} of frame {index} moves back from {previous:?}: \
                 {response:#?} {request:?}"
            );
        }
        previous = Some(position);
    }
    assert_scan_positions(request, response);
}

/// Check every placed frame of `scan`, one paging's frames in order, against
/// the items of the paging: ascending, every item delivered up to and
/// including the frame lies before its position, and every later one at or
/// after it; descending, every item delivered up to and including the frame
/// lies at or after its position, and every later one before it.
fn assert_scan_positions(request: &Request, scan: &[Frame]) {
    let keys: Vec<Option<LedgerPoint>> = scan
        .iter()
        .map(|frame| frame.item.map(|item| item_key(request, item)))
        .collect();
    for (index, frame) in scan.iter().enumerate() {
        let Some(position) = frame.position else {
            continue;
        };
        for (other, key) in keys.iter().enumerate() {
            let Some(key) = key else {
                continue;
            };
            let delivered_side = if request.ascending {
                *key < position
            } else {
                *key >= position
            };
            assert_eq!(
                delivered_side,
                other <= index,
                "item {key:?} of frame {other} against position {position:?} of frame \
                 {index}: {request:?}"
            );
        }
    }
}

fn is_natural_end(reason: QueryEndReason) -> bool {
    matches!(
        reason,
        QueryEndReason::LedgerTip | QueryEndReason::CheckpointBound | QueryEndReason::CursorBound
    )
}

/// Check every coverage claim of one response against the ledger. A claim
/// `c` says every checkpoint `<= c` (ascending; `>= c` descending) within the
/// request's bounds is fully covered: each item of `expected` in those
/// checkpoints is among `received_before` (earlier pages of the same paging)
/// or the items of the response up to and including the claiming frame.
/// Claims never move backwards within a response.
fn assert_claims_sound(
    request: &Request,
    expected: &[(u64, u32)],
    received_before: &[(u64, u32)],
    response: &[Frame],
) {
    let ledger = ledger_checkpoints(request);
    let mut delivered: BTreeSet<(u64, u32)> = received_before.iter().copied().collect();
    let mut previous_claim: Option<u64> = None;
    for (index, frame) in response.iter().enumerate() {
        delivered.extend(frame.item);
        let Some(claim) = frame.covered else {
            continue;
        };
        if let Some(previous) = previous_claim {
            let monotone = if request.ascending {
                claim >= previous
            } else {
                claim <= previous
            };
            assert!(
                monotone,
                "claim {claim} of frame {index} moves back from {previous}: {request:?}"
            );
        }
        previous_claim = Some(claim);
        for item in expected {
            let checkpoint = ledger[item];
            let claimed = if request.ascending {
                checkpoint <= claim
            } else {
                checkpoint >= claim
            };
            assert!(
                !claimed || delivered.contains(item),
                "claim {claim} of frame {index} covers item {item:?} of checkpoint \
                 {checkpoint} before it is delivered: {request:?}"
            );
        }
    }
}

/// How many of a response's progress beacons each name a position strictly
/// past every beacon before them in scan order.
fn advancing_beacons(response: &[Frame], ascending: bool) -> usize {
    let mut frontier: Option<(u64, u32)> = None;
    let mut advancing = 0;
    let beacons = response
        .iter()
        .filter(|frame| frame.item.is_none() && frame.end.is_none());
    for beacon in beacons {
        let position = CursorToken::decode(&beacon.cursor)
            .expect("beacon cursor decodes")
            .position;
        let key = scan_key(position);
        let advances = frontier.is_none_or(|frontier| {
            if ascending {
                key > frontier
            } else {
                key < frontier
            }
        });
        if advances {
            advancing += 1;
            frontier = Some(key);
        }
    }
    advancing
}

/// How paging a request to its end went: the reason the last page ended, and
/// every frame of the paging, in order.
struct PagedToEnd {
    reason: QueryEndReason,
    frames: Vec<Frame>,
}

/// Page from `resume` until the range is exhausted, continuing after each
/// page's last frame in `mode`. Appends the items to `received`, checks each
/// response's coverage claims against `expected` and the items received
/// before it, and checks the positions of the frames of `prior` (the frames
/// that delivered `received`) and of the paging together (see
/// [`assert_scan_positions`]).
async fn page_to_end(
    ctx: &QueryContext,
    request: &Request,
    expected: &[(u64, u32)],
    received: &mut Vec<(u64, u32)>,
    prior: &[Frame],
    mut resume: Option<Resume>,
    mode: ResumeMode,
) -> PagedToEnd {
    let mut scan = prior.to_vec();
    for _ in 0..1_000 {
        let frames = run(ctx, request, resume.as_ref()).await;
        assert_claims_sound(request, expected, received, &frames);
        received.extend(items(&frames));
        scan.extend(frames.iter().cloned());
        let last = frames.last().expect("every response has a frame");
        let reason = last.end.expect("every response ends with a terminal frame");
        if is_natural_end(reason) {
            assert_scan_positions(request, &scan);
            let frames = scan.split_off(prior.len());
            return PagedToEnd { reason, frames };
        }
        assert!(
            matches!(
                reason,
                QueryEndReason::ItemLimit | QueryEndReason::ScanLimit
            ),
            "unexpected terminal reason {reason:?}"
        );
        resume = Some(Resume::after(last, mode));
    }
    panic!("paging did not reach the end of the range: {request:?}");
}

/// A scenario's first response, and the most progress beacons at strictly
/// advancing positions (`advancing_beacons`) any of its cut responses held.
struct CutPages {
    first: Vec<Frame>,
    most_advancing_beacons: usize,
}

/// Cut the first response of `request` after each of its frames (and the
/// second response too, when the first ends on a limit), resume after the
/// last frame received in each [`ResumeMode`], and require exactly `expected`
/// every time, as well as from uncut paging. Every response's coverage claims
/// and positions are checked against the ledger. On the sparse ledger, also
/// check the checkpoint of every beacon and scan-limit cursor in the cut
/// responses.
async fn assert_resume_property(
    ctx: &QueryContext,
    request: &Request,
    expected_terminal: QueryEndReason,
    expected: &[(u64, u32)],
) -> CutPages {
    assert!(!expected.is_empty(), "scenario without items: {request:?}");
    let first = run(ctx, request, None).await;
    let terminal = first.last().and_then(|frame| frame.end);
    assert_eq!(terminal, Some(expected_terminal), "{request:?}");

    // Each cut page with the frames of the pages before it.
    let mut pages = vec![(Vec::new(), first.clone())];
    let mut uncut = first.clone();
    if !is_natural_end(expected_terminal) {
        let resume = Resume::after(first.last().unwrap(), ResumeMode::Cursor);
        let second = run(ctx, request, Some(&resume)).await;
        pages.push((uncut.clone(), second.clone()));
        uncut.extend(second.iter().cloned());
        let last = second.last().expect("every response has a frame");
        if !last.end.is_some_and(is_natural_end) {
            let mut received = items(&uncut);
            let resume = Some(Resume::after(last, ResumeMode::Cursor));
            let paged = page_to_end(
                ctx,
                request,
                expected,
                &mut received,
                &uncut,
                resume,
                ResumeMode::Cursor,
            )
            .await;
            uncut.extend(paged.frames);
        }
    }
    assert_eq!(items(&uncut), expected, "uncut paging: {request:?}");
    assert_scan_positions(request, &uncut);

    let mut most_advancing_beacons = 0;
    for (prior, page) in &pages {
        let received_before = items(prior);
        assert_claims_sound(request, expected, &received_before, page);
        most_advancing_beacons =
            most_advancing_beacons.max(advancing_beacons(page, request.ascending));
        for (cut, frame) in page.iter().enumerate() {
            let resolved_cursor =
                frame.item.is_none() && matches!(frame.end, None | Some(QueryEndReason::ScanLimit));
            if request.filter != Filter::None && resolved_cursor {
                let position = CursorToken::decode(&frame.cursor)
                    .expect("cursor decodes")
                    .position;
                if let Some(checkpoint) = sparse_boundary_checkpoint(position, request.ascending) {
                    assert_eq!(
                        position.checkpoint(),
                        checkpoint,
                        "checkpoint of {position:?}: {request:?}"
                    );
                }
            }
            let mut scan = prior.clone();
            scan.extend(page[..=cut].iter().cloned());
            let modes = if frame.end.is_some_and(is_natural_end) {
                // Nothing follows a natural end, so there is nothing to resume.
                vec![None]
            } else {
                vec![Some(ResumeMode::Cursor), Some(ResumeMode::Position)]
            };
            for mode in modes {
                let mut received = items(&scan);
                if let Some(mode) = mode {
                    let resume = Some(Resume::after(frame, mode));
                    page_to_end(ctx, request, expected, &mut received, &scan, resume, mode).await;
                }
                assert_eq!(
                    received,
                    expected,
                    "cut after frame {cut} of {}, resumed by {mode:?}: {request:?}",
                    page.len()
                );
            }
        }
    }
    CutPages {
        first,
        most_advancing_beacons,
    }
}

/// The cursor of the item `index` items into the uncut paging of `request`.
async fn item_cursor(ctx: &QueryContext, request: &Request, index: usize) -> Bytes {
    let mut seen = 0;
    let mut resume = None;
    loop {
        let frames = run(ctx, request, resume.as_ref()).await;
        for frame in &frames {
            if frame.item.is_some() {
                if seen == index {
                    return frame.cursor.clone();
                }
                seen += 1;
            }
        }
        let last = frames.last().unwrap();
        assert!(
            !is_natural_end(last.end.unwrap()),
            "fewer than {} items in {request:?}",
            index + 1
        );
        resume = Some(Resume::after(last, ResumeMode::Cursor));
    }
}

/// Every reachable terminal reason for one endpoint, filter, and ordering.
/// Returns the most progress beacons at strictly advancing positions any cut
/// response held among the scenarios that emit a beacon per bucket row.
async fn assert_endpoint_resumes(
    endpoint: Endpoint,
    filter: Filter,
    ascending: bool,
    mock: &MockBigtableServer,
    checkpoint_hi_exclusive: u64,
) -> usize {
    let method = endpoint.method();
    let (ctx, server) = context(
        mock,
        checkpoint_hi_exclusive,
        method,
        Limits {
            bucket_budget: 10_000,
            max_limit_items: 100,
            beacon_buckets: NonZeroU64::MIN,
        },
    )
    .await;
    let base = Request {
        endpoint,
        filter,
        ascending,
        start_checkpoint: None,
        end_checkpoint: None,
        limit: 100,
        far_bound: None,
    };
    let all = expected_items(&base, checkpoint_hi_exclusive);
    let mut most_advancing_beacons = 0;

    // Item limit.
    let request = Request {
        limit: 3,
        ..base.clone()
    };
    let pages = assert_resume_property(&ctx, &request, QueryEndReason::ItemLimit, &all).await;
    most_advancing_beacons = most_advancing_beacons.max(pages.most_advancing_beacons);

    // Natural end of an open range: the ledger tip ascending, genesis (a
    // checkpoint bound) descending.
    let natural = if ascending {
        QueryEndReason::LedgerTip
    } else {
        QueryEndReason::CheckpointBound
    };
    let pages = assert_resume_property(&ctx, &base, natural, &all).await;
    most_advancing_beacons = most_advancing_beacons.max(pages.most_advancing_beacons);
    // The natural end is a checkpoint boundary: the tip ascending, genesis descending.
    let natural_end = if ascending {
        LedgerPoint::checkpoint_start(checkpoint_hi_exclusive)
    } else {
        LedgerPoint::GENESIS
    };
    assert_eq!(
        terminal_position(&pages.first),
        Some(natural_end),
        "{base:?}"
    );

    // Explicit checkpoint bounds inside the ledger.
    let request = Request {
        start_checkpoint: Some(1),
        end_checkpoint: Some(checkpoint_hi_exclusive - 1),
        ..base.clone()
    };
    let expected = expected_items(&request, checkpoint_hi_exclusive);
    let pages =
        assert_resume_property(&ctx, &request, QueryEndReason::CheckpointBound, &expected).await;
    most_advancing_beacons = most_advancing_beacons.max(pages.most_advancing_beacons);
    let bound = if ascending {
        checkpoint_hi_exclusive - 1
    } else {
        1
    };
    assert_eq!(
        terminal_position(&pages.first),
        Some(LedgerPoint::checkpoint_start(bound)),
        "{request:?}"
    );

    // A cursor bound on the far side of the scan: the cursor of the fourth
    // item, so exactly the first three are returned.
    let request = Request {
        far_bound: Some(item_cursor(&ctx, &base, 3).await),
        ..base.clone()
    };
    let pages =
        assert_resume_property(&ctx, &request, QueryEndReason::CursorBound, &all[..3]).await;
    most_advancing_beacons = most_advancing_beacons.max(pages.most_advancing_beacons);
    server.abort();

    // Scan limit: a bucket budget that runs out partway (filtered scans), or
    // the unfiltered event scan's per-request transaction cap. The budget
    // covers two buckets per literal, so every page moves the frontier past
    // at least one whole bucket.
    if filter != Filter::None {
        let (ctx, server) = context(
            mock,
            checkpoint_hi_exclusive,
            method,
            Limits {
                bucket_budget: 2 * filter.senders().len() as u64,
                max_limit_items: 100,
                beacon_buckets: NonZeroU64::MIN,
            },
        )
        .await;
        let pages = assert_resume_property(&ctx, &base, QueryEndReason::ScanLimit, &all).await;
        most_advancing_beacons = most_advancing_beacons.max(pages.most_advancing_beacons);
        server.abort();
        if !matches!(endpoint, Endpoint::Checkpoints) {
            // The budget runs out at a bucket edge, inside a checkpoint.
            let position = terminal_position(&pages.first).expect("SCAN_LIMIT is placed");
            assert!(
                (1..SPARSE_TXS_PER_CHECKPOINT).contains(&position.transaction_index),
                "SCAN_LIMIT position {position:?}: {base:?}"
            );
            assert_cursor_bound_claim(&base, &all, mock, checkpoint_hi_exclusive).await;
            assert_mid_checkpoint_cursor_bound(&base, &all, mock, checkpoint_hi_exclusive).await;
        }
    } else if matches!(endpoint, Endpoint::Events) {
        let (ctx, server) = context(
            mock,
            checkpoint_hi_exclusive,
            method,
            Limits {
                bucket_budget: 10_000,
                max_limit_items: 3,
                beacon_buckets: NonZeroU64::MIN,
            },
        )
        .await;
        let request = Request { limit: 3, ..base };
        let pages = assert_resume_property(&ctx, &request, QueryEndReason::ScanLimit, &all).await;
        most_advancing_beacons = most_advancing_beacons.max(pages.most_advancing_beacons);
        server.abort();
    }
    most_advancing_beacons
}

/// A cursor bound deep in a stretch of the sparse ledger without matches,
/// with beacon thresholds no scan reaches: the scan's first watermark is
/// emitted at once, and every later one is held. The terminal frame claims
/// exactly the coverage of the frames before it, so its checkpoint claim
/// covers the scan up to the bound only if the held watermark is emitted and
/// resolved when the cursor-bound scan ends.
async fn assert_cursor_bound_claim(
    base: &Request,
    all: &[(u64, u32)],
    mock: &MockBigtableServer,
    checkpoint_hi_exclusive: u64,
) {
    // Between the matches at 140,000 and 200,000, at the first transaction
    // of checkpoint 19.
    const BOUND_TX: u64 = 190_000;
    let (ctx, server) = context(
        mock,
        checkpoint_hi_exclusive,
        base.endpoint.method(),
        Limits {
            bucket_budget: 10_000,
            max_limit_items: 100,
            beacon_buckets: NonZeroU64::MAX,
        },
    )
    .await;
    let checkpoint = BOUND_TX / SPARSE_TXS_PER_CHECKPOINT;
    let position = match base.endpoint {
        Endpoint::Events => Position::Events {
            checkpoint,
            tx_seq: BOUND_TX,
            event_index: 0,
        },
        Endpoint::Transactions => Position::Transactions {
            checkpoint,
            tx_seq: BOUND_TX,
        },
        Endpoint::Checkpoints => unreachable!("checkpoint positions are not modeled"),
    };
    // The cursor carries its public position, as every cursor this server mints does.
    let point = LedgerPoint::transaction(checkpoint, BOUND_TX % SPARSE_TXS_PER_CHECKPOINT);
    let request = Request {
        far_bound: Some(
            CursorToken::boundary(position)
                .with_public_position(point.public_position(&position))
                .encode(),
        ),
        ..base.clone()
    };
    let expected: Vec<_> = all
        .iter()
        .copied()
        .filter(|&(tx, _)| {
            if base.ascending {
                tx < BOUND_TX
            } else {
                tx > BOUND_TX
            }
        })
        .collect();
    let pages =
        assert_resume_property(&ctx, &request, QueryEndReason::CursorBound, &expected).await;

    // The scan's last watermark is the far edge of the last bucket it read,
    // clamped to the bound: AND scans read every bucket up to the bound, while
    // a single-literal scan stops at the bucket of the last match before it.
    // The checkpoint holding the last covered transaction may hold more
    // matches as far as the claim goes, so the claim stops just short of it.
    let ascending = base.ascending;
    let bucket_txs = match base.endpoint {
        Endpoint::Events => EVENT_BUCKET >> event_seq::EVENT_BITS,
        Endpoint::Transactions | Endpoint::Checkpoints => TX_BUCKET,
    };
    let last_read_tx = match base.filter {
        Filter::AndSenders if ascending => BOUND_TX - 1,
        Filter::AndSenders => BOUND_TX,
        Filter::Sender | Filter::None => {
            let past_bound = expected.iter().map(|&(tx, _)| tx);
            let last = if ascending {
                past_bound.max()
            } else {
                past_bound.min()
            };
            last.expect("matches before the bound")
        }
    };
    let bucket = last_read_tx / bucket_txs;
    let claim = if ascending {
        let frontier = ((bucket + 1) * bucket_txs).min(BOUND_TX);
        (frontier - 1) / SPARSE_TXS_PER_CHECKPOINT - 1
    } else {
        let frontier = (bucket * bucket_txs).max(BOUND_TX);
        frontier / SPARSE_TXS_PER_CHECKPOINT + 1
    };
    let terminal = pages.first.last().expect("every response has a frame");
    assert_eq!(terminal.covered, Some(claim), "terminal claim: {request:?}");
    server.abort();
}

/// A far cursor bound inside a checkpoint, a Boundary cursor at
/// [`MID_CHECKPOINT_BOUND_TX`], ends the scan with a `CURSOR_BOUND` frame
/// placed exactly at that boundary by the public position the cursor carries,
/// and every frame of the scan resumes by cursor and by position.
async fn assert_mid_checkpoint_cursor_bound(
    base: &Request,
    all: &[(u64, u32)],
    mock: &MockBigtableServer,
    checkpoint_hi_exclusive: u64,
) {
    let (ctx, server) = context(
        mock,
        checkpoint_hi_exclusive,
        base.endpoint.method(),
        Limits {
            bucket_budget: 10_000,
            max_limit_items: 100,
            beacon_buckets: NonZeroU64::MIN,
        },
    )
    .await;
    let checkpoint = MID_CHECKPOINT_BOUND_TX / SPARSE_TXS_PER_CHECKPOINT;
    let position = match base.endpoint {
        Endpoint::Events => Position::Events {
            checkpoint,
            tx_seq: MID_CHECKPOINT_BOUND_TX,
            event_index: 0,
        },
        Endpoint::Transactions => Position::Transactions {
            checkpoint,
            tx_seq: MID_CHECKPOINT_BOUND_TX,
        },
        Endpoint::Checkpoints => unreachable!("checkpoint positions are not modeled"),
    };
    let boundary = LedgerPoint::transaction(
        checkpoint,
        MID_CHECKPOINT_BOUND_TX % SPARSE_TXS_PER_CHECKPOINT,
    );
    let request = Request {
        far_bound: Some(
            CursorToken::boundary(position)
                .with_public_position(boundary.public_position(&position))
                .encode(),
        ),
        ..base.clone()
    };
    // Ascending, `before` keeps the items before the boundary; descending, `after` keeps those
    // at or after it.
    let expected: Vec<_> = all
        .iter()
        .copied()
        .filter(|&(tx, _)| (tx < MID_CHECKPOINT_BOUND_TX) == base.ascending)
        .collect();
    let pages =
        assert_resume_property(&ctx, &request, QueryEndReason::CursorBound, &expected).await;
    assert_eq!(
        terminal_position(&pages.first),
        Some(boundary),
        "{request:?}"
    );
    server.abort();
}

/// Runs every resume scenario of `endpoint` in each of `directions` (`true` is ascending).
/// Resuming events after every frame, by cursor and by position, takes about two minutes per
/// direction against the mock's read latency, so each events direction is its own test to stay
/// under CI's five-minute limit.
async fn assert_all_modes(endpoint: Endpoint, directions: &[bool]) {
    let sparse = MockBigtableServer::new();
    sparse_ledger(&sparse).await;
    let dense = MockBigtableServer::new();
    dense_ledger(&dense).await;
    for mock in [&sparse, &dense] {
        mock.set_read_rows_latency(READ_LATENCY).await;
    }
    let mut most_advancing_beacons = 0;
    for &ascending in directions {
        for filter in [Filter::Sender, Filter::AndSenders] {
            let advancing =
                assert_endpoint_resumes(endpoint, filter, ascending, &sparse, SPARSE_CHECKPOINTS)
                    .await;
            most_advancing_beacons = most_advancing_beacons.max(advancing);
        }
        assert_endpoint_resumes(endpoint, Filter::None, ascending, &dense, DENSE_CHECKPOINTS).await;
    }
    // With a beacon after every bucket row charged without an item, the
    // filtered scans cross item-free buckets that must each be reported:
    // some response carries several beacons, each past the one before. One
    // beacon (the scan's first watermark) would mean later watermarks are
    // never emitted or never reach the client, and no beacon would leave
    // cuts after beacons untested.
    assert!(
        most_advancing_beacons >= 2,
        "no filtered response carries two progress beacons at advancing positions \
         (most: {most_advancing_beacons})"
    );
}

#[tokio::test]
async fn list_events_resumes_after_any_frame_ascending() {
    assert_all_modes(Endpoint::Events, &[true]).await;
}

#[tokio::test]
async fn list_events_resumes_after_any_frame_descending() {
    assert_all_modes(Endpoint::Events, &[false]).await;
}

#[tokio::test]
async fn list_transactions_resumes_after_any_frame() {
    assert_all_modes(Endpoint::Transactions, &[true, false]).await;
}

#[tokio::test]
async fn list_checkpoints_resumes_after_any_frame() {
    assert_all_modes(Endpoint::Checkpoints, &[true, false]).await;
}

/// Every cursor a response carries, sent back in the opposite ordering, returns exactly the
/// items its position and kind admit: strictly after an item, from a boundary, and below either
/// as `before`. The cursors come from paging the endpoint to its end in both orderings, with and
/// without a far cursor bound (whose terminal restates the bound), over filtered scans whose
/// small bucket budget ends pages on scan-limit frontiers inside checkpoints, and over the dense
/// ledger, where the unfiltered event scan's transaction cap does the same. Each opposite request
/// is paged to its end, with its coverage claims checked against the items it returns. The
/// bounded paging's closing cursor also resumes in its own ordering, without the far bound, with
/// exactly the items the bound excluded.
async fn assert_opposite_direction_resumes(endpoint: Endpoint) {
    let sparse = MockBigtableServer::new();
    sparse_ledger(&sparse).await;
    let dense = MockBigtableServer::new();
    dense_ledger(&dense).await;
    let method = endpoint.method();
    for filter in [Filter::Sender, Filter::AndSenders, Filter::None] {
        let (mock, checkpoint_hi_exclusive) = if filter == Filter::None {
            (&dense, DENSE_CHECKPOINTS)
        } else {
            (&sparse, SPARSE_CHECKPOINTS)
        };
        // Small pages: two buckets per literal, or three items, which caps the unfiltered event
        // scan's transactions.
        let (minting_limits, limit) = if filter == Filter::None {
            let limits = Limits {
                bucket_budget: 10_000,
                max_limit_items: 3,
                beacon_buckets: NonZeroU64::MIN,
            };
            (limits, 3)
        } else {
            let limits = Limits {
                bucket_budget: 2 * filter.senders().len() as u64,
                max_limit_items: 100,
                beacon_buckets: NonZeroU64::MIN,
            };
            (limits, 100)
        };
        let (minting, minting_server) =
            context(mock, checkpoint_hi_exclusive, method, minting_limits).await;
        let (paging, paging_server) = context(
            mock,
            checkpoint_hi_exclusive,
            method,
            Limits {
                bucket_budget: 10_000,
                max_limit_items: 100,
                beacon_buckets: NonZeroU64::MIN,
            },
        )
        .await;
        for ascending in [true, false] {
            let base = Request {
                endpoint,
                filter,
                ascending,
                start_checkpoint: None,
                end_checkpoint: None,
                limit,
                far_bound: None,
            };
            let all = expected_items(
                &Request {
                    ascending: true,
                    ..base.clone()
                },
                checkpoint_hi_exclusive,
            );
            let far_bound = item_cursor(&minting, &base, 3).await;
            let far_key = scan_key(
                CursorToken::decode(&far_bound)
                    .expect("cursor decodes")
                    .position,
            );
            let bounded = Request {
                far_bound: Some(far_bound),
                ..base.clone()
            };
            let within_far_bound = |item: (u64, u32)| {
                if ascending {
                    item < far_key
                } else {
                    item > far_key
                }
            };
            let mut cursors = Vec::new();
            let mut closing = None;
            for request in [&base, &bounded] {
                let in_bounds: Vec<_> = expected_items(request, checkpoint_hi_exclusive)
                    .into_iter()
                    .filter(|&item| request.far_bound.is_none() || within_far_bound(item))
                    .collect();
                let mut received = Vec::new();
                let paged = page_to_end(
                    &minting,
                    request,
                    &in_bounds,
                    &mut received,
                    &[],
                    None,
                    ResumeMode::Cursor,
                )
                .await;
                assert_eq!(received, in_bounds, "{request:?}");
                cursors.extend(paged.frames.into_iter().map(|frame| frame.cursor));
                closing = cursors.last().cloned();
            }

            // The bounded paging's closing cursor, sent back in the same ordering without the
            // far bound (an item), resumes with exactly the items the bound excluded: from the
            // item on ascending, where `before` excludes it, and from the item down descending,
            // where `after` excludes it.
            let excluded: Vec<_> = if ascending {
                all.iter()
                    .copied()
                    .filter(|&item| item >= far_key)
                    .collect()
            } else {
                all.iter()
                    .rev()
                    .copied()
                    .filter(|&item| item <= far_key)
                    .collect()
            };
            let unbounded = Request {
                limit: 100,
                ..base.clone()
            };
            let mut received = Vec::new();
            page_to_end(
                &paging,
                &unbounded,
                &excluded,
                &mut received,
                &[],
                closing.map(Resume::from_cursor),
                ResumeMode::Cursor,
            )
            .await;
            assert_eq!(received, excluded, "closing cursor of {bounded:?}");

            let opposite = Request {
                ascending: !ascending,
                limit: 100,
                ..base.clone()
            };
            for cursor in cursors {
                let token = CursorToken::decode(&cursor).expect("cursor decodes");
                let key = scan_key(token.position);
                let expected: Vec<_> = if opposite.ascending {
                    all.iter()
                        .copied()
                        .filter(|&item| match token.kind {
                            sui_rpc_cursor::CursorKind::Item => item > key,
                            sui_rpc_cursor::CursorKind::Boundary => item >= key,
                        })
                        .collect()
                } else {
                    all.iter()
                        .rev()
                        .copied()
                        .filter(|&item| item < key)
                        .collect()
                };
                let mut received = Vec::new();
                let reason = page_to_end(
                    &paging,
                    &opposite,
                    &expected,
                    &mut received,
                    &[],
                    Some(Resume::from_cursor(cursor)),
                    ResumeMode::Cursor,
                )
                .await
                .reason;
                assert_eq!(received, expected, "{token:?} sent back: {opposite:?}");
                if !expected.is_empty() {
                    let natural_end = if opposite.ascending {
                        QueryEndReason::LedgerTip
                    } else {
                        QueryEndReason::CheckpointBound
                    };
                    assert_eq!(reason, natural_end, "{token:?} sent back: {opposite:?}");
                }
            }
        }
        minting_server.abort();
        paging_server.abort();
    }
}

#[tokio::test]
async fn list_events_resumes_in_the_opposite_direction() {
    assert_opposite_direction_resumes(Endpoint::Events).await;
}

#[tokio::test]
async fn list_transactions_resumes_in_the_opposite_direction() {
    assert_opposite_direction_resumes(Endpoint::Transactions).await;
}

#[tokio::test]
async fn list_checkpoints_resumes_in_the_opposite_direction() {
    assert_opposite_direction_resumes(Endpoint::Checkpoints).await;
}

/// An ascending request whose start fields lie past the indexed tip gets one `LEDGER_TIP` frame
/// echoing them as its `cursor_position`, with no `Watermark.checkpoint`, and the Boundary cursor
/// at the tip that a request starting anywhere past the tip gets. One whose only bound past the
/// tip is an `after` cursor, with no start fields or start fields behind the cursor, gets a
/// `LEDGER_TIP` frame echoing that cursor byte for byte, placed at the public position the cursor
/// carries. A transaction or event cursor that carries none (minted before cursors carried one)
/// leaves the frame without a `cursor_position`; a checkpoint cursor is its own position.
#[tokio::test]
async fn ascending_past_tip_frames_echo_the_start_fields() {
    let dense = MockBigtableServer::new();
    dense_ledger(&dense).await;
    let total_txs: u64 = DENSE_CHECKPOINT_TXS.iter().sum();
    let beyond_tip = DENSE_CHECKPOINTS + 3;
    for endpoint in [
        Endpoint::Checkpoints,
        Endpoint::Transactions,
        Endpoint::Events,
    ] {
        let (ctx, server) = context(
            &dense,
            DENSE_CHECKPOINTS,
            endpoint.method(),
            Limits {
                bucket_budget: 10_000,
                max_limit_items: 100,
                beacon_buckets: NonZeroU64::MIN,
            },
        )
        .await;
        let base = Request {
            endpoint,
            filter: Filter::None,
            ascending: true,
            start_checkpoint: None,
            end_checkpoint: None,
            limit: 10,
            far_bound: None,
        };
        // The echoed start, the tip, a position past the tip, and that position's point.
        let (start, tip, past_tip, past_tip_point) = match endpoint {
            Endpoint::Checkpoints => (
                LedgerPoint::checkpoint_start(beyond_tip),
                Position::Checkpoints {
                    checkpoint: DENSE_CHECKPOINTS,
                },
                Position::Checkpoints {
                    checkpoint: beyond_tip,
                },
                LedgerPoint::checkpoint_start(beyond_tip),
            ),
            Endpoint::Transactions => (
                LedgerPoint::transaction(beyond_tip, 2),
                Position::Transactions {
                    checkpoint: DENSE_CHECKPOINTS,
                    tx_seq: total_txs,
                },
                Position::Transactions {
                    checkpoint: beyond_tip,
                    tx_seq: total_txs + 3,
                },
                LedgerPoint::transaction(beyond_tip, 1),
            ),
            Endpoint::Events => (
                LedgerPoint::event(beyond_tip, 1, 1),
                Position::Events {
                    checkpoint: DENSE_CHECKPOINTS,
                    tx_seq: total_txs,
                    event_index: 0,
                },
                Position::Events {
                    checkpoint: beyond_tip,
                    tx_seq: total_txs + 3,
                    event_index: 0,
                },
                LedgerPoint::event(beyond_tip, 1, 0),
            ),
        };

        let from_start = Resume {
            cursor: None,
            position: Some(start),
        };
        let frames = run(&ctx, &base, Some(&from_start)).await;
        let case = format!("{endpoint:?} from {start:?}: {frames:#?}");
        let [frame] = frames.as_slice() else {
            panic!("one frame: {case}");
        };
        assert_eq!(frame.item, None, "{case}");
        assert_eq!(frame.end, Some(QueryEndReason::LedgerTip), "{case}");
        assert_eq!(frame.position, Some(start), "{case}");
        assert_eq!(frame.covered, None, "{case}");
        let tip_cursor = CursorToken::boundary(tip).with_public_position(
            LedgerPoint::checkpoint_start(DENSE_CHECKPOINTS).public_position(&tip),
        );
        assert_eq!(frame.cursor, tip_cursor.encode(), "{case}");

        let unlabelled = CursorToken::boundary(past_tip);
        let labelled = unlabelled
            .clone()
            .with_public_position(past_tip_point.public_position(&past_tip));
        let unlabelled_position =
            matches!(endpoint, Endpoint::Checkpoints).then_some(past_tip_point);
        for (after, expected_position) in [
            (labelled, Some(past_tip_point)),
            (unlabelled, unlabelled_position),
        ] {
            let after = after.encode();
            for start_checkpoint in [None, Some(1)] {
                let request = Request {
                    start_checkpoint,
                    ..base.clone()
                };
                let frames = fetch(&ctx, &request, Some(&Resume::from_cursor(after.clone()))).await;
                let case = format!("{request:?} after {after:?}: {frames:#?}");
                let [frame] = frames.as_slice() else {
                    panic!("one frame: {case}");
                };
                assert_eq!(frame.item, None, "{case}");
                assert_eq!(frame.end, Some(QueryEndReason::LedgerTip), "{case}");
                assert_eq!(frame.position, expected_position, "{case}");
                assert_eq!(frame.covered, None, "{case}");
                assert_eq!(frame.cursor, after, "{case}");
            }
        }
        server.abort();
    }
}

/// When the unfiltered descending event scan's per-request transaction cap ends a page right
/// after an item, its `SCAN_LIMIT` frame carries a Boundary cursor whose position is exactly the
/// item frame's: the item itself.
#[tokio::test]
async fn list_events_descending_scan_limit_after_an_item_names_it() {
    let dense = MockBigtableServer::new();
    dense_ledger(&dense).await;
    let (ctx, server) = context(
        &dense,
        DENSE_CHECKPOINTS,
        Endpoint::Events.method(),
        Limits {
            bucket_budget: 10_000,
            max_limit_items: 3,
            beacon_buckets: NonZeroU64::MIN,
        },
    )
    .await;
    let request = Request {
        endpoint: Endpoint::Events,
        filter: Filter::None,
        ascending: false,
        start_checkpoint: None,
        end_checkpoint: None,
        limit: 3,
        far_bound: None,
    };
    let frames = run(&ctx, &request, None).await;
    let case = format!("{request:?}: {frames:#?}");
    let [.., item, terminal] = frames.as_slice() else {
        panic!("an item and a terminal frame: {case}");
    };
    assert!(item.item.is_some(), "{case}");
    assert_eq!(terminal.end, Some(QueryEndReason::ScanLimit), "{case}");
    assert_eq!(
        terminal.position.expect("SCAN_LIMIT is placed"),
        item.position.expect("item frames are placed"),
        "{case}"
    );
    server.abort();
}
