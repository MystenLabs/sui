// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream;
use futures::stream::BoxStream;
use sui_inverted_index::ScanDirection;
use sui_inverted_index::ScanStop;
use sui_kvstore::BitmapIndexSpec;
use sui_kvstore::CheckpointData;
use sui_kvstore::tables;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::Checkpoint;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEnd;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_rpc_api::ledger_history::query_options::CheckpointRange;
use sui_rpc_api::ledger_history::query_options::QueryOptions;
use sui_rpc_api::ledger_history::query_options::ResolvedRange;
use sui_rpc_api::ledger_history::watermark::boundary_watermark;
use sui_rpc_api::ledger_history::watermark::item_watermark;
use sui_rpc_api::ledger_history::watermark::merge_covered_checkpoint_bound;
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
use crate::pipeline::ResolvedScanStop;
use crate::pipeline::ResolvedWatermarked;
use crate::pipeline::Watermarked;
use crate::pipeline::dedup_consecutive;
use crate::pipeline::pipelined_chunks;
use crate::pipeline::resolve_scan_watermarks;
use crate::pipeline::take_items;
use crate::render::render_full_checkpoint;
use crate::resolve;
use crate::resolve::list_checkpoint_columns;
use crate::resolve::needs_transactions_or_objects;

const READ_MASK_DEFAULT: &str = sui_rpc_api::read_mask_defaults::CHECKPOINT;

pub(crate) type ListCheckpointsStream =
    BoxStream<'static, Result<ListCheckpointsResponse, RpcError>>;

enum CheckpointListItem {
    Summary(u64, CheckpointData),
    Full(resolve::ResolvedCp),
}

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
    let range_exhaustion_reason = cp_range.end_reason;
    let range_end_position = cp_range.end_position;
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
        let terminal_watermark = reached_range_end(range_exhaustion_reason).then(|| {
            terminal_boundary_watermark(
                &options,
                Position::Checkpoints {
                    checkpoint: range_end_position,
                },
            )
        });
        return Ok(stream::iter([Ok(end_response(
            terminal_watermark,
            range_exhaustion_reason,
        ))])
        .boxed());
    }

    let needs_full = needs_transactions_or_objects(&read_mask);
    let cp_columns: Arc<[&'static str]> = list_checkpoint_columns(&read_mask, needs_full).into();

    // Stage A: discover checkpoint rows for the requested response. Filtered
    // requests use sparse bitmap-eval over transactions and then fetch the
    // deduped checkpoint rows. Unfiltered requests scan the dense checkpoint
    // keyspace directly, bounded by limit_items.
    let cp_data_stream: BoxStream<'static, Result<Watermarked<(u64, CheckpointData)>, ScanStop>> =
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
                                .map(|s| s.map_err(ScanStop::Fault).boxed())
                                .map_err(ScanStop::Fault)
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
                                .map(|s| s.map_err(ScanStop::Fault).boxed())
                                .map_err(ScanStop::Fault)
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

    let checkpoint_stream: BoxStream<'static, Result<Watermarked<CheckpointListItem>, ScanStop>> =
        if needs_full {
            // Heavy path: needs transactions and/or objects.
            resolve::resolve_checkpoints(
                client.clone(),
                &read_mask,
                transactions_stage,
                objects_stage,
                cp_data_stream,
            )
            .map_ok(|marked| marked.map_item(CheckpointListItem::Full))
            .boxed()
        } else {
            // Fast path: render directly from CheckpointData without transaction or
            // object lookups.
            cp_data_stream
                .map_ok(|marked| {
                    marked.map_item(|(seq, data)| CheckpointListItem::Summary(seq, data))
                })
                .boxed()
        };
    let checkpoint_stream = resolve_scan_watermarks(
        checkpoint_stream,
        client.tx_wm_resolver(direction),
        std::convert::identity,
    );

    // Stage E: sync render — build the requested Checkpoint shape (CPU-only,
    // with no further IO).
    Ok(async_stream::try_stream! {
        futures::pin_mut!(checkpoint_stream);
        let mut emitted = 0usize;
        let mut covered_checkpoint_bound: Option<u64> = None;
        let mut latest_emitted_watermark: Option<Watermark> = None;
        let terminal_reason = loop {
            let Some(item) = checkpoint_stream.next().await else {
                let terminal_watermark_candidate =
                    reached_range_end(range_exhaustion_reason).then(|| {
                        terminal_boundary_watermark(
                            &options,
                            Position::Checkpoints {
                                checkpoint: range_end_position,
                            },
                        )
                    });
                let terminal_watermark = terminal_watermark_candidate
                    .filter(|candidate| latest_emitted_watermark.as_ref() != Some(candidate));
                yield end_response(terminal_watermark, range_exhaustion_reason);
                break range_exhaustion_reason;
            };
            match item {
                Ok(ResolvedWatermarked::Item(item)) => {
                    let item_checkpoint = match &item {
                        CheckpointListItem::Summary(item_checkpoint, _) => *item_checkpoint,
                        CheckpointListItem::Full((item_checkpoint, _, _, _)) => *item_checkpoint,
                    };
                    // Checkpoint items are emitted in scan order and deduped, so
                    // emitting this item proves its checkpoint fully covered.
                    covered_checkpoint_bound = Some(item_checkpoint);
                    let watermark = item_watermark(
                        Position::Checkpoints {
                            checkpoint: item_checkpoint,
                        },
                        covered_checkpoint_bound,
                    );
                    latest_emitted_watermark = Some(watermark.clone());
                    let message = match item {
                        CheckpointListItem::Summary(_, cp_data) => {
                            crate::render::checkpoint_to_response(cp_data, &read_mask)?
                        }
                        CheckpointListItem::Full((_, cp_data, txs, objects)) => {
                            render_full_checkpoint(cp_data, txs, objects, &read_mask)?
                        }
                    };
                    emitted += 1;
                    let mut response = response_for(watermark, message);
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
                    position: _,
                    cp: checkpoint_at_frontier,
                }) => {
                    let Some(watermark) = checkpoint_frontier_watermark(
                        checkpoint_at_frontier,
                        direction,
                        &options,
                        &mut covered_checkpoint_bound,
                    ) else {
                        continue;
                    };
                    latest_emitted_watermark = Some(watermark.clone());
                    yield watermark_response(watermark);
                }
                Err(ResolvedScanStop::ScanLimit {
                    position: _,
                    checkpoint: checkpoint_at_frontier,
                }) => {
                    let terminal_watermark_candidate =
                        checkpoint_at_frontier.and_then(|checkpoint_at_frontier| {
                            checkpoint_frontier_watermark(
                                checkpoint_at_frontier,
                                direction,
                                &options,
                                &mut covered_checkpoint_bound,
                            )
                        });
                    let terminal_watermark = terminal_watermark_candidate
                        .filter(|candidate| latest_emitted_watermark.as_ref() != Some(candidate));
                    yield end_response(terminal_watermark, QueryEndReason::ScanLimit);
                    break QueryEndReason::ScanLimit;
                }
                Err(ResolvedScanStop::Cancelled) => {
                    Err(RpcError::new(
                        tonic::Code::Cancelled,
                        ScanStop::Cancelled.to_string(),
                    ))?;
                }
                Err(ResolvedScanStop::Fault(inner)) => {
                    Err(RpcError::from(inner))?;
                }
            }
        };
        info!(
            filtered,
            limit_items,
            ?ordering,
            emitted,
            ?terminal_reason,
            elapsed_ms = started.elapsed().as_millis(),
            "list_checkpoints: done"
        );
    }
    .boxed())
}

/// Convert a cp-space scan frontier from `filtered_checkpoint_seq_stream`
/// into a fully covered checkpoint candidate for
/// `merge_covered_checkpoint_bound`.
///
/// ListCheckpoints item handling directly assigns its ordered, deduped
/// checkpoint number. The independently resolved frontier path needs this
/// adjustment instead:
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

fn checkpoint_frontier_watermark(
    checkpoint_at_frontier: u64,
    direction: ScanDirection,
    options: &QueryOptions,
    covered_checkpoint_bound: &mut Option<u64>,
) -> Option<Watermark> {
    // Tx-space → cp-space translation is already done by the resolver; here we
    // clamp past anything we've already emitted and convert to a boundary
    // cursor.
    let resume_checkpoint = if direction.is_ascending() {
        Some(checkpoint_at_frontier)
    } else {
        checkpoint_at_frontier.checked_add(1)
    };
    let resume_checkpoint =
        clamp_cp_frontier_past_last(resume_checkpoint, *covered_checkpoint_bound, direction)?;
    if let Some(covered_bound_candidate) =
        frontier_to_boundary_candidate(resume_checkpoint, options)
    {
        *covered_checkpoint_bound = merge_covered_checkpoint_bound(
            *covered_checkpoint_bound,
            covered_bound_candidate,
            options,
        );
    }
    Some(boundary_watermark(
        Position::Checkpoints {
            checkpoint: resume_checkpoint,
        },
        *covered_checkpoint_bound,
    ))
}

fn watermark_response(watermark: Watermark) -> ListCheckpointsResponse {
    let mut response = ListCheckpointsResponse::default();
    response.watermark = Some(watermark);
    response
}

async fn scan_checkpoint_data(
    client: BigTableClient,
    columns: Arc<[&'static str]>,
    range: std::ops::Range<u64>,
    limit: usize,
    options: &QueryOptions,
) -> Result<BoxStream<'static, Result<Watermarked<(u64, CheckpointData)>, ScanStop>>, RpcError> {
    let column_filter = BigTableClient::column_filter(&columns);
    let rows = client
        .scan_checkpoints_stream(range, options.scan_direction(), limit, Some(column_filter))
        .await?;
    Ok(rows
        .map_ok(Watermarked::Item)
        .map_err(ScanStop::Fault)
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
    let mut response = ListCheckpointsResponse::default();
    response.checkpoint = Some(message);
    response.watermark = Some(watermark);
    response
}

fn end_response(watermark: Option<Watermark>, reason: QueryEndReason) -> ListCheckpointsResponse {
    let mut end = QueryEnd::default();
    end.reason = Some(reason as i32);

    let mut response = ListCheckpointsResponse::default();
    response.watermark = watermark;
    response.end = Some(end);
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
    use sui_rpc_cursor::CursorToken;

    use crate::v2alpha::test_utils::ascending_options;
    use crate::v2alpha::test_utils::query_context;

    #[tokio::test]
    async fn natural_empty_range_emits_one_standalone_terminal_boundary() {
        let (ctx, server) = query_context("test_list_checkpoints_natural_end", 10).await;
        let mut request = ListCheckpointsRequest::default();
        request.start_checkpoint = Some(0);
        request.end_checkpoint = Some(0);
        request.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
        request.options = Some(ascending_options());

        let responses: Vec<_> = list_checkpoints(ctx, request)
            .await
            .expect("construct checkpoint stream")
            .try_collect()
            .await
            .expect("collect checkpoint stream");
        server.abort();

        assert_eq!(responses.len(), 1, "empty range has one terminal frame");
        let response = &responses[0];
        assert!(
            response.checkpoint.is_none(),
            "terminal frame has no payload"
        );
        assert_eq!(
            response.end.as_ref().and_then(|end| end.reason),
            Some(QueryEndReason::CheckpointBound as i32),
        );
        let watermark = response
            .watermark
            .as_ref()
            .expect("natural completion proves a terminal boundary");
        let expected_cursor =
            CursorToken::boundary(Position::Checkpoints { checkpoint: 0 }).encode();
        assert_eq!(watermark.cursor.as_ref(), Some(&expected_cursor));
        assert_eq!(watermark.checkpoint, None);
    }

    #[tokio::test]
    async fn cursor_bounded_empty_range_emits_end_without_false_boundary() {
        let (ctx, server) = query_context("test_list_checkpoints_cursor_end", 10).await;
        let mut options = ascending_options();
        options.before =
            Some(CursorToken::boundary(Position::Checkpoints { checkpoint: 0 }).encode());
        let mut request = ListCheckpointsRequest::default();
        request.start_checkpoint = Some(0);
        request.end_checkpoint = Some(10);
        request.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
        request.options = Some(options);

        let responses: Vec<_> = list_checkpoints(ctx, request)
            .await
            .expect("construct checkpoint stream")
            .try_collect()
            .await
            .expect("collect checkpoint stream");
        server.abort();

        assert_eq!(responses.len(), 1, "empty range has one terminal frame");
        let response = &responses[0];
        assert!(
            response.checkpoint.is_none(),
            "terminal frame has no payload"
        );
        assert!(
            response.watermark.is_none(),
            "cursor truncation must not claim natural range completion"
        );
        assert_eq!(
            response.end.as_ref().and_then(|end| end.reason),
            Some(QueryEndReason::CursorBound as i32),
        );
    }

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
