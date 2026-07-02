// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::ops::Range;
use std::time::Instant;

use futures::StreamExt;
use futures::stream::BoxStream;
use prost_types::FieldMask;
use sui_inverted_index::BitmapQuery;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::Checkpoint;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::get_checkpoint_request::CheckpointId;
use sui_rpc::proto::sui::rpc::v2alpha::CheckpointItem;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEnd;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc::proto::sui::rpc::v2alpha::list_checkpoints_response;
use sui_rpc_cursor::QueryType;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use crate::grpc::v2::ledger_service::get_checkpoint::get_checkpoint;
use crate::ledger_history::filter::transaction_filter_to_query;
use crate::ledger_history::query_options::CheckpointRange;
use crate::ledger_history::query_options::QueryOptions;
use crate::ledger_history::query_options::ResolvedRange;

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
use super::ledger_read::checkpoint_to_tx_range;
use super::ledger_read::ensure_ledger_history_enabled;
use super::ledger_read::get_tx_seq_digest_multi;
use super::ledger_read::lowest_available_tx_seq;
use super::ledger_read::remaining_range_after;
use super::ledger_read::resolve_frontier_checkpoint;
use super::ledger_read::validate_checkpoint_bounds;
use super::query_end::query_end;
use crate::ledger_history::watermark::advance_boundary_excluding_cp;
use crate::ledger_history::watermark::advance_checkpoint_boundary;
use crate::ledger_history::watermark::boundary_cursor_cp;
use crate::ledger_history::watermark::boundary_watermark;
use crate::ledger_history::watermark::item_watermark;
use crate::ledger_history::watermark::reached_range_end;
use crate::ledger_history::watermark::terminal_boundary_watermark;

const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::CHECKPOINT;

pub(crate) type ListCheckpointsStream =
    BoxStream<'static, Result<ListCheckpointsResponse, RpcError>>;

pub(crate) async fn list_checkpoints(
    service: RpcService,
    request: ListCheckpointsRequest,
) -> Result<ListCheckpointsStream, RpcError> {
    ensure_ledger_history_enabled(&service)?;
    let started = Instant::now();
    let start_checkpoint = request.start_checkpoint;
    let end_checkpoint = request.end_checkpoint;
    let filter = request.filter;
    let request_options = request.options;
    let filtered = filter.is_some();
    validate_checkpoint_bounds(start_checkpoint, end_checkpoint)?;
    let read_mask = validate_read_mask(request.read_mask)?;
    let ledger_history = service.config.ledger_history();
    let endpoint = ledger_history.list_checkpoints();
    let bitmap_bucket_scan_budget = ledger_history.bitmap_bucket_scan_budget();
    let chunk_bucket_scan_budget = ledger_history.chunk_bucket_scan_budget();
    let max_bitmap_filter_literals = ledger_history.max_bitmap_filter_literals();
    let options = QueryOptions::from_proto(
        request_options.as_ref(),
        endpoint.default_limit_items,
        endpoint.max_limit_items,
        QueryType::Checkpoints,
    )?;
    let limit_items = options.limit_items;
    let ordering = options.ordering;
    let filter_query = filter
        .as_ref()
        .map(|filter| transaction_filter_to_query(filter, max_bitmap_filter_literals))
        .transpose()?;

    let initial_state = CheckpointScanState::Init {
        start_checkpoint,
        end_checkpoint,
        filter_query,
    };

    let terminal_options = options.clone();
    Ok(async_stream::try_stream! {
        let mut scan = ChunkedScan::new(
            initial_state,
            limit_items,
            endpoint.chunk_max,
            bitmap_bucket_scan_budget,
            move |state, args: ChunkArgs| {
                spawn_checkpoint_chunk(
                    service.clone(),
                    state,
                    read_mask.clone(),
                    options.clone(),
                    args.scan_budget,
                    chunk_bucket_scan_budget,
                    args.chunk_item_limit,
                    args.remaining_request_item_limit,
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
            "list_checkpoints: done"
        );
    }
    .boxed())
}

fn spawn_checkpoint_chunk(
    service: RpcService,
    state: CheckpointScanState,
    read_mask: FieldMaskTree,
    options: QueryOptions,
    scan_budget: usize,
    chunk_scan_budget: usize,
    chunk_item_limit: usize,
    remaining_request_item_limit: usize,
    cancel: CancellationToken,
) -> JoinHandle<Result<CheckpointChunkDone, RpcError>> {
    let metrics = service.metrics.clone();
    let queued_at = Instant::now();
    tokio::task::spawn_blocking(move || {
        if let Some(metrics) = &metrics {
            metrics
                .blocking_queue_wait_seconds
                .with_label_values(&["list_checkpoints"])
                .observe(queued_at.elapsed().as_secs_f64());
        }
        let work = Instant::now();
        let r = next_checkpoint_chunk(
            service,
            state,
            read_mask,
            options,
            scan_budget,
            chunk_scan_budget,
            chunk_item_limit,
            remaining_request_item_limit,
            &cancel,
        );
        if let Some(metrics) = &metrics {
            metrics
                .blocking_work_seconds
                .with_label_values(&["list_checkpoints"])
                .observe(work.elapsed().as_secs_f64());
        }
        r
    })
}

#[derive(Clone)]
enum CheckpointScanState {
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
        tx_range: Option<Range<u64>>,
        pending_bucket: Option<PendingBitmapBucket>,
        buffered_cp_seqs: VecDeque<u64>,
        last_cp_seq: Option<u64>,
        end_reason: QueryEndReason,
        end_checkpoint: u64,
        end_position: u64,
    },
}

type CheckpointChunkDone = ScanChunkDone<CheckpointScanState, ListCheckpointsResponse>;

fn next_checkpoint_chunk(
    service: RpcService,
    state: CheckpointScanState,
    read_mask: FieldMaskTree,
    options: QueryOptions,
    scan_budget: usize,
    chunk_scan_budget: usize,
    chunk_item_limit: usize,
    remaining_request_item_limit: usize,
    cancel: &CancellationToken,
) -> Result<CheckpointChunkDone, RpcError> {
    match state {
        CheckpointScanState::Init {
            start_checkpoint,
            end_checkpoint,
            filter_query,
        } => {
            let checkpoint_range = CheckpointRange::from_request(
                start_checkpoint,
                end_checkpoint,
                checkpoint_hi_exclusive(&service)?,
            )?;
            let cp_range = resolve_cp_range(checkpoint_range, &options);
            if cancel.is_cancelled() {
                return Err(cancelled());
            }
            let terminal = ChunkTerminal {
                reason: cp_range.end_reason,
                end_checkpoint: cp_range.end_checkpoint,
                end_position: cp_range.end_position,
            };
            let range = cp_range.range;
            if range.is_empty() {
                return Ok(CheckpointChunkDone {
                    items: Vec::new(),
                    produced: 0,
                    next_state: None,
                    terminal,
                    remaining_scan_budget: scan_budget,
                });
            }
            let state = if let Some(query) = filter_query {
                let mut tx_range = checkpoint_to_tx_range(&service, range)?;
                if !tx_range.is_empty() {
                    let explicit_lower = start_checkpoint.is_some() || options.has_after_cursor();
                    let floor = lowest_available_tx_seq(&service)?;
                    tx_range.start = apply_tx_seq_floor(tx_range.start, explicit_lower, floor)?;
                }
                if tx_range.is_empty() {
                    return Ok(CheckpointChunkDone {
                        items: Vec::new(),
                        produced: 0,
                        next_state: None,
                        terminal,
                        remaining_scan_budget: scan_budget,
                    });
                }
                CheckpointScanState::Filtered {
                    query,
                    tx_range: Some(tx_range),
                    pending_bucket: None,
                    buffered_cp_seqs: VecDeque::new(),
                    last_cp_seq: None,
                    end_reason: terminal.reason,
                    end_checkpoint: terminal.end_checkpoint,
                    end_position: terminal.end_position,
                }
            } else {
                CheckpointScanState::Unfiltered {
                    range,
                    end_reason: terminal.reason,
                    end_checkpoint: terminal.end_checkpoint,
                    end_position: terminal.end_position,
                }
            };
            next_checkpoint_chunk(
                service,
                state,
                read_mask,
                options,
                scan_budget,
                chunk_scan_budget,
                chunk_item_limit,
                remaining_request_item_limit,
                cancel,
            )
        }
        CheckpointScanState::Unfiltered {
            range,
            end_reason,
            end_checkpoint,
            end_position,
        } => next_unfiltered_checkpoint_chunk(
            service,
            range,
            end_reason,
            end_checkpoint,
            end_position,
            read_mask,
            options,
            scan_budget,
            chunk_item_limit,
            cancel,
        ),
        CheckpointScanState::Filtered {
            query,
            tx_range,
            pending_bucket,
            buffered_cp_seqs,
            last_cp_seq,
            end_reason,
            end_checkpoint,
            end_position,
        } => next_filtered_checkpoint_chunk(
            service,
            query,
            tx_range,
            pending_bucket,
            buffered_cp_seqs,
            last_cp_seq,
            end_reason,
            end_checkpoint,
            end_position,
            read_mask,
            options,
            scan_budget,
            chunk_scan_budget,
            chunk_item_limit,
            remaining_request_item_limit,
            cancel,
        ),
    }
}

fn next_unfiltered_checkpoint_chunk(
    service: RpcService,
    range: Range<u64>,
    end_reason: QueryEndReason,
    end_checkpoint: u64,
    end_position: u64,
    read_mask: FieldMaskTree,
    options: QueryOptions,
    scan_budget: usize,
    chunk_item_limit: usize,
    cancel: &CancellationToken,
) -> Result<CheckpointChunkDone, RpcError> {
    let ascending = options.is_ascending();
    let seqs = checkpoint_seqs_for_range(range.clone(), ascending, chunk_item_limit);
    let next_state = seqs
        .last()
        .and_then(|cp_seq| remaining_range_after(range, *cp_seq, ascending))
        .map(|range| CheckpointScanState::Unfiltered {
            range,
            end_reason,
            end_checkpoint,
            end_position,
        });
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let items = render_checkpoint_seqs(&service, seqs, &read_mask, &options, cancel)?;
    let produced = items.len();
    Ok(CheckpointChunkDone {
        items,
        produced,
        next_state,
        terminal: ChunkTerminal {
            reason: end_reason,
            end_checkpoint,
            end_position,
        },
        remaining_scan_budget: scan_budget,
    })
}

fn next_filtered_checkpoint_chunk(
    service: RpcService,
    query: BitmapQuery,
    mut tx_range: Option<Range<u64>>,
    mut pending_bucket: Option<PendingBitmapBucket>,
    mut buffered_cp_seqs: VecDeque<u64>,
    mut last_cp_seq: Option<u64>,
    end_reason: QueryEndReason,
    end_checkpoint: u64,
    end_position: u64,
    read_mask: FieldMaskTree,
    options: QueryOptions,
    scan_budget: usize,
    chunk_scan_budget: usize,
    chunk_item_limit: usize,
    remaining_request_item_limit: usize,
    cancel: &CancellationToken,
) -> Result<CheckpointChunkDone, RpcError> {
    let ascending = options.is_ascending();
    let item_limit = chunk_item_limit.min(remaining_request_item_limit);
    let mut remaining_scan_budget = scan_budget;
    let mut cp_seqs = buffered_cp_seqs
        .drain(..item_limit.min(buffered_cp_seqs.len()))
        .collect::<Vec<_>>();
    let mut scan_limited = false;
    let mut frontier: Option<u64> = None;
    // Per-chunk bucket cap, bounding this chunk's total scan across all drain
    // iterations so a sparse scan yields incremental scan watermarks instead of
    // one at the per-request limit. Stays <= remaining_scan_budget (both shrink
    // by the same amount each iteration).
    let mut chunk_scan_budget = remaining_scan_budget.min(chunk_scan_budget);

    while cp_seqs.len() < item_limit
        && (pending_bucket.is_some() || tx_range.is_some())
        && chunk_scan_budget > 0
    {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let tx_hit_limit =
            remaining_request_item_limit.saturating_sub(cp_seqs.len() + buffered_cp_seqs.len());
        if tx_hit_limit == 0 {
            break;
        }
        let hits = drain_bitmap_hits_with_budget(
            service.clone(),
            LedgerBitmapKind::Transaction,
            TX_BITMAP_BUCKET_SIZE,
            query.clone(),
            pending_bucket,
            tx_range,
            options.scan_direction(),
            tx_hit_limit,
            chunk_scan_budget,
            cancel,
        )?;
        remaining_scan_budget -= hits.buckets_scanned;
        chunk_scan_budget -= hits.buckets_scanned;
        pending_bucket = hits.pending_bucket;
        tx_range = hits.next_range;
        if hits.scan_limit_hit {
            scan_limited = true;
            frontier = hits.coalesced_frontier;
            break;
        }
        if hits.items.is_empty() {
            break;
        }

        let tx_seqs = hits.items;
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let rows = get_tx_seq_digest_multi(&service, &tx_seqs)?;
        for row in rows {
            let cp_seq = row.checkpoint_number;
            if last_cp_seq == Some(cp_seq) {
                continue;
            }

            last_cp_seq = Some(cp_seq);
            if cp_seqs.len() < item_limit {
                cp_seqs.push(cp_seq);
            } else if cp_seqs.len() + buffered_cp_seqs.len() < remaining_request_item_limit {
                buffered_cp_seqs.push_back(cp_seq);
            } else {
                break;
            }
        }
    }

    if cp_seqs.len() + buffered_cp_seqs.len() == remaining_request_item_limit {
        pending_bucket = None;
        tx_range = None;
    }

    // Only the per-request budget (or a scan-limit with no resume point) ends
    // the query; a per-chunk cap resumes in the next chunk. The scan watermark
    // below carries the resume point when this chunk surfaced no new checkpoints.
    let request_exhausted = scan_limited
        && (remaining_scan_budget == 0
            || (tx_range.is_none() && pending_bucket.is_none() && buffered_cp_seqs.is_empty()));
    let next_state = if request_exhausted {
        None
    } else {
        (!buffered_cp_seqs.is_empty() || pending_bucket.is_some() || tx_range.is_some()).then_some(
            CheckpointScanState::Filtered {
                query,
                tx_range,
                pending_bucket,
                buffered_cp_seqs,
                last_cp_seq,
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
    let scan_watermark = scan_checkpoint_watermark(
        &service,
        &options,
        scan_limited,
        cp_seqs.is_empty(),
        frontier,
        ascending,
    )?;

    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let mut items = render_checkpoint_seqs(&service, cp_seqs, &read_mask, &options, cancel)?;
    let produced = items.len();
    if let Some(watermark) = scan_watermark {
        items.push(watermark);
    }
    Ok(CheckpointChunkDone {
        items,
        produced,
        next_state,
        terminal: ChunkTerminal {
            reason,
            end_checkpoint,
            end_position,
        },
        remaining_scan_budget,
    })
}

/// Scan watermark for a filtered checkpoint chunk that surfaced no new
/// checkpoints before the scan budget ran out. The filter scans the transaction
/// bitmap, so the frontier is a `tx_sequence_number`; resolve it to its
/// checkpoint and emit a checkpoint-space watermark there.
fn scan_checkpoint_watermark(
    service: &RpcService,
    options: &QueryOptions,
    scan_limited: bool,
    no_items: bool,
    frontier: Option<u64>,
    ascending: bool,
) -> Result<Option<ListCheckpointsResponse>, RpcError> {
    if !(scan_limited && no_items) {
        return Ok(None);
    }
    let Some(frontier) = frontier else {
        return Ok(None);
    };
    let Some(cp) = resolve_frontier_checkpoint(service, frontier, ascending, |p| p)? else {
        return Ok(None);
    };
    // The frontier lands partway through checkpoint `cp`, so `cp` itself is not
    // yet proven complete — exclude it from the boundary (`cp ∓ 1`). Contrast
    // the item path, which feeds an emitted (hence complete) cp_seq straight
    // into `advance_checkpoint_boundary`.
    let boundary = advance_boundary_excluding_cp(None, cp, options);
    // Checkpoint cursors live in checkpoint space: position == checkpoint.
    let cursor_cp = boundary_cursor_cp(cp, options.scan_direction());
    let watermark = boundary_watermark(options, cursor_cp, cp, boundary);
    Ok(Some(watermark_response(watermark)))
}

fn checkpoint_seqs_for_range(
    range: Range<u64>,
    ascending: bool,
    checkpoint_limit: usize,
) -> Vec<u64> {
    if ascending {
        range.take(checkpoint_limit).collect()
    } else {
        range.rev().take(checkpoint_limit).collect()
    }
}

fn render_checkpoint_seqs(
    service: &RpcService,
    seqs: Vec<u64>,
    read_mask: &FieldMaskTree,
    options: &QueryOptions,
    cancel: &CancellationToken,
) -> Result<Vec<ListCheckpointsResponse>, RpcError> {
    let mut items = Vec::with_capacity(seqs.len());
    // Per-chunk running boundary; monotonic across chunks because seqs are
    // emitted in scan order.
    let mut checkpoint_boundary: Option<u64> = None;
    for cp_seq in seqs {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        checkpoint_boundary = advance_checkpoint_boundary(checkpoint_boundary, cp_seq, options);
        items.push(render_checkpoint_seq(
            service,
            cp_seq,
            read_mask,
            options,
            checkpoint_boundary,
        )?);
    }
    Ok(items)
}

fn render_checkpoint_seq(
    service: &RpcService,
    cp_seq: u64,
    read_mask: &FieldMaskTree,
    options: &QueryOptions,
    checkpoint_boundary: Option<u64>,
) -> Result<ListCheckpointsResponse, RpcError> {
    let read_mask = read_mask.to_field_mask();
    let mut request = GetCheckpointRequest::default();
    request.checkpoint_id = Some(CheckpointId::SequenceNumber(cp_seq));
    request.read_mask = Some(read_mask);
    let checkpoint = get_checkpoint(service, request)?
        .checkpoint
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                format!("get_checkpoint returned no checkpoint for {cp_seq}"),
            )
        })?;
    let watermark = item_watermark(options, cp_seq, cp_seq, checkpoint_boundary);
    Ok(response_for(watermark, checkpoint))
}

fn validate_read_mask(read_mask: Option<FieldMask>) -> Result<FieldMaskTree, RpcError> {
    let read_mask = read_mask.unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
    read_mask.validate::<Checkpoint>().map_err(|path| {
        FieldViolation::new("read_mask")
            .with_description(format!("invalid read_mask path: {path}"))
            .with_reason(ErrorReason::FieldInvalid)
    })?;
    Ok(FieldMaskTree::from(read_mask))
}

fn resolve_cp_range(checkpoint_range: CheckpointRange, options: &QueryOptions) -> ResolvedRange {
    let cp_range = checkpoint_range.resolve(options);
    let range = cp_range.range.clone();
    options.apply_cursor_bounds(cp_range.with_range(range, options.ordering))
}

fn response_for(watermark: Watermark, message: Checkpoint) -> ListCheckpointsResponse {
    let mut item = CheckpointItem::default();
    item.watermark = Some(watermark);
    item.checkpoint = Some(message);

    let mut response = ListCheckpointsResponse::default();
    response.response = Some(list_checkpoints_response::Response::Item(item));
    response
}

fn watermark_response(watermark: Watermark) -> ListCheckpointsResponse {
    let mut response = ListCheckpointsResponse::default();
    response.response = Some(list_checkpoints_response::Response::Watermark(watermark));
    response
}

fn end_response(reason: QueryEndReason) -> ListCheckpointsResponse {
    let mut end = QueryEnd::default();
    end.reason = reason as i32;

    let mut response = ListCheckpointsResponse::default();
    response.response = Some(list_checkpoints_response::Response::End(end));
    response
}
