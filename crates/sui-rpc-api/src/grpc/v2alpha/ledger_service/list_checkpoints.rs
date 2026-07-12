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
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEnd;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc_cursor::Position;
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
use crate::ledger_history::query_options::RangeExhaustion;
use crate::ledger_history::query_options::ResolvedRange;

use super::bitmap_scan::DrainedBitmapHits;
use super::bitmap_scan::LedgerBitmapKind;
use super::bitmap_scan::PendingBitmapBucket;
use super::bitmap_scan::TX_BITMAP_BUCKET_SIZE;
use super::bitmap_scan::drain_bitmap_hits_with_budget;
use super::chunked_scan::ChunkArgs;
use super::chunked_scan::ChunkedScan;
use super::chunked_scan::ScanChunkDone;
use super::chunked_scan::cancelled;
use super::chunked_scan::scan_limit_or_range;
use super::ledger_read::apply_tx_seq_floor;
use super::ledger_read::checkpoint_hi_exclusive;
use super::ledger_read::checkpoint_to_tx_range;
use super::ledger_read::get_tx_seq_digest_multi;
use super::ledger_read::lowest_available_tx_seq;
use super::ledger_read::remaining_range_after;
use super::ledger_read::sequence_frontier_checkpoint;
use super::ledger_read::tx_checkpoint;
use super::ledger_read::validate_checkpoint_bounds;
use crate::ledger_history::watermark::ScanTerminal;
use crate::ledger_history::watermark::advance_covered_bound_before_checkpoint;
use crate::ledger_history::watermark::boundary_watermark;
use crate::ledger_history::watermark::item_watermark;
use crate::ledger_history::watermark::merge_covered_checkpoint_bound;
use crate::ledger_history::watermark::scan_frontier_cursor_cp;

const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::CHECKPOINT;

pub(crate) type ListCheckpointsStream =
    BoxStream<'static, Result<ListCheckpointsResponse, RpcError>>;

pub(crate) async fn list_checkpoints(
    service: RpcService,
    request: ListCheckpointsRequest,
) -> Result<ListCheckpointsStream, RpcError> {
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
    let options = QueryOptions::checkpoints_from_proto(
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

        let mut covered_checkpoint_bound = None;
        while let Some(mut response) = scan.next_item().await? {
            if let Some(checkpoint) = response
                .watermark
                .as_ref()
                .and_then(|watermark| watermark.checkpoint)
            {
                covered_checkpoint_bound = Some(checkpoint);
            }
            if response.checkpoint.is_some()
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
        let terminal_reason = super::query_end::effective_terminal_reason(
            produced,
            limit_items,
            chunk_terminal.reason(),
        );
        if terminal_reason != QueryEndReason::ItemLimit {
            let terminal_watermark =
                chunk_terminal.into_watermark(&terminal_options, covered_checkpoint_bound);
            yield end_response(terminal_watermark, terminal_reason);
        }
        info!(
            filtered,
            limit_items,
            ?ordering,
            emitted = produced,
            ?terminal_reason,
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
    tokio::task::spawn_blocking(move || {
        next_checkpoint_chunk(
            service,
            state,
            read_mask,
            options,
            scan_budget,
            chunk_scan_budget,
            chunk_item_limit,
            remaining_request_item_limit,
            &cancel,
        )
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
        exhaustion: RangeExhaustion,
        end_checkpoint: u64,
        end_position: u64,
    },
    Filtered {
        query: BitmapQuery,
        tx_range: Option<Range<u64>>,
        pending_bucket: Option<PendingBitmapBucket>,
        buffered_cp_seqs: VecDeque<u64>,
        last_cp_seq: Option<u64>,
        covered_checkpoint_bound: Option<u64>,
        /// Checkpoint containing the cursor-trimmed interval's first scan position.
        entry_checkpoint: u64,
        exhaustion: RangeExhaustion,
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
            let interval_empty = cp_range.is_empty();
            let mut end_checkpoint = cp_range.end_checkpoint;
            let mut end_position = cp_range.end_position;
            let mut terminal = ScanTerminal::Range {
                exhaustion: cp_range.exhaustion,
                position: Position::Checkpoints {
                    checkpoint: end_position,
                },
                interval_empty,
            };
            let mut entry_checkpoint = if options.is_ascending() {
                cp_range.range.start
            } else {
                cp_range.range.end.saturating_sub(1)
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
                    let original_start = tx_range.start;
                    let clamped = apply_tx_seq_floor(original_start, explicit_lower, floor)?;
                    tx_range.start = clamped;
                    if clamped != original_start {
                        let floor_checkpoint = tx_checkpoint(&service, clamped)?;
                        if options.is_ascending() {
                            entry_checkpoint = entry_checkpoint.max(floor_checkpoint);
                        } else {
                            end_checkpoint = floor_checkpoint;
                            end_position = floor_checkpoint;
                        }
                        // The clamp can consume the whole filtered span; then
                        // nothing was actually scanned and natural completion
                        // must not claim coverage.
                        terminal = ScanTerminal::Range {
                            exhaustion: cp_range.exhaustion,
                            position: Position::Checkpoints {
                                checkpoint: end_position,
                            },
                            interval_empty: tx_range.is_empty(),
                        };
                    }
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
                    covered_checkpoint_bound: None,
                    entry_checkpoint,
                    exhaustion: cp_range.exhaustion,
                    end_checkpoint,
                    end_position,
                }
            } else {
                CheckpointScanState::Unfiltered {
                    range,
                    exhaustion: cp_range.exhaustion,
                    end_checkpoint,
                    end_position,
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
            exhaustion,
            end_checkpoint,
            end_position,
        } => next_unfiltered_checkpoint_chunk(
            service,
            range,
            exhaustion,
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
            covered_checkpoint_bound,
            entry_checkpoint,
            exhaustion,
            end_checkpoint,
            end_position,
        } => next_filtered_checkpoint_chunk(
            service,
            query,
            tx_range,
            pending_bucket,
            buffered_cp_seqs,
            last_cp_seq,
            covered_checkpoint_bound,
            entry_checkpoint,
            exhaustion,
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
    exhaustion: RangeExhaustion,
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
            exhaustion,
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
        terminal: ScanTerminal::Range {
            exhaustion,
            position: Position::Checkpoints {
                checkpoint: end_position,
            },
            interval_empty: false,
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
    mut covered_checkpoint_bound: Option<u64>,
    entry_checkpoint: u64,
    exhaustion: RangeExhaustion,
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
    let mut chunk_scan_limit_reached = false;
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
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let drain_control = absorb_drained_checkpoint_hits(
            &hits,
            &mut cp_seqs,
            &mut buffered_cp_seqs,
            &mut last_cp_seq,
            item_limit,
            remaining_request_item_limit,
            |tx_seqs| {
                Ok(get_tx_seq_digest_multi(&service, tx_seqs)?
                    .into_iter()
                    .map(|row| row.checkpoint_number)
                    .collect())
            },
        )?;
        remaining_scan_budget -= hits.buckets_scanned;
        chunk_scan_budget -= hits.buckets_scanned;
        pending_bucket = hits.pending_bucket;
        tx_range = hits.next_range;
        if let CheckpointDrainControl::ScanLimit(scan_frontier) = drain_control {
            chunk_scan_limit_reached = true;
            frontier = scan_frontier;
            break;
        }
        if hits.items.is_empty() {
            break;
        }
    }

    if cp_seqs.len() + buffered_cp_seqs.len() == remaining_request_item_limit {
        pending_bucket = None;
        tx_range = None;
    }

    if let Some(&checkpoint) = cp_seqs.last() {
        covered_checkpoint_bound =
            merge_covered_checkpoint_bound(covered_checkpoint_bound, checkpoint, &options);
    }

    // A chunk scan-limit only ends the request when the request budget is also
    // exhausted, or when there is no continuation state. Otherwise the next
    // chunk resumes from the frontier carried below.
    let request_scan_limit_reached = chunk_scan_limit_reached
        && (remaining_scan_budget == 0
            || (tx_range.is_none() && pending_bucket.is_none() && buffered_cp_seqs.is_empty()));
    let frontier = if chunk_scan_limit_reached {
        Some(frontier.ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                "checkpoint scan limit missing authoritative transaction frontier",
            )
        })?)
    } else {
        None
    };
    let frontier_watermark =
        if request_scan_limit_reached || (chunk_scan_limit_reached && cp_seqs.is_empty()) {
            Some(scan_checkpoint_watermark(
                &service,
                &options,
                frontier.expect("checked for scan-limit chunk"),
                covered_checkpoint_bound,
                entry_checkpoint,
                ascending,
            )?)
        } else {
            None
        };
    if let Some(checkpoint) = frontier_watermark
        .as_ref()
        .and_then(|watermark| watermark.checkpoint)
    {
        covered_checkpoint_bound =
            merge_covered_checkpoint_bound(covered_checkpoint_bound, checkpoint, &options);
    }
    let next_state = if request_scan_limit_reached {
        None
    } else {
        (!buffered_cp_seqs.is_empty() || pending_bucket.is_some() || tx_range.is_some()).then_some(
            CheckpointScanState::Filtered {
                query,
                tx_range,
                pending_bucket,
                buffered_cp_seqs,
                last_cp_seq,
                covered_checkpoint_bound,
                entry_checkpoint,
                exhaustion,
                end_checkpoint,
                end_position,
            },
        )
    };
    let scan_watermark = if !request_scan_limit_reached && cp_seqs.is_empty() {
        frontier_watermark.clone().map(watermark_response)
    } else {
        None
    };

    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let mut items = render_checkpoint_seqs(&service, cp_seqs, &read_mask, &options, cancel)?;
    let produced = items.len();
    if let Some(watermark) = scan_watermark {
        items.push(watermark);
    }
    let terminal_position = Position::Checkpoints {
        checkpoint: end_position,
    };
    let terminal = scan_limit_or_range(
        request_scan_limit_reached,
        exhaustion,
        terminal_position,
        || {
            frontier_watermark.ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    "request scan limit missing checkpoint frontier watermark",
                )
            })
        },
    )?;
    Ok(CheckpointChunkDone {
        items,
        produced,
        next_state,
        terminal,
        remaining_scan_budget,
    })
}

enum CheckpointDrainControl {
    Continue,
    ScanLimit(Option<u64>),
}

fn absorb_drained_checkpoint_hits(
    hits: &DrainedBitmapHits,
    cp_seqs: &mut Vec<u64>,
    buffered_cp_seqs: &mut VecDeque<u64>,
    last_cp_seq: &mut Option<u64>,
    item_limit: usize,
    remaining_request_item_limit: usize,
    checkpoint_seqs_for: impl FnOnce(&[u64]) -> Result<Vec<u64>, RpcError>,
) -> Result<CheckpointDrainControl, RpcError> {
    if !hits.items.is_empty() {
        merge_checkpoint_seqs(
            checkpoint_seqs_for(&hits.items)?,
            cp_seqs,
            buffered_cp_seqs,
            last_cp_seq,
            item_limit,
            remaining_request_item_limit,
        );
    }

    if hits.chunk_scan_limit_reached {
        // Hits were earned before this authoritative first-unscanned frontier.
        // Keep every mapped checkpoint ahead of the terminal, even when doing
        // so temporarily exceeds the internal chunk target.
        cp_seqs.extend(buffered_cp_seqs.drain(..));
        Ok(CheckpointDrainControl::ScanLimit(hits.coalesced_frontier))
    } else {
        Ok(CheckpointDrainControl::Continue)
    }
}

fn merge_checkpoint_seqs(
    checkpoint_seqs: impl IntoIterator<Item = u64>,
    cp_seqs: &mut Vec<u64>,
    buffered_cp_seqs: &mut VecDeque<u64>,
    last_cp_seq: &mut Option<u64>,
    item_limit: usize,
    remaining_request_item_limit: usize,
) {
    for cp_seq in checkpoint_seqs {
        if *last_cp_seq == Some(cp_seq) {
            continue;
        }

        *last_cp_seq = Some(cp_seq);
        if cp_seqs.len() < item_limit {
            cp_seqs.push(cp_seq);
        } else if cp_seqs.len() + buffered_cp_seqs.len() < remaining_request_item_limit {
            buffered_cp_seqs.push_back(cp_seq);
        } else {
            break;
        }
    }
}

/// Scan watermark for a filtered checkpoint chunk whose scan budget ran out.
/// The filter scans the transaction bitmap, so the frontier is a
/// `tx_sequence_number`; resolve it to checkpoint space. At ascending genesis
/// no checkpoint is fully covered, but checkpoint coordinate zero remains a
/// safe resume cursor and is emitted with no checkpoint coverage claim.
fn scan_checkpoint_watermark(
    service: &RpcService,
    options: &QueryOptions,
    frontier: u64,
    covered_checkpoint_bound: Option<u64>,
    entry_checkpoint: u64,
    ascending: bool,
) -> Result<Watermark, RpcError> {
    checkpoint_frontier_watermark(
        options,
        frontier,
        sequence_frontier_checkpoint(service, frontier, ascending)?,
        covered_checkpoint_bound,
        entry_checkpoint,
    )
}

fn checkpoint_frontier_watermark(
    options: &QueryOptions,
    frontier: u64,
    checkpoint: Option<u64>,
    covered_checkpoint_bound: Option<u64>,
    entry_checkpoint: u64,
) -> Result<Watermark, RpcError> {
    // The frontier lands partway through its checkpoint, so that checkpoint is
    // not proven complete. Preserve any stronger proof already established by
    // emitted checkpoints.
    let boundary = match checkpoint {
        Some(cp) => advance_covered_bound_before_checkpoint(
            covered_checkpoint_bound,
            cp,
            entry_checkpoint,
            options,
        ),
        None => covered_checkpoint_bound,
    };
    let cursor_cp = scan_frontier_cursor_cp(checkpoint, frontier, options.scan_direction())
        .map(|cursor_cp| {
            clamp_checkpoint_frontier_past_covered(cursor_cp, covered_checkpoint_bound, options)
        })
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                format!(
                    "checkpoint scan frontier transaction {frontier} has no checkpoint mapping"
                ),
            )
        })?;
    Ok(boundary_watermark(
        Position::Checkpoints {
            checkpoint: cursor_cp,
        },
        boundary,
    ))
}

fn clamp_checkpoint_frontier_past_covered(
    frontier: u64,
    covered_checkpoint_bound: Option<u64>,
    options: &QueryOptions,
) -> u64 {
    match covered_checkpoint_bound {
        Some(covered) if options.is_ascending() => frontier.max(covered.saturating_add(1)),
        Some(covered) => frontier.min(covered),
        None => frontier,
    }
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
        checkpoint_boundary = merge_covered_checkpoint_bound(checkpoint_boundary, cp_seq, options);
        items.push(render_checkpoint_seq(
            service,
            cp_seq,
            read_mask,
            checkpoint_boundary,
        )?);
    }
    Ok(items)
}

fn render_checkpoint_seq(
    service: &RpcService,
    cp_seq: u64,
    read_mask: &FieldMaskTree,
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
    let watermark = item_watermark(
        Position::Checkpoints { checkpoint: cp_seq },
        checkpoint_boundary,
    );
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
    let mut response = ListCheckpointsResponse::default();
    response.checkpoint = Some(message);
    response.watermark = Some(watermark);
    response
}

fn watermark_response(watermark: Watermark) -> ListCheckpointsResponse {
    let mut response = ListCheckpointsResponse::default();
    response.watermark = Some(watermark);
    response
}

fn end_response(watermark: Watermark, reason: QueryEndReason) -> ListCheckpointsResponse {
    let mut end = QueryEnd::default();
    end.reason = Some(reason as i32);

    let mut response = ListCheckpointsResponse::default();
    response.watermark = Some(watermark);
    response.end = Some(end);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_rpc::proto::sui::rpc::v2alpha::Ordering;
    use sui_rpc::proto::sui::rpc::v2alpha::QueryOptions as ProtoQueryOptions;
    use sui_rpc_cursor::CursorToken;

    fn options(ascending: bool) -> QueryOptions {
        let mut proto = ProtoQueryOptions::default();
        if !ascending {
            proto.ordering = Some(Ordering::Descending as i32);
        }
        QueryOptions::checkpoints_from_proto(Some(&proto), 100, 100).unwrap()
    }

    #[test]
    fn scan_limit_drain_hits_precede_terminal_and_resume_without_replay() {
        for (
            case,
            ascending,
            first_tx_hits,
            first_checkpoint_hits,
            frontier,
            resumed_tx_hits,
            resumed_checkpoint_hits,
            expected_cursor,
        ) in [
            (
                "ascending",
                true,
                vec![10, 12, 13],
                vec![4, 5, 5],
                14,
                vec![14, 15, 16],
                vec![6, 6, 7],
                6,
            ),
            (
                "descending",
                false,
                vec![16, 14, 13],
                vec![7, 6, 6],
                12,
                vec![12, 11, 10],
                vec![5, 5, 4],
                6,
            ),
        ] {
            let first = DrainedBitmapHits {
                items: first_tx_hits.clone(),
                pending_bucket: None,
                next_range: if ascending {
                    Some(frontier..20)
                } else {
                    Some(0..frontier)
                },
                buckets_scanned: 1,
                coalesced_frontier: Some(frontier),
                chunk_scan_limit_reached: true,
            };
            let mut emitted = Vec::new();
            let mut buffered = VecDeque::new();
            let mut last_checkpoint = None;
            let drain_control = absorb_drained_checkpoint_hits(
                &first,
                &mut emitted,
                &mut buffered,
                &mut last_checkpoint,
                1,
                100,
                |tx_hits| {
                    assert_eq!(tx_hits, first_tx_hits.as_slice(), "{case}");
                    Ok(first_checkpoint_hits)
                },
            )
            .unwrap();
            assert!(
                matches!(
                    drain_control,
                    CheckpointDrainControl::ScanLimit(Some(scan_frontier))
                        if scan_frontier == frontier
                ),
                "{case}",
            );
            assert_eq!(
                emitted,
                if ascending { vec![4, 5] } else { vec![7, 6] },
                "{case}",
            );
            assert!(buffered.is_empty(), "{case}");

            let options = options(ascending);
            let terminal_watermark = checkpoint_frontier_watermark(
                &options,
                first.coalesced_frontier.unwrap(),
                Some(if ascending { 6 } else { 5 }),
                emitted.last().copied(),
                if ascending { 4 } else { 7 },
            )
            .unwrap();
            let terminal = end_response(terminal_watermark, QueryEndReason::ScanLimit);
            let frames = emitted
                .iter()
                .copied()
                .map(|checkpoint| (Some(checkpoint), false))
                .chain(std::iter::once((None, terminal.end.is_some())))
                .collect::<Vec<_>>();
            assert_eq!(
                frames,
                emitted
                    .iter()
                    .copied()
                    .map(|checkpoint| (Some(checkpoint), false))
                    .chain(std::iter::once((None, true)))
                    .collect::<Vec<_>>(),
                "{case}: every earned checkpoint must be emitted exactly once before ScanLimit",
            );
            assert_eq!(
                terminal.watermark.as_ref().and_then(|wm| wm.checkpoint),
                emitted.last().copied(),
                "{case}: terminal proof must cover the emitted checkpoint without skipping it",
            );
            assert_eq!(
                CursorToken::decode(
                    terminal
                        .watermark
                        .as_ref()
                        .and_then(|wm| wm.cursor.as_ref())
                        .expect("scan-limit cursor"),
                )
                .unwrap(),
                CursorToken::boundary(Position::Checkpoints {
                    checkpoint: expected_cursor,
                }),
                "{case}",
            );

            let resumed = DrainedBitmapHits {
                items: resumed_tx_hits.clone(),
                pending_bucket: None,
                next_range: None,
                buckets_scanned: 1,
                coalesced_frontier: None,
                chunk_scan_limit_reached: false,
            };
            let mut suffix = Vec::new();
            let resume_control = absorb_drained_checkpoint_hits(
                &resumed,
                &mut suffix,
                &mut buffered,
                &mut last_checkpoint,
                100,
                100,
                |tx_hits| {
                    assert_eq!(tx_hits, resumed_tx_hits.as_slice(), "{case}");
                    Ok(resumed_checkpoint_hits)
                },
            )
            .unwrap();
            assert!(
                matches!(resume_control, CheckpointDrainControl::Continue),
                "{case}",
            );
            assert_eq!(
                suffix,
                if ascending { vec![6, 7] } else { vec![5, 4] },
                "{case}: resuming at the terminal frontier must return the exact suffix",
            );
        }
    }

    #[test]
    fn scan_limit_checkpoint_frontiers_are_clamped_past_emitted_coverage() {
        for (case, ascending, frontier, checkpoint, covered, expected_cursor, expected_proof) in [
            (
                "ascending frontier maps to emitted checkpoint",
                true,
                41,
                Some(10),
                Some(10),
                11,
                Some(10),
            ),
            (
                "ascending frontier advances to new checkpoint",
                true,
                42,
                Some(12),
                Some(10),
                12,
                Some(11),
            ),
            (
                "ascending genesis does not fabricate proof",
                true,
                0,
                None,
                None,
                0,
                None,
            ),
            (
                "ascending numeric edge preserves prior proof",
                true,
                0,
                None,
                Some(5),
                6,
                Some(5),
            ),
            (
                "ascending terminal proof does not regress",
                true,
                43,
                Some(8),
                Some(10),
                11,
                Some(10),
            ),
            (
                "descending frontier maps to emitted checkpoint",
                false,
                19,
                Some(10),
                Some(10),
                10,
                Some(10),
            ),
            (
                "descending frontier advances to new checkpoint",
                false,
                18,
                Some(8),
                Some(10),
                9,
                Some(9),
            ),
            (
                "descending numeric edge does not fabricate proof",
                false,
                u64::MAX,
                None,
                None,
                u64::MAX,
                None,
            ),
            (
                "descending numeric edge preserves prior proof",
                false,
                u64::MAX,
                None,
                Some(5),
                5,
                Some(5),
            ),
            (
                "descending terminal proof does not regress",
                false,
                17,
                Some(12),
                Some(10),
                10,
                Some(10),
            ),
        ] {
            let options = options(ascending);
            let entry_checkpoint = if ascending { 0 } else { u64::MAX - 1 };
            let watermark = checkpoint_frontier_watermark(
                &options,
                frontier,
                checkpoint,
                covered,
                entry_checkpoint,
            )
            .unwrap();
            let expected_position = Position::Checkpoints {
                checkpoint: expected_cursor,
            };
            assert_eq!(
                CursorToken::decode(
                    watermark
                        .cursor
                        .as_ref()
                        .expect("checkpoint frontier cursor")
                )
                .unwrap(),
                CursorToken::boundary(expected_position),
                "{case}",
            );
            assert_eq!(watermark.checkpoint, expected_proof, "{case}");
            if let Some(covered) = covered {
                if ascending && covered != u64::MAX {
                    assert!(
                        expected_cursor > covered,
                        "{case}: ascending resume must exclude the covered checkpoint",
                    );
                } else if !ascending {
                    assert!(
                        expected_cursor <= covered,
                        "{case}: descending resume must exclude the covered checkpoint",
                    );
                }
            }

            let terminal = ScanTerminal::ScanLimit { watermark };
            let response = end_response(
                terminal.into_watermark(&options, covered),
                QueryEndReason::ScanLimit,
            );
            assert!(response.checkpoint.is_none(), "{case}");
            assert_eq!(
                response.watermark.as_ref().and_then(|wm| wm.checkpoint),
                expected_proof,
                "{case}",
            );
            assert_eq!(
                response.end.as_ref().map(|end| end.reason()),
                Some(QueryEndReason::ScanLimit),
                "{case}",
            );
        }
    }
}
