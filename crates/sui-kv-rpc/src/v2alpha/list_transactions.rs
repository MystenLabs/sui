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
use sui_kvstore::BitmapIndexSpec;
use sui_kvstore::TransactionData;
use sui_kvstore::TxSeqDigestData;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEnd;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc_api::RpcError;
use sui_rpc_api::ledger_history::query_options::CheckpointRange;
use sui_rpc_api::ledger_history::query_options::QueryOptions;
use sui_rpc_api::ledger_history::query_options::ResolvedRange;
use sui_rpc_api::ledger_history::watermark::BoundaryTerminal;
use sui_rpc_api::ledger_history::watermark::advance_covered_bound_before_checkpoint;
use sui_rpc_api::ledger_history::watermark::boundary_watermark;
use sui_rpc_api::ledger_history::watermark::item_watermark;
use sui_rpc_api::ledger_history::watermark::scan_frontier_cursor_cp;
use sui_rpc_api::ledger_history::watermark::terminal_watermark;
use sui_rpc_cursor::Position;
use sui_types::digests::TransactionDigest;
use tracing::Instrument;
use tracing::debug_span;
use tracing::info;

use crate::bigtable_client::BigTableClient;
use crate::config::PipelineStage;
use crate::object_cache::ObjectMap;
use crate::operation::QueryContext;
use crate::pipeline::InputOrderEmitter;
use crate::pipeline::ResolvedScanStop;
use crate::pipeline::ResolvedWatermarked;
use crate::pipeline::Watermarked;
use crate::pipeline::pipelined_chunks;
use crate::pipeline::resolve_scan_watermarks;
use crate::pipeline::take_items;
use crate::render::transaction_to_response;
use crate::resolve;
use crate::resolve::compute_object_keys;
use crate::resolve::needs_object_types;
use crate::resolve::transaction_columns;
use crate::v2::get_transaction::validate_read_mask;

pub(crate) type ListTransactionsStream =
    BoxStream<'static, Result<ListTransactionsResponse, RpcError>>;
type TransactionWithObjectsStreamItem = (u64, u32, Box<TransactionData>, ObjectMap);
enum TransactionListItem {
    Digest(TxSeqDigestData),
    Full(TransactionWithObjectsStreamItem),
}

pub(crate) async fn list_transactions(
    ctx: QueryContext,
    request: ListTransactionsRequest,
) -> Result<ListTransactionsStream, RpcError> {
    let started = Instant::now();
    let filtered = request.filter.is_some();
    let client: BigTableClient = ctx.client().clone();
    let resolver: crate::PackageResolver = ctx.package_resolver().clone();
    let checkpoint_hi_exclusive = ctx.checkpoint_hi_exclusive();
    let lh = ctx.ledger_history();
    let endpoint = lh.list_transactions();
    let tx_seq_digest_stage = ctx.stage(PipelineStage::TxSeqDigest);
    let transactions_stage = ctx.stage(PipelineStage::Transactions);
    let objects_stage = ctx.stage(PipelineStage::Objects);

    let checkpoint_range = CheckpointRange::from_request(
        request.start_checkpoint,
        request.end_checkpoint,
        checkpoint_hi_exclusive,
    )?;
    let read_mask = validate_read_mask(request.read_mask)?;
    let options = QueryOptions::transactions_from_proto(
        request.options.as_ref(),
        endpoint.default_limit_items,
        endpoint.max_limit_items,
    )?;
    let limit_items = options.limit_items;
    let ordering = options.ordering;
    let direction = options.scan_direction();

    let tx_range = resolve_tx_range(&client, checkpoint_range, &options)
        .instrument(debug_span!("resolve_tx_range"))
        .await?;
    let range_exhaustion_reason = tx_range.end_reason;
    let range_end_checkpoint = tx_range.end_checkpoint;
    let range_end_position = tx_range.end_position;
    let tx_range = tx_range.range;

    if tx_range.is_empty() {
        info!(
            filtered,
            limit_items,
            ?ordering,
            elapsed_ms = started.elapsed().as_millis(),
            "list_transactions: empty range"
        );
        // Empty resolved ranges still surface their terminal cursor. Natural
        // completion may claim the final checkpoint; a cursor bound may not.
        let terminal_watermark = terminal_watermark(
            &options,
            BoundaryTerminal::new(
                range_exhaustion_reason,
                Position::Transactions {
                    checkpoint: range_end_checkpoint,
                    tx_seq: range_end_position,
                },
                None,
            ),
            None,
        );
        return Ok(futures::stream::iter([Ok(end_response(
            terminal_watermark,
            range_exhaustion_reason,
        ))])
        .boxed());
    }

    // Stage 1: discover tx_seq_digest rows for the requested response.
    // Filtered requests are sparse bitmap hits and still use chunked
    // multi_get lookups. Unfiltered requests scan the dense tx_seq_digest
    // keyspace directly, bounded by limit_items.
    let digest_stream: BoxStream<'static, Result<Watermarked<TxSeqDigestData>, ScanStop>> =
        if let Some(filter) = &request.filter {
            let scan_budget = ctx.scan_budget(BitmapIndexSpec::tx());
            let query = ctx.transaction_filter_query(filter)?;
            let seq_stream = client.eval_bitmap_query_stream(
                query,
                tx_range.clone(),
                BitmapIndexSpec::tx(),
                options.scan_direction(),
                scan_budget,
                ctx.bitmap_scan_observer(),
            );
            let seq_stream = take_items(seq_stream, limit_items);
            pipelined_chunks(
                seq_stream,
                tx_seq_digest_stage.chunk_size,
                tx_seq_digest_stage.concurrency,
                {
                    let client = client.clone();
                    move |seqs| {
                        let client = client.clone();
                        async move {
                            fetch_tx_seq_digests(client, seqs)
                                .await
                                .map(|s| s.map_err(ScanStop::Fault).boxed())
                                .map_err(ScanStop::Fault)
                        }
                    }
                },
            )
        } else {
            scan_tx_seq_digests(client.clone(), tx_range.clone(), limit_items, &options).await?
        };

    let render_transaction_contents = should_render_transaction_contents(&read_mask);
    let transaction_stream: BoxStream<'static, Result<Watermarked<TransactionListItem>, ScanStop>> =
        if render_transaction_contents {
            let columns: Arc<[&'static str]> = transaction_columns(&read_mask).into();
            let needs_objects = needs_object_types(&read_mask);

            // Stage 3: Watermarked<TxSeqDigestData> ->
            // Watermarked<(tx_seq, TransactionData)>.
            let tx_stream = pipelined_chunks(
                digest_stream,
                transactions_stage.chunk_size,
                transactions_stage.concurrency,
                {
                    let client = client.clone();
                    let columns = columns.clone();
                    move |rows| {
                        let client = client.clone();
                        let columns = columns.clone();
                        async move {
                            fetch_transactions(client, columns, rows)
                                .await
                                .map(|s| s.map_err(ScanStop::Fault).boxed())
                                .map_err(ScanStop::Fault)
                        }
                    }
                },
            );

            // Stage 4: + ObjectMap. Object refs are precomputed per Item; Frontier
            // watermarks pass through pipelined_keyed_batches unchanged.
            resolve::with_object_maps(
                tx_stream,
                client.clone(),
                objects_stage,
                needs_objects,
                |(_, _, tx): &(u64, u32, TransactionData)| {
                    compute_object_keys(tx).into_iter().collect()
                },
            )
            .map_ok(|marked| {
                marked.map_item(|((seq, offset, tx), objects)| {
                    TransactionListItem::Full((seq, offset, Box::new(tx), objects))
                })
            })
            .boxed()
        } else {
            digest_stream
                .map_ok(|marked| marked.map_item(TransactionListItem::Digest))
                .boxed()
        };
    let transaction_stream = resolve_scan_watermarks(
        transaction_stream,
        client.tx_wm_resolver(direction),
        std::convert::identity,
    );

    Ok(async_stream::try_stream! {
        futures::pin_mut!(transaction_stream);
        let mut emitted = 0usize;
        let mut covered_checkpoint_bound: Option<u64> = None;
        let terminal_reason = loop {
            let Some(item) = transaction_stream.next().await else {
                let terminal_watermark = terminal_watermark(
                    &options,
                    BoundaryTerminal::new(
                        range_exhaustion_reason,
                        Position::Transactions {
                            checkpoint: range_end_checkpoint,
                            tx_seq: range_end_position,
                        },
                        None,
                    ),
                    covered_checkpoint_bound,
                );
                yield end_response(terminal_watermark, range_exhaustion_reason);
                break range_exhaustion_reason;
            };
            match item {
                Ok(ResolvedWatermarked::Item(item)) => {
                    let (tx_seq, item_checkpoint) = match &item {
                        TransactionListItem::Digest(row) => {
                            (row.tx_sequence_number, row.checkpoint_number)
                        }
                        TransactionListItem::Full((tx_seq, _, tx_data, _)) => {
                            (*tx_seq, tx_data.checkpoint_number)
                        }
                    };
                    covered_checkpoint_bound = advance_covered_bound_before_checkpoint(
                        covered_checkpoint_bound,
                        item_checkpoint,
                        &options,
                    );
                    let watermark = item_watermark(
                        Position::Transactions {
                            checkpoint: item_checkpoint,
                            tx_seq,
                        },
                        covered_checkpoint_bound,
                    );
                    let mut response = match item {
                        TransactionListItem::Digest(row) => {
                            transaction_response_from_tx_seq_digest(row, &read_mask, watermark)
                        }
                        TransactionListItem::Full((_, tx_offset, tx_data, objects)) => {
                            let render_started = Instant::now();
                            let executed =
                                transaction_to_response(*tx_data, &read_mask, &objects, &resolver)
                                    .await?;
                            ctx.observe_response_render(render_started.elapsed());
                            transaction_item_response(
                                watermark,
                                executed,
                                tx_offset,
                                &read_mask,
                            )
                        }
                    };
                    emitted += 1;
                    let yield_started = Instant::now();
                    if emitted == limit_items {
                        let mut end = QueryEnd::default();
                        end.reason = Some(QueryEndReason::ItemLimit as i32);
                        response.end = Some(end);
                        yield response;
                        ctx.observe_stream_item_yield_wait(yield_started.elapsed());
                        break QueryEndReason::ItemLimit;
                    }
                    yield response;
                    ctx.observe_stream_item_yield_wait(yield_started.elapsed());
                }
                Ok(ResolvedWatermarked::Watermark {
                    position,
                    cp: checkpoint_at_frontier,
                }) => {
                    let watermark = transaction_frontier_watermark(
                        &options,
                        direction,
                        &mut covered_checkpoint_bound,
                        position,
                        Some(checkpoint_at_frontier),
                    )?;
                    yield watermark_response(watermark);
                }
                Err(ResolvedScanStop::ScanLimit {
                    position,
                    checkpoint: checkpoint_at_frontier,
                }) => {
                    let scan_frontier_watermark = transaction_frontier_watermark(
                        &options,
                        direction,
                        &mut covered_checkpoint_bound,
                        position,
                        checkpoint_at_frontier,
                    )?;
                    yield end_response(scan_frontier_watermark, QueryEndReason::ScanLimit);
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
            "list_transactions: done"
        );
    }
    .boxed())
}

/// Wrap a constructed `Watermark` as a progress-only wire frame.
fn watermark_response(watermark: Watermark) -> ListTransactionsResponse {
    let mut response = ListTransactionsResponse::default();
    response.watermark = Some(watermark);
    response
}

fn transaction_frontier_watermark(
    options: &QueryOptions,
    direction: ScanDirection,
    covered_checkpoint_bound: &mut Option<u64>,
    position: u64,
    checkpoint_at_frontier: Option<u64>,
) -> Result<Watermark, RpcError> {
    if let Some(checkpoint) = checkpoint_at_frontier {
        *covered_checkpoint_bound =
            advance_covered_bound_before_checkpoint(*covered_checkpoint_bound, checkpoint, options);
    }
    let cursor_checkpoint = scan_frontier_cursor_cp(checkpoint_at_frontier, position, direction)
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                format!("transaction scan frontier {position} has no checkpoint mapping"),
            )
        })?;
    Ok(boundary_watermark(
        Position::Transactions {
            checkpoint: cursor_checkpoint,
            tx_seq: position,
        },
        *covered_checkpoint_bound,
    ))
}

async fn scan_tx_seq_digests(
    client: BigTableClient,
    range: std::ops::Range<u64>,
    limit: usize,
    options: &QueryOptions,
) -> Result<BoxStream<'static, Result<Watermarked<TxSeqDigestData>, ScanStop>>, RpcError> {
    let rows = client
        .scan_tx_seq_digests_stream(range, options.scan_direction(), limit)
        .await?;
    Ok(rows
        .map_ok(Watermarked::Item)
        .map_err(ScanStop::Fault)
        .boxed())
}

async fn fetch_tx_seq_digests(
    client: BigTableClient,
    seqs: Vec<u64>,
) -> Result<BoxStream<'static, Result<TxSeqDigestData, anyhow::Error>>, anyhow::Error> {
    if seqs.is_empty() {
        return Ok(futures::stream::empty().boxed());
    }
    // The permit lives inside the stream returned by `BigTableClient::
    // resolve_tx_digests_stream` and drops with that stream — propagated
    // through to the stream we return below.
    let digest_stream = client.resolve_tx_digests_stream(seqs.clone()).await?;
    Ok(async_stream::try_stream! {
        let mut emitter: InputOrderEmitter<u64, TxSeqDigestData> =
            InputOrderEmitter::new(seqs);
        futures::pin_mut!(digest_stream);
        while let Some(row) = digest_stream.next().await {
            let row = row?;
            for v in emitter.push(
                row.tx_sequence_number,
                row,
                "list_transactions: transaction digest lookup",
            )? {
                yield v;
            }
        }
        for v in emitter.finish("list_transactions: missing selected transaction digest")? {
            yield v;
        }
    }
    .boxed())
}

async fn fetch_transactions(
    client: BigTableClient,
    columns: Arc<[&'static str]>,
    rows: Vec<TxSeqDigestData>,
) -> Result<BoxStream<'static, Result<(u64, u32, TransactionData), anyhow::Error>>, anyhow::Error> {
    if rows.is_empty() {
        return Ok(futures::stream::empty().boxed());
    }
    let column_filter = BigTableClient::column_filter(&columns);
    let digests: Vec<TransactionDigest> = rows.iter().map(|row| row.digest).collect();
    // Map each digest back to its (tx_sequence_number, within-checkpoint offset)
    // so the output can carry them alongside the tx body as it arrives.
    let meta_by_digest: HashMap<TransactionDigest, (u64, u32)> = rows
        .iter()
        .map(|row| (row.digest, (row.tx_sequence_number, row.tx_offset)))
        .collect();
    let tx_stream = client
        .get_transactions_stream(digests.clone(), Some(column_filter))
        .await?;
    Ok(async_stream::try_stream! {
        let mut emitter: InputOrderEmitter<TransactionDigest, (u64, u32, TransactionData)> =
            InputOrderEmitter::new(digests);
        futures::pin_mut!(tx_stream);
        while let Some(row) = tx_stream.next().await {
            let (digest, tx) = row?;
            let (seq, offset) = meta_by_digest.get(&digest).copied().ok_or_else(|| {
                anyhow::anyhow!("list_transactions: unexpected transaction body row {digest}")
            })?;
            for v in emitter.push(
                digest,
                (seq, offset, tx),
                "list_transactions: transaction body lookup",
            )? {
                yield v;
            }
        }
        for v in emitter.finish("list_transactions: missing selected transaction body")? {
            yield v;
        }
    }
    .boxed())
}

fn should_render_transaction_contents(read_mask: &FieldMaskTree) -> bool {
    // `digest`, `checkpoint`, and `transaction_index` are all available from the
    // tx_seq_digest index row, so a mask limited to them skips the full
    // transaction fetch.
    let paths = read_mask.to_field_mask().paths;
    paths.is_empty()
        || paths.len() > 3
        || paths.iter().any(|path| {
            path != ExecutedTransaction::DIGEST_FIELD.name
                && path != ExecutedTransaction::CHECKPOINT_FIELD.name
                && path != ExecutedTransaction::TRANSACTION_INDEX_FIELD.name
        })
}

fn transaction_response_from_tx_seq_digest(
    row: TxSeqDigestData,
    read_mask: &FieldMaskTree,
    watermark: Watermark,
) -> ListTransactionsResponse {
    let mut transaction = ExecutedTransaction::default();
    if read_mask.contains(ExecutedTransaction::DIGEST_FIELD.name) {
        transaction.digest = Some(row.digest.to_string());
    }
    if read_mask.contains(ExecutedTransaction::CHECKPOINT_FIELD.name) {
        transaction.checkpoint = Some(row.checkpoint_number);
    }

    transaction_item_response(watermark, transaction, row.tx_offset, read_mask)
}

/// Determine the tx_sequence_number scan window from the logical checkpoint
/// bounds. The checkpoint window is already clamped to indexed history and
/// any cursor bounds before this converts it into tx sequence space.
/// Filtered scans are additionally bounded at runtime by the per-request
/// bitmap bucket budget; that limit surfaces as SCAN_LIMIT, not as an
/// up-front cp-range clamp.
async fn resolve_tx_range(
    client: &BigTableClient,
    checkpoint_range: CheckpointRange,
    options: &QueryOptions,
) -> Result<ResolvedRange, RpcError> {
    let cp_range = checkpoint_range.resolve(options);
    if cp_range.is_empty() {
        let tx_boundary =
            checkpoint_to_tx_boundary(client, cp_range.terminal_checkpoint(options.ordering))
                .await?;
        return Ok(cp_range.with_range(tx_boundary..tx_boundary, options.ordering));
    }

    let start_fut = {
        let client = client.clone();
        let start_cp = cp_range.range.start;
        async move {
            if start_cp == 0 {
                return Ok(0);
            }
            Ok(client
                .checkpoint_to_tx_range(start_cp..start_cp + 1)
                .await?
                .start)
        }
    };

    let end_fut = {
        let client = client.clone();
        let end_cp = cp_range.range.end;
        async move { Ok::<u64, RpcError>(client.checkpoint_to_tx_range(0..end_cp).await?.end) }
    };

    let (start_tx, end_tx) = tokio::try_join!(start_fut, end_fut)?;
    Ok(options.apply_cursor_bounds(cp_range.with_range(start_tx..end_tx, options.ordering)))
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

fn transaction_item_response(
    watermark: Watermark,
    mut transaction: ExecutedTransaction,
    tx_offset: u32,
    read_mask: &FieldMaskTree,
) -> ListTransactionsResponse {
    // The within-checkpoint position rides on the `ExecutedTransaction` rather
    // than the response frame; populate it only when the read mask requests it.
    if read_mask.contains(ExecutedTransaction::TRANSACTION_INDEX_FIELD.name) {
        transaction.transaction_index = Some(tx_offset as u64);
    }

    let mut response = ListTransactionsResponse::default();
    response.transaction = Some(transaction);
    response.watermark = Some(watermark);
    response
}

fn end_response(watermark: Watermark, reason: QueryEndReason) -> ListTransactionsResponse {
    let mut end = QueryEnd::default();
    end.reason = Some(reason as i32);

    let mut response = ListTransactionsResponse::default();
    response.watermark = Some(watermark);
    response.end = Some(end);
    response
}

#[cfg(test)]
mod tests {
    use sui_rpc::field::FieldMask;
    use sui_rpc::field::FieldMaskUtil;
    use sui_types::digests::TransactionDigest;

    use super::*;
    use sui_rpc_cursor::CursorToken;

    use crate::v2alpha::test_utils::ascending_options;
    use crate::v2alpha::test_utils::query_context;

    #[tokio::test]
    async fn empty_ledger_tip_emits_one_standalone_transaction_boundary() {
        let (ctx, server) = query_context("test_list_transactions_natural_end", 0).await;
        let mut request = ListTransactionsRequest::default();
        request.read_mask = Some(FieldMask::from_paths(["digest"]));
        request.options = Some(ascending_options());

        let responses: Vec<_> = list_transactions(ctx, request)
            .await
            .expect("construct transaction stream")
            .try_collect()
            .await
            .expect("collect transaction stream");
        server.abort();

        assert_eq!(responses.len(), 1, "empty ledger has one terminal frame");
        let response = &responses[0];
        assert!(
            response.transaction.is_none(),
            "terminal frame has no payload"
        );
        assert_eq!(
            response.end.as_ref().and_then(|end| end.reason),
            Some(QueryEndReason::LedgerTip as i32),
        );
        let watermark = response
            .watermark
            .as_ref()
            .expect("ledger exhaustion proves a terminal boundary");
        let expected_cursor = CursorToken::boundary(Position::Transactions {
            checkpoint: 0,
            tx_seq: 0,
        })
        .encode();
        assert_eq!(watermark.cursor.as_ref(), Some(&expected_cursor));
        assert_eq!(watermark.checkpoint, None);
    }

    fn read_mask(paths: &[&str]) -> FieldMaskTree {
        validate_read_mask(Some(FieldMask::from_paths(paths.iter().copied()))).unwrap()
    }

    fn unwrap_item(response: ListTransactionsResponse) -> ListTransactionsResponse {
        assert!(response.end.is_none(), "expected item frame");
        assert!(response.transaction.is_some(), "expected item frame");
        response
    }

    #[test]
    fn renders_transaction_response_from_tx_seq_digest() {
        let row = TxSeqDigestData {
            tx_sequence_number: 42,
            digest: TransactionDigest::new([7; 32]),
            event_count: 3,
            tx_offset: 5,
            checkpoint_number: 9,
        };
        let wm = || {
            item_watermark(
                Position::Transactions {
                    checkpoint: row.checkpoint_number,
                    tx_seq: row.tx_sequence_number,
                },
                Some(8),
            )
        };

        let digest_only = unwrap_item(transaction_response_from_tx_seq_digest(
            row,
            &read_mask(&["digest"]),
            wm(),
        ));
        let digest_wm = digest_only.watermark.as_ref().expect("watermark");
        assert_eq!(
            digest_wm.cursor.as_ref(),
            Some(
                &sui_rpc_cursor::CursorToken::item(Position::Transactions {
                    checkpoint: row.checkpoint_number,
                    tx_seq: 42,
                })
                .encode()
            )
        );
        assert_eq!(digest_wm.checkpoint, Some(8));
        let transaction = digest_only.transaction.expect("executed transaction");
        assert_eq!(
            transaction.transaction_index, None,
            "transaction_index must be omitted when the read mask does not request it"
        );
        assert_eq!(transaction.digest, Some(row.digest.to_string()));
        assert_eq!(transaction.checkpoint, None);

        let with_index = unwrap_item(transaction_response_from_tx_seq_digest(
            row,
            &read_mask(&["digest", "transaction_index"]),
            wm(),
        ));
        let transaction = with_index.transaction.expect("executed transaction");
        assert_eq!(
            transaction.transaction_index,
            Some(row.tx_offset as u64),
            "within-checkpoint offset should propagate when requested"
        );
        assert_eq!(transaction.digest, Some(row.digest.to_string()));

        let checkpoint_only = unwrap_item(transaction_response_from_tx_seq_digest(
            row,
            &read_mask(&["checkpoint"]),
            wm(),
        ));
        let transaction = checkpoint_only.transaction.expect("executed transaction");
        assert_eq!(transaction.digest, None);
        assert_eq!(transaction.checkpoint, Some(9));

        let both = unwrap_item(transaction_response_from_tx_seq_digest(
            row,
            &read_mask(&["digest", "checkpoint"]),
            wm(),
        ));
        let transaction = both.transaction.expect("executed transaction");
        assert_eq!(transaction.digest, Some(row.digest.to_string()));
        assert_eq!(transaction.checkpoint, Some(9));
    }
}
