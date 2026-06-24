// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream;
use futures::stream::BoxStream;
use sui_kvstore::BitmapIndexSpec;
use sui_kvstore::CheckpointData;
use sui_kvstore::tables;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::Checkpoint;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_rpc_api::proto::google::rpc::bad_request::FieldViolation;
use tracing::Instrument;
use tracing::debug_span;
use tracing::info;

use crate::bigtable_client::BigTableClient;
use crate::bigtable_client::stage;
use crate::config::PipelineStage;
use crate::operation::QueryContext;
use crate::pipeline::InputOrderEmitter;
use crate::pipeline::ResolvedWatermarked;
use crate::pipeline::Watermarked;
use crate::pipeline::pipelined_chunks;
use crate::pipeline::resolve_watermarks;
use crate::pipeline::take_items;
use crate::render::render_full_checkpoint;
use crate::resolve;
use crate::resolve::list_checkpoint_columns;
use crate::resolve::needs_transactions_or_objects;
use sui_inverted_index::BitmapScanLimitExceeded;
use sui_inverted_index::ScanDirection;
use sui_inverted_index::error_contains;
use sui_rpc_api::ledger_history::query_options::CheckpointRange;
use sui_rpc_api::ledger_history::query_options::QueryOptions;
use sui_rpc_api::ledger_history::query_options::QueryType;
use sui_rpc_api::ledger_history::query_options::ResolvedRange;
use sui_rpc_api::ledger_history::watermark::advance_checkpoint_boundary;
use sui_rpc_api::ledger_history::watermark::boundary_watermark;
use sui_rpc_api::ledger_history::watermark::item_watermark;
use sui_rpc_api::ledger_history::watermark::reached_range_end;
use sui_rpc_api::ledger_history::watermark::terminal_boundary_watermark;

use sui_rpc::proto::sui::rpc::v2alpha::CheckpointItem;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEnd;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc::proto::sui::rpc::v2alpha::list_checkpoints_response;

const READ_MASK_DEFAULT: &str = sui_rpc_api::read_mask_defaults::CHECKPOINT;

pub(crate) type ListCheckpointsStream =
    BoxStream<'static, Result<ListCheckpointsResponse, RpcError>>;

pub(crate) async fn list_checkpoints(
    ctx: QueryContext,
    request: ListCheckpointsRequest,
) -> Result<ListCheckpointsStream, RpcError> {
    let started = Instant::now();
    let filtered = request.filter.is_some();
    let client: BigTableClient = ctx.client().clone();
    let checkpoint_hi_exclusive = ctx.checkpoint_hi_exclusive();
    let lh = ctx.ledger_history();
    let endpoint = lh.list_checkpoints();
    let checkpoints_stage = ctx.stage(PipelineStage::Checkpoints);
    let transactions_stage = ctx.stage(PipelineStage::Transactions);
    let objects_stage = ctx.stage(PipelineStage::Objects);

    let checkpoint_range = CheckpointRange::from_request(
        request.start_checkpoint,
        request.end_checkpoint,
        checkpoint_hi_exclusive,
    )?;
    let read_mask = {
        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
        read_mask.validate::<Checkpoint>().map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        FieldMaskTree::from(read_mask)
    };
    let options = QueryOptions::from_proto(
        request.options.as_ref(),
        endpoint.default_limit_items,
        endpoint.max_limit_items,
        QueryType::Checkpoints,
    )?;
    let limit_items = options.limit_items;
    let ordering = options.ordering;
    let direction = options.scan_direction();

    let cp_range = async { Ok::<_, RpcError>(resolve_cp_range(checkpoint_range, &options)) }
        .instrument(debug_span!("resolve_cp_range"))
        .await?;
    let end_reason = cp_range.end_reason;
    let end_checkpoint = cp_range.end_checkpoint;
    let end_position = cp_range.end_position;
    let cp_range = cp_range.range;

    if cp_range.is_empty() {
        info!(
            filtered,
            limit_items,
            ?ordering,
            emitted = 0usize,
            elapsed_ms = started.elapsed().as_millis(),
            "list_checkpoints: empty range"
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
        return Ok(stream::iter(
            terminal
                .into_iter()
                .chain([end_response(end_reason)])
                .map(Ok),
        )
        .boxed());
    }

    let needs_full = needs_transactions_or_objects(&read_mask);
    let cp_columns: Arc<[&'static str]> = list_checkpoint_columns(&read_mask, needs_full).into();

    // Stage A: discover checkpoint rows for the requested response. Filtered
    // requests use sparse bitmap-eval over transactions and then fetch the
    // deduped checkpoint rows. Unfiltered requests scan the dense checkpoint
    // keyspace directly, bounded by limit_items.
    let cp_data_stream: BoxStream<
        'static,
        Result<Watermarked<(u64, CheckpointData)>, anyhow::Error>,
    > = if let Some(filter) = &request.filter {
        let scan_budget = ctx.scan_budget(BitmapIndexSpec::tx());
        let tx_range = client.checkpoint_to_tx_range(cp_range.clone()).await?;
        let seq_stream = filtered_checkpoint_seq_stream(
            &ctx,
            filter,
            tx_range,
            limit_items,
            options.clone(),
            scan_budget,
        )
        .await?;
        let seq_stream = take_items(seq_stream, limit_items);
        pipelined_chunks(
            seq_stream,
            checkpoints_stage.chunk_size,
            checkpoints_stage.concurrency,
            {
                let client = client.clone();
                let columns = cp_columns.clone();
                move |seqs| fetch_checkpoint_data(client.clone(), columns.clone(), seqs)
            },
        )
    } else {
        scan_checkpoint_data(
            client.clone(),
            cp_columns.clone(),
            cp_range.clone(),
            limit_items,
            &options,
        )
        .await?
    };

    // Fast path: read_mask doesn't request transactions or objects → render
    // directly from CheckpointData via the existing `checkpoint_to_response`.
    if !needs_full {
        let cp_data_stream = resolve_watermarks(cp_data_stream, client.tx_wm_resolver(direction));
        return Ok(async_stream::try_stream! {
            futures::pin_mut!(cp_data_stream);
            let mut emitted = 0usize;
            let mut checkpoint_boundary: Option<u64> = None;
            let mut scan_limit_hit = false;
            while let Some(item) = cp_data_stream.next().await {
                match item {
                    Ok(ResolvedWatermarked::Item((cp_seq, cp_data))) => {
                        checkpoint_boundary = advance_checkpoint_boundary(checkpoint_boundary, cp_seq, &options);
                        let wm = item_watermark(&options, cp_seq, cp_seq, checkpoint_boundary);
                        emitted += 1;
                        let message =
                            crate::render::checkpoint_to_response(cp_data, &read_mask)?;
                        yield response_for(wm, message);
                    }
                    Ok(ResolvedWatermarked::Watermark { position: _, cp: raw_cp }) => {
                        // Tx-space → cp-space translation done in the
                        // combinator; here we just clamp past anything
                        // we've already emitted and convert to a
                        // boundary cursor.
                        let cp_frontier = if direction.is_ascending() {
                            Some(raw_cp)
                        } else {
                            raw_cp.checked_add(1)
                        };
                        let Some(cp_frontier) = clamp_cp_frontier_past_last(cp_frontier, checkpoint_boundary, direction) else {
                            continue;
                        };
                        if let Some(c) = frontier_to_boundary_candidate(cp_frontier, &options) {
                            checkpoint_boundary = advance_checkpoint_boundary(checkpoint_boundary, c, &options);
                        }
                        let wm = boundary_watermark(&options, cp_frontier, cp_frontier, checkpoint_boundary);
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
                limit_items,
                ?ordering,
                emitted,
                ?reason,
                elapsed_ms = started.elapsed().as_millis(),
                "list_checkpoints: done (summary only)"
            );
        }
        .boxed());
    }

    // Heavy path: needs transactions and/or objects.
    let cp_full_stream = resolve::resolve_checkpoints(
        client.clone(),
        &read_mask,
        transactions_stage,
        objects_stage,
        cp_data_stream,
    );
    let cp_full_stream = resolve_watermarks(cp_full_stream, client.tx_wm_resolver(direction));

    // Stage E: sync render — build full_checkpoint_content::Checkpoint and
    // merge into the proto Checkpoint (CPU-only, no further IO).
    Ok(async_stream::try_stream! {
        futures::pin_mut!(cp_full_stream);
        let mut emitted = 0usize;
        let mut checkpoint_boundary: Option<u64> = None;
        let mut scan_limit_hit = false;
        while let Some(item) = cp_full_stream.next().await {
            match item {
                Ok(ResolvedWatermarked::Item((cp_seq, cp_data, txs, objects))) => {
                    checkpoint_boundary = advance_checkpoint_boundary(checkpoint_boundary, cp_seq, &options);
                    let wm = item_watermark(&options, cp_seq, cp_seq, checkpoint_boundary);
                    let message =
                        render_full_checkpoint(cp_data, txs, objects.as_ref(), &read_mask)?;
                    emitted += 1;
                    yield response_for(wm, message);
                }
                Ok(ResolvedWatermarked::Watermark { position: _, cp: raw_cp }) => {
                    // See light-path arm — same clamp + boundary logic.
                    let cp_frontier = if direction.is_ascending() {
                        Some(raw_cp)
                    } else {
                        raw_cp.checked_add(1)
                    };
                    let Some(cp_frontier) = clamp_cp_frontier_past_last(cp_frontier, checkpoint_boundary, direction) else {
                        continue;
                    };
                    if let Some(c) = frontier_to_boundary_candidate(cp_frontier, &options) {
                        checkpoint_boundary = advance_checkpoint_boundary(checkpoint_boundary, c, &options);
                    }
                    let wm = boundary_watermark(&options, cp_frontier, cp_frontier, checkpoint_boundary);
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

/// Convert a cp-space scan frontier from `filtered_checkpoint_seq_stream`
/// into a checkpoint-boundary candidate for `advance_checkpoint_boundary`.
///
/// ListCheckpoints dedupes cp_seq, so "cp X emitted" ≡ "cp X complete" and
/// the item path feeds the item's cp straight into `advance_checkpoint_boundary`.
/// The frontier path needs this adjustment instead:
///
/// - Ascending: frontier means "all matching cps strictly less than `p`
///   have been emitted." The last fully-scanned cp is `p - 1`.
/// - Descending: frontier means "all matching cps at least `p` have
///   been emitted." The candidate is `p` directly.
///
/// Returns `None` only when `p == 0` ascending (no preceding cp), in
/// which case the boundary stays at whatever the items have built up.
fn frontier_to_boundary_candidate(frontier: u64, options: &QueryOptions) -> Option<u64> {
    if options.is_ascending() {
        frontier.checked_sub(1)
    } else {
        Some(frontier)
    }
}

fn watermark_response(watermark: Watermark) -> ListCheckpointsResponse {
    let mut response = ListCheckpointsResponse::default();
    response.response = Some(list_checkpoints_response::Response::Watermark(watermark));
    response
}

async fn scan_checkpoint_data(
    client: BigTableClient,
    columns: Arc<[&'static str]>,
    range: std::ops::Range<u64>,
    limit: usize,
    options: &QueryOptions,
) -> Result<BoxStream<'static, Result<Watermarked<(u64, CheckpointData)>, anyhow::Error>>, RpcError>
{
    let column_filter = BigTableClient::column_filter(&columns);
    let rows = client
        .scan_checkpoints_stream(range, options.scan_direction(), limit, Some(column_filter))
        .await?;
    Ok(rows.map_ok(Watermarked::Item).boxed())
}

async fn fetch_checkpoint_data(
    client: BigTableClient,
    columns: Arc<[&'static str]>,
    seqs: Vec<u64>,
) -> Result<BoxStream<'static, Result<(u64, CheckpointData), anyhow::Error>>, anyhow::Error> {
    if seqs.is_empty() {
        return Ok(stream::empty().boxed());
    }
    let column_filter = BigTableClient::column_filter(&columns);
    let keys: Vec<Vec<u8>> = seqs
        .iter()
        .copied()
        .map(tables::checkpoints::encode_key)
        .collect();
    let rows = client
        .multi_get_stream(
            tables::checkpoints::NAME,
            keys,
            Some(column_filter),
            stage::CHECKPOINTS,
        )
        .await?;

    Ok(async_stream::try_stream! {
        let mut emitter: InputOrderEmitter<u64, (u64, CheckpointData)> =
            InputOrderEmitter::new(seqs);
        futures::pin_mut!(rows);
        while let Some(row) = rows.next().await {
            let (key, cells) = row?;
            let seq = decode_checkpoint_row_key(&key)?;
            let checkpoint = tables::checkpoints::decode(&cells)?;
            for v in emitter.push(
                seq,
                (seq, checkpoint),
                "list_checkpoints: checkpoint lookup",
            )? {
                yield v;
            }
        }
        for v in emitter.finish("list_checkpoints: missing selected checkpoint row")? {
            yield v;
        }
    }
    .boxed())
}

fn response_for(watermark: Watermark, message: Checkpoint) -> ListCheckpointsResponse {
    let mut item = CheckpointItem::default();
    item.checkpoint = Some(message);
    item.watermark = Some(watermark);

    let mut response = ListCheckpointsResponse::default();
    response.response = Some(list_checkpoints_response::Response::Item(item));
    response
}

fn end_response(reason: QueryEndReason) -> ListCheckpointsResponse {
    let mut end = QueryEnd::default();
    end.reason = reason as i32;

    let mut response = ListCheckpointsResponse::default();
    response.response = Some(list_checkpoints_response::Response::End(end));
    response
}

/// Filtered cp_seq discovery for `ListCheckpoints`. The tx-bitmap scan and
/// the `tx_seq -> cp_seq` mapping live in the same chunked loop so cp dedup
/// stays local. Tx-space watermarks emitted by the bitmap are
/// translated into cp-space watermarks in-band: each marker arrival
/// flushes the current chunk (so the marker stays ordered AFTER any cps it
/// dominates) and then emits `Watermarked::Watermark(cp_seq)` of its own.
///
/// Returns the cp_seq stream. `BitmapScanLimitExceeded` propagates as `anyhow::Error`
/// through the stream; the parent handler downcasts to detect it.
async fn filtered_checkpoint_seq_stream(
    ctx: &QueryContext,
    filter: &sui_rpc::proto::sui::rpc::v2alpha::TransactionFilter,
    tx_range: std::ops::Range<u64>,
    limit: usize,
    options: QueryOptions,
    budget: u64,
) -> Result<BoxStream<'static, Result<Watermarked<u64>, anyhow::Error>>, RpcError> {
    if limit == 0 || tx_range.is_empty() {
        return Ok(stream::empty().boxed());
    }

    let client = ctx.client();
    let query = ctx.transaction_filter_query(filter)?;
    let chunk_max = ctx.stage(PipelineStage::TxSeqDigest).chunk_size;

    let tx_seq_stream = client.eval_bitmap_query_stream(
        query,
        tx_range,
        BitmapIndexSpec::tx(),
        options.scan_direction(),
        budget,
        ctx.bitmap_scan_observer(),
    );
    let fetch_client: BigTableClient = client.clone();
    let direction = options.scan_direction();

    let stream = async_stream::try_stream! {
        futures::pin_mut!(tx_seq_stream);
        let mut tx_seq_chunk: Vec<u64> = Vec::with_capacity(chunk_max);
        let mut last_cp_seq: Option<u64> = None;
        let mut emitted = 0usize;

        loop {
            // Read until we have a full chunk of tx_seq Items, OR a Frontier
            // marker arrives (forcing flush), OR the upstream ends.
            let mut pending_watermark: Option<u64> = None;
            while tx_seq_chunk.len() < chunk_max && pending_watermark.is_none() {
                match tx_seq_stream.try_next().await? {
                    Some(Watermarked::Item(tx_seq)) => tx_seq_chunk.push(tx_seq),
                    Some(Watermarked::Watermark(p)) => pending_watermark = Some(p),
                    None => break,
                }
            }

            // Resolve items' cps via one multi_get; dedupe by cp_seq.
            // The WM (if any) stays in tx-space — it passes through to
            // the tail `coalesce_watermarks`, which may drop it as
            // item-superseded. Surviving WMs do their own one-row
            // lookup in the handler at emit time. This decouples the
            // small WM lookup from the bulky item multi_get, mirroring
            // the structure in list_transactions / list_events.
            if !tx_seq_chunk.is_empty() {
                let mut tx_checkpoints = fetch_client.resolve_tx_checkpoints(&tx_seq_chunk).await?;
                if direction.is_ascending() {
                    tx_checkpoints.sort_by_key(|(tx_seq, _)| *tx_seq);
                } else {
                    tx_checkpoints.sort_by_key(|(tx_seq, _)| std::cmp::Reverse(*tx_seq));
                }
                tx_seq_chunk.clear();
                for (_, cp_seq) in tx_checkpoints {
                    if last_cp_seq == Some(cp_seq) {
                        continue;
                    }
                    last_cp_seq = Some(cp_seq);
                    emitted += 1;
                    yield Watermarked::Item(cp_seq);
                    if emitted >= limit {
                        return;
                    }
                }
            }

            // Pass the WM through untranslated (tx-space position).
            // Downstream `coalesce_watermarks` drops it if items
            // follow; otherwise the handler resolves it to cp and
            // applies the clamp at emit time.
            if let Some(tx_frontier) = pending_watermark {
                yield Watermarked::Watermark(tx_frontier);
                continue;
            }

            // Upstream ended.
            break;
        }
    }
    .boxed();

    Ok(stream)
}

/// Determine the checkpoint_sequence_number scan window from the logical
/// checkpoint bounds, indexed history, and cursor bounds. Filtered scans
/// are additionally bounded at runtime by the per-request bitmap bucket
/// budget; that limit surfaces as SCAN_LIMIT, not as an up-front cp-range
/// clamp.
fn resolve_cp_range(checkpoint_range: CheckpointRange, options: &QueryOptions) -> ResolvedRange {
    let cp_range = checkpoint_range.resolve(options);
    let range = cp_range.range.clone();
    options.apply_cursor_bounds(cp_range.with_range(range, options.ordering))
}

fn decode_checkpoint_row_key(key: &Bytes) -> Result<u64, RpcError> {
    let bytes: [u8; 8] = key
        .as_ref()
        .try_into()
        .map_err(|_| RpcError::new(tonic::Code::Internal, "invalid checkpoint row key length"))?;
    Ok(u64::from_be_bytes(bytes))
}

/// Clamp a translated cp frontier so the emitted Boundary cursor never
/// causes the client to re-request a checkpoint already delivered as an
/// Item. `ListCheckpoints` dedupes items, so additional matching txs
/// inside an already-delivered cp cannot produce a second item — it is
/// safe (and required) to advance past the last delivered cp even when
/// the tx→cp translation lands on it.
///
///   Ascending:  Item(C) resumes at cp ≥ C+1, so the boundary must be ≥ C+1.
///   Descending: Item(C) resumes at cp < C+1 (i.e. ≤ C), so the boundary
///               must be ≤ C; emitting at C is equivalent to the item's
///               own resume (harmless but redundant).
fn clamp_cp_frontier_past_last(
    cp_frontier: Option<u64>,
    last_cp_seq: Option<u64>,
    direction: ScanDirection,
) -> Option<u64> {
    match (cp_frontier, last_cp_seq) {
        (Some(frontier), Some(last)) if direction.is_ascending() => {
            Some(frontier.max(last.saturating_add(1)))
        }
        (Some(frontier), Some(last)) => Some(frontier.min(last)),
        (Some(frontier), None) => Some(frontier),
        (None, _) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_passes_through_when_no_item_emitted() {
        assert_eq!(
            clamp_cp_frontier_past_last(Some(10), None, ScanDirection::Ascending),
            Some(10),
        );
        assert_eq!(
            clamp_cp_frontier_past_last(Some(10), None, ScanDirection::Descending),
            Some(10),
        );
    }

    #[test]
    fn clamp_returns_none_when_translation_failed() {
        assert_eq!(
            clamp_cp_frontier_past_last(None, None, ScanDirection::Ascending),
            None,
        );
        assert_eq!(
            clamp_cp_frontier_past_last(None, Some(10), ScanDirection::Ascending),
            None,
        );
        assert_eq!(
            clamp_cp_frontier_past_last(None, Some(10), ScanDirection::Descending),
            None,
        );
    }

    #[test]
    fn clamp_ascending_advances_past_last_emitted_cp() {
        // Frontier behind the last emitted cp — clamp up to last+1 so we
        // don't re-request a cp already delivered.
        assert_eq!(
            clamp_cp_frontier_past_last(Some(5), Some(10), ScanDirection::Ascending),
            Some(11),
        );
        // Frontier exactly at the last emitted cp — same regression
        // shape: clamp to last+1.
        assert_eq!(
            clamp_cp_frontier_past_last(Some(10), Some(10), ScanDirection::Ascending),
            Some(11),
        );
        // Frontier already past last — pass through unchanged.
        assert_eq!(
            clamp_cp_frontier_past_last(Some(20), Some(10), ScanDirection::Ascending),
            Some(20),
        );
    }

    #[test]
    fn clamp_descending_advances_past_last_emitted_cp() {
        // Descending: "advanced" means smaller cp. Frontier > last is
        // behind — clamp down to last (equivalent to the item's own
        // resume in descending).
        assert_eq!(
            clamp_cp_frontier_past_last(Some(20), Some(10), ScanDirection::Descending),
            Some(10),
        );
        // Frontier at last is the boundary case.
        assert_eq!(
            clamp_cp_frontier_past_last(Some(10), Some(10), ScanDirection::Descending),
            Some(10),
        );
        // Frontier already past last (smaller) — pass through.
        assert_eq!(
            clamp_cp_frontier_past_last(Some(5), Some(10), ScanDirection::Descending),
            Some(5),
        );
    }

    #[test]
    fn clamp_ascending_saturates_at_u64_max() {
        // last = u64::MAX means there is no cp past it; saturating_add
        // stays at MAX so we don't wrap into 0 and re-request from the
        // start.
        assert_eq!(
            clamp_cp_frontier_past_last(Some(5), Some(u64::MAX), ScanDirection::Ascending),
            Some(u64::MAX),
        );
    }
}
