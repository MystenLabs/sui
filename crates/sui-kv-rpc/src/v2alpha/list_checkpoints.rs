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
use sui_rpc_api::ledger_history::watermark::CheckpointBoundary;
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
    let ascending = options.is_ascending();

    let cp_range = async { Ok::<_, RpcError>(resolve_cp_range(checkpoint_range, &options)) }
        .instrument(debug_span!("resolve_cp_range"))
        .await?;
    let end_reason = cp_range.end_reason;
    let terminal_watermark = terminal_boundary_watermark(
        Position::Checkpoints {
            checkpoint: cp_range.end_position,
        },
        ascending,
    );
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
        let terminal =
            reached_range_end(end_reason).then(|| watermark_response(terminal_watermark));
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
            let mut checkpoint_boundary = CheckpointBoundary::new(ascending);
            let mut scan_limit_hit = false;
            while let Some(item) = cp_data_stream.next().await {
                match item {
                    Ok(ResolvedWatermarked::Item((cp_seq, cp_data))) => {
                        let wm = checkpoint_boundary.item_watermark_covered(Position::Checkpoints { checkpoint: cp_seq });
                        emitted += 1;
                        let message =
                            crate::render::checkpoint_to_response(cp_data, &read_mask)?;
                        yield response_for(wm, message);
                    }
                    Ok(ResolvedWatermarked::Watermark { position: _, cp: raw_cp }) => {
                        // Tx-space → cp-space translation done in the
                        // combinator; the boundary clamps the resume cursor
                        // past already-delivered checkpoints itself.
                        if let Some(wm) = checkpoint_boundary.cp_frontier_watermark(raw_cp) {
                            yield watermark_response(wm);
                        }
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
        let mut checkpoint_boundary = CheckpointBoundary::new(ascending);
        let mut scan_limit_hit = false;
        while let Some(item) = cp_full_stream.next().await {
            match item {
                Ok(ResolvedWatermarked::Item((cp_seq, cp_data, txs, objects))) => {
                    let wm = checkpoint_boundary.item_watermark_covered(Position::Checkpoints { checkpoint: cp_seq });
                    let message =
                        render_full_checkpoint(cp_data, txs, objects, &read_mask)?;
                    emitted += 1;
                    yield response_for(wm, message);
                }
                Ok(ResolvedWatermarked::Watermark { position: _, cp: raw_cp }) => {
                    // See light-path arm.
                    if let Some(wm) = checkpoint_boundary.cp_frontier_watermark(raw_cp) {
                        yield watermark_response(wm);
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use sui_rpc_cursor::CursorToken;

    /// Drives `CheckpointBoundary::cp_frontier_watermark`, the consolidated
    /// form of the stream arms' pre-consolidation standalone-frontier
    /// composition (tx→cp cursor translation → clamp past delivered items →
    /// boundary candidate → fold → frame). The frames asserted below were
    /// pinned against that composition.
    fn frontier_arm(boundary: &mut CheckpointBoundary, raw_cp: u64) -> Option<Watermark> {
        boundary.cp_frontier_watermark(raw_cp)
    }

    #[track_caller]
    fn assert_frame(wm: &Watermark, cursor_cp: u64, bound: Option<u64>) {
        assert_eq!(
            wm.cursor.as_ref(),
            Some(
                &CursorToken::boundary(Position::Checkpoints {
                    checkpoint: cursor_cp,
                })
                .encode()
            )
        );
        assert_eq!(wm.checkpoint, bound);
    }

    /// Pins the wire frames of the standalone cp-frontier watermark for every
    /// direction/clamp/edge combination the stream arms can produce.
    #[test]
    fn frontier_arm_frames_pinned() {
        // Ascending, nothing delivered: cursor at the frontier cp, prior cp covered.
        let mut b = CheckpointBoundary::new(true);
        let wm = frontier_arm(&mut b, 10).unwrap();
        assert_frame(&wm, 10, Some(9));

        // Ascending, frontier inside the delivered cp: cursor clamped past it,
        // claim holds at the delivered cp.
        let mut b = CheckpointBoundary::new(true);
        b.checkpoint_covered(10);
        let wm = frontier_arm(&mut b, 10).unwrap();
        assert_frame(&wm, 11, Some(10));

        // Ascending, frontier behind the delivered cp: same clamp shape.
        let mut b = CheckpointBoundary::new(true);
        b.checkpoint_covered(10);
        let wm = frontier_arm(&mut b, 5).unwrap();
        assert_frame(&wm, 11, Some(10));

        // Ascending, frontier past the delivered cp: no clamp.
        let mut b = CheckpointBoundary::new(true);
        b.checkpoint_covered(10);
        let wm = frontier_arm(&mut b, 12).unwrap();
        assert_frame(&wm, 12, Some(11));

        // Ascending at genesis: nothing covered yet.
        let mut b = CheckpointBoundary::new(true);
        let wm = frontier_arm(&mut b, 0).unwrap();
        assert_frame(&wm, 0, None);

        // Ascending saturation: delivered cp u64::MAX has no cp past it.
        let mut b = CheckpointBoundary::new(true);
        b.checkpoint_covered(u64::MAX);
        let wm = frontier_arm(&mut b, 5).unwrap();
        assert_frame(&wm, u64::MAX, Some(u64::MAX));

        // Descending, nothing delivered: exclusive-upper cursor keeps the
        // frontier cp included on resume.
        let mut b = CheckpointBoundary::new(false);
        let wm = frontier_arm(&mut b, 10).unwrap();
        assert_frame(&wm, 11, Some(11));

        // Descending, frontier at the delivered cp: clamped to the item's own
        // resume (harmless but redundant).
        let mut b = CheckpointBoundary::new(false);
        b.checkpoint_covered(10);
        let wm = frontier_arm(&mut b, 10).unwrap();
        assert_frame(&wm, 10, Some(10));

        // Descending, frontier behind (above) the delivered cp: clamped down.
        let mut b = CheckpointBoundary::new(false);
        b.checkpoint_covered(10);
        let wm = frontier_arm(&mut b, 20).unwrap();
        assert_frame(&wm, 10, Some(10));

        // Descending, frontier past (below) the delivered cp: no clamp.
        let mut b = CheckpointBoundary::new(false);
        b.checkpoint_covered(10);
        let wm = frontier_arm(&mut b, 5).unwrap();
        assert_frame(&wm, 6, Some(6));

        // Descending overflow edge: no resume coordinate exists, no frame.
        let mut b = CheckpointBoundary::new(false);
        assert!(frontier_arm(&mut b, u64::MAX).is_none());
    }
}
