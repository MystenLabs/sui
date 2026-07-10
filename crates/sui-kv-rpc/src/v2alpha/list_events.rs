// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream::BoxStream;
use sui_inverted_index::BitmapScanError;
use sui_inverted_index::BitmapScanResult;
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
use sui_rpc::proto::sui::rpc::v2alpha::EventItem;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEnd;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc::proto::sui::rpc::v2alpha::list_events_response;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_rpc_api::ledger_history::query_options::CheckpointRange;
use sui_rpc_api::ledger_history::query_options::EventPosition;
use sui_rpc_api::ledger_history::query_options::EventScanBounds;
use sui_rpc_api::ledger_history::query_options::QueryOptions;
use sui_rpc_api::ledger_history::query_options::ResolvedEventRange;
use sui_rpc_api::ledger_history::watermark::CheckpointBoundary;
use sui_rpc_api::ledger_history::watermark::reached_range_end;
use sui_rpc_api::ledger_history::watermark::terminal_boundary_watermark;
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
use crate::pipeline::ResolvedWatermarked;
use crate::pipeline::Watermarked;
use crate::pipeline::pipelined_chunks;
use crate::pipeline::resolve_watermarks;
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
    let ascending = options.is_ascending();
    let wants_json = read_mask.contains(ProtoEvent::JSON_FIELD.name);

    let event_range = resolve_event_range(&client, checkpoint_range, &options)
        .instrument(debug_span!("resolve_event_range"))
        .await?;
    let end_reason = event_range.end_reason;
    let terminal_watermark = terminal_boundary_watermark(
        Position::Events {
            checkpoint: event_range.end_checkpoint,
            tx_seq: event_range.end_position.tx_seq,
            event_index: event_range.end_position.event_index,
        },
        ascending,
    );
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
        // A caught-up tail (e.g. polling at the ledger tip) resolves to an empty
        // range; still surface the terminal boundary so the client learns the
        // final checkpoint is complete without waiting for the next item.
        let terminal =
            reached_range_end(end_reason).then(|| watermark_response(terminal_watermark));
        return Ok(futures::stream::iter(
            terminal
                .into_iter()
                .chain([end_response(end_reason)])
                .map(Ok),
        )
        .boxed());
    }

    let scan_budget = ctx.scan_budget(BitmapIndexSpec::event());

    // Stage A: stream of EventRefs. Filtered requests discover event positions
    // through the event bitmap. Unfiltered requests scan tx_seq_digest rows and
    // expand each row's event_count into concrete EventRefs.
    let request_bigtable_concurrency = ctx.request_bigtable_concurrency();
    let event_ref_stream: BoxStream<
        'static,
        BitmapScanResult<Watermarked<EventRef, EventPosition>>,
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
        BitmapScanResult<Watermarked<EventRef, EventPosition>>,
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
                            .map(|s| s.map_err(BitmapScanError::Source).boxed())
                            .map_err(BitmapScanError::Source)
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
                        .map(|s| s.map_err(BitmapScanError::Source).boxed())
                        .map_err(BitmapScanError::Source)
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
                                .map_err(|e| BitmapScanError::Source(anyhow::Error::new(e)))?;
                        Ok::<Watermarked<RenderedEvent, EventPosition>, BitmapScanError>(
                            Watermarked::Item(rendered),
                        )
                    }
                    Watermarked::Watermark(p) => Ok(Watermarked::Watermark(p)),
                }
            }
        })
        .buffered(request_bigtable_concurrency)
        .boxed();

    let event_stream = resolve_watermarks(event_stream, client.event_wm_resolver(direction));

    Ok(async_stream::try_stream! {
        futures::pin_mut!(event_stream);
        let mut emitted = 0usize;
        let mut checkpoint_boundary = CheckpointBoundary::new(ascending);
        let mut scan_limit_hit = false;
        while let Some(item) = event_stream.next().await {
            match item {
                Ok(ResolvedWatermarked::Item(rendered)) => {
                    let wm = checkpoint_boundary.item_watermark_entered(
                        Position::Events { checkpoint: rendered.checkpoint_number, tx_seq: rendered.position.tx_seq, event_index: rendered.position.event_index },
                    );
                    emitted += 1;
                    yield event_item_response(rendered.item, wm);
                }
                Ok(ResolvedWatermarked::Watermark { position, cp }) => {
                    let wm = checkpoint_boundary.frontier_watermark(Position::Events {
                        checkpoint: cp,
                        tx_seq: position.tx_seq,
                        event_index: position.event_index,
                    });
                    yield watermark_response(wm);
                }
                Err(BitmapScanError::ScanLimit) => {
                    scan_limit_hit = true;
                    break;
                }
                Err(BitmapScanError::Cancelled) => {
                    Err(RpcError::new(
                        tonic::Code::Cancelled,
                        BitmapScanError::Cancelled.to_string(),
                    ))?;
                }
                Err(BitmapScanError::Source(inner)) => {
                    Err(RpcError::from(inner))?;
                }
            }
        }
        let reason = if scan_limit_hit {
            QueryEndReason::ScanLimit
        } else if emitted == limit_items {
            QueryEndReason::ItemLimit
        } else {
            end_reason
        };
        if reached_range_end(reason) {
            yield watermark_response(terminal_watermark);
        }
        yield end_response(reason);
        info!(
            filtered,
            wants_json,
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

fn watermark_response(watermark: Watermark) -> ListEventsResponse {
    let mut response = ListEventsResponse::default();
    response.response = Some(list_events_response::Response::Watermark(watermark));
    response
}

fn event_item_response(mut item: EventItem, watermark: Watermark) -> ListEventsResponse {
    item.watermark = Some(watermark);
    let mut response = ListEventsResponse::default();
    response.response = Some(list_events_response::Response::Item(item));
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

/// Carries the rendered `EventItem` (without its `watermark` field — the
/// main loop fills that in with the current checkpoint boundary) plus
/// the checkpoint and event position the main loop needs to update the
/// watermark.
struct RenderedEvent {
    item: EventItem,
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
    // than the enclosing `EventItem`; populate each position field only when the
    // read mask requests it. Authenticated-stream clients that need to
    // reconstruct the `EventCommitment` leaf ask for these paths.
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

    let mut item = EventItem::default();
    item.event = Some(proto_event);

    Ok(RenderedEvent {
        item,
        checkpoint_number: tx.checkpoint_number,
        position: event_ref.position,
    })
}

fn end_response(reason: QueryEndReason) -> ListEventsResponse {
    let mut end = QueryEnd::default();
    end.reason = Some(reason as i32);

    let mut response = ListEventsResponse::default();
    response.response = Some(list_events_response::Response::End(end));
    response
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
/// without touching the tx body.
fn unfiltered_event_refs(
    client: BigTableClient,
    bounds: EventScanBounds,
    options: QueryOptions,
    source_limit: usize,
) -> BoxStream<'static, BitmapScanResult<Watermarked<EventRef, EventPosition>>> {
    async_stream::try_stream! {
        let Some(tx_range) = bounds.tx_range() else {
            return;
        };
        let (scan_range, scan_limited, frontier_tx) =
            clamp_tx_scan_range(tx_range, source_limit, &options);
        let rows = client
            .scan_tx_seq_digests_stream(scan_range, options.scan_direction(), source_limit)
            .await
            .map_err(|e| BitmapScanError::Source(anyhow::Error::new(e)))?;

        futures::pin_mut!(rows);
        while let Some(row) = rows.next().await {
            for event_ref in expand_event_refs(row?, bounds, &options) {
                yield Watermarked::Item(event_ref);
            }
        }

        if scan_limited {
            yield Watermarked::Watermark(EventPosition::start_of_tx(frontier_tx));
            Err(BitmapScanError::ScanLimit)?;
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
            cp_range.end_reason,
        ));
    }

    let tx_range = client
        .checkpoint_to_tx_range(cp_range.range.clone())
        .await?;
    Ok(options.apply_event_cursor_bounds(ResolvedEventRange {
        bounds: EventScanBounds::tx_span(tx_range.start, tx_range.end),
        end_checkpoint: cp_range.terminal_checkpoint(options.ordering),
        end_position: match options.ordering {
            sui_rpc_api::ledger_history::query_options::Ordering::Ascending => {
                EventPosition::start_of_tx(tx_range.end)
            }
            sui_rpc_api::ledger_history::query_options::Ordering::Descending => {
                EventPosition::start_of_tx(tx_range.start)
            }
        },
        end_reason: cp_range.end_reason,
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
