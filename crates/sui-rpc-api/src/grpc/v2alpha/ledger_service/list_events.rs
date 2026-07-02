// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::Range;
use std::time::Instant;

use futures::FutureExt;
use futures::StreamExt;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use mysten_common::ZipDebugEqIteratorExt;
use prost_types::FieldMask;
use sui_inverted_index::BitmapQuery;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::Event as ProtoEvent;
use sui_rpc::proto::sui::rpc::v2alpha::EventItem;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEnd;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc::proto::sui::rpc::v2alpha::list_events_response;
use sui_rpc_cursor::QueryType;
use sui_types::storage::LedgerTxSeqDigest;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use crate::ledger_history::filter::event_filter_to_query;
use crate::ledger_history::query_options::CheckpointRange;
use crate::ledger_history::query_options::QueryOptions;
use crate::ledger_history::query_options::ResolvedRange;

use super::query_end::query_end;

use super::bitmap_scan::EVENT_BITMAP_BUCKET_SIZE;
use super::bitmap_scan::LedgerBitmapKind;
use super::bitmap_scan::PendingBitmapBucket;
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
use event_seq::decode_event_seq;
use event_seq::encode_event_seq;
use event_seq::event_seq_lo;

const EVENT_READ_MASK_DEFAULT: &str = crate::read_mask_defaults::EVENT;

pub(crate) type ListEventsStream = BoxStream<'static, Result<ListEventsResponse, RpcError>>;

pub(crate) async fn list_events(
    service: RpcService,
    request: ListEventsRequest,
) -> Result<ListEventsStream, RpcError> {
    ensure_ledger_history_enabled(&service)?;
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
    let options = QueryOptions::from_proto(
        request_options.as_ref(),
        endpoint.default_limit_items,
        endpoint.max_limit_items,
        QueryType::Events,
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

        while let Some(response) = scan.next_item().await? {
            yield response;
        }

        let emitted = scan.produced();
        let terminal = scan.into_terminal().expect("query emits terminal state");
        let reason = query_end(emitted, limit_items, terminal.reason);
        // Natural completion proves the range's final checkpoint complete; emit a
        // terminal watermark carrying that bound and the resume cursor.
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
) -> BoxFuture<'static, Result<EventChunkDone, RpcError>> {
    async move {
        let pool = service.read_pool()?;
        pool.run("list_events", move || {
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
        .await?
    }
    .boxed()
}

#[derive(Clone)]
enum EventScanState {
    Init {
        start_checkpoint: Option<u64>,
        end_checkpoint: Option<u64>,
        filter_query: Option<BitmapQuery>,
    },
    Unfiltered {
        range: Range<u64>,
        // Remaining tx rows this scan may read before stopping with `ScanLimit`.
        // Bounds an unfiltered scan that walks event-less history (each scanned
        // tx may carry zero events) to the endpoint's configured `max_limit_items`
        // rows per request.
        row_scan_budget: usize,
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
                reason: event_range.end_reason,
                end_checkpoint: event_range.end_checkpoint,
                end_position: event_range.end_position,
            };
            let range = event_range.range;
            if range.is_empty() {
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
                    range: Some(range),
                    pending_bucket: None,
                    end_reason: terminal.reason,
                    end_checkpoint: terminal.end_checkpoint,
                    end_position: terminal.end_position,
                },
                None => EventScanState::Unfiltered {
                    range,
                    row_scan_budget: unfiltered_row_scan_budget,
                    end_reason: terminal.reason,
                    end_checkpoint: terminal.end_checkpoint,
                    end_position: terminal.end_position,
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
            range,
            row_scan_budget,
            end_reason,
            end_checkpoint,
            end_position,
        } => {
            if cancel.is_cancelled() {
                return Err(cancelled());
            }
            let row_scan_limit = row_scan_budget.min(chunk_scan_budget);
            let UnfilteredScan {
                refs,
                next_range,
                rows_scanned,
                scan_limit_hit: scan_limited,
            } = next_unfiltered_event_refs(
                &service,
                range,
                ascending,
                chunk_item_limit,
                row_scan_limit,
            )?;
            let row_scan_budget = row_scan_budget - rows_scanned;
            // A per-chunk row cap hit (`scan_limited`) emits a scan watermark and
            // resumes next chunk; only the spent per-request budget ends the query
            // with `ScanLimit`. Mirrors the filtered path's per-chunk / per-request
            // split so a long event-less scan reports incremental progress.
            let request_exhausted = scan_limited && row_scan_budget == 0;
            // Frontier (last covered event_seq) sits just past the resume range,
            // mirroring the filtered path's `coalesced_frontier` semantics.
            let frontier = if scan_limited {
                next_range.as_ref().and_then(|r| {
                    if ascending {
                        r.start.checked_sub(1)
                    } else {
                        Some(r.end)
                    }
                })
            } else {
                None
            };
            let next_state = if request_exhausted {
                None
            } else {
                next_range.map(|range| EventScanState::Unfiltered {
                    range,
                    row_scan_budget,
                    end_reason,
                    end_checkpoint,
                    end_position,
                })
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
            let scan_watermark = scan_event_watermark(
                &service,
                &options,
                scan_limited,
                refs.is_empty(),
                frontier,
                ascending,
            )?;
            (refs, next_state, terminal, scan_watermark)
        }
        EventScanState::Filtered {
            query,
            range,
            pending_bucket,
            end_reason,
            end_checkpoint,
            end_position,
        } => {
            let hit_limit = chunk_item_limit.min(remaining_request_item_limit);
            let chunk_scan_budget = remaining_scan_budget.min(chunk_scan_budget);
            let hits = drain_bitmap_hits_with_budget(
                service.clone(),
                LedgerBitmapKind::Event,
                EVENT_BITMAP_BUCKET_SIZE,
                query.clone(),
                pending_bucket,
                range,
                options.scan_direction(),
                hit_limit,
                chunk_scan_budget,
                cancel,
            )?;
            remaining_scan_budget -= hits.buckets_scanned;
            let scan_limited = hits.scan_limit_hit;
            let coalesced_frontier = hits.coalesced_frontier;
            // The drain stops at the per-chunk cap or the per-request budget;
            // only the latter (or a cap-hit with no resume point) ends the query.
            let request_exhausted = scan_limited
                && (remaining_scan_budget == 0
                    || (hits.next_range.is_none() && hits.pending_bucket.is_none()));
            let refs = hits
                .items
                .into_iter()
                .map(|event_seq| {
                    let (tx_seq, event_idx) = decode_event_seq(event_seq);
                    EventRef {
                        event_seq,
                        tx_seq,
                        event_idx,
                        tx_seq_digest: None,
                    }
                })
                .collect::<Vec<_>>();
            // The per-request scan limit halts the query; a per-chunk cap or a
            // normal stop resumes from the pending bucket or remaining range.
            let next_state = if request_exhausted {
                None
            } else {
                (hits.pending_bucket.is_some() || hits.next_range.is_some()).then_some(
                    EventScanState::Filtered {
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
            // A chunk that matched nothing but advanced the scan emits one scan
            // watermark so a client learns the resume point and how far the gap
            // was covered — both per-chunk cap hits (mid-query progress) and the
            // final per-request limit. Natural range exhaustion ends via the
            // terminal watermark instead.
            let scan_watermark = scan_event_watermark(
                &service,
                &options,
                scan_limited,
                refs.is_empty(),
                coalesced_frontier,
                ascending,
            )?;
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

/// Build the scan watermark frame for a filtered chunk that matched
/// nothing while the scan budget ran out mid-gap. Resolves the coalesced
/// frontier to its checkpoint; yields nothing at genesis (asc) where no progress
/// can be claimed.
fn scan_event_watermark(
    service: &RpcService,
    options: &QueryOptions,
    scan_limited: bool,
    no_items: bool,
    coalesced_frontier: Option<u64>,
    ascending: bool,
) -> Result<Option<ListEventsResponse>, RpcError> {
    if !(scan_limited && no_items) {
        return Ok(None);
    }
    let Some(frontier) = coalesced_frontier else {
        return Ok(None);
    };
    let Some(cp) =
        resolve_frontier_checkpoint(service, frontier, ascending, |p| decode_event_seq(p).0)?
    else {
        return Ok(None);
    };
    let boundary = advance_boundary_excluding_cp(None, cp, options);
    let cursor_cp = boundary_cursor_cp(cp, options.scan_direction());
    let watermark = boundary_watermark(options, cursor_cp, frontier, boundary);
    Ok(Some(watermark_response(watermark)))
}

/// One unfiltered tx-row scan: the matching event refs, the resume range (`None`
/// at end of range), how many tx rows were read (charged to the row budget), and
/// whether the read hit the row cap with history still ahead (`scan_limit_hit`) —
/// distinct from stopping on the item limit or the end of the range.
struct UnfilteredScan {
    refs: Vec<EventRef>,
    next_range: Option<Range<u64>>,
    rows_scanned: usize,
    scan_limit_hit: bool,
}

fn next_unfiltered_event_refs(
    service: &RpcService,
    event_range: Range<u64>,
    ascending: bool,
    event_ref_limit: usize,
    row_scan_limit: usize,
) -> Result<UnfilteredScan, RpcError> {
    let Some(tx_range) = tx_range_for_event_range(event_range.clone()) else {
        return Ok(UnfilteredScan {
            refs: Vec::new(),
            next_range: None,
            rows_scanned: 0,
            scan_limit_hit: false,
        });
    };
    let rows = get_tx_seq_digest_rows(service, tx_range, !ascending, row_scan_limit)?;
    let mut refs = Vec::with_capacity(event_ref_limit);
    let mut next_range = None;
    let mut rows_scanned = 0;

    for row in rows {
        rows_scanned += 1;
        let filled_next = push_event_refs_for_row_until_limit(
            &mut refs,
            row,
            &event_range,
            ascending,
            event_ref_limit,
        );
        if refs.len() == event_ref_limit {
            // Stopped on the item limit, not the row cap.
            return Ok(UnfilteredScan {
                refs,
                next_range: filled_next,
                rows_scanned,
                scan_limit_hit: false,
            });
        }
        next_range = remaining_range_after_scanned_tx(event_range.clone(), row, ascending);
    }

    // Consumed the whole row batch without filling the item limit: capped iff the
    // batch was full (`row_scan_limit` rows) and history remains beyond it.
    let scan_limit_hit = rows_scanned == row_scan_limit && next_range.is_some();
    Ok(UnfilteredScan {
        refs,
        next_range,
        rows_scanned,
        scan_limit_hit,
    })
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
                        "selected event {} transaction {} has no events",
                        event_ref.event_seq, row.digest
                    ),
                )
            })?;
        let event = tx_events
            .data
            .get(event_ref.event_idx as usize)
            .ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!(
                        "selected event {} index {} out of range for transaction {}",
                        event_ref.event_seq, event_ref.event_idx, row.digest
                    ),
                )
            })?;
        #[allow(unused_mut)]
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
        checkpoint_boundary =
            advance_boundary_excluding_cp(checkpoint_boundary, row.checkpoint_number, options);
        let watermark = item_watermark(
            options,
            row.checkpoint_number,
            event_ref.event_seq,
            checkpoint_boundary,
        );

        let mut item = EventItem::default();
        item.watermark = Some(watermark);
        item.checkpoint = Some(row.checkpoint_number);
        item.event_index = Some(event_ref.event_idx);
        item.transaction_digest = Some(row.digest.to_string());
        item.transaction_offset = Some(row.tx_offset as u64);
        item.event = Some(proto_event);

        let mut response = ListEventsResponse::default();
        response.response = Some(list_events_response::Response::Item(item));
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
        .map(|event_ref| event_ref.tx_seq)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EventRef {
    event_seq: u64,
    tx_seq: u64,
    event_idx: u32,
    tx_seq_digest: Option<LedgerTxSeqDigest>,
}

fn push_event_refs_for_row_until_limit(
    refs: &mut Vec<EventRef>,
    row: LedgerTxSeqDigest,
    event_range: &Range<u64>,
    ascending: bool,
    event_ref_limit: usize,
) -> Option<Range<u64>> {
    if row.event_count == 0 {
        return None;
    }

    let mut next_range = None;
    if ascending {
        for event_idx in 0..row.event_count {
            let event_seq = encode_event_seq(row.tx_sequence_number, event_idx);
            if event_seq < event_range.start {
                continue;
            }
            if event_seq >= event_range.end {
                break;
            }
            refs.push(EventRef {
                event_seq,
                tx_seq: row.tx_sequence_number,
                event_idx,
                tx_seq_digest: Some(row),
            });
            next_range = remaining_range_after(event_range.clone(), event_seq, ascending);
            if refs.len() == event_ref_limit {
                return next_range;
            }
        }
    } else {
        for event_idx in (0..row.event_count).rev() {
            let event_seq = encode_event_seq(row.tx_sequence_number, event_idx);
            if event_seq >= event_range.end {
                continue;
            }
            if event_seq < event_range.start {
                break;
            }
            refs.push(EventRef {
                event_seq,
                tx_seq: row.tx_sequence_number,
                event_idx,
                tx_seq_digest: Some(row),
            });
            next_range = remaining_range_after(event_range.clone(), event_seq, ascending);
            if refs.len() == event_ref_limit {
                return next_range;
            }
        }
    }

    next_range
}

fn remaining_range_after_scanned_tx(
    event_range: Range<u64>,
    row: LedgerTxSeqDigest,
    ascending: bool,
) -> Option<Range<u64>> {
    let remaining = if ascending {
        let next_tx = row.tx_sequence_number.saturating_add(1);
        event_seq_lo(next_tx)..event_range.end
    } else {
        event_range.start..event_seq_lo(row.tx_sequence_number)
    };
    (!remaining.is_empty()).then_some(remaining)
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
) -> Result<ResolvedRange, RpcError> {
    let cp_range = checkpoint_range.resolve(options);
    if cp_range.is_empty() {
        let tx_boundary =
            checkpoint_to_tx_boundary(service, cp_range.terminal_checkpoint(options.ordering))?;
        let event_boundary = event_seq_lo(tx_boundary);
        return Ok(cp_range.with_range(event_boundary..event_boundary, options.ordering));
    }

    let tx_range = checkpoint_to_tx_range(service, cp_range.range.clone())?;
    let start_event_seq = event_seq_lo(tx_range.start);
    let end_event_seq = event_seq_lo(tx_range.end);
    let mut resolved = options
        .apply_cursor_bounds(cp_range.with_range(start_event_seq..end_event_seq, options.ordering));
    if !resolved.range.is_empty() {
        // The floor is tx-seq; the range is packed event-seq. Enforce on the low
        // end's transaction in tx-seq space. Only re-pack on an actual clamp, so a
        // cursor resuming mid-transaction keeps its event index when above the floor.
        let explicit_lower = start_checkpoint.is_some() || options.has_after_cursor();
        let floor = lowest_available_tx_seq(service)?;
        let start_tx = decode_event_seq(resolved.range.start).0;
        let clamped_tx = apply_tx_seq_floor(start_tx, explicit_lower, floor)?;
        if clamped_tx != start_tx {
            resolved.range.start = event_seq_lo(clamped_tx);
        }
    }
    Ok(resolved)
}

fn tx_range_for_event_range(event_range: Range<u64>) -> Option<Range<u64>> {
    let last_event_seq = event_range.end.checked_sub(1)?;
    let start_tx = decode_event_seq(event_range.start).0;
    let end_tx = decode_event_seq(last_event_seq).0.checked_add(1)?;
    if start_tx >= end_tx {
        return None;
    }

    Some(start_tx..end_tx)
}

fn watermark_response(watermark: Watermark) -> ListEventsResponse {
    let mut response = ListEventsResponse::default();
    response.response = Some(list_events_response::Response::Watermark(watermark));
    response
}

fn end_response(reason: QueryEndReason) -> ListEventsResponse {
    let mut end = QueryEnd::default();
    end.reason = reason as i32;

    let mut response = ListEventsResponse::default();
    response.response = Some(list_events_response::Response::End(end));
    response
}

/// Packed `(tx_seq, event_idx)` representation used by the event-stream index.
///
/// The low [`EVENT_BITS`] bits hold the per-transaction event index; the high
/// bits hold the transaction sequence number. Keeping a single `u64` lets the
/// inverted index store event references in a roaring bitmap.
mod event_seq {
    pub(super) const EVENT_BITS: u32 = 16;
    pub(super) const MAX_EVENTS_PER_TX: u32 = 1 << EVENT_BITS;

    pub(super) fn encode_event_seq(tx_seq: u64, event_idx: u32) -> u64 {
        (tx_seq << EVENT_BITS) | event_idx as u64
    }

    pub(super) fn decode_event_seq(event_seq: u64) -> (u64, u32) {
        let tx_seq = event_seq >> EVENT_BITS;
        let event_idx = (event_seq & (MAX_EVENTS_PER_TX as u64 - 1)) as u32;
        (tx_seq, event_idx)
    }

    pub(super) fn event_seq_lo(tx_seq: u64) -> u64 {
        tx_seq << EVENT_BITS
    }
}
