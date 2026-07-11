// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::Bound;
use std::time::Instant;

use crate::ledger_history::query_options::EventPosition;
use futures::StreamExt;
use futures::stream::BoxStream;
use mysten_common::ZipDebugEqIteratorExt;
use prost_types::FieldMask;
use sui_inverted_index::BitmapQuery;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::Event as ProtoEvent;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEnd;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc_cursor::Position;
use sui_types::storage::LedgerTxSeqDigest;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use crate::ledger_history::filter::event_filter_to_query;
use crate::ledger_history::query_options::CheckpointRange;
use crate::ledger_history::query_options::EventScanBounds;
use crate::ledger_history::query_options::QueryOptions;
use crate::ledger_history::query_options::ResolvedEventRange;

use super::query_end::effective_terminal_reason;
use super::query_end::terminal_watermark;

use super::bitmap_scan::PendingBitmapBucket;
use super::chunked_scan::ChunkArgs;
use super::chunked_scan::ChunkTerminal;
use super::chunked_scan::ChunkedScan;
use super::chunked_scan::ScanChunkDone;
use super::chunked_scan::cancelled;
use super::event_scan::EventRef;
use super::event_scan::drain_event_bitmap_hits;
use super::event_scan::event_frontier_checkpoint;
use super::event_scan::next_unfiltered_event_refs;
use super::ledger_read::apply_tx_seq_floor;
use super::ledger_read::checkpoint_hi_exclusive;
use super::ledger_read::checkpoint_to_tx_boundary;
use super::ledger_read::checkpoint_to_tx_range;
use super::ledger_read::get_tx_seq_digest_multi;
use super::ledger_read::lowest_available_tx_seq;
use super::ledger_read::validate_checkpoint_bounds;
use crate::ledger_history::watermark::advance_covered_bound_before_checkpoint;
use crate::ledger_history::watermark::boundary_cursor_cp;
use crate::ledger_history::watermark::boundary_watermark;
use crate::ledger_history::watermark::item_watermark;

const EVENT_READ_MASK_DEFAULT: &str = crate::read_mask_defaults::EVENT;

pub(crate) type ListEventsStream = BoxStream<'static, Result<ListEventsResponse, RpcError>>;

pub(crate) async fn list_events(
    service: RpcService,
    request: ListEventsRequest,
) -> Result<ListEventsStream, RpcError> {
    let started = Instant::now();
    let start_checkpoint = request.start_checkpoint;
    let end_checkpoint = request.end_checkpoint;
    let filter = request.filter;
    let request_options = request.options;
    let filtered = filter.is_some();
    validate_checkpoint_bounds(start_checkpoint, end_checkpoint)?;
    let read_mask = validate_event_read_mask(request.read_mask)?;
    let ledger_history = service.config.ledger_history();
    let endpoint = ledger_history.list_events();
    let bitmap_bucket_scan_budget = ledger_history.bitmap_bucket_scan_budget();
    let chunk_bucket_scan_budget = ledger_history.chunk_bucket_scan_budget();
    let max_bitmap_filter_literals = ledger_history.max_bitmap_filter_literals();
    let options = QueryOptions::events_from_proto(
        request_options.as_ref(),
        endpoint.default_limit_items,
        endpoint.max_limit_items,
    )?;
    let limit_items = options.limit_items;
    let ordering = options.ordering;
    let filter_query = filter
        .as_ref()
        .map(|filter| event_filter_to_query(filter, max_bitmap_filter_literals))
        .transpose()?;

    let initial_state = EventScanState::Init {
        start_checkpoint,
        end_checkpoint,
        filter_query,
    };

    let terminal_options = options.clone();
    Ok(async_stream::try_stream! {
        let unfiltered_row_scan_budget = endpoint.max_limit_items as usize;
        let mut scan = ChunkedScan::new(
            initial_state,
            limit_items,
            endpoint.chunk_max,
            bitmap_bucket_scan_budget,
            move |state, args: ChunkArgs| {
                spawn_event_chunk(
                    service.clone(),
                    state,
                    read_mask.clone(),
                    options.clone(),
                    args.scan_budget,
                    chunk_bucket_scan_budget,
                    unfiltered_row_scan_budget,
                    args.chunk_item_limit,
                    args.remaining_request_item_limit,
                    args.cancel,
                )
            },
        );

        let mut latest_emitted_watermark: Option<Watermark> = None;
        while let Some(mut response) = scan.next_item().await? {
            if response.watermark.is_some() {
                latest_emitted_watermark = response.watermark.clone();
            }
            if response.event.is_some() && scan.produced() == limit_items && scan.exhausted() {
                let mut end = QueryEnd::default();
                end.reason = Some(QueryEndReason::ItemLimit as i32);
                response.end = Some(end);
            }
            yield response;
        }

        let produced = scan.produced();
        let chunk_terminal = scan.into_terminal().expect("query emits terminal state");
        let terminal_reason =
            effective_terminal_reason(produced, limit_items, chunk_terminal.scan_end_reason);
        if terminal_reason != QueryEndReason::ItemLimit {
            let terminal_watermark = terminal_watermark(
                &terminal_options,
                chunk_terminal.position,
                chunk_terminal.scan_frontier_watermark,
                terminal_reason,
                latest_emitted_watermark.as_ref(),
            );
            yield end_response(terminal_watermark, terminal_reason);
        }
        info!(
            filtered,
            limit_items,
            ?ordering,
            emitted = produced,
            ?terminal_reason,
            elapsed_ms = started.elapsed().as_millis(),
            "list_events: done"
        );
    }
    .boxed())
}

fn spawn_event_chunk(
    service: RpcService,
    state: EventScanState,
    read_mask: FieldMaskTree,
    options: QueryOptions,
    scan_budget: usize,
    chunk_scan_budget: usize,
    unfiltered_row_scan_budget: usize,
    chunk_item_limit: usize,
    remaining_request_item_limit: usize,
    cancel: CancellationToken,
) -> JoinHandle<Result<EventChunkDone, RpcError>> {
    tokio::task::spawn_blocking(move || {
        next_event_chunk(
            service,
            state,
            read_mask,
            options,
            scan_budget,
            chunk_scan_budget,
            unfiltered_row_scan_budget,
            chunk_item_limit,
            remaining_request_item_limit,
            &cancel,
        )
    })
}

#[derive(Clone)]
enum EventScanState {
    Init {
        start_checkpoint: Option<u64>,
        end_checkpoint: Option<u64>,
        filter_query: Option<BitmapQuery>,
    },
    Unfiltered {
        bounds: EventScanBounds,
        // Remaining tx rows this scan may read before stopping with `ScanLimit`.
        // Bounds an unfiltered scan that walks event-less history (each scanned
        // tx may carry zero events) to the endpoint's configured `max_limit_items`
        // rows per request.
        row_scan_budget: usize,
        range_exhaustion_reason: QueryEndReason,
        end_checkpoint: u64,
        end_position: EventPosition,
    },
    Filtered {
        query: BitmapQuery,
        bounds: Option<EventScanBounds>,
        pending_bucket: Option<PendingBitmapBucket>,
        range_exhaustion_reason: QueryEndReason,
        end_checkpoint: u64,
        end_position: EventPosition,
    },
}

type EventChunkDone = ScanChunkDone<EventScanState, ListEventsResponse>;

fn next_event_chunk(
    service: RpcService,
    state: EventScanState,
    read_mask: FieldMaskTree,
    options: QueryOptions,
    scan_budget: usize,
    chunk_scan_budget: usize,
    unfiltered_row_scan_budget: usize,
    chunk_item_limit: usize,
    remaining_request_item_limit: usize,
    cancel: &CancellationToken,
) -> Result<EventChunkDone, RpcError> {
    let ascending = options.is_ascending();
    let mut remaining_scan_budget = scan_budget;
    let (refs, next_state, terminal, scan_watermark) = match state {
        EventScanState::Init {
            start_checkpoint,
            end_checkpoint,
            filter_query,
        } => {
            let checkpoint_range = CheckpointRange::from_request(
                start_checkpoint,
                end_checkpoint,
                checkpoint_hi_exclusive(&service)?,
            )?;
            let event_range =
                resolve_event_range(&service, start_checkpoint, checkpoint_range, &options)?;
            if cancel.is_cancelled() {
                return Err(cancelled());
            }
            let terminal = ChunkTerminal {
                scan_end_reason: event_range.end_reason,
                position: Position::Events {
                    checkpoint: event_range.end_checkpoint,
                    tx_seq: event_range.end_position.tx_seq,
                    event_index: event_range.end_position.event_index,
                },
                scan_frontier_watermark: None,
            };
            let bounds = event_range.bounds;
            if event_range.is_empty() {
                return Ok(EventChunkDone {
                    items: Vec::new(),
                    produced: 0,
                    next_state: None,
                    terminal,
                    remaining_scan_budget,
                });
            }
            let state = match filter_query {
                Some(query) => EventScanState::Filtered {
                    query,
                    bounds: Some(bounds),
                    pending_bucket: None,
                    range_exhaustion_reason: terminal.scan_end_reason,
                    end_checkpoint: event_range.end_checkpoint,
                    end_position: event_range.end_position,
                },
                None => EventScanState::Unfiltered {
                    bounds,
                    row_scan_budget: unfiltered_row_scan_budget,
                    range_exhaustion_reason: terminal.scan_end_reason,
                    end_checkpoint: event_range.end_checkpoint,
                    end_position: event_range.end_position,
                },
            };
            return next_event_chunk(
                service,
                state,
                read_mask,
                options,
                remaining_scan_budget,
                chunk_scan_budget,
                unfiltered_row_scan_budget,
                chunk_item_limit,
                remaining_request_item_limit,
                cancel,
            );
        }
        EventScanState::Unfiltered {
            bounds,
            row_scan_budget,
            range_exhaustion_reason,
            end_checkpoint,
            end_position,
        } => {
            if cancel.is_cancelled() {
                return Err(cancelled());
            }
            let row_scan_limit = row_scan_budget.min(chunk_scan_budget);
            let scan = next_unfiltered_event_refs(
                &service,
                &bounds,
                ascending,
                chunk_item_limit,
                row_scan_limit,
            )?;
            let row_scan_budget = row_scan_budget - scan.rows_scanned;
            let request_scan_limit_reached = scan.scan_limit_hit && row_scan_budget == 0;
            let next_state = if request_scan_limit_reached {
                None
            } else {
                scan.next_bounds.map(|bounds| EventScanState::Unfiltered {
                    bounds,
                    row_scan_budget,
                    range_exhaustion_reason,
                    end_checkpoint,
                    end_position,
                })
            };
            let scan_end_reason = if request_scan_limit_reached {
                QueryEndReason::ScanLimit
            } else {
                range_exhaustion_reason
            };
            let frontier_watermark = if request_scan_limit_reached || scan.refs.is_empty() {
                scan_event_watermark(
                    &service,
                    &options,
                    scan.scan_limit_hit,
                    scan.frontier,
                    ascending,
                )?
            } else {
                None
            };
            let scan_watermark = if !request_scan_limit_reached && scan.refs.is_empty() {
                frontier_watermark.clone().map(watermark_response)
            } else {
                None
            };
            let terminal = ChunkTerminal {
                scan_end_reason,
                position: Position::Events {
                    checkpoint: end_checkpoint,
                    tx_seq: end_position.tx_seq,
                    event_index: end_position.event_index,
                },
                scan_frontier_watermark: request_scan_limit_reached
                    .then_some(frontier_watermark)
                    .flatten(),
            };
            (scan.refs, next_state, terminal, scan_watermark)
        }
        EventScanState::Filtered {
            query,
            bounds,
            pending_bucket,
            range_exhaustion_reason,
            end_checkpoint,
            end_position,
        } => {
            let hit_limit = chunk_item_limit.min(remaining_request_item_limit);
            let chunk_scan_budget = remaining_scan_budget.min(chunk_scan_budget);
            let hits = drain_event_bitmap_hits(
                service.clone(),
                query.clone(),
                pending_bucket,
                bounds,
                options.scan_direction(),
                hit_limit,
                chunk_scan_budget,
                cancel,
            )?;
            remaining_scan_budget -= hits.buckets_scanned;
            let scan_limited = hits.scan_limit_hit;
            let frontier = hits.frontier;
            let request_scan_limit_reached = scan_limited
                && (remaining_scan_budget == 0
                    || (hits.next_bounds.is_none() && hits.pending_bucket.is_none()));
            let refs = hits
                .items
                .into_iter()
                .map(|position| EventRef {
                    position,
                    tx_seq_digest: None,
                })
                .collect::<Vec<_>>();
            let next_state = if request_scan_limit_reached {
                None
            } else {
                (hits.pending_bucket.is_some() || hits.next_bounds.is_some()).then_some(
                    EventScanState::Filtered {
                        query,
                        bounds: hits.next_bounds,
                        pending_bucket: hits.pending_bucket,
                        range_exhaustion_reason,
                        end_checkpoint,
                        end_position,
                    },
                )
            };
            let scan_end_reason = if request_scan_limit_reached {
                QueryEndReason::ScanLimit
            } else {
                range_exhaustion_reason
            };
            let frontier_watermark = if request_scan_limit_reached || refs.is_empty() {
                scan_event_watermark(&service, &options, scan_limited, frontier, ascending)?
            } else {
                None
            };
            let scan_watermark = if !request_scan_limit_reached && refs.is_empty() {
                frontier_watermark.clone().map(watermark_response)
            } else {
                None
            };
            let terminal = ChunkTerminal {
                scan_end_reason,
                position: Position::Events {
                    checkpoint: end_checkpoint,
                    tx_seq: end_position.tx_seq,
                    event_index: end_position.event_index,
                },
                scan_frontier_watermark: request_scan_limit_reached
                    .then_some(frontier_watermark)
                    .flatten(),
            };
            (refs, next_state, terminal, scan_watermark)
        }
    };

    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let mut items = render_event_chunk(&service, refs, &read_mask, &options, cancel)?;
    let produced = items.len();
    if let Some(watermark) = scan_watermark {
        items.push(watermark);
    }
    Ok(EventChunkDone {
        items,
        produced,
        next_state,
        terminal,
        remaining_scan_budget,
    })
}

/// Build the scan watermark for a chunk whose scan budget ran out mid-gap.
/// Resolves the coalesced frontier to its checkpoint; yields nothing at genesis
/// (asc) where no progress can be claimed.
fn scan_event_watermark(
    service: &RpcService,
    options: &QueryOptions,
    scan_limited: bool,
    frontier: Option<EventPosition>,
    ascending: bool,
) -> Result<Option<Watermark>, RpcError> {
    if !scan_limited {
        return Ok(None);
    }
    let Some(frontier) = frontier else {
        return Ok(None);
    };
    let Some(cp) = event_frontier_checkpoint(service, frontier, ascending)? else {
        return Ok(None);
    };
    let boundary = advance_covered_bound_before_checkpoint(None, cp, options);
    let cursor_cp = boundary_cursor_cp(cp, options.scan_direction());
    let watermark = boundary_watermark(
        Position::Events {
            checkpoint: cursor_cp,
            tx_seq: frontier.tx_seq,
            event_index: frontier.event_index,
        },
        boundary,
    );
    Ok(Some(watermark))
}

fn render_event_chunk(
    service: &RpcService,
    refs: Vec<EventRef>,
    read_mask: &FieldMaskTree,
    options: &QueryOptions,
    cancel: &CancellationToken,
) -> Result<Vec<ListEventsResponse>, RpcError> {
    let rows = tx_seq_digest_rows_for_event_refs(service, &refs)?;
    let mut unique_digests = Vec::new();
    let mut seen_digests = HashSet::new();
    for row in &rows {
        if seen_digests.insert(row.digest) {
            unique_digests.push(row.digest);
        }
    }
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let events = service.reader.multi_get_events(&unique_digests);
    let mut events_by_digest = HashMap::with_capacity(unique_digests.len());
    for (digest, events) in unique_digests.into_iter().zip_debug_eq(events) {
        events_by_digest.insert(digest, events);
    }

    let mut items = Vec::with_capacity(refs.len());
    // Per-chunk running boundary; monotonic across chunks because items are
    // emitted in scan-checkpoint order.
    let mut checkpoint_boundary: Option<u64> = None;
    for (event_ref, row) in refs.into_iter().zip_debug_eq(rows) {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let tx_events = events_by_digest
            .get(&row.digest)
            .and_then(Option::as_ref)
            .ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!(
                        "selected event {}/{} transaction {} has no events",
                        event_ref.position.tx_seq, event_ref.position.event_index, row.digest
                    ),
                )
            })?;
        let event = tx_events
            .data
            .get(event_ref.position.event_index as usize)
            .ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!(
                        "selected event {}/{} index out of range for transaction {}",
                        event_ref.position.tx_seq, event_ref.position.event_index, row.digest
                    ),
                )
            })?;
        let mut proto_event = service.render_event_to_proto(
            event,
            read_mask,
            &sui_types::full_checkpoint_content::ObjectSet::default(),
        );
        sui_macros::fail_point_if!("corrupt_authenticated_event", || {
            if let Some(bcs) = proto_event.contents.as_mut() {
                bcs.value = Some(vec![0xDE, 0xAD, 0xBE, 0xEF].into());
            }
        });
        // The event's ledger position rides on the `Event` message itself
        // rather than the response frame; populate each position field only when
        // the read mask requests it. Authenticated-stream clients that need to
        // reconstruct the `EventCommitment` leaf ask for these paths.
        if read_mask.contains(ProtoEvent::CHECKPOINT_FIELD.name) {
            proto_event.checkpoint = Some(row.checkpoint_number);
        }
        if read_mask.contains(ProtoEvent::TRANSACTION_DIGEST_FIELD.name) {
            proto_event.transaction_digest = Some(row.digest.to_string());
        }
        if read_mask.contains(ProtoEvent::TRANSACTION_INDEX_FIELD.name) {
            proto_event.transaction_index = Some(row.tx_offset as u64);
        }
        if read_mask.contains(ProtoEvent::EVENT_INDEX_FIELD.name) {
            proto_event.event_index = Some(event_ref.position.event_index);
        }
        checkpoint_boundary = advance_covered_bound_before_checkpoint(
            checkpoint_boundary,
            row.checkpoint_number,
            options,
        );
        let watermark = item_watermark(
            Position::Events {
                checkpoint: row.checkpoint_number,
                tx_seq: event_ref.position.tx_seq,
                event_index: event_ref.position.event_index,
            },
            checkpoint_boundary,
        );

        let mut response = ListEventsResponse::default();
        response.event = Some(proto_event);
        response.watermark = Some(watermark);
        items.push(response);
    }
    Ok(items)
}

fn tx_seq_digest_rows_for_event_refs(
    service: &RpcService,
    refs: &[EventRef],
) -> Result<Vec<LedgerTxSeqDigest>, RpcError> {
    let missing_tx_seqs = refs
        .iter()
        .filter(|event_ref| event_ref.tx_seq_digest.is_none())
        .map(|event_ref| event_ref.position.tx_seq)
        .collect::<Vec<_>>();
    let mut fetched = get_tx_seq_digest_multi(service, &missing_tx_seqs)?.into_iter();

    refs.iter()
        .map(|event_ref| match event_ref.tx_seq_digest {
            Some(row) => Ok(row),
            None => fetched.next().ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    "tx_seq_digest multi-get returned too few rows",
                )
            }),
        })
        .collect()
}

fn validate_event_read_mask(read_mask: Option<FieldMask>) -> Result<FieldMaskTree, RpcError> {
    let read_mask = read_mask.unwrap_or_else(|| FieldMask::from_str(EVENT_READ_MASK_DEFAULT));
    read_mask.validate::<ProtoEvent>().map_err(|path| {
        FieldViolation::new("read_mask")
            .with_description(format!("invalid read_mask path: {path}"))
            .with_reason(ErrorReason::FieldInvalid)
    })?;
    Ok(FieldMaskTree::from(read_mask))
}

fn resolve_event_range(
    service: &RpcService,
    start_checkpoint: Option<u64>,
    checkpoint_range: CheckpointRange,
    options: &QueryOptions,
) -> Result<ResolvedEventRange, RpcError> {
    let cp_range = checkpoint_range.resolve(options);
    if cp_range.is_empty() {
        let tx_boundary =
            checkpoint_to_tx_boundary(service, cp_range.terminal_checkpoint(options.ordering))?;
        return Ok(ResolvedEventRange::empty_at(
            cp_range.terminal_checkpoint(options.ordering),
            EventPosition::start_of_tx(tx_boundary),
            cp_range.end_reason,
        ));
    }

    let tx_range = checkpoint_to_tx_range(service, cp_range.range.clone())?;
    let mut resolved = ResolvedEventRange {
        bounds: EventScanBounds::tx_span(tx_range.start, tx_range.end),
        end_checkpoint: cp_range.terminal_checkpoint(options.ordering),
        end_position: match options.ordering {
            crate::ledger_history::query_options::Ordering::Ascending => {
                EventPosition::start_of_tx(tx_range.end)
            }
            crate::ledger_history::query_options::Ordering::Descending => {
                EventPosition::start_of_tx(tx_range.start)
            }
        },
        end_reason: cp_range.end_reason,
    };
    resolved = options.apply_event_cursor_bounds(resolved);
    if !resolved.is_empty() {
        let explicit_lower = start_checkpoint.is_some() || options.has_after_cursor();
        let floor = lowest_available_tx_seq(service)?;
        let start_tx = match resolved.bounds.lo {
            Bound::Included(position) | Bound::Excluded(position) => position.tx_seq,
            Bound::Unbounded => 0,
        };
        let clamped_tx = apply_tx_seq_floor(start_tx, explicit_lower, floor)?;
        if clamped_tx != start_tx {
            resolved.bounds.lo = Bound::Included(EventPosition::start_of_tx(clamped_tx));
        }
    }
    Ok(resolved)
}

fn watermark_response(watermark: Watermark) -> ListEventsResponse {
    let mut response = ListEventsResponse::default();
    response.watermark = Some(watermark);
    response
}

fn end_response(watermark: Option<Watermark>, reason: QueryEndReason) -> ListEventsResponse {
    let mut end = QueryEnd::default();
    end.reason = Some(reason as i32);

    let mut response = ListEventsResponse::default();
    response.watermark = watermark;
    response.end = Some(end);
    response
}
