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
//! cursor names an item the ledger holds, the item's own checkpoint field
//! names the checkpoint the ledger put it in, and each frame's checkpoint
//! coverage claim is sound (every item of the request in the claimed
//! checkpoints has been delivered by that frame or by earlier pages) and never
//! moves backwards within the response.
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
/// its cursor, its checkpoint coverage claim, and its terminal reason (if it
/// ends the stream).
#[derive(Clone, Debug)]
struct Frame {
    item: Option<(u64, u32)>,
    cursor: Bytes,
    covered: Option<u64>,
    end: Option<QueryEndReason>,
}

fn options(request: &Request, resume: Option<Bytes>) -> proto::QueryOptions {
    let mut options = proto::QueryOptions::default();
    options.limit = Some(request.limit);
    if request.ascending {
        options.ordering = Some(proto::Ordering::Ascending as i32);
        options.after = resume;
        options.before = request.far_bound.clone();
    } else {
        options.ordering = Some(proto::Ordering::Descending as i32);
        options.before = resume;
        options.after = request.far_bound.clone();
    }
    options
}

fn frame(
    item: Option<Position>,
    watermark: Option<proto::Watermark>,
    end: Option<proto::QueryEnd>,
) -> Frame {
    let covered = watermark
        .as_ref()
        .and_then(|watermark| watermark.checkpoint);
    let cursor = watermark
        .and_then(|watermark| watermark.cursor)
        .expect("every frame carries a resume cursor");
    let item = item.map(scan_key);
    Frame {
        item,
        cursor,
        covered,
        end: end.map(|end| end.reason()),
    }
}

/// The scan-order key of a position: `(transaction, event index)` for
/// events, `(transaction, 0)` for transactions, `(checkpoint, 0)` for
/// checkpoints. Keys grow in ascending scan order.
fn scan_key(position: Position) -> (u64, u32) {
    match position {
        Position::Events {
            tx_seq,
            event_index,
        } => (tx_seq, event_index),
        Position::Transactions { tx_seq } => (tx_seq, 0),
        Position::Checkpoints { checkpoint } => (checkpoint, 0),
    }
}

/// Check an item frame against the ledger: the item is one the ledger holds
/// for the request's filter, and its own checkpoint field names the checkpoint
/// the ledger put it in.
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

/// Run one request to the end of its stream, checking every item frame's
/// checkpoints against the ledger.
async fn run(ctx: &QueryContext, request: &Request, resume: Option<Bytes>) -> Vec<Frame> {
    let options = Some(options(request, resume));
    let ledger = ledger_checkpoints(request);
    match request.endpoint {
        Endpoint::Events => {
            let mut proto_request = proto::ListEventsRequest::default();
            proto_request.start_checkpoint = request.start_checkpoint;
            proto_request.end_checkpoint = request.end_checkpoint;
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
                    frame(item, response.watermark, response.end)
                })
                .collect()
        }
        Endpoint::Transactions => {
            let mut proto_request = proto::ListTransactionsRequest::default();
            proto_request.start_checkpoint = request.start_checkpoint;
            proto_request.end_checkpoint = request.end_checkpoint;
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
                    frame(item, response.watermark, response.end)
                })
                .collect()
        }
        Endpoint::Checkpoints => {
            let mut proto_request = proto::ListCheckpointsRequest::default();
            proto_request.start_checkpoint = request.start_checkpoint;
            proto_request.end_checkpoint = request.end_checkpoint;
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
                    frame(item, response.watermark, response.end)
                })
                .collect()
        }
    }
}

fn items(frames: &[Frame]) -> Vec<(u64, u32)> {
    frames.iter().filter_map(|frame| frame.item).collect()
}

/// Checkpoint of dense-ledger transaction `tx`.
fn dense_checkpoint(tx: u64) -> u64 {
    let mut first_tx = 0;
    for (checkpoint, tx_count) in DENSE_CHECKPOINT_TXS.iter().enumerate() {
        if tx < first_tx + tx_count {
            return checkpoint as u64;
        }
        first_tx += tx_count;
    }
    panic!("transaction {tx} is past the dense ledger");
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

/// How paging a request to its end went: the reason the last page ended, and every frame's
/// cursor, in order.
struct PagedToEnd {
    reason: QueryEndReason,
    cursors: Vec<Bytes>,
}

/// Page from `resume` until the range is exhausted, following each page's
/// last cursor. Appends the items to `received`, and checks each response's
/// coverage claims against `expected` and the items received before it.
async fn page_to_end(
    ctx: &QueryContext,
    request: &Request,
    expected: &[(u64, u32)],
    received: &mut Vec<(u64, u32)>,
    mut resume: Option<Bytes>,
) -> PagedToEnd {
    let mut cursors = Vec::new();
    for _ in 0..1_000 {
        let frames = run(ctx, request, resume).await;
        assert_claims_sound(request, expected, received, &frames);
        received.extend(items(&frames));
        cursors.extend(frames.iter().map(|frame| frame.cursor.clone()));
        let last = frames.last().expect("every response has a frame");
        let reason = last.end.expect("every response ends with a terminal frame");
        if is_natural_end(reason) {
            return PagedToEnd { reason, cursors };
        }
        assert!(
            matches!(
                reason,
                QueryEndReason::ItemLimit | QueryEndReason::ScanLimit
            ),
            "unexpected terminal reason {reason:?}"
        );
        resume = Some(last.cursor.clone());
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
/// second response too, when the first ends on a limit), resume from the
/// last cursor received, and require exactly `expected` every time, as well
/// as from uncut paging. Every response's coverage claims are checked
/// against the ledger. On the sparse ledger, also check the checkpoint of
/// every beacon and scan-limit cursor in the cut responses.
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

    // Each cut page with the items received before it.
    let mut pages = vec![(Vec::new(), first.clone())];
    let mut uncut = items(&first);
    if !is_natural_end(expected_terminal) {
        let second = run(ctx, request, Some(first.last().unwrap().cursor.clone())).await;
        pages.push((uncut.clone(), second.clone()));
        uncut.extend(items(&second));
        let last = second.last().expect("every response has a frame");
        if !last.end.is_some_and(is_natural_end) {
            page_to_end(
                ctx,
                request,
                expected,
                &mut uncut,
                Some(last.cursor.clone()),
            )
            .await;
        }
    }
    assert_eq!(uncut, expected, "uncut paging: {request:?}");

    let mut most_advancing_beacons = 0;
    for (received_before, page) in &pages {
        assert_claims_sound(request, expected, received_before, page);
        most_advancing_beacons =
            most_advancing_beacons.max(advancing_beacons(page, request.ascending));
        for (cut, frame) in page.iter().enumerate() {
            let mut received = received_before.clone();
            received.extend(items(&page[..=cut]));
            if !frame.end.is_some_and(is_natural_end) {
                let resume = Some(frame.cursor.clone());
                page_to_end(ctx, request, expected, &mut received, resume).await;
            }
            assert_eq!(
                received,
                expected,
                "cut after frame {cut} of {}: {request:?}",
                page.len()
            );
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
        let frames = run(ctx, request, resume).await;
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
        resume = Some(last.cursor.clone());
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
            assert_cursor_bound_claim(&base, &all, mock, checkpoint_hi_exclusive).await;
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
    let position = match base.endpoint {
        Endpoint::Events => Position::Events {
            tx_seq: BOUND_TX,
            event_index: 0,
        },
        Endpoint::Transactions => Position::Transactions { tx_seq: BOUND_TX },
        Endpoint::Checkpoints => unreachable!("checkpoint positions are not modeled"),
    };
    let request = Request {
        far_bound: Some(CursorToken::boundary(position).encode()),
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

async fn assert_all_modes(endpoint: Endpoint) {
    let sparse = MockBigtableServer::new();
    sparse_ledger(&sparse).await;
    let dense = MockBigtableServer::new();
    dense_ledger(&dense).await;
    for mock in [&sparse, &dense] {
        mock.set_read_rows_latency(READ_LATENCY).await;
    }
    let mut most_advancing_beacons = 0;
    for ascending in [true, false] {
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
async fn list_events_resumes_after_any_frame() {
    assert_all_modes(Endpoint::Events).await;
}

#[tokio::test]
async fn list_transactions_resumes_after_any_frame() {
    assert_all_modes(Endpoint::Transactions).await;
}

#[tokio::test]
async fn list_checkpoints_resumes_after_any_frame() {
    assert_all_modes(Endpoint::Checkpoints).await;
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
                let paged = page_to_end(&minting, request, &in_bounds, &mut received, None).await;
                assert_eq!(received, in_bounds, "{request:?}");
                closing = paged.cursors.last().cloned();
                cursors.extend(paged.cursors);
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
            page_to_end(&paging, &unbounded, &excluded, &mut received, closing).await;
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
                let reason =
                    page_to_end(&paging, &opposite, &expected, &mut received, Some(cursor))
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
