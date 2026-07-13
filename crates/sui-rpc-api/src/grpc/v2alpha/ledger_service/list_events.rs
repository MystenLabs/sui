// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::Bound;
use std::time::Instant;

use crate::ledger_history::query_options::EventPosition;
use crate::ledger_history::query_options::RangeExhaustion;
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

use super::bitmap_scan::PendingBitmapBucket;
use super::chunked_scan::ChunkArgs;
use super::chunked_scan::ChunkedScan;
use super::chunked_scan::ScanChunkDone;
use super::chunked_scan::cancelled;
use super::chunked_scan::scan_limit_or_range;
use super::event_scan::EventRef;
use super::event_scan::drain_event_bitmap_hits;
use super::event_scan::event_frontier_checkpoint;
use super::event_scan::next_unfiltered_event_refs;
use super::ledger_read::checkpoint_hi_exclusive;
use super::ledger_read::checkpoint_to_tx_boundary;
use super::ledger_read::checkpoint_to_tx_range;
use super::ledger_read::clamp_to_serving_floor;
use super::ledger_read::get_tx_seq_digest_multi;
use super::ledger_read::validate_checkpoint_bounds;
use crate::ledger_history::watermark::ScanTerminal;
use crate::ledger_history::watermark::advance_covered_bound_before_checkpoint;
use crate::ledger_history::watermark::boundary_watermark;
use crate::ledger_history::watermark::item_watermark;
use crate::ledger_history::watermark::scan_frontier_cursor_cp;

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

        let mut covered_checkpoint_bound = None;
        while let Some(mut response) = scan.next_item().await? {
            if let Some(checkpoint) = response
                .watermark
                .as_ref()
                .and_then(|watermark| watermark.checkpoint)
            {
                covered_checkpoint_bound = Some(checkpoint);
            }
            if response.event.is_some()
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
        /// Checkpoint containing the effective interval's first event.
        entry_checkpoint: u64,
        // Remaining tx rows this scan may read before stopping with `ScanLimit`.
        // Bounds an unfiltered scan that walks event-less history (each scanned
        // tx may carry zero events) to the endpoint's configured `max_limit_items`
        // rows per request.
        row_scan_budget: usize,
        exhaustion: RangeExhaustion,
        end_checkpoint: u64,
        end_position: EventPosition,
    },
    Filtered {
        query: BitmapQuery,
        bounds: Option<EventScanBounds>,
        /// Checkpoint containing the effective interval's first event.
        entry_checkpoint: u64,
        pending_bucket: Option<PendingBitmapBucket>,
        exhaustion: RangeExhaustion,
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
    let (refs, next_state, terminal, scan_watermark, entry_checkpoint) = match state {
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
            let terminal_position = Position::Events {
                checkpoint: event_range.end_checkpoint,
                tx_seq: event_range.end_position.tx_seq,
                event_index: event_range.end_position.event_index,
            };
            let terminal = ScanTerminal::from_range_exhaustion(
                event_range.exhaustion,
                terminal_position,
                event_range.is_empty(),
            );
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
                    entry_checkpoint: event_range.entry_checkpoint,
                    pending_bucket: None,
                    exhaustion: event_range.exhaustion,
                    end_checkpoint: event_range.end_checkpoint,
                    end_position: event_range.end_position,
                },
                None => EventScanState::Unfiltered {
                    bounds,
                    entry_checkpoint: event_range.entry_checkpoint,
                    row_scan_budget: unfiltered_row_scan_budget,
                    exhaustion: event_range.exhaustion,
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
            entry_checkpoint,
            row_scan_budget,
            exhaustion,
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
            let remaining_row_scan_budget = row_scan_budget - scan.rows_scanned;
            // `row_limit_reached` only says this chunk stopped at its local
            // `row_scan_limit`; more request budget may remain for another chunk.
            // Conversely, a range can end naturally on the request's last
            // budgeted row. Only both conditions make this a request ScanLimit.
            let request_scan_limit_reached =
                scan.row_limit_reached && remaining_row_scan_budget == 0;
            let next_state = if request_scan_limit_reached {
                None
            } else {
                scan.next_bounds.map(|bounds| EventScanState::Unfiltered {
                    bounds,
                    entry_checkpoint,
                    row_scan_budget: remaining_row_scan_budget,
                    exhaustion,
                    end_checkpoint,
                    end_position,
                })
            };
            let frontier = if scan.row_limit_reached {
                Some(scan.frontier.ok_or_else(|| {
                    RpcError::new(
                        tonic::Code::Internal,
                        "event row scan limit missing authoritative frontier",
                    )
                })?)
            } else {
                None
            };
            let frontier_watermark =
                if request_scan_limit_reached || (scan.row_limit_reached && scan.refs.is_empty()) {
                    Some(scan_event_watermark(
                        &service,
                        &options,
                        frontier.expect("checked for scan-limit chunk"),
                        entry_checkpoint,
                        ascending,
                    )?)
                } else {
                    None
                };
            let scan_watermark = if !request_scan_limit_reached && scan.refs.is_empty() {
                frontier_watermark.clone().map(watermark_response)
            } else {
                None
            };
            let terminal_position = Position::Events {
                checkpoint: end_checkpoint,
                tx_seq: end_position.tx_seq,
                event_index: end_position.event_index,
            };
            let terminal = scan_limit_or_range(
                request_scan_limit_reached,
                exhaustion,
                terminal_position,
                || {
                    frontier_watermark.ok_or_else(|| {
                        RpcError::new(
                            tonic::Code::Internal,
                            "request scan limit missing event row frontier watermark",
                        )
                    })
                },
            )?;
            (
                scan.refs,
                next_state,
                terminal,
                scan_watermark,
                entry_checkpoint,
            )
        }
        EventScanState::Filtered {
            query,
            bounds,
            entry_checkpoint,
            pending_bucket,
            exhaustion,
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
            let chunk_scan_limit_reached = hits.chunk_scan_limit_reached;
            let frontier = hits.frontier;
            // A chunk scan-limit only ends the request when the request budget
            // is also exhausted, or when there is no continuation.
            let request_scan_limit_reached = chunk_scan_limit_reached
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
                        entry_checkpoint,
                        bounds: hits.next_bounds,
                        pending_bucket: hits.pending_bucket,
                        exhaustion,
                        end_checkpoint,
                        end_position,
                    },
                )
            };
            let frontier = if chunk_scan_limit_reached {
                Some(frontier.ok_or_else(|| {
                    RpcError::new(
                        tonic::Code::Internal,
                        "event bitmap scan limit missing authoritative frontier",
                    )
                })?)
            } else {
                None
            };
            let frontier_watermark =
                if request_scan_limit_reached || (chunk_scan_limit_reached && refs.is_empty()) {
                    Some(scan_event_watermark(
                        &service,
                        &options,
                        frontier.expect("checked for scan-limit chunk"),
                        entry_checkpoint,
                        ascending,
                    )?)
                } else {
                    None
                };
            let scan_watermark = if !request_scan_limit_reached && refs.is_empty() {
                frontier_watermark.clone().map(watermark_response)
            } else {
                None
            };
            let terminal_position = Position::Events {
                checkpoint: end_checkpoint,
                tx_seq: end_position.tx_seq,
                event_index: end_position.event_index,
            };
            let terminal = scan_limit_or_range(
                request_scan_limit_reached,
                exhaustion,
                terminal_position,
                || {
                    frontier_watermark.ok_or_else(|| {
                        RpcError::new(
                            tonic::Code::Internal,
                            "request scan limit missing event bitmap frontier watermark",
                        )
                    })
                },
            )?;
            (refs, next_state, terminal, scan_watermark, entry_checkpoint)
        }
    };

    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let mut items = render_event_chunk(
        &service,
        refs,
        &read_mask,
        &options,
        entry_checkpoint,
        cancel,
    )?;
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
/// The frontier cursor is mandatory. Checkpoint resolution only determines the
/// optional completed-checkpoint claim: at ascending genesis `(0, 0)` remains
/// a safe event frontier even though no checkpoint is yet fully covered.
fn scan_event_watermark(
    service: &RpcService,
    options: &QueryOptions,
    frontier: EventPosition,
    entry_checkpoint: u64,
    ascending: bool,
) -> Result<Watermark, RpcError> {
    event_frontier_watermark(
        options,
        frontier,
        entry_checkpoint,
        event_frontier_checkpoint(service, frontier, ascending)?,
    )
}

fn event_frontier_watermark(
    options: &QueryOptions,
    frontier: EventPosition,
    entry_checkpoint: u64,
    checkpoint: Option<u64>,
) -> Result<Watermark, RpcError> {
    let boundary = checkpoint.and_then(|cp| {
        advance_covered_bound_before_checkpoint(None, cp, entry_checkpoint, options)
    });
    let cursor_cp = scan_frontier_cursor_cp(checkpoint, frontier.tx_seq, options.scan_direction())
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                format!(
                    "event scan frontier {}/{} has no checkpoint mapping",
                    frontier.tx_seq, frontier.event_index
                ),
            )
        })?;
    Ok(boundary_watermark(
        Position::Events {
            checkpoint: cursor_cp,
            tx_seq: frontier.tx_seq,
            event_index: frontier.event_index,
        },
        boundary,
    ))
}

fn render_event_chunk(
    service: &RpcService,
    refs: Vec<EventRef>,
    read_mask: &FieldMaskTree,
    options: &QueryOptions,
    entry_checkpoint: u64,
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
            entry_checkpoint,
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
            cp_range.exhaustion,
        ));
    }

    let tx_range = checkpoint_to_tx_range(service, cp_range.range.clone())?;
    let mut resolved = ResolvedEventRange {
        bounds: EventScanBounds::tx_span(tx_range.start, tx_range.end),
        entry_checkpoint: if options.is_ascending() {
            cp_range.range.start
        } else {
            cp_range.range.end.saturating_sub(1)
        },
        end_checkpoint: cp_range.terminal_checkpoint(options.ordering),
        end_position: match options.ordering {
            crate::ledger_history::query_options::Ordering::Ascending => {
                EventPosition::start_of_tx(tx_range.end)
            }
            crate::ledger_history::query_options::Ordering::Descending => {
                EventPosition::start_of_tx(tx_range.start)
            }
        },
        exhaustion: cp_range.exhaustion,
    };
    resolved = options.apply_event_cursor_bounds(resolved);
    if !resolved.is_empty() {
        let start_tx = match resolved.bounds.lo {
            Bound::Included(position) | Bound::Excluded(position) => position.tx_seq,
            Bound::Unbounded => 0,
        };
        if let Some(floor) = clamp_to_serving_floor(service, start_tx, start_checkpoint, options)? {
            resolved.apply_serving_floor(floor.tx_seq, floor.checkpoint, options);
        }
    }
    Ok(resolved)
}

fn watermark_response(watermark: Watermark) -> ListEventsResponse {
    let mut response = ListEventsResponse::default();
    response.watermark = Some(watermark);
    response
}

fn end_response(watermark: Watermark, reason: QueryEndReason) -> ListEventsResponse {
    let mut end = QueryEnd::default();
    end.reason = Some(reason as i32);

    let mut response = ListEventsResponse::default();
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
        QueryOptions::events_from_proto(Some(&proto), 100, 100).unwrap()
    }

    #[test]
    fn scan_limit_terminal_frames_are_directional_event_cursors() {
        for (
            ascending,
            frontier,
            checkpoint,
            entry_checkpoint,
            expected_position,
            expected_proof,
        ) in [
            (
                true,
                EventPosition::from((0, 0)),
                None,
                0,
                Position::Events {
                    checkpoint: 0,
                    tx_seq: 0,
                    event_index: 0,
                },
                None,
            ),
            (
                true,
                EventPosition::from((41, 3)),
                Some(7),
                7,
                Position::Events {
                    checkpoint: 7,
                    tx_seq: 41,
                    event_index: 3,
                },
                None,
            ),
            (
                true,
                EventPosition::from((42, 1)),
                Some(9),
                7,
                Position::Events {
                    checkpoint: 9,
                    tx_seq: 42,
                    event_index: 1,
                },
                Some(8),
            ),
            (
                false,
                EventPosition::from((u64::MAX, u32::MAX)),
                None,
                u64::MAX,
                Position::Events {
                    checkpoint: u64::MAX,
                    tx_seq: u64::MAX,
                    event_index: u32::MAX,
                },
                None,
            ),
            (
                false,
                EventPosition::from((19, 4)),
                Some(7),
                7,
                Position::Events {
                    checkpoint: 8,
                    tx_seq: 19,
                    event_index: 4,
                },
                None,
            ),
            (
                false,
                EventPosition::from((18, 2)),
                Some(5),
                7,
                Position::Events {
                    checkpoint: 6,
                    tx_seq: 18,
                    event_index: 2,
                },
                Some(6),
            ),
        ] {
            let options = options(ascending);
            let watermark =
                event_frontier_watermark(&options, frontier, entry_checkpoint, checkpoint).unwrap();
            assert_eq!(
                CursorToken::decode(watermark.cursor.as_ref().expect("event frontier cursor"))
                    .unwrap(),
                CursorToken::boundary(expected_position)
            );
            assert_eq!(watermark.checkpoint, expected_proof);
            let terminal = ScanTerminal::ScanLimit { watermark };
            let response = end_response(
                terminal.into_watermark(&options, Some(123)),
                QueryEndReason::ScanLimit,
            );
            assert!(response.event.is_none());
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
}
