// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream;
use futures::stream::BoxStream;
use sui_inverted_index::BitmapScanError;
use sui_inverted_index::BitmapScanResult;
use sui_inverted_index::ScanDirection;
use sui_kvstore::BitmapIndexSpec;
use sui_kvstore::CheckpointData;
use sui_kvstore::tables;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::Checkpoint;
use sui_rpc::proto::sui::rpc::v2alpha::CheckpointItem;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEnd;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc::proto::sui::rpc::v2alpha::list_checkpoints_response;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_rpc_api::ledger_history::query_options::CheckpointRange;
use sui_rpc_api::ledger_history::query_options::QueryOptions;
use sui_rpc_api::ledger_history::query_options::ResolvedRange;
use sui_rpc_api::ledger_history::watermark::advance_checkpoint_boundary;
use sui_rpc_api::ledger_history::watermark::boundary_watermark;
use sui_rpc_api::ledger_history::watermark::item_watermark;
use sui_rpc_api::ledger_history::watermark::reached_range_end;
use sui_rpc_api::ledger_history::watermark::terminal_boundary_watermark;
use sui_rpc_api::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc_cursor::Position;
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
use crate::pipeline::dedup_consecutive;
use crate::pipeline::pipelined_chunks;
use crate::pipeline::resolve_watermarks;
use crate::pipeline::take_items;
use crate::render::render_full_checkpoint;
use crate::resolve;
use crate::resolve::list_checkpoint_columns;
use crate::resolve::needs_transactions_or_objects;

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
    let tx_seq_digest_stage = ctx.stage(PipelineStage::TxSeqDigest);

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
    let options = QueryOptions::checkpoints_from_proto(
        request.options.as_ref(),
        endpoint.default_limit_items,
        endpoint.max_limit_items,
    )?;
    let limit_items = options.limit_items;
    let ordering = options.ordering;
    let direction = options.scan_direction();

    let cp_range = async { Ok::<_, RpcError>(resolve_cp_range(checkpoint_range, &options)) }
        .instrument(debug_span!("resolve_cp_range"))
        .await?;
    let end_reason = cp_range.end_reason;
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
                Position::Checkpoints {
                    checkpoint: end_position,
                },
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
    let cp_data_stream: BoxStream<'static, BitmapScanResult<Watermarked<(u64, CheckpointData)>>> =
        if let Some(filter) = &request.filter {
            let scan_budget = ctx.scan_budget(BitmapIndexSpec::tx());
            let tx_range = client.checkpoint_to_tx_range(cp_range.clone()).await?;
            let query = ctx.transaction_filter_query(filter)?;
            let tx_seq_stream = client.eval_bitmap_query_stream(
                query,
                tx_range,
                BitmapIndexSpec::tx(),
                direction,
                scan_budget,
                ctx.bitmap_scan_observer(),
            );
            // Stage A2: resolve tx_seq -> cp_seq, streaming each cp as soon as the
            // scan-order prefix is contiguous, then collapse each checkpoint's
            // (contiguous) transactions to a single cp_seq. Dedup is its own stage
            // rather than a per-chunk mapper because it carries state across chunk
            // boundaries.
            let cp_seq_stream = pipelined_chunks(
                tx_seq_stream,
                tx_seq_digest_stage.chunk_size,
                tx_seq_digest_stage.concurrency,
                {
                    let client = client.clone();
                    move |tx_seqs| {
                        let client = client.clone();
                        async move {
                            resolve_checkpoint_seqs(client, tx_seqs)
                                .await
                                .map(|s| s.map_err(BitmapScanError::Source).boxed())
                                .map_err(BitmapScanError::Source)
                        }
                    }
                },
            );
            let cp_seq_stream = take_items(dedup_consecutive(cp_seq_stream), limit_items);
            // Stage A3: fetch checkpoint rows for the deduped cp_seqs.
            pipelined_chunks(
                cp_seq_stream,
                checkpoints_stage.chunk_size,
                checkpoints_stage.concurrency,
                {
                    let client = client.clone();
                    let columns = cp_columns.clone();
                    move |seqs| {
                        let client = client.clone();
                        let columns = columns.clone();
                        async move {
                            fetch_checkpoint_data(client, columns, seqs)
                                .await
                                .map(|s| s.map_err(BitmapScanError::Source).boxed())
                                .map_err(BitmapScanError::Source)
                        }
                    }
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
                        let wm = item_watermark(Position::Checkpoints { checkpoint: cp_seq }, checkpoint_boundary);
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
                        let wm = boundary_watermark(Position::Checkpoints { checkpoint: cp_frontier }, checkpoint_boundary);
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
                yield watermark_response(terminal_boundary_watermark(&options, Position::Checkpoints { checkpoint: end_position }));
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
                    let wm = item_watermark(Position::Checkpoints { checkpoint: cp_seq }, checkpoint_boundary);
                    let message =
                        render_full_checkpoint(cp_data, txs, objects, &read_mask)?;
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
                    let wm = boundary_watermark(Position::Checkpoints { checkpoint: cp_frontier }, checkpoint_boundary);
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
            yield watermark_response(terminal_boundary_watermark(&options, Position::Checkpoints { checkpoint: end_position }));
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
) -> Result<BoxStream<'static, BitmapScanResult<Watermarked<(u64, CheckpointData)>>>, RpcError> {
    let column_filter = BigTableClient::column_filter(&columns);
    let rows = client
        .scan_checkpoints_stream(range, options.scan_direction(), limit, Some(column_filter))
        .await?;
    Ok(rows
        .map_ok(Watermarked::Item)
        .map_err(BitmapScanError::Source)
        .boxed())
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

/// Resolve a chunk of `tx_sequence_number`s (already in scan order) to their
/// checkpoint sequence numbers, streaming each cp_seq as soon as the scan-order
/// prefix is contiguously available rather than buffering the whole batch.
/// BigTable multi-get rows arrive unordered, so `InputOrderEmitter` (keyed by
/// the input scan order) releases the contiguous front as rows fill in — the
/// same ordering trick as `fetch_checkpoint_data`. Consecutive duplicates (one
/// checkpoint's multiple transactions) are collapsed downstream by
/// `dedup_consecutive`.
async fn resolve_checkpoint_seqs(
    client: BigTableClient,
    tx_seqs: Vec<u64>,
) -> Result<BoxStream<'static, Result<u64, anyhow::Error>>, anyhow::Error> {
    if tx_seqs.is_empty() {
        return Ok(stream::empty().boxed());
    }
    let rows = client
        .resolve_tx_checkpoints_stream(tx_seqs.clone())
        .await?;
    Ok(async_stream::try_stream! {
        let mut emitter: InputOrderEmitter<u64, u64> = InputOrderEmitter::new(tx_seqs);
        futures::pin_mut!(rows);
        while let Some(row) = rows.next().await {
            let (tx_seq, cp_seq) = row?;
            for cp in emitter.push(tx_seq, cp_seq, "list_checkpoints: tx -> checkpoint resolution")? {
                yield cp;
            }
        }
        for cp in emitter.finish("list_checkpoints: missing tx -> checkpoint row")? {
            yield cp;
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
    end.reason = Some(reason as i32);

    let mut response = ListCheckpointsResponse::default();
    response.response = Some(list_checkpoints_response::Response::End(end));
    response
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
