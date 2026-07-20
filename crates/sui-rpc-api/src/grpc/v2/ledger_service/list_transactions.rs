// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures::StreamExt;
use futures::stream::BoxStream;
use mysten_common::ZipDebugEqIteratorExt;
use sui_inverted_index::BitmapQuery;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::ListTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2::ListTransactionsResponse;
use sui_rpc::proto::sui::rpc::v2::QueryEnd;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2::Watermark;
use sui_rpc_cursor::Position;
use sui_sdk_types::Digest;
use sui_types::storage::LedgerTxSeqDigest;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::RpcError;
use crate::RpcService;
use crate::grpc::v2::ledger_service::get_transaction::render_executed_transaction;
use crate::ledger_history::filter::transaction_filter_to_query;
use crate::ledger_history::query_options::CheckpointRange;
use crate::ledger_history::query_options::QueryOptions;
use crate::ledger_history::query_options::RangeExhaustion;
use crate::ledger_history::query_options::ResolvedRange;
use crate::ledger_history::watermark::ScanTerminal;
use crate::ledger_history::watermark::advance_covered_bound_before_checkpoint;
use crate::ledger_history::watermark::boundary_watermark;
use crate::ledger_history::watermark::item_watermark;
use crate::ledger_history::watermark::scan_frontier_cursor_cp;
use crate::metrics::{
    ListChunkSetupTimer, ListRequestMetrics, ListStreamMetrics, list_chunks_per_spawn,
};
use crate::read_mask_defaults;

use super::bitmap_scan::LedgerBitmapKind;
use super::bitmap_scan::PendingBitmapBucket;
use super::bitmap_scan::TX_BITMAP_BUCKET_SIZE;
use super::bitmap_scan::drain_bitmap_hits_with_budget;
use super::chunked_scan::ChunkArgs;
use super::chunked_scan::ChunkedScan;
use super::chunked_scan::ScanChunkDone;
use super::chunked_scan::cancelled;
use super::chunked_scan::scan_limit_or_range;
use super::chunked_scan::run_list_chunk_in_admission;
use super::chunked_scan::should_coalesce_next_chunk;
use super::chunked_scan::spawn_list_chunk_admission;
use super::chunked_scan::spawn_list_chunk;
use super::ledger_read::checkpoint_hi_exclusive;
use super::ledger_read::checkpoint_to_tx_boundary;
use super::ledger_read::checkpoint_to_tx_range;
use super::ledger_read::clamp_to_serving_floor;
use super::ledger_read::get_tx_seq_digest_multi;
use super::ledger_read::get_tx_seq_digest_rows;
use super::ledger_read::remaining_range_after;
use super::ledger_read::sequence_frontier_checkpoint;
use super::ledger_read::validate_checkpoint_bounds;
use super::object_set::RequestObjectCache;
use super::object_set::fetch_object_sets_for_chunk;
use super::object_set::mask_requests_object_set;

const METHOD: &str = "list_transactions";

fn resolution(read_mask: &FieldMaskTree) -> &'static str {
    if mask_requests_object_set(read_mask) {
        "full_objects"
    } else if should_render_transaction_contents(read_mask) {
        "full"
    } else {
        "digest"
    }
}

pub(crate) type ListTransactionsStream =
    BoxStream<'static, Result<ListTransactionsResponse, RpcError>>;

pub(crate) async fn list_transactions(
    service: RpcService,
    request: ListTransactionsRequest,
) -> Result<ListTransactionsStream, RpcError> {
    let started = Instant::now();
    let start_checkpoint = request.start_checkpoint;
    let end_checkpoint = request.end_checkpoint;
    let filter = request.filter;
    let request_options = request.options;
    let filtered = filter.is_some();
    validate_checkpoint_bounds(start_checkpoint, end_checkpoint)?;
    let read_mask = read_mask_defaults::validate_read_mask::<ExecutedTransaction>(
        request.read_mask,
        read_mask_defaults::TRANSACTION,
    )?;
    let mut request_metrics = ListRequestMetrics::new(
        service
            .list_metrics
            .as_ref()
            .map(|metrics| metrics.stream_metrics(METHOD, resolution(&read_mask))),
        started,
    );
    let ledger_history = service.config.ledger_history();
    let endpoint = ledger_history.list_transactions();
    let bitmap_bucket_scan_budget = ledger_history.bitmap_bucket_scan_budget();
    let chunk_bucket_scan_budget = ledger_history.chunk_bucket_scan_budget();
    let max_bitmap_filter_literals = ledger_history.max_bitmap_filter_literals();
    let options = QueryOptions::transactions_from_proto(
        request_options.as_ref(),
        endpoint.default_limit_items,
        endpoint.max_limit_items,
    )?;
    let limit_items = options.limit_items;
    let ordering = options.ordering;
    let filter_query = filter
        .as_ref()
        .map(|filter| transaction_filter_to_query(filter, max_bitmap_filter_literals))
        .transpose()?;

    let initial_state = TransactionScanState::Init {
        start_checkpoint,
        end_checkpoint,
        filter_query,
    };

    let terminal_options = options.clone();
    let chunk_metrics = request_metrics.chunk_metrics();
    if list_chunks_per_spawn() == 2 {
        // This experiment intentionally changes first-byte timing and prefetch
        // overlap: the first chunk is not yielded until both chunks complete.
        // Campaign results from this arm are architectural sensitivity data only.
        return Ok(async_stream::try_stream! {
            let render_contents = should_render_transaction_contents(&read_mask);
            let object_cache = Arc::new(Mutex::new(RequestObjectCache::default()));
            let mut scan = ChunkedScan::new(
                CoalescedTransactionScanState::Ready(initial_state),
                limit_items,
                endpoint.chunk_max,
                bitmap_bucket_scan_budget,
                move |state, args: ChunkArgs| {
                    spawn_coalesced_transaction_chunk(
                        service.clone(),
                        chunk_metrics.clone(),
                        state,
                        read_mask.clone(),
                        options.clone(),
                        args.scan_budget,
                        chunk_bucket_scan_budget,
                        args.chunk_item_limit,
                        args.remaining_request_item_limit,
                        render_contents,
                        object_cache.clone(),
                        args.cancel,
                    )
                },
            );

            let mut covered_checkpoint_bound = None;
            while let Some(mut response) = scan.next_item().await? {
                if let Some(checkpoint) = response
                    .watermark
                    .as_ref()
                    .and_then(|watermark| watermark.checkpoint)
                {
                    covered_checkpoint_bound = Some(checkpoint);
                }
                let is_data = response.transaction.is_some();
                let ends_at_item_limit =
                    is_data && scan.produced() == limit_items && scan.exhausted();
                if ends_at_item_limit {
                    let mut end = QueryEnd::default();
                    end.reason = Some(QueryEndReason::ItemLimit as i32);
                    response.end = Some(end);
                    request_metrics.finish_success(
                        QueryEndReason::ItemLimit,
                        filtered.then(|| scan.bitmap_buckets_evaluated()),
                    );
                }
                request_metrics.observe_frame(&response, is_data);
                let yield_started = request_metrics.yield_clock();
                yield response;
                request_metrics.observe_yield_wait(yield_started);
            }

            let produced = scan.produced();
            let bitmap_buckets_evaluated = filtered.then(|| scan.bitmap_buckets_evaluated());
            let chunk_terminal = scan.into_terminal().expect("query emits terminal state");
            let terminal_reason = super::query_end::effective_terminal_reason(
                produced,
                limit_items,
                chunk_terminal.reason(),
            );
            if terminal_reason != QueryEndReason::ItemLimit {
                let terminal_watermark =
                    chunk_terminal.into_watermark(&terminal_options, covered_checkpoint_bound);
                let response = end_response(terminal_watermark, terminal_reason);
                request_metrics.observe_frame(&response, false);
                request_metrics.finish_success(terminal_reason, bitmap_buckets_evaluated);
                let yield_started = request_metrics.yield_clock();
                yield response;
                request_metrics.observe_yield_wait(yield_started);
            }
            info!(
                filtered,
                limit_items,
                ?ordering,
                emitted = produced,
                ?terminal_reason,
                elapsed_ms = started.elapsed().as_millis(),
                "list_transactions: done"
            );
        }
        .boxed());
    }

    Ok(async_stream::try_stream! {
        let render_contents = should_render_transaction_contents(&read_mask);
        let object_cache = Arc::new(Mutex::new(RequestObjectCache::default()));
        let mut scan = ChunkedScan::new(
            initial_state,
            limit_items,
            endpoint.chunk_max,
            bitmap_bucket_scan_budget,
            move |state, args: ChunkArgs| {
                spawn_transaction_chunk(
                    service.clone(),
                    chunk_metrics.clone(),
                    state,
                    read_mask.clone(),
                    options.clone(),
                    args.scan_budget,
                    chunk_bucket_scan_budget,
                    args.chunk_item_limit,
                    args.remaining_request_item_limit,
                    render_contents,
                    object_cache.clone(),
                    args.cancel,
                )
            },
        );

        let mut covered_checkpoint_bound = None;
        while let Some(mut response) = scan.next_item().await? {
            if let Some(checkpoint) = response
                .watermark
                .as_ref()
                .and_then(|watermark| watermark.checkpoint)
            {
                covered_checkpoint_bound = Some(checkpoint);
            }
            let is_data = response.transaction.is_some();
            let ends_at_item_limit =
                is_data && scan.produced() == limit_items && scan.exhausted();
            if ends_at_item_limit {
                let mut end = QueryEnd::default();
                end.reason = Some(QueryEndReason::ItemLimit as i32);
                response.end = Some(end);
                request_metrics.finish_success(
                    QueryEndReason::ItemLimit,
                    filtered.then(|| scan.bitmap_buckets_evaluated()),
                );
            }
            request_metrics.observe_frame(&response, is_data);
            let yield_started = request_metrics.yield_clock();
            yield response;
            request_metrics.observe_yield_wait(yield_started);
        }

        let produced = scan.produced();
        let bitmap_buckets_evaluated = filtered.then(|| scan.bitmap_buckets_evaluated());
        let chunk_terminal = scan.into_terminal().expect("query emits terminal state");
        let terminal_reason = super::query_end::effective_terminal_reason(
            produced,
            limit_items,
            chunk_terminal.reason(),
        );
        if terminal_reason != QueryEndReason::ItemLimit {
            let terminal_watermark =
                chunk_terminal.into_watermark(&terminal_options, covered_checkpoint_bound);
            let response = end_response(terminal_watermark, terminal_reason);
            request_metrics.observe_frame(&response, false);
            request_metrics.finish_success(terminal_reason, bitmap_buckets_evaluated);
            let yield_started = request_metrics.yield_clock();
            yield response;
            request_metrics.observe_yield_wait(yield_started);
        }
        info!(
            filtered,
            limit_items,
            ?ordering,
            emitted = produced,
            ?terminal_reason,
            elapsed_ms = started.elapsed().as_millis(),
            "list_transactions: done"
        );
    }
    .boxed())
}

fn spawn_transaction_chunk(
    service: RpcService,
    metrics: Option<ListStreamMetrics>,
    state: TransactionScanState,
    read_mask: FieldMaskTree,
    options: QueryOptions,
    scan_budget: usize,
    chunk_scan_budget: usize,
    chunk_item_limit: usize,
    remaining_request_item_limit: usize,
    render_contents: bool,
    object_cache: Arc<Mutex<RequestObjectCache>>,
    cancel: CancellationToken,
) -> JoinHandle<Result<TransactionChunkDone, RpcError>> {
    spawn_list_chunk(metrics, move |metrics, setup_timer| {
        next_transaction_chunk(
            service,
            state,
            read_mask,
            options,
            render_contents,
            &object_cache,
            scan_budget,
            chunk_scan_budget,
            chunk_item_limit,
            remaining_request_item_limit,
            &cancel,
            metrics,
            setup_timer,
        )
    })
}

#[derive(Clone)]
enum TransactionScanState {
    Init {
        start_checkpoint: Option<u64>,
        end_checkpoint: Option<u64>,
        filter_query: Option<BitmapQuery>,
    },
    Unfiltered {
        range: Range<u64>,
        entry_checkpoint: u64,
        exhaustion: RangeExhaustion,
        end_checkpoint: u64,
        end_position: u64,
    },
    Filtered {
        query: BitmapQuery,
        range: Option<Range<u64>>,
        pending_bucket: Option<PendingBitmapBucket>,
        entry_checkpoint: u64,
        exhaustion: RangeExhaustion,
        end_checkpoint: u64,
        end_position: u64,
    },
}

type TransactionChunkDone = ScanChunkDone<TransactionScanState, ListTransactionsResponse>;

enum CoalescedTransactionScanState {
    Ready(TransactionScanState),
    Prefetched(Box<Result<TransactionChunkDone, RpcError>>),
}

type CoalescedTransactionChunkDone =
    ScanChunkDone<CoalescedTransactionScanState, ListTransactionsResponse>;

fn into_coalesced_transaction_chunk_done(
    done: TransactionChunkDone,
) -> CoalescedTransactionChunkDone {
    ScanChunkDone {
        items: done.items,
        produced: done.produced,
        next_state: done.next_state.map(CoalescedTransactionScanState::Ready),
        terminal: done.terminal,
        remaining_scan_budget: done.remaining_scan_budget,
    }
}

fn spawn_coalesced_transaction_chunk(
    service: RpcService,
    metrics: Option<ListStreamMetrics>,
    state: CoalescedTransactionScanState,
    read_mask: FieldMaskTree,
    options: QueryOptions,
    scan_budget: usize,
    chunk_scan_budget: usize,
    chunk_item_limit: usize,
    remaining_request_item_limit: usize,
    render_contents: bool,
    object_cache: Arc<Mutex<RequestObjectCache>>,
    cancel: CancellationToken,
) -> JoinHandle<Result<CoalescedTransactionChunkDone, RpcError>> {
    let state = match state {
        CoalescedTransactionScanState::Ready(state) => state,
        CoalescedTransactionScanState::Prefetched(prefetched) => {
            return tokio::spawn(async move {
                (*prefetched).map(into_coalesced_transaction_chunk_done)
            });
        }
    };

    spawn_list_chunk_admission(metrics, move |metrics| {
        let first = run_list_chunk_in_admission(metrics, |metrics, setup_timer| {
            next_transaction_chunk(
                service.clone(),
                state,
                read_mask.clone(),
                options.clone(),
                render_contents,
                &object_cache,
                scan_budget,
                chunk_scan_budget,
                chunk_item_limit,
                remaining_request_item_limit,
                &cancel,
                metrics,
                setup_timer,
            )
        })?;

        let remaining_item_budget =
            remaining_request_item_limit.saturating_sub(first.produced);
        if !should_coalesce_next_chunk(
            2,
            first.next_state.is_none(),
            remaining_item_budget,
            first.remaining_scan_budget,
        ) {
            return Ok(into_coalesced_transaction_chunk_done(first));
        }

        let mut first = first;
        let second_state = first
            .next_state
            .take()
            .expect("coalescing decision requires a next chunk state");
        let second_chunk_item_limit = chunk_item_limit.min(remaining_item_budget);
        let second = run_list_chunk_in_admission(metrics, |metrics, setup_timer| {
            next_transaction_chunk(
                service,
                second_state,
                read_mask,
                options,
                render_contents,
                &object_cache,
                first.remaining_scan_budget,
                chunk_scan_budget,
                second_chunk_item_limit,
                remaining_item_budget,
                &cancel,
                metrics,
                setup_timer,
            )
        });
        if let Some(metrics) = metrics {
            metrics.observe_coalesced_admission();
        }

        Ok(ScanChunkDone {
            items: first.items,
            produced: first.produced,
            next_state: Some(CoalescedTransactionScanState::Prefetched(Box::new(
                second,
            ))),
            terminal: first.terminal,
            remaining_scan_budget: first.remaining_scan_budget,
        })
    })
}

fn next_transaction_chunk(
    service: RpcService,
    mut state: TransactionScanState,
    read_mask: FieldMaskTree,
    options: QueryOptions,
    render_contents: bool,
    object_cache: &Mutex<RequestObjectCache>,
    scan_budget: usize,
    chunk_scan_budget: usize,
    chunk_item_limit: usize,
    remaining_request_item_limit: usize,
    cancel: &CancellationToken,
    metrics: Option<&ListStreamMetrics>,
    setup_timer: &mut ListChunkSetupTimer,
) -> Result<TransactionChunkDone, RpcError> {
    let ascending = options.is_ascending();
    let mut remaining_scan_budget = scan_budget;
    let (rows, next_state, terminal, scan_watermark, entry_checkpoint) = loop {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        match state {
            TransactionScanState::Init {
                start_checkpoint,
                end_checkpoint,
                filter_query,
            } => {
                let checkpoint_range = CheckpointRange::from_request(
                    start_checkpoint,
                    end_checkpoint,
                    checkpoint_hi_exclusive(&service)?,
                )?;
                let tx_range =
                    resolve_tx_range(&service, start_checkpoint, checkpoint_range, &options)?;
                let entry_checkpoint = tx_range.entry_checkpoint;
                let terminal = ScanTerminal::from_range_exhaustion(
                    tx_range.exhaustion,
                    Position::Transactions {
                        checkpoint: tx_range.end_checkpoint,
                        tx_seq: tx_range.end_position,
                    },
                    tx_range.is_empty(),
                );
                let range = tx_range.range;
                if range.is_empty() {
                    return Ok(TransactionChunkDone {
                        items: Vec::new(),
                        produced: 0,
                        next_state: None,
                        terminal,
                        remaining_scan_budget,
                    });
                }
                state = match filter_query {
                    Some(query) => TransactionScanState::Filtered {
                        query,
                        range: Some(range),
                        entry_checkpoint,
                        pending_bucket: None,
                        exhaustion: tx_range.exhaustion,
                        end_checkpoint: tx_range.end_checkpoint,
                        end_position: tx_range.end_position,
                    },
                    None => TransactionScanState::Unfiltered {
                        range,
                        entry_checkpoint,
                        exhaustion: tx_range.exhaustion,
                        end_checkpoint: tx_range.end_checkpoint,
                        end_position: tx_range.end_position,
                    },
                };
                continue;
            }
            TransactionScanState::Unfiltered {
                range,
                entry_checkpoint,
                exhaustion,
                end_checkpoint,
                end_position,
            } => {
                let rows =
                    get_tx_seq_digest_rows(&service, range.clone(), !ascending, chunk_item_limit)?;
                let next_state = rows
                    .last()
                    .and_then(|row| remaining_range_after(range, row.tx_sequence_number, ascending))
                    .map(|range| TransactionScanState::Unfiltered {
                        range,
                        entry_checkpoint,
                        exhaustion,
                        end_checkpoint,
                        end_position,
                    });
                let terminal = ScanTerminal::from_range_exhaustion(
                    exhaustion,
                    Position::Transactions {
                        checkpoint: end_checkpoint,
                        tx_seq: end_position,
                    },
                    false,
                );
                break (rows, next_state, terminal, None, entry_checkpoint);
            }
            TransactionScanState::Filtered {
                query,
                range,
                entry_checkpoint,
                pending_bucket,
                exhaustion,
                end_checkpoint,
                end_position,
            } => {
                let hit_limit = chunk_item_limit.min(remaining_request_item_limit);
                let chunk_scan_budget = remaining_scan_budget.min(chunk_scan_budget);
                let hits = drain_bitmap_hits_with_budget(
                    service.clone(),
                    LedgerBitmapKind::Transaction,
                    TX_BITMAP_BUCKET_SIZE,
                    query.clone(),
                    pending_bucket,
                    range,
                    options.scan_direction(),
                    hit_limit,
                    chunk_scan_budget,
                    cancel,
                )?;
                remaining_scan_budget -= hits.buckets_scanned;
                if let Some(metrics) = metrics {
                    metrics.observe_chunk_buckets_decoded(hits.buckets_scanned);
                }
                if cancel.is_cancelled() {
                    return Err(cancelled());
                }
                let chunk_scan_limit_reached = hits.chunk_scan_limit_reached;
                let coalesced_frontier = hits.coalesced_frontier;
                // A chunk scan-limit only ends the request when the request
                // budget is also exhausted, or when there is no continuation.
                let request_scan_limit_reached = chunk_scan_limit_reached
                    && (remaining_scan_budget == 0
                        || (hits.next_range.is_none() && hits.pending_bucket.is_none()));
                let rows = get_tx_seq_digest_multi(&service, &hits.items)?;
                let next_state = if request_scan_limit_reached {
                    None
                } else {
                    (hits.pending_bucket.is_some() || hits.next_range.is_some()).then_some(
                        TransactionScanState::Filtered {
                            query,
                            entry_checkpoint,
                            range: hits.next_range,
                            pending_bucket: hits.pending_bucket,
                            exhaustion,
                            end_checkpoint,
                            end_position,
                        },
                    )
                };
                let coalesced_frontier = if chunk_scan_limit_reached {
                    Some(coalesced_frontier.ok_or_else(|| {
                        RpcError::new(
                            tonic::Code::Internal,
                            "transaction scan limit missing authoritative frontier",
                        )
                    })?)
                } else {
                    None
                };
                let frontier_watermark = if request_scan_limit_reached
                    || (chunk_scan_limit_reached && rows.is_empty())
                {
                    Some(scan_transaction_watermark(
                        &service,
                        &options,
                        coalesced_frontier.expect("checked for scan-limit chunk"),
                        entry_checkpoint,
                        ascending,
                    )?)
                } else {
                    None
                };
                let scan_watermark = if !request_scan_limit_reached && rows.is_empty() {
                    frontier_watermark.clone().map(watermark_response)
                } else {
                    None
                };
                let terminal_position = Position::Transactions {
                    checkpoint: end_checkpoint,
                    tx_seq: end_position,
                };
                let terminal = scan_limit_or_range(
                    request_scan_limit_reached,
                    exhaustion,
                    terminal_position,
                    || {
                        frontier_watermark.ok_or_else(|| {
                            RpcError::new(
                                tonic::Code::Internal,
                                "request scan limit missing transaction frontier watermark",
                            )
                        })
                    },
                )?;
                break (rows, next_state, terminal, scan_watermark, entry_checkpoint);
            }
        }
    };
    setup_timer.finish_setup();

    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let mut items = render_transaction_rows(
        &service,
        rows,
        &read_mask,
        &options,
        entry_checkpoint,
        render_contents,
        object_cache,
        cancel,
        metrics,
    )?;
    let produced = items.len();
    if let Some(watermark) = scan_watermark {
        items.push(watermark);
    }
    Ok(TransactionChunkDone {
        items,
        produced,
        next_state,
        terminal,
        remaining_scan_budget,
    })
}

/// Scan watermark for a filtered chunk whose scan budget ran out mid-gap.
/// A transaction's member id is its own `tx_sequence_number`, so the frontier
/// decodes to itself. Checkpoint coverage is independent: at the ascending
/// genesis frontier there is no completed checkpoint, but `(0, 0)` is still
/// the authoritative safe resume cursor.
fn scan_transaction_watermark(
    service: &RpcService,
    options: &QueryOptions,
    frontier: u64,
    entry_checkpoint: u64,
    ascending: bool,
) -> Result<Watermark, RpcError> {
    transaction_frontier_watermark(
        options,
        frontier,
        entry_checkpoint,
        sequence_frontier_checkpoint(service, frontier, ascending)?,
    )
}

fn transaction_frontier_watermark(
    options: &QueryOptions,
    frontier: u64,
    entry_checkpoint: u64,
    checkpoint: Option<u64>,
) -> Result<Watermark, RpcError> {
    let boundary = checkpoint.and_then(|cp| {
        advance_covered_bound_before_checkpoint(None, cp, entry_checkpoint, options)
    });
    let cursor_cp = scan_frontier_cursor_cp(checkpoint, frontier, options.scan_direction())
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                format!("transaction scan frontier {frontier} has no checkpoint mapping"),
            )
        })?;
    Ok(boundary_watermark(
        Position::Transactions {
            checkpoint: cursor_cp,
            tx_seq: frontier,
        },
        boundary,
    ))
}

fn render_transaction_rows(
    service: &RpcService,
    rows: Vec<LedgerTxSeqDigest>,
    read_mask: &FieldMaskTree,
    options: &QueryOptions,
    entry_checkpoint: u64,
    render_contents: bool,
    object_cache: &Mutex<RequestObjectCache>,
    cancel: &CancellationToken,
    metrics: Option<&ListStreamMetrics>,
) -> Result<Vec<ListTransactionsResponse>, RpcError> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let mut transaction_reads_and_objects = if render_contents {
        let items = rows
            .iter()
            .map(|row| (row.digest.into(), row.checkpoint_number))
            .collect::<Vec<(Digest, u64)>>();
        let read_started = metrics.map(|_| Instant::now());
        let (reads, stats) = service.reader.multi_get_transaction_reads(&items)?;
        let (object_sets, object_fetch_stats) =
            fetch_object_sets_for_chunk(&service.reader, &reads, read_mask, object_cache)?;
        if let (Some(metrics), Some(read_started)) = (metrics, read_started) {
            metrics.observe_chunk_read(read_started.elapsed());
            metrics.observe_store_read_batch("transactions", rows.len());
            metrics.observe_store_read_batch("effects", rows.len());
            metrics.observe_store_read_batch("events", rows.len());
            metrics.observe_store_read_batch("unchanged_loaded_runtime_objects", rows.len());
            metrics.observe_store_read_batch("checkpoint_summaries", stats.checkpoint_summary_keys);
            if mask_requests_object_set(read_mask) {
                metrics.observe_store_read_batch("objects", object_fetch_stats.store_keys);
                metrics.observe_object_cache_hits(object_fetch_stats.cache_hits);
            }
        }
        reads.into_iter().zip_debug_eq(object_sets)
    } else {
        Vec::new().into_iter().zip_debug_eq(Vec::new())
    };

    let mut items = Vec::with_capacity(rows.len());
    // Per-chunk running boundary; monotonic across chunks because rows are
    // emitted in scan-checkpoint order.
    let mut checkpoint_boundary: Option<u64> = None;
    for row in rows {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let render_started = metrics.map(|_| Instant::now());
        checkpoint_boundary = advance_covered_bound_before_checkpoint(
            checkpoint_boundary,
            row.checkpoint_number,
            entry_checkpoint,
            options,
        );
        let watermark = item_watermark(
            Position::Transactions {
                checkpoint: row.checkpoint_number,
                tx_seq: row.tx_sequence_number,
            },
            checkpoint_boundary,
        );
        let response = if render_contents {
            let (transaction_read, object_set) = transaction_reads_and_objects
                .next()
                .expect("transaction reads and object sets match tx_seq rows");
            let transaction = render_executed_transaction(
                service,
                transaction_read,
                &object_set,
                row.checkpoint_number,
                read_mask,
            )?;
            transaction_item_response(watermark, transaction, row.tx_offset, read_mask)
        } else {
            let mut transaction = ExecutedTransaction::default();
            if read_mask.contains(ExecutedTransaction::DIGEST_FIELD.name) {
                transaction.digest = Some(row.digest.to_string());
            }
            if read_mask.contains(ExecutedTransaction::CHECKPOINT_FIELD.name) {
                transaction.checkpoint = Some(row.checkpoint_number);
            }
            transaction_item_response(watermark, transaction, row.tx_offset, read_mask)
        };
        items.push(response);
        if let (Some(metrics), Some(render_started)) = (metrics, render_started) {
            metrics.observe_render(render_started.elapsed());
        }
    }
    Ok(items)
}

fn should_render_transaction_contents(read_mask: &FieldMaskTree) -> bool {
    // `digest`, `checkpoint`, and `transaction_index` are all available from the
    // tx_seq_digest index row, so a mask limited to them skips the full
    // transaction fetch.
    let paths = read_mask.to_field_mask().paths;
    paths.is_empty()
        || paths.len() > 3
        || paths.iter().any(|path| {
            path != ExecutedTransaction::DIGEST_FIELD.name
                && path != ExecutedTransaction::CHECKPOINT_FIELD.name
                && path != ExecutedTransaction::TRANSACTION_INDEX_FIELD.name
        })
}

fn resolve_tx_range(
    service: &RpcService,
    start_checkpoint: Option<u64>,
    checkpoint_range: CheckpointRange,
    options: &QueryOptions,
) -> Result<ResolvedRange, RpcError> {
    let cp_range = checkpoint_range.resolve(options);
    if cp_range.is_empty() {
        let tx_boundary =
            checkpoint_to_tx_boundary(service, cp_range.terminal_checkpoint(options.ordering))?;
        return Ok(cp_range.with_range(tx_boundary..tx_boundary, options.ordering));
    }

    let tx_range = checkpoint_to_tx_range(service, cp_range.range.clone())?;
    let resolved = cp_range.with_range(tx_range, options.ordering);
    let mut resolved = options.apply_cursor_bounds(resolved);
    if !resolved.range.is_empty()
        && let Some(floor) =
            clamp_to_serving_floor(service, resolved.range.start, start_checkpoint, options)?
    {
        resolved.apply_serving_floor(floor.tx_seq, floor.checkpoint, options);
    }
    Ok(resolved)
}

fn transaction_item_response(
    watermark: Watermark,
    mut transaction: ExecutedTransaction,
    tx_offset: u32,
    read_mask: &FieldMaskTree,
) -> ListTransactionsResponse {
    // The within-checkpoint position rides on the `ExecutedTransaction` rather
    // than the response frame; populate it only when the read mask requests it.
    if read_mask.contains(ExecutedTransaction::TRANSACTION_INDEX_FIELD.name) {
        transaction.transaction_index = Some(tx_offset as u64);
    }

    let mut response = ListTransactionsResponse::default();
    response.transaction = Some(transaction);
    response.watermark = Some(watermark);
    response
}

fn watermark_response(watermark: Watermark) -> ListTransactionsResponse {
    let mut response = ListTransactionsResponse::default();
    response.watermark = Some(watermark);
    response
}

fn end_response(watermark: Watermark, reason: QueryEndReason) -> ListTransactionsResponse {
    let mut end = QueryEnd::default();
    end.reason = Some(reason as i32);

    let mut response = ListTransactionsResponse::default();
    response.watermark = Some(watermark);
    response.end = Some(end);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_rpc::proto::sui::rpc::v2::Ordering;
    use sui_rpc::proto::sui::rpc::v2::QueryOptions as ProtoQueryOptions;
    use sui_rpc_cursor::CursorToken;

    fn options(ascending: bool) -> QueryOptions {
        let mut proto = ProtoQueryOptions::default();
        if !ascending {
            proto.ordering = Some(Ordering::Descending as i32);
        }
        QueryOptions::transactions_from_proto(Some(&proto), 100, 100).unwrap()
    }

    #[test]
    fn resolution_uses_validated_read_mask() {
        use sui_rpc::field::FieldMask;
        use sui_rpc::field::FieldMaskUtil;

        for (mask, expected) in [
            ("digest", "digest"),
            ("transaction", "full"),
            ("balance_changes", "full_objects"),
            ("effects", "full_objects"),
            ("transaction,effects", "full_objects"),
        ] {
            let read_mask = read_mask_defaults::validate_read_mask::<ExecutedTransaction>(
                Some(FieldMask::from_str(mask)),
                read_mask_defaults::TRANSACTION,
            )
            .unwrap();

            assert_eq!(resolution(&read_mask), expected, "read mask: {mask}");
        }
    }

    #[test]
    fn scan_limit_terminal_frames_are_directional_transaction_cursors() {
        for (ascending, frontier, checkpoint, expected_position, expected_proof) in [
            (
                true,
                0,
                None,
                Position::Transactions {
                    checkpoint: 0,
                    tx_seq: 0,
                },
                None,
            ),
            (
                true,
                41,
                Some(7),
                Position::Transactions {
                    checkpoint: 7,
                    tx_seq: 41,
                },
                None,
            ),
            (
                true,
                42,
                Some(9),
                Position::Transactions {
                    checkpoint: 9,
                    tx_seq: 42,
                },
                None,
            ),
            (
                false,
                u64::MAX,
                None,
                Position::Transactions {
                    checkpoint: u64::MAX,
                    tx_seq: u64::MAX,
                },
                None,
            ),
            (
                false,
                19,
                Some(7),
                Position::Transactions {
                    checkpoint: 8,
                    tx_seq: 19,
                },
                None,
            ),
            (
                false,
                18,
                Some(5),
                Position::Transactions {
                    checkpoint: 6,
                    tx_seq: 18,
                },
                None,
            ),
        ] {
            let options = options(ascending);
            let entry_checkpoint = checkpoint.unwrap_or(if ascending { 0 } else { u64::MAX });
            let watermark =
                transaction_frontier_watermark(&options, frontier, entry_checkpoint, checkpoint)
                    .unwrap();
            assert_eq!(
                CursorToken::decode(
                    watermark
                        .cursor
                        .as_ref()
                        .expect("transaction frontier cursor")
                )
                .unwrap(),
                CursorToken::boundary(expected_position)
            );
            assert_eq!(watermark.checkpoint, expected_proof);
            let terminal = ScanTerminal::ScanLimit { watermark };
            let response = end_response(
                terminal.into_watermark(&options, Some(123)),
                QueryEndReason::ScanLimit,
            );
            assert!(response.transaction.is_none());
            assert_eq!(
                response.watermark.as_ref().and_then(|wm| wm.checkpoint),
                expected_proof
            );
            assert_eq!(
                response.end.as_ref().map(|end| end.reason()),
                Some(QueryEndReason::ScanLimit)
            );
        }
    }

    async fn assert_error_terminates_response_driver(
        expected_code: tonic::Code,
        expected_message: &'static str,
    ) {
        let scan = ChunkedScan::new(0usize, 5, 1, 10, move |state, args: ChunkArgs| {
            tokio::task::spawn(async move {
                if state == 0 {
                    let mut transaction = ExecutedTransaction::default();
                    transaction.digest = Some("successful-transaction".into());
                    let mut watermark = Watermark::default();
                    watermark.checkpoint = Some(7);
                    let mut response = ListTransactionsResponse::default();
                    response.transaction = Some(transaction);
                    response.watermark = Some(watermark);
                    Ok(ScanChunkDone {
                        items: vec![response],
                        produced: 1,
                        next_state: Some(1),
                        terminal: ScanTerminal::from_range_exhaustion(
                            RangeExhaustion::CheckpointBound,
                            Position::Transactions {
                                checkpoint: 8,
                                tx_seq: 42,
                            },
                            false,
                        ),
                        remaining_scan_budget: args.scan_budget,
                    })
                } else if expected_code == tonic::Code::Cancelled {
                    Err(cancelled())
                } else {
                    Err(RpcError::new(expected_code, expected_message))
                }
            })
        });
        let terminal_options = options(true);
        let limit_items = 5;
        // Mirrors the endpoint response loop above; keep terminal ordering in sync.
        let mut responses: BoxStream<'_, Result<ListTransactionsResponse, RpcError>> =
            async_stream::try_stream! {
                let mut scan = scan;
                let mut covered_checkpoint_bound = None;
                while let Some(mut response) = scan.next_item().await? {
                    if let Some(checkpoint) = response
                        .watermark
                        .as_ref()
                        .and_then(|watermark| watermark.checkpoint)
                    {
                        covered_checkpoint_bound = Some(checkpoint);
                    }
                    if response.transaction.is_some()
                        && scan.produced() == limit_items
                        && scan.exhausted()
                    {
                        let mut end = QueryEnd::default();
                        end.reason = Some(QueryEndReason::ItemLimit as i32);
                        response.end = Some(end);
                    }
                    yield response;
                }

                let produced = scan.produced();
                let chunk_terminal = scan.into_terminal().expect("query emits terminal state");
                let terminal_reason = super::super::query_end::effective_terminal_reason(
                    produced,
                    limit_items,
                    chunk_terminal.reason(),
                );
                if terminal_reason != QueryEndReason::ItemLimit {
                    let terminal_watermark =
                        chunk_terminal.into_watermark(&terminal_options, covered_checkpoint_bound);
                    yield end_response(terminal_watermark, terminal_reason);
                }
            }
            .boxed();

        let response = responses
            .next()
            .await
            .expect("successful response precedes worker error")
            .expect("first response is successful");
        assert_eq!(
            response
                .transaction
                .as_ref()
                .and_then(|transaction| transaction.digest.as_deref()),
            Some("successful-transaction")
        );
        assert_eq!(
            response
                .watermark
                .as_ref()
                .and_then(|watermark| watermark.checkpoint),
            Some(7)
        );
        assert!(
            response.end.is_none(),
            "the endpoint driver must not attach a clean end before a worker error"
        );

        let error = responses
            .next()
            .await
            .expect("worker error is the next stream result")
            .expect_err("worker error must not become a QueryEnd response");
        let status = tonic::Status::from(error);
        assert_eq!(status.code(), expected_code);
        assert_eq!(status.message(), expected_message);
        assert!(
            responses.next().await.is_none(),
            "endpoint terminal construction must be unreachable after the error"
        );
    }

    #[tokio::test]
    async fn response_driver_ends_with_internal_status_after_successful_frame() {
        assert_error_terminates_response_driver(tonic::Code::Internal, "injected scan fault").await;
    }

    #[tokio::test]
    async fn response_driver_ends_with_cancelled_status_after_successful_frame() {
        assert_error_terminates_response_driver(tonic::Code::Cancelled, "request cancelled").await;
    }
}
