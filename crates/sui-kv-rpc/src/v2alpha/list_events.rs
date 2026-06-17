// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream::BoxStream;
use sui_kvstore::BitmapIndexSpec;
use sui_kvstore::TransactionData;
use sui_kvstore::TxSeqDigestData;
use sui_kvstore::tables::event_bitmap_index;
use sui_kvstore::tables::transactions::col;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::Event as ProtoEvent;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_rpc_api::proto::google::rpc::bad_request::FieldViolation;
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
use sui_inverted_index::BitmapScanLimitExceeded;
use sui_inverted_index::error_contains;
use sui_rpc_api::ledger_history::query_options::CheckpointRange;
use sui_rpc_api::ledger_history::query_options::QueryOptions;
use sui_rpc_api::ledger_history::query_options::QueryType;
use sui_rpc_api::ledger_history::query_options::ResolvedRange;
use sui_rpc_api::ledger_history::watermark::advance_boundary_excluding_cp;
use sui_rpc_api::ledger_history::watermark::boundary_cursor_cp;
use sui_rpc_api::ledger_history::watermark::boundary_watermark;
use sui_rpc_api::ledger_history::watermark::item_watermark;
use sui_rpc_api::ledger_history::watermark::reached_range_end;
use sui_rpc_api::ledger_history::watermark::terminal_boundary_watermark;

use sui_rpc::proto::sui::rpc::v2alpha::EventItem;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEnd;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc::proto::sui::rpc::v2alpha::list_events_response;

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
    let options = QueryOptions::from_proto(
        request.options.as_ref(),
        endpoint.default_limit_items,
        endpoint.max_limit_items,
        QueryType::Events,
        request.filter.as_ref(),
    )?;
    let limit_items = options.limit_items;
    let ordering = options.ordering;
    let direction = options.scan_direction();
    let wants_json = read_mask.contains(ProtoEvent::JSON_FIELD.name);

    let event_range = resolve_event_range(&client, checkpoint_range, &options)
        .instrument(debug_span!("resolve_event_range"))
        .await?;
    let end_reason = event_range.end_reason;
    let end_checkpoint = event_range.end_checkpoint;
    let end_position = event_range.end_position;
    let event_range = event_range.range;

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
        let terminal = reached_range_end(end_reason).then(|| {
            watermark_response(terminal_boundary_watermark(
                &options,
                end_checkpoint,
                end_position,
            ))
        });
        return Ok(futures::stream::iter(
            terminal
                .into_iter()
                .chain([end_response(end_reason)])
                .map(Ok),
        )
        .boxed());
    }

    let scan_budget = ctx.scan_budget(BitmapIndexSpec::event());

    // Stage A: stream of EventRefs. Filtered requests discover event_seq
    // values through the event bitmap (per-bucket and periodic-frontier
    // watermarks). Unfiltered requests scan tx_seq_digest rows and expand
    // each row's event_count into concrete EventRefs.
    let request_bigtable_concurrency = ctx.request_bigtable_concurrency();
    let event_ref_stream: BoxStream<'static, Result<Watermarked<EventRef>, anyhow::Error>> =
        if let Some(filter) = &request.filter {
            let query = ctx.event_filter_query(filter)?;
            client
                .eval_bitmap_query_stream(
                    query,
                    event_range.clone(),
                    BitmapIndexSpec::event(),
                    options.scan_direction(),
                    scan_budget,
                    ctx.bitmap_scan_observer(),
                )
                .map_ok(|m| {
                    m.map_item(|event_seq| {
                        let (tx_seq, event_idx) = event_bitmap_index::decode_event_seq(event_seq);
                        EventRef {
                            event_seq,
                            tx_seq,
                            event_idx,
                            tx_seq_digest: None,
                        }
                    })
                })
                .boxed()
        } else {
            unfiltered_event_refs(
                client.clone(),
                event_range.clone(),
                options.clone(),
                endpoint.max_limit_items as usize,
            )
        };
    let ref_stream = take_items(event_ref_stream, limit_items);

    // Stage B (filtered path only): enrich refs with tx_seq_digest. The
    // unfiltered path already populates `tx_seq_digest` from point lookups,
    // so we skip this stage there.
    let ref_with_digest_stream: BoxStream<'static, Result<Watermarked<EventRef>, anyhow::Error>> =
        if filtered {
            pipelined_chunks(
                ref_stream,
                tx_seq_digest_stage.chunk_size,
                tx_seq_digest_stage.concurrency,
                {
                    let client = client.clone();
                    move |refs| attach_tx_seq_digests(client.clone(), refs)
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
            move |refs| fetch_txs_for_refs(client.clone(), columns.clone(), refs)
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
                            render_event(event_ref, tx, &read_mask, &resolver, wants_json).await?;
                        Ok::<Watermarked<RenderedEvent>, anyhow::Error>(Watermarked::Item(rendered))
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
        let mut checkpoint_boundary: Option<u64> = None;
        let mut scan_limit_hit = false;
        while let Some(item) = event_stream.next().await {
            match item {
                Ok(ResolvedWatermarked::Item(rendered)) => {
                    checkpoint_boundary = advance_boundary_excluding_cp(checkpoint_boundary, rendered.checkpoint_number, &options);
                    let wm = item_watermark(
                        &options,
                        rendered.checkpoint_number,
                        rendered.event_seq,
                        checkpoint_boundary,
                    );
                    emitted += 1;
                    yield event_item_response(rendered.item, wm);
                }
                Ok(ResolvedWatermarked::Watermark { position, cp }) => {
                    checkpoint_boundary = advance_boundary_excluding_cp(checkpoint_boundary, cp, &options);
                    let wm = boundary_watermark(&options, boundary_cursor_cp(cp, direction), position, checkpoint_boundary);
                    yield watermark_response(wm);
                }
                Err(e) => {
                    if error_contains::<BitmapScanLimitExceeded>(&e).is_some() {
                        scan_limit_hit = true;
                        break;
                    } else {
                        Err(RpcError::from(e))?;
                    }
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
            yield watermark_response(terminal_boundary_watermark(&options, end_checkpoint, end_position));
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

    let mut unique_seqs: Vec<u64> = refs.iter().map(|p| p.tx_seq).collect();
    unique_seqs.sort_unstable();
    unique_seqs.dedup();
    // tx_seq -> indices of refs sharing that tx_seq, so a single arriving
    // digest can release all dependent refs at once.
    let mut indices_by_seq: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, p) in refs.iter().enumerate() {
        indices_by_seq.entry(p.tx_seq).or_default().push(i);
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
                "list_events: selected event {} is missing transaction digest",
                p.event_seq
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
/// the cp and packed event_seq the main loop needs to update the
/// watermark.
struct RenderedEvent {
    item: EventItem,
    checkpoint_number: u64,
    event_seq: u64,
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
                "list_events: selected event {} transaction {} has no events column",
                event_ref.event_seq, tx.digest
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
                    "list_events: selected event {} index {} out of range for transaction {}",
                    event_ref.event_seq, event_ref.event_idx, tx.digest
                ),
            )
        })?;

    let mut proto_event = ProtoEvent::merge_from(event, read_mask);
    if wants_json {
        proto_event.json = render_json(resolver, &event.type_, &event.contents)
            .await
            .map(Box::new);
    }

    let mut item = EventItem::default();
    item.checkpoint = Some(tx.checkpoint_number);
    item.event_index = Some(event_ref.event_idx);
    item.transaction_digest = Some(tx.digest.to_string());
    item.event = Some(proto_event);
    item.transaction_offset = event_ref.tx_seq_digest.map(|row| row.tx_offset as u64);

    Ok(RenderedEvent {
        item,
        checkpoint_number: tx.checkpoint_number,
        event_seq: event_ref.event_seq,
    })
}

fn end_response(reason: QueryEndReason) -> ListEventsResponse {
    let mut end = QueryEnd::default();
    end.reason = reason as i32;

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
    event_seq: u64,
    tx_seq: u64,
    event_idx: u32,
    tx_seq_digest: Option<TxSeqDigestData>,
}

/// Range-scan `tx_seq_digest` across the tx range covered by `event_range`,
/// using each row's `event_count` to enumerate real event_seqs per tx without
/// touching the tx body.
fn unfiltered_event_refs(
    client: BigTableClient,
    event_range: std::ops::Range<u64>,
    options: QueryOptions,
    source_limit: usize,
) -> BoxStream<'static, Result<Watermarked<EventRef>, anyhow::Error>> {
    async_stream::try_stream! {
        let lower_bound = event_range.start;
        let upper_bound = event_range.end;
        let Some(tx_range) = tx_range_for_event_range(event_range) else {
            return;
        };
        let (scan_range, scan_limited, frontier_tx) =
            clamp_tx_scan_range(tx_range, source_limit, &options);
        let rows = client
            .scan_tx_seq_digests_stream(scan_range, options.scan_direction(), source_limit)
            .await?;

        futures::pin_mut!(rows);
        while let Some(row) = rows.next().await {
            for event_ref in expand_event_refs(row?, lower_bound, upper_bound, &options) {
                yield Watermarked::Item(event_ref);
            }
        }

        if scan_limited {
            yield Watermarked::Watermark(event_bitmap_index::event_seq_lo(frontier_tx));
            Err(anyhow::Error::new(BitmapScanLimitExceeded))?;
        }
    }
    .boxed()
}

fn tx_range_for_event_range(event_range: std::ops::Range<u64>) -> Option<std::ops::Range<u64>> {
    let last_event_seq = event_range.end.checked_sub(1)?;
    let start_tx = event_bitmap_index::decode_event_seq(event_range.start).0;
    let end_tx = event_bitmap_index::decode_event_seq(last_event_seq)
        .0
        .checked_add(1)?;
    (start_tx < end_tx).then_some(start_tx..end_tx)
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
    lower_bound: u64,
    upper_bound: u64,
    options: &QueryOptions,
) -> Vec<EventRef> {
    if row.event_count == 0 {
        return Vec::new();
    }

    let mut refs = Vec::with_capacity(row.event_count as usize);
    if options.is_ascending() {
        for event_idx in 0..row.event_count {
            push_event_ref_if_in_bounds(&mut refs, row, event_idx, lower_bound, upper_bound);
        }
    } else {
        for event_idx in (0..row.event_count).rev() {
            push_event_ref_if_in_bounds(&mut refs, row, event_idx, lower_bound, upper_bound);
        }
    }
    refs
}

fn push_event_ref_if_in_bounds(
    refs: &mut Vec<EventRef>,
    row: TxSeqDigestData,
    event_idx: u32,
    lower_bound: u64,
    upper_bound: u64,
) {
    let event_seq = event_bitmap_index::encode_event_seq(row.tx_sequence_number, event_idx);
    if event_seq < lower_bound || event_seq >= upper_bound {
        return;
    }
    refs.push(EventRef {
        event_seq,
        tx_seq: row.tx_sequence_number,
        event_idx,
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

/// Resolve the packed-event_seq scan window from the logical checkpoint bounds.
/// The checkpoint window is first clamped to indexed history and any cursor
/// bounds, then shifted into packed event space. Query cursors are applied
/// in that packed space so they can resume from the middle of a transaction's
/// events. Filtered scans are additionally bounded at runtime by the
/// per-request bitmap bucket budget; that limit surfaces as SCAN_LIMIT, not
/// as an up-front cp-range clamp.
async fn resolve_event_range(
    client: &BigTableClient,
    checkpoint_range: CheckpointRange,
    options: &QueryOptions,
) -> Result<ResolvedRange, RpcError> {
    let cp_range = checkpoint_range.resolve(options);
    if cp_range.is_empty() {
        let tx_boundary =
            checkpoint_to_tx_boundary(client, cp_range.terminal_checkpoint(options.ordering))
                .await?;
        let event_boundary = event_bitmap_index::event_seq_lo(tx_boundary);
        return Ok(cp_range.with_range(event_boundary..event_boundary, options.ordering));
    }

    let tx_range = client
        .checkpoint_to_tx_range(cp_range.range.clone())
        .await?;
    let start_event_seq = event_bitmap_index::event_seq_lo(tx_range.start);
    let end_event_seq = event_bitmap_index::event_seq_lo(tx_range.end);
    Ok(options
        .apply_cursor_bounds(cp_range.with_range(start_event_seq..end_event_seq, options.ordering)))
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
    use sui_rpc_api::ledger_history::query_options::Ordering;

    fn options(ordering: Ordering) -> QueryOptions {
        let mut request = sui_rpc::proto::sui::rpc::v2alpha::QueryOptions::default();
        request.ordering = match ordering {
            Ordering::Ascending => 0,
            Ordering::Descending => 1,
        };

        QueryOptions::from_proto(
            Some(&request),
            100,
            100,
            QueryType::Events,
            Option::<&sui_rpc::proto::sui::rpc::v2alpha::EventFilter>::None,
        )
        .unwrap()
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
                assert_eq!(row.tx_sequence_number, r.tx_seq);
                assert!(row.event_count > r.event_idx);
                (r.tx_seq, r.event_idx)
            })
            .collect()
    }

    #[test]
    fn expand_event_refs_skips_zero_event_transactions() {
        let row = tx_row(10, 0);
        let refs = expand_event_refs(
            row,
            event_bitmap_index::event_seq_lo(10),
            event_bitmap_index::event_seq_lo(11),
            &options(Ordering::Ascending),
        );
        assert!(refs.is_empty());
    }

    #[test]
    fn expand_event_refs_applies_ascending_bounds() {
        let row = tx_row(10, 4);
        let refs = expand_event_refs(
            row,
            event_bitmap_index::encode_event_seq(10, 1),
            event_bitmap_index::encode_event_seq(10, 3),
            &options(Ordering::Ascending),
        );
        assert_eq!(event_refs(&refs), vec![(10, 1), (10, 2)]);
        assert_eq!(
            refs.iter().map(|r| r.event_seq).collect::<Vec<_>>(),
            vec![
                event_bitmap_index::encode_event_seq(10, 1),
                event_bitmap_index::encode_event_seq(10, 2),
            ]
        );
    }

    #[test]
    fn expand_event_refs_applies_descending_bounds() {
        let row = tx_row(10, 4);
        let refs = expand_event_refs(
            row,
            event_bitmap_index::encode_event_seq(10, 1),
            event_bitmap_index::encode_event_seq(10, 4),
            &options(Ordering::Descending),
        );
        assert_eq!(event_refs(&refs), vec![(10, 3), (10, 2), (10, 1)]);
    }

    #[test]
    fn tx_range_for_event_range_covers_partial_endpoint_transactions() {
        let range =
            event_bitmap_index::encode_event_seq(10, 2)..event_bitmap_index::event_seq_lo(13);

        assert_eq!(tx_range_for_event_range(range), Some(10..13));
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
