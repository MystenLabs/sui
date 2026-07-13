// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream::BoxStream;
use sui_inverted_index::ScanDirection;
use sui_inverted_index::ScanStop;
use sui_inverted_index::event_seq;
use sui_kvstore::BitmapIndexSpec;
use sui_kvstore::TransactionData;
use sui_kvstore::TxSeqDigestData;
use sui_kvstore::tables::transactions::col;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::Event as ProtoEvent;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEnd;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_rpc_api::ledger_history::query_options::CheckpointRange;
use sui_rpc_api::ledger_history::query_options::EventPosition;
use sui_rpc_api::ledger_history::query_options::EventScanBounds;
use sui_rpc_api::ledger_history::query_options::QueryOptions;
use sui_rpc_api::ledger_history::query_options::RangeExhaustion;
use sui_rpc_api::ledger_history::query_options::ResolvedEventRange;
use sui_rpc_api::ledger_history::watermark::ScanTerminal;
use sui_rpc_api::ledger_history::watermark::advance_covered_bound_before_checkpoint;
use sui_rpc_api::ledger_history::watermark::boundary_watermark;
use sui_rpc_api::ledger_history::watermark::item_watermark;
use sui_rpc_api::ledger_history::watermark::scan_frontier_cursor_cp;
use sui_rpc_api::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc_cursor::Position;
use sui_types::digests::TransactionDigest;
use tracing::Instrument;
use tracing::debug_span;
use tracing::info;

use crate::PackageResolver;
use crate::bigtable_client::BigTableClient;
use crate::config::PipelineStage;
use crate::operation::QueryContext;
use crate::pipeline::InputOrderEmitter;
use crate::pipeline::ResolvedScanStop;
use crate::pipeline::ResolvedWatermarked;
use crate::pipeline::Watermarked;
use crate::pipeline::pipelined_chunks;
use crate::pipeline::resolve_scan_watermarks;
use crate::pipeline::take_items;
use crate::render::render_json;

const EVENT_READ_MASK_DEFAULT: &str = sui_rpc_api::read_mask_defaults::EVENT;

pub(crate) type ListEventsStream = BoxStream<'static, Result<ListEventsResponse, RpcError>>;

pub(crate) async fn list_events(
    ctx: QueryContext,
    request: ListEventsRequest,
) -> Result<ListEventsStream, RpcError> {
    let started = Instant::now();
    let filtered = request.filter.is_some();
    let client: BigTableClient = ctx.client().clone();
    let resolver: crate::PackageResolver = ctx.package_resolver().clone();
    let checkpoint_hi_exclusive = ctx.checkpoint_hi_exclusive();
    let lh = ctx.ledger_history();
    let endpoint = lh.list_events();
    let tx_seq_digest_stage = ctx.stage(PipelineStage::TxSeqDigest);
    let transactions_stage = ctx.stage(PipelineStage::Transactions);

    let checkpoint_range = CheckpointRange::from_request(
        request.start_checkpoint,
        request.end_checkpoint,
        checkpoint_hi_exclusive,
    )?;
    let read_mask = Arc::new(validate_event_read_mask(request.read_mask)?);
    let options = QueryOptions::events_from_proto(
        request.options.as_ref(),
        endpoint.default_limit_items,
        endpoint.max_limit_items,
    )?;
    let limit_items = options.limit_items;
    let ordering = options.ordering;
    let direction = options.scan_direction();
    let wants_json = read_mask.contains(ProtoEvent::JSON_FIELD.name);

    let event_range = resolve_event_range(&client, checkpoint_range, &options)
        .instrument(debug_span!("resolve_event_range"))
        .await?;
    let exhaustion = event_range.exhaustion;
    let entry_checkpoint = event_range.entry_checkpoint;
    let range_end_checkpoint = event_range.end_checkpoint;
    let range_end_position = event_range.end_position;
    let event_bounds = event_range.bounds;

    if event_range.is_empty() {
        info!(
            filtered,
            wants_json,
            limit_items,
            ?ordering,
            emitted = 0,
            elapsed_ms = started.elapsed().as_millis(),
            "list_events: empty range"
        );
        // Empty resolved ranges still surface their terminal cursor, but
        // natural completion claims no checkpoint.
        let terminal_position = Position::Events {
            checkpoint: range_end_checkpoint,
            tx_seq: range_end_position.tx_seq,
            event_index: range_end_position.event_index,
        };
        return Ok(futures::stream::iter([Ok(range_end_response(
            &options,
            exhaustion,
            terminal_position,
            None,
            true,
        )
        .0)])
        .boxed());
    }

    let scan_budget = ctx.scan_budget(BitmapIndexSpec::event());
    let frontier_to_position: fn(u64) -> EventPosition = if filtered {
        |seq| EventPosition::from(event_seq::decode_event_seq(seq))
    } else {
        EventPosition::start_of_tx
    };

    // Stage A: stream of EventRefs. Filtered requests discover event positions
    // through the event bitmap. Unfiltered requests scan tx_seq_digest rows and
    // expand each row's event_count into concrete EventRefs.
    let request_bigtable_concurrency = ctx.request_bigtable_concurrency();
    let event_ref_stream: BoxStream<
        'static,
        Result<Watermarked<EventRef, EventPosition>, ScanStop>,
    > = if let Some(filter) = &request.filter {
        let query = ctx.event_filter_query(filter)?;
        client
            .eval_bitmap_query_stream(
                query,
                event_seq::packed_range(event_bounds.lo, event_bounds.hi),
                BitmapIndexSpec::event(),
                options.scan_direction(),
                scan_budget,
                ctx.bitmap_scan_observer(),
            )
            .map_ok(|m| {
                m.map_item(|seq| EventRef {
                    position: EventPosition::from(event_seq::decode_event_seq(seq)),
                    tx_seq_digest: None,
                })
                .map_watermark(|seq| EventPosition::from(event_seq::decode_event_seq(seq)))
            })
            .boxed()
    } else {
        unfiltered_event_refs(
            client.clone(),
            event_bounds,
            options.clone(),
            endpoint.max_limit_items as usize,
        )
    };
    let ref_stream = take_items(event_ref_stream, limit_items);

    // Stage B (filtered path only): enrich refs with tx_seq_digest. The
    // unfiltered path already populates `tx_seq_digest` from point lookups,
    // so we skip this stage there.
    let ref_with_digest_stream: BoxStream<
        'static,
        Result<Watermarked<EventRef, EventPosition>, ScanStop>,
    > = if filtered {
        pipelined_chunks(
            ref_stream,
            tx_seq_digest_stage.chunk_size,
            tx_seq_digest_stage.concurrency,
            {
                let client = client.clone();
                move |refs| {
                    let client = client.clone();
                    async move {
                        attach_tx_seq_digests(client, refs)
                            .await
                            .map(|s| s.map_err(ScanStop::Fault).boxed())
                            .map_err(ScanStop::Fault)
                    }
                }
            },
        )
    } else {
        ref_stream
    };

    let columns: Arc<[&'static str]> = Arc::from([col::EVENTS, col::CHECKPOINT_NUMBER]);

    // Stage C: Watermarked<EventRef> -> Watermarked<(EventRef, TransactionData)>.
    let tx_ref_stream = pipelined_chunks(
        ref_with_digest_stream,
        transactions_stage.chunk_size,
        transactions_stage.concurrency,
        {
            let client = client.clone();
            let columns = columns.clone();
            move |refs| {
                let client = client.clone();
                let columns = columns.clone();
                async move {
                    fetch_txs_for_refs(client, columns, refs)
                        .await
                        .map(|s| s.map_err(ScanStop::Fault).boxed())
                        .map_err(ScanStop::Fault)
                }
            }
        },
    );

    // Stage D: render. `buffered` (ordered) lets per-event JSON rendering
    // overlap while preserving input ref order in the output. Frontier
    // watermarks pass through unchanged.
    //
    // Package resolution uses the server-global `PackageResolver`, backed by
    // a cross-request LRU over the raw BigTable client. It is intentionally
    // outside this request's downstream BigTable semaphore: tying a global
    // cache miss to one request's budget would let that request's budget stall
    // unrelated requests waiting on the same package. The local buffer below
    // only bounds how many render/package-resolution attempts this request
    // runs concurrently.
    //
    // TODO: add global single-flight dedupe around package cache misses so
    // concurrent requests for the same uncached package share one BigTable
    // fetch.
    let event_stream = tx_ref_stream
        .map(move |item| {
            let resolver = resolver.clone();
            let read_mask = read_mask.clone();
            async move {
                match item? {
                    Watermarked::Item((event_ref, tx)) => {
                        let rendered =
                            render_event(event_ref, tx, &read_mask, &resolver, wants_json)
                                .await
                                .map_err(|e| ScanStop::Fault(anyhow::Error::new(e)))?;
                        Ok::<Watermarked<RenderedEvent, EventPosition>, ScanStop>(
                            Watermarked::Item(rendered),
                        )
                    }
                    Watermarked::Watermark(p) => Ok(Watermarked::Watermark(p)),
                }
            }
        })
        .buffered(request_bigtable_concurrency)
        .boxed();

    let event_stream = resolve_scan_watermarks(
        event_stream,
        client.event_wm_resolver(direction),
        frontier_to_position,
    );

    Ok(async_stream::try_stream! {
        futures::pin_mut!(event_stream);
        let mut emitted = 0usize;
        let mut covered_checkpoint_bound: Option<u64> = None;
        let terminal_reason = loop {
            let Some(item) = event_stream.next().await else {
                let terminal_position = Position::Events {
                    checkpoint: range_end_checkpoint,
                    tx_seq: range_end_position.tx_seq,
                    event_index: range_end_position.event_index,
                };
                let (response, reason) = range_end_response(
                    &options,
                    exhaustion,
                    terminal_position,
                    covered_checkpoint_bound,
                    false,
                );
                yield response;
                break reason;
            };
            match item {
                Ok(ResolvedWatermarked::Item(rendered)) => {
                    let item_checkpoint = rendered.checkpoint_number;
                    covered_checkpoint_bound = advance_covered_bound_before_checkpoint(
                        covered_checkpoint_bound,
                        item_checkpoint,
                        entry_checkpoint,
                        &options,
                    );
                    let watermark = item_watermark(
                        Position::Events {
                            checkpoint: item_checkpoint,
                            tx_seq: rendered.position.tx_seq,
                            event_index: rendered.position.event_index,
                        },
                        covered_checkpoint_bound,
                    );
                    emitted += 1;
                    let mut response = event_item_response(rendered.event, watermark);
                    if emitted == limit_items {
                        let mut end = QueryEnd::default();
                        end.reason = Some(QueryEndReason::ItemLimit as i32);
                        response.end = Some(end);
                        yield response;
                        break QueryEndReason::ItemLimit;
                    }
                    yield response;
                }
                Ok(ResolvedWatermarked::Watermark {
                    position,
                    cp: checkpoint_at_frontier,
                }) => {
                    let watermark = event_frontier_watermark(
                        &options,
                        direction,
                        entry_checkpoint,
                        &mut covered_checkpoint_bound,
                        position,
                        Some(checkpoint_at_frontier),
                    )?;
                    yield watermark_response(watermark);
                }
                Err(stop) => {
                    yield terminal_response_from_scan_stop(
                        stop,
                        &options,
                        direction,
                        entry_checkpoint,
                        &mut covered_checkpoint_bound,
                    )?;
                    break QueryEndReason::ScanLimit;
                }
            }
        };
        info!(
            filtered,
            wants_json,
            limit_items,
            ?ordering,
            emitted,
            ?terminal_reason,
            elapsed_ms = started.elapsed().as_millis(),
            "list_events: done"
        );
    }
    .boxed())
}

fn watermark_response(watermark: Watermark) -> ListEventsResponse {
    let mut response = ListEventsResponse::default();
    response.watermark = Some(watermark);
    response
}

fn event_item_response(event: ProtoEvent, watermark: Watermark) -> ListEventsResponse {
    let mut response = ListEventsResponse::default();
    response.event = Some(event);
    response.watermark = Some(watermark);
    response
}

/// Stage B: for filtered refs (no `tx_seq_digest`), dedupe tx_seqs in the
/// chunk, fetch their digests via one multi-get, and emit enriched refs in
/// input ref-index order. A single arriving digest can release multiple
/// refs (multiple events from one tx).
async fn attach_tx_seq_digests(
    client: BigTableClient,
    refs: Vec<EventRef>,
) -> Result<BoxStream<'static, Result<EventRef, anyhow::Error>>, anyhow::Error> {
    if refs.is_empty() {
        return Ok(futures::stream::empty().boxed());
    }

    let mut unique_seqs: Vec<u64> = refs.iter().map(|p| p.position.tx_seq).collect();
    unique_seqs.sort_unstable();
    unique_seqs.dedup();
    // tx_seq -> indices of refs sharing that tx_seq, so a single arriving
    // digest can release all dependent refs at once.
    let mut indices_by_seq: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, p) in refs.iter().enumerate() {
        indices_by_seq.entry(p.position.tx_seq).or_default().push(i);
    }

    let digest_stream = client.resolve_tx_digests_stream(unique_seqs).await?;
    Ok(async_stream::try_stream! {
        // Keyed by ref index so the emitter releases in input-ref order.
        let input_indices: Vec<usize> = (0..refs.len()).collect();
        let mut emitter: InputOrderEmitter<usize, EventRef> =
            InputOrderEmitter::new(input_indices);
        futures::pin_mut!(digest_stream);
        while let Some(row) = digest_stream.next().await {
            let row = row?;
            let idxs = indices_by_seq
                .remove(&row.tx_sequence_number)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "list_events: unexpected transaction digest row {}",
                        row.tx_sequence_number
                    )
                })?;
            for idx in idxs {
                let mut p = refs[idx];
                p.tx_seq_digest = Some(row);
                for v in emitter.push(
                    idx,
                    p,
                    "list_events: transaction digest lookup for selected event",
                )? {
                    yield v;
                }
            }
        }
        for v in emitter.finish("list_events: missing selected event transaction digest")? {
            yield v;
        }
    }
    .boxed())
}

/// Stage C: fetch transactions for a chunk of refs, emitting
/// `(EventRef, TransactionData)` in input ref-index order.
async fn fetch_txs_for_refs(
    client: BigTableClient,
    columns: Arc<[&'static str]>,
    refs: Vec<EventRef>,
) -> Result<BoxStream<'static, Result<(EventRef, TransactionData), anyhow::Error>>, anyhow::Error> {
    if refs.is_empty() {
        return Ok(futures::stream::empty().boxed());
    }

    let mut unique_digests: Vec<TransactionDigest> = Vec::new();
    let mut seen_digests: std::collections::HashSet<TransactionDigest> =
        std::collections::HashSet::new();
    let mut indices_by_digest: HashMap<TransactionDigest, Vec<usize>> = HashMap::new();
    for (i, p) in refs.iter().enumerate() {
        let row = p.tx_seq_digest.ok_or_else(|| {
            anyhow::anyhow!(
                "list_events: selected event {}/{} is missing transaction digest",
                p.position.tx_seq,
                p.position.event_index
            )
        })?;
        if seen_digests.insert(row.digest) {
            unique_digests.push(row.digest);
        }
        indices_by_digest.entry(row.digest).or_default().push(i);
    }

    let column_filter = BigTableClient::column_filter(&columns);
    let tx_stream = client
        .get_transactions_stream(unique_digests, Some(column_filter))
        .await?;
    Ok(async_stream::try_stream! {
        let input_indices: Vec<usize> = (0..refs.len()).collect();
        let mut emitter: InputOrderEmitter<usize, (EventRef, TransactionData)> =
            InputOrderEmitter::new(input_indices);
        futures::pin_mut!(tx_stream);
        while let Some(row) = tx_stream.next().await {
            let (digest, tx) = row?;
            // All refs sharing this digest become ready at once. We clone
            // `tx` per ref so each event from the same tx has its own
            // `TransactionData`; downstream consumes by-value when reading
            // `tx.events.data[event_idx]`.
            let idxs = indices_by_digest.remove(&digest).ok_or_else(|| {
                anyhow::anyhow!("list_events: unexpected transaction body row {digest}")
            })?;
            for idx in idxs {
                for v in emitter.push(
                    idx,
                    (refs[idx], tx.clone()),
                    "list_events: transaction body lookup for selected event",
                )? {
                    yield v;
                }
            }
        }
        for v in emitter.finish("list_events: missing selected event transaction body")? {
            yield v;
        }
    }
    .boxed())
}

/// Carries the rendered `Event` plus the checkpoint and event position the main
/// loop needs to update the watermark.
struct RenderedEvent {
    event: ProtoEvent,
    checkpoint_number: u64,
    position: EventPosition,
}

async fn render_event(
    event_ref: EventRef,
    tx: TransactionData,
    read_mask: &FieldMaskTree,
    resolver: &PackageResolver,
    wants_json: bool,
) -> Result<RenderedEvent, RpcError> {
    let tx_events = tx.events.as_ref().ok_or_else(|| {
        RpcError::new(
            tonic::Code::Internal,
            format!(
                "list_events: selected event {}/{} transaction {} has no events column",
                event_ref.position.tx_seq, event_ref.position.event_index, tx.digest
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
                    "list_events: selected event {}/{} index out of range for transaction {}",
                    event_ref.position.tx_seq, event_ref.position.event_index, tx.digest
                ),
            )
        })?;

    let mut proto_event = ProtoEvent::merge_from(event, read_mask);
    if wants_json {
        proto_event.json = render_json(resolver, &event.type_, &event.contents)
            .await
            .map(Box::new);
    }

    // The event's ledger position rides on the `Event` message itself rather
    // than the response frame; populate each position field only when the read
    // mask requests it. Authenticated-stream clients that need to reconstruct
    // the `EventCommitment` leaf ask for these paths.
    if read_mask.contains(ProtoEvent::CHECKPOINT_FIELD.name) {
        proto_event.checkpoint = Some(tx.checkpoint_number);
    }
    if read_mask.contains(ProtoEvent::TRANSACTION_DIGEST_FIELD.name) {
        proto_event.transaction_digest = Some(tx.digest.to_string());
    }
    if read_mask.contains(ProtoEvent::TRANSACTION_INDEX_FIELD.name) {
        proto_event.transaction_index = event_ref.tx_seq_digest.map(|row| row.tx_offset as u64);
    }
    if read_mask.contains(ProtoEvent::EVENT_INDEX_FIELD.name) {
        proto_event.event_index = Some(event_ref.position.event_index);
    }

    Ok(RenderedEvent {
        event: proto_event,
        checkpoint_number: tx.checkpoint_number,
        position: event_ref.position,
    })
}

fn end_response(watermark: Watermark, reason: QueryEndReason) -> ListEventsResponse {
    let mut end = QueryEnd::default();
    end.reason = Some(reason as i32);

    let mut response = ListEventsResponse::default();
    response.watermark = Some(watermark);
    response.end = Some(end);
    response
}

/// Trailing terminal frame for range exhaustion. Reason and watermark derive
/// from one `ScanTerminal`, so they cannot disagree. Natural completion of an
/// empty interval retains its cursor but claims no checkpoint.
fn range_end_response(
    options: &QueryOptions,
    exhaustion: RangeExhaustion,
    position: Position,
    covered_checkpoint_bound: Option<u64>,
    interval_empty: bool,
) -> (ListEventsResponse, QueryEndReason) {
    let terminal = ScanTerminal::from_range_exhaustion(exhaustion, position, interval_empty);
    let reason = terminal.reason();
    (
        end_response(
            terminal.into_watermark(options, covered_checkpoint_bound),
            reason,
        ),
        reason,
    )
}

fn event_frontier_watermark(
    options: &QueryOptions,
    direction: ScanDirection,
    entry_checkpoint: u64,
    covered_checkpoint_bound: &mut Option<u64>,
    position: EventPosition,
    checkpoint_at_frontier: Option<u64>,
) -> Result<Watermark, RpcError> {
    if let Some(checkpoint) = checkpoint_at_frontier {
        *covered_checkpoint_bound = advance_covered_bound_before_checkpoint(
            *covered_checkpoint_bound,
            checkpoint,
            entry_checkpoint,
            options,
        );
    }
    let cursor_checkpoint =
        scan_frontier_cursor_cp(checkpoint_at_frontier, position.tx_seq, direction).ok_or_else(
            || {
                RpcError::new(
                    tonic::Code::Internal,
                    format!(
                        "event scan frontier {}/{} has no checkpoint mapping",
                        position.tx_seq, position.event_index
                    ),
                )
            },
        )?;
    Ok(boundary_watermark(
        Position::Events {
            checkpoint: cursor_checkpoint,
            tx_seq: position.tx_seq,
            event_index: position.event_index,
        },
        *covered_checkpoint_bound,
    ))
}

fn terminal_response_from_scan_stop(
    stop: ResolvedScanStop<EventPosition>,
    options: &QueryOptions,
    direction: ScanDirection,
    entry_checkpoint: u64,
    covered_checkpoint_bound: &mut Option<u64>,
) -> Result<ListEventsResponse, RpcError> {
    let (position, checkpoint) = stop.into_scan_limit()?;
    let terminal = ScanTerminal::ScanLimit {
        watermark: event_frontier_watermark(
            options,
            direction,
            entry_checkpoint,
            covered_checkpoint_bound,
            position,
            checkpoint,
        )?,
    };
    let reason = terminal.reason();
    Ok(end_response(
        terminal.into_watermark(options, *covered_checkpoint_bound),
        reason,
    ))
}

/// A reference to a single event from the bitmap scan or tx_seq_digest lookup,
/// carrying just enough to look up the concrete event after a bulk tx fetch.
/// Unfiltered discovery already reads tx_seq_digest rows to enumerate real
/// events, so those rows are carried forward instead of being fetched again.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EventRef {
    position: EventPosition,
    tx_seq_digest: Option<TxSeqDigestData>,
}

/// Range-scan `tx_seq_digest` across the tx range covered by `bounds`,
/// using each row's `event_count` to enumerate real event coordinates per tx
/// without touching the tx body. This direct driver source speaks the terminal
/// [`ScanStop`] type because it is its own single-source merge.
fn unfiltered_event_refs(
    client: BigTableClient,
    bounds: EventScanBounds,
    options: QueryOptions,
    source_limit: usize,
) -> BoxStream<'static, Result<Watermarked<EventRef, EventPosition>, ScanStop>> {
    async_stream::try_stream! {
        let Some(tx_range) = bounds.tx_range() else {
            return;
        };
        let (scan_range, scan_limited, frontier_tx) =
            clamp_tx_scan_range(tx_range, source_limit, &options);
        let rows = client
            .scan_tx_seq_digests_stream(scan_range, options.scan_direction(), source_limit)
            .await
            .map_err(|e| ScanStop::Fault(anyhow::Error::new(e)))?;

        futures::pin_mut!(rows);
        while let Some(row) = rows.next().await {
            for event_ref in expand_event_refs(row?, bounds, &options) {
                yield Watermarked::Item(event_ref);
            }
        }

        if scan_limited {
            Err(ScanStop::ScanLimit {
                scan_frontier: frontier_tx,
            })?;
        }
    }
    .boxed()
}

fn clamp_tx_scan_range(
    tx_range: std::ops::Range<u64>,
    source_limit: usize,
    options: &QueryOptions,
) -> (std::ops::Range<u64>, bool, u64) {
    let source_limit = source_limit as u64;
    if options.is_ascending() {
        let end = tx_range
            .start
            .saturating_add(source_limit)
            .min(tx_range.end);
        (tx_range.start..end, end < tx_range.end, end)
    } else {
        let start = tx_range
            .end
            .saturating_sub(source_limit)
            .max(tx_range.start);
        (start..tx_range.end, start > tx_range.start, start)
    }
}

fn expand_event_refs(
    row: TxSeqDigestData,
    bounds: EventScanBounds,
    options: &QueryOptions,
) -> Vec<EventRef> {
    if row.event_count == 0 {
        return Vec::new();
    }

    let mut refs = Vec::with_capacity(row.event_count as usize);
    if options.is_ascending() {
        for event_index in 0..row.event_count {
            push_event_ref_if_in_bounds(&mut refs, row, event_index, bounds);
        }
    } else {
        for event_index in (0..row.event_count).rev() {
            push_event_ref_if_in_bounds(&mut refs, row, event_index, bounds);
        }
    }
    refs
}

fn push_event_ref_if_in_bounds(
    refs: &mut Vec<EventRef>,
    row: TxSeqDigestData,
    event_index: u32,
    bounds: EventScanBounds,
) {
    let position = EventPosition {
        tx_seq: row.tx_sequence_number,
        event_index,
    };
    if !bounds.contains(position) {
        return;
    }
    refs.push(EventRef {
        position,
        tx_seq_digest: Some(row),
    });
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

/// Resolve the explicit event-coordinate scan window from the logical checkpoint
/// bounds. Filtered scans are additionally bounded at runtime by the per-request
/// bitmap bucket budget; that limit surfaces as SCAN_LIMIT, not as an up-front
/// cp-range clamp.
async fn resolve_event_range(
    client: &BigTableClient,
    checkpoint_range: CheckpointRange,
    options: &QueryOptions,
) -> Result<ResolvedEventRange, RpcError> {
    let cp_range = checkpoint_range.resolve(options);
    if cp_range.is_empty() {
        let tx_boundary =
            checkpoint_to_tx_boundary(client, cp_range.terminal_checkpoint(options.ordering))
                .await?;
        return Ok(ResolvedEventRange::empty_at(
            cp_range.terminal_checkpoint(options.ordering),
            EventPosition::start_of_tx(tx_boundary),
            cp_range.exhaustion,
        ));
    }

    let tx_range = client
        .checkpoint_to_tx_range(cp_range.range.clone())
        .await?;
    Ok(options.apply_event_cursor_bounds(ResolvedEventRange {
        bounds: EventScanBounds::tx_span(tx_range.start, tx_range.end),
        entry_checkpoint: if options.is_ascending() {
            cp_range.range.start
        } else {
            cp_range.range.end.saturating_sub(1)
        },
        end_checkpoint: cp_range.terminal_checkpoint(options.ordering),
        end_position: match options.ordering {
            sui_rpc_api::ledger_history::query_options::Ordering::Ascending => {
                EventPosition::start_of_tx(tx_range.end)
            }
            sui_rpc_api::ledger_history::query_options::Ordering::Descending => {
                EventPosition::start_of_tx(tx_range.start)
            }
        },
        exhaustion: cp_range.exhaustion,
    }))
}

async fn checkpoint_to_tx_boundary(
    client: &BigTableClient,
    checkpoint: u64,
) -> Result<u64, RpcError> {
    if checkpoint == 0 {
        return Ok(0);
    }
    Ok(client.checkpoint_to_tx_range(0..checkpoint).await?.end)
}

#[cfg(test)]
mod tests {
    use sui_types::digests::TransactionDigest;

    use super::*;
    use sui_rpc_cursor::CursorToken;

    use crate::v2alpha::test_utils::ascending_options;
    use crate::v2alpha::test_utils::query_context;

    #[tokio::test]
    async fn empty_ledger_tip_emits_one_standalone_event_boundary() {
        let (ctx, server) = query_context("test_list_events_natural_end", 0).await;
        let mut request = ListEventsRequest::default();
        request.read_mask = Some(FieldMask::from_paths(["event_type"]));
        request.options = Some(ascending_options());

        let responses: Vec<_> = list_events(ctx, request)
            .await
            .expect("construct event stream")
            .try_collect()
            .await
            .expect("collect event stream");
        server.abort();

        assert_eq!(responses.len(), 1, "empty ledger has one terminal frame");
        let response = &responses[0];
        assert!(response.event.is_none(), "terminal frame has no payload");
        assert_eq!(
            response.end.as_ref().and_then(|end| end.reason),
            Some(QueryEndReason::LedgerTip as i32),
        );
        let watermark = response
            .watermark
            .as_ref()
            .expect("ledger exhaustion proves a terminal boundary");
        let expected_cursor = CursorToken::boundary(Position::Events {
            checkpoint: 0,
            tx_seq: 0,
            event_index: 0,
        })
        .encode();
        assert_eq!(watermark.cursor.as_ref(), Some(&expected_cursor));
        assert_eq!(watermark.checkpoint, None);
    }
    use std::ops::Bound;
    use sui_rpc_api::ledger_history::query_options::Ordering;

    fn options(ordering: Ordering) -> QueryOptions {
        let mut request = sui_rpc::proto::sui::rpc::v2alpha::QueryOptions::default();
        request.ordering = Some(match ordering {
            Ordering::Ascending => 0,
            Ordering::Descending => 1,
        });

        QueryOptions::events_from_proto(Some(&request), 100, 100).unwrap()
    }

    fn tx_row(tx_sequence_number: u64, event_count: u32) -> TxSeqDigestData {
        TxSeqDigestData {
            tx_sequence_number,
            digest: TransactionDigest::new([tx_sequence_number as u8; 32]),
            event_count,
            tx_offset: 0,
            checkpoint_number: 7,
        }
    }
    fn event_refs(refs: &[EventRef]) -> Vec<(u64, u32)> {
        refs.iter()
            .map(|r| {
                let row = r.tx_seq_digest.expect("unfiltered refs carry digest row");
                assert_eq!(row.tx_sequence_number, r.position.tx_seq);
                assert!(row.event_count > r.position.event_index);
                (r.position.tx_seq, r.position.event_index)
            })
            .collect()
    }

    #[test]
    fn expand_event_refs_skips_zero_event_transactions() {
        let row = tx_row(10, 0);
        let refs = expand_event_refs(
            row,
            EventScanBounds::tx_span(10, 11),
            &options(Ordering::Ascending),
        );
        assert!(refs.is_empty());
    }

    #[test]
    fn expand_event_refs_applies_ascending_bounds() {
        let row = tx_row(10, 4);
        let refs = expand_event_refs(
            row,
            EventScanBounds {
                lo: Bound::Included(EventPosition {
                    tx_seq: 10,
                    event_index: 1,
                }),
                hi: Bound::Excluded(EventPosition {
                    tx_seq: 10,
                    event_index: 3,
                }),
            },
            &options(Ordering::Ascending),
        );
        assert_eq!(event_refs(&refs), vec![(10, 1), (10, 2)]);
        assert_eq!(
            refs.iter().map(|r| r.position).collect::<Vec<_>>(),
            vec![
                EventPosition {
                    tx_seq: 10,
                    event_index: 1
                },
                EventPosition {
                    tx_seq: 10,
                    event_index: 2
                },
            ]
        );
    }

    #[test]
    fn expand_event_refs_applies_descending_bounds() {
        let row = tx_row(10, 4);
        let refs = expand_event_refs(
            row,
            EventScanBounds {
                lo: Bound::Included(EventPosition {
                    tx_seq: 10,
                    event_index: 1,
                }),
                hi: Bound::Excluded(EventPosition {
                    tx_seq: 10,
                    event_index: 4,
                }),
            },
            &options(Ordering::Descending),
        );
        assert_eq!(event_refs(&refs), vec![(10, 3), (10, 2), (10, 1)]);
    }

    #[test]
    fn clamp_tx_scan_range_limits_ascending_frontier() {
        let options = options(Ordering::Ascending);
        assert_eq!(clamp_tx_scan_range(10..20, 4, &options), (10..14, true, 14));
        assert_eq!(
            clamp_tx_scan_range(10..14, 4, &options),
            (10..14, false, 14)
        );
    }

    #[test]
    fn clamp_tx_scan_range_limits_descending_frontier() {
        let options = options(Ordering::Descending);
        assert_eq!(clamp_tx_scan_range(10..20, 4, &options), (16..20, true, 16));
        assert_eq!(
            clamp_tx_scan_range(16..20, 4, &options),
            (16..20, false, 16)
        );
    }
}
