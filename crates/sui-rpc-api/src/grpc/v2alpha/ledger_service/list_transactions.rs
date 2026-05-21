// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::time::Instant;

use futures::StreamExt;
use futures::stream::BoxStream;
use prost_types::FieldMask;
use sui_inverted_index::BitmapQuery;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEnd;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionItem;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc::proto::sui::rpc::v2alpha::list_transactions_response;
use sui_sdk_types::Digest;
use sui_types::storage::LedgerTxSeqDigest;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use crate::grpc::v2::ledger_service::get_transaction::render_executed_transaction;
use crate::ledger_history::filter::transaction_filter_to_query;
use crate::ledger_history::query_options::CheckpointRange;
use crate::ledger_history::query_options::QueryOptions;
use crate::ledger_history::query_options::QueryType;
use crate::ledger_history::query_options::ResolvedRange;

use super::query_end::query_end;

use super::bitmap_scan::BITMAP_BUCKET_SCAN_BUDGET;
use super::bitmap_scan::CHUNK_BUCKET_SCAN_BUDGET;
use super::bitmap_scan::LedgerBitmapKind;
use super::bitmap_scan::PendingBitmapBucket;
use super::bitmap_scan::TX_BITMAP_BUCKET_SIZE;
use super::bitmap_scan::drain_bitmap_hits_with_budget;
use super::chunked_scan::ChunkArgs;
use super::chunked_scan::ChunkTerminal;
use super::chunked_scan::ChunkedScan;
use super::chunked_scan::ScanChunkDone;
use super::chunked_scan::cancelled;
use super::ledger_read::apply_tx_seq_floor;
use super::ledger_read::checkpoint_hi_exclusive;
use super::ledger_read::checkpoint_to_tx_boundary;
use super::ledger_read::checkpoint_to_tx_range;
use super::ledger_read::ensure_ledger_history_enabled;
use super::ledger_read::get_tx_seq_digest_multi;
use super::ledger_read::get_tx_seq_digest_rows;
use super::ledger_read::lowest_available_tx_seq;
use super::ledger_read::remaining_range_after;
use super::ledger_read::resolve_frontier_checkpoint;
use super::ledger_read::validate_checkpoint_bounds;
use crate::ledger_history::watermark::advance_boundary_excluding_cp;
use crate::ledger_history::watermark::boundary_cursor_cp;
use crate::ledger_history::watermark::boundary_watermark;
use crate::ledger_history::watermark::item_watermark;
use crate::ledger_history::watermark::reached_range_end;
use crate::ledger_history::watermark::terminal_boundary_watermark;

pub(super) const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

const DEFAULT_LIMIT_ITEMS: u32 = 50;
const MAX_LIMIT_ITEMS: u32 = 500;
const CHUNK_MAX: usize = 32;
const MAX_BITMAP_FILTER_LITERALS: usize = 10;
const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::TRANSACTION;

pub(crate) type ListTransactionsStream =
    BoxStream<'static, Result<ListTransactionsResponse, RpcError>>;

pub(crate) async fn list_transactions(
    service: RpcService,
    request: ListTransactionsRequest,
) -> Result<ListTransactionsStream, RpcError> {
    ensure_ledger_history_enabled(&service)?;
    let started = Instant::now();
    let start_checkpoint = request.start_checkpoint;
    let end_checkpoint = request.end_checkpoint;
    let filter = request.filter;
    let request_options = request.options;
    let filtered = filter.is_some();
    validate_checkpoint_bounds(start_checkpoint, end_checkpoint)?;
    let read_mask = validate_read_mask(request.read_mask)?;
    let options = QueryOptions::from_proto(
        request_options.as_ref(),
        DEFAULT_LIMIT_ITEMS,
        MAX_LIMIT_ITEMS,
        QueryType::Transactions,
        filter.as_ref(),
    )?;
    let limit_items = options.limit_items;
    let ordering = options.ordering;
    let filter_query = filter
        .as_ref()
        .map(|filter| transaction_filter_to_query(filter, MAX_BITMAP_FILTER_LITERALS))
        .transpose()?;

    let initial_state = TransactionScanState::Init {
        start_checkpoint,
        end_checkpoint,
        filter_query,
    };

    let terminal_options = options.clone();
    Ok(async_stream::try_stream! {
        let render_contents = should_render_transaction_contents(&read_mask);
        let mut scan = ChunkedScan::new(
            initial_state,
            limit_items,
            CHUNK_MAX,
            BITMAP_BUCKET_SCAN_BUDGET,
            move |state, args: ChunkArgs| {
                spawn_transaction_chunk(
                    service.clone(),
                    state,
                    read_mask.clone(),
                    options.clone(),
                    args.scan_budget,
                    args.chunk_item_limit,
                    args.remaining_request_item_limit,
                    render_contents,
                    args.cancel,
                )
            },
        );

        while let Some(response) = scan.next_item().await? {
            yield response;
        }

        let emitted = scan.produced();
        let terminal = scan.into_terminal().expect("query emits terminal state");
        let reason = query_end(emitted, limit_items, terminal.reason);
        if reached_range_end(reason) {
            yield watermark_response(terminal_boundary_watermark(
                &terminal_options,
                terminal.end_checkpoint,
                terminal.end_position,
            ));
        }
        yield end_response(reason);
        info!(
            filtered,
            limit_items,
            ?ordering,
            emitted,
            ?reason,
            elapsed_ms = started.elapsed().as_millis(),
            "list_transactions: done"
        );
    }
    .boxed())
}

fn spawn_transaction_chunk(
    service: RpcService,
    state: TransactionScanState,
    read_mask: FieldMaskTree,
    options: QueryOptions,
    scan_budget: usize,
    chunk_item_limit: usize,
    remaining_request_item_limit: usize,
    render_contents: bool,
    cancel: CancellationToken,
) -> JoinHandle<Result<TransactionChunkDone, RpcError>> {
    tokio::task::spawn_blocking(move || {
        next_transaction_chunk(
            service,
            state,
            read_mask,
            options,
            render_contents,
            scan_budget,
            chunk_item_limit,
            remaining_request_item_limit,
            &cancel,
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
        end_reason: QueryEndReason,
        end_checkpoint: u64,
        end_position: u64,
    },
    Filtered {
        query: BitmapQuery,
        range: Option<Range<u64>>,
        pending_bucket: Option<PendingBitmapBucket>,
        end_reason: QueryEndReason,
        end_checkpoint: u64,
        end_position: u64,
    },
}

type TransactionChunkDone = ScanChunkDone<TransactionScanState, ListTransactionsResponse>;

fn next_transaction_chunk(
    service: RpcService,
    mut state: TransactionScanState,
    read_mask: FieldMaskTree,
    options: QueryOptions,
    render_contents: bool,
    scan_budget: usize,
    chunk_item_limit: usize,
    remaining_request_item_limit: usize,
    cancel: &CancellationToken,
) -> Result<TransactionChunkDone, RpcError> {
    let ascending = options.is_ascending();
    let mut remaining_scan_budget = scan_budget;
    let (rows, next_state, terminal, scan_watermark) = loop {
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
                let terminal = ChunkTerminal {
                    reason: tx_range.end_reason,
                    end_checkpoint: tx_range.end_checkpoint,
                    end_position: tx_range.end_position,
                };
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
                        pending_bucket: None,
                        end_reason: terminal.reason,
                        end_checkpoint: terminal.end_checkpoint,
                        end_position: terminal.end_position,
                    },
                    None => TransactionScanState::Unfiltered {
                        range,
                        end_reason: terminal.reason,
                        end_checkpoint: terminal.end_checkpoint,
                        end_position: terminal.end_position,
                    },
                };
                continue;
            }
            TransactionScanState::Unfiltered {
                range,
                end_reason,
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
                        end_reason,
                        end_checkpoint,
                        end_position,
                    });
                let terminal = ChunkTerminal {
                    reason: end_reason,
                    end_checkpoint,
                    end_position,
                };
                break (rows, next_state, terminal, None);
            }
            TransactionScanState::Filtered {
                query,
                range,
                pending_bucket,
                end_reason,
                end_checkpoint,
                end_position,
            } => {
                let hit_limit = chunk_item_limit.min(remaining_request_item_limit);
                let chunk_scan_budget = remaining_scan_budget.min(CHUNK_BUCKET_SCAN_BUDGET);
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
                if cancel.is_cancelled() {
                    return Err(cancelled());
                }
                let scan_limited = hits.scan_limit_hit;
                let coalesced_frontier = hits.coalesced_frontier;
                // The drain stops at the per-chunk cap or the per-request budget;
                // only the latter (or a cap-hit with no resume point) ends the query.
                let request_exhausted = scan_limited
                    && (remaining_scan_budget == 0
                        || (hits.next_range.is_none() && hits.pending_bucket.is_none()));
                let rows = get_tx_seq_digest_multi(&service, &hits.items)?;
                let next_state = if request_exhausted {
                    None
                } else {
                    (hits.pending_bucket.is_some() || hits.next_range.is_some()).then_some(
                        TransactionScanState::Filtered {
                            query,
                            range: hits.next_range,
                            pending_bucket: hits.pending_bucket,
                            end_reason,
                            end_checkpoint,
                            end_position,
                        },
                    )
                };
                let reason = if request_exhausted {
                    QueryEndReason::ScanLimit
                } else {
                    end_reason
                };
                let terminal = ChunkTerminal {
                    reason,
                    end_checkpoint,
                    end_position,
                };
                // A transaction member id is its own tx_sequence_number, so the
                // frontier decodes to itself.
                let scan_watermark = scan_transaction_watermark(
                    &service,
                    &options,
                    scan_limited,
                    rows.is_empty(),
                    coalesced_frontier,
                    ascending,
                )?;
                break (rows, next_state, terminal, scan_watermark);
            }
        }
    };

    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let mut items = render_transaction_rows(
        &service,
        rows,
        &read_mask,
        &options,
        render_contents,
        cancel,
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

/// Scan watermark for a filtered chunk that matched nothing before the
/// scan budget ran out mid-gap. A transaction's member id is its own
/// `tx_sequence_number`, so the frontier decodes to itself.
fn scan_transaction_watermark(
    service: &RpcService,
    options: &QueryOptions,
    scan_limited: bool,
    no_items: bool,
    coalesced_frontier: Option<u64>,
    ascending: bool,
) -> Result<Option<ListTransactionsResponse>, RpcError> {
    if !(scan_limited && no_items) {
        return Ok(None);
    }
    let Some(frontier) = coalesced_frontier else {
        return Ok(None);
    };
    let Some(cp) = resolve_frontier_checkpoint(service, frontier, ascending, |p| p)? else {
        return Ok(None);
    };
    let boundary = advance_boundary_excluding_cp(None, cp, options);
    let cursor_cp = boundary_cursor_cp(cp, options.scan_direction());
    let watermark = boundary_watermark(options, cursor_cp, frontier, boundary);
    Ok(Some(watermark_response(watermark)))
}

fn render_transaction_rows(
    service: &RpcService,
    rows: Vec<LedgerTxSeqDigest>,
    read_mask: &FieldMaskTree,
    options: &QueryOptions,
    render_contents: bool,
    cancel: &CancellationToken,
) -> Result<Vec<ListTransactionsResponse>, RpcError> {
    let mut transaction_reads = if render_contents {
        let digests = rows
            .iter()
            .map(|row| {
                let digest: Digest = row.digest.into();
                digest
            })
            .collect::<Vec<_>>();
        service
            .reader
            .multi_get_transaction_reads(&digests)?
            .into_iter()
    } else {
        Vec::new().into_iter()
    };

    let mut items = Vec::with_capacity(rows.len());
    // Per-chunk running boundary; monotonic across chunks because rows are
    // emitted in scan-checkpoint order.
    let mut checkpoint_boundary: Option<u64> = None;
    for row in rows {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        checkpoint_boundary =
            advance_boundary_excluding_cp(checkpoint_boundary, row.checkpoint_number, options);
        let watermark = item_watermark(
            options,
            row.checkpoint_number,
            row.tx_sequence_number,
            checkpoint_boundary,
        );
        let response = if render_contents {
            let transaction_read = transaction_reads
                .next()
                .expect("transaction reads match tx_seq rows");
            let transaction = render_executed_transaction(
                service,
                transaction_read,
                row.checkpoint_number,
                read_mask,
            )?;
            transaction_item_response(watermark, transaction, row.tx_offset)
        } else {
            let mut transaction = ExecutedTransaction::default();
            if read_mask.contains(ExecutedTransaction::DIGEST_FIELD.name) {
                transaction.digest = Some(row.digest.to_string());
            }
            if read_mask.contains(ExecutedTransaction::CHECKPOINT_FIELD.name) {
                transaction.checkpoint = Some(row.checkpoint_number);
            }
            transaction_item_response(watermark, transaction, row.tx_offset)
        };
        items.push(response);
    }
    Ok(items)
}

pub(crate) fn validate_read_mask(read_mask: Option<FieldMask>) -> Result<FieldMaskTree, RpcError> {
    let read_mask = read_mask.unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
    read_mask
        .validate::<ExecutedTransaction>()
        .map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
    Ok(FieldMaskTree::from(read_mask))
}

fn should_render_transaction_contents(read_mask: &FieldMaskTree) -> bool {
    let paths = read_mask.to_field_mask().paths;
    paths.is_empty()
        || paths.len() > 2
        || paths.iter().any(|path| {
            path != ExecutedTransaction::DIGEST_FIELD.name
                && path != ExecutedTransaction::CHECKPOINT_FIELD.name
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
    let mut resolved = options.apply_cursor_bounds(cp_range.with_range(tx_range, options.ordering));
    if !resolved.range.is_empty() {
        let explicit_lower = start_checkpoint.is_some() || options.has_after_cursor();
        let floor = lowest_available_tx_seq(service)?;
        resolved.range.start = apply_tx_seq_floor(resolved.range.start, explicit_lower, floor)?;
    }
    Ok(resolved)
}

fn transaction_item_response(
    watermark: Watermark,
    transaction: ExecutedTransaction,
    tx_offset: u32,
) -> ListTransactionsResponse {
    let mut item = TransactionItem::default();
    item.watermark = Some(watermark);
    item.transaction = Some(transaction);
    item.transaction_offset = Some(tx_offset as u64);

    let mut response = ListTransactionsResponse::default();
    response.response = Some(list_transactions_response::Response::Item(item));
    response
}

fn watermark_response(watermark: Watermark) -> ListTransactionsResponse {
    let mut response = ListTransactionsResponse::default();
    response.response = Some(list_transactions_response::Response::Watermark(watermark));
    response
}

fn end_response(reason: QueryEndReason) -> ListTransactionsResponse {
    let mut end = QueryEnd::default();
    end.reason = reason as i32;

    let mut response = ListTransactionsResponse::default();
    response.response = Some(list_transactions_response::Response::End(end));
    response
}
