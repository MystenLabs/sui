// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream::BoxStream;
use sui_inverted_index::BitmapScanLimitExceeded;
use sui_inverted_index::error_contains;
use sui_kvstore::BitmapIndexSpec;
use sui_kvstore::TransactionData;
use sui_kvstore::TxSeqDigestData;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEnd;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionItem;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc::proto::sui::rpc::v2alpha::list_transactions_response;
use sui_rpc_api::RpcError;
use sui_rpc_api::ledger_history::query_options::CheckpointRange;
use sui_rpc_api::ledger_history::query_options::QueryOptions;
use sui_rpc_api::ledger_history::query_options::ResolvedRange;
use sui_rpc_api::ledger_history::watermark::advance_boundary_excluding_cp;
use sui_rpc_api::ledger_history::watermark::boundary_cursor_cp;
use sui_rpc_api::ledger_history::watermark::boundary_watermark;
use sui_rpc_api::ledger_history::watermark::item_watermark;
use sui_rpc_api::ledger_history::watermark::reached_range_end;
use sui_rpc_api::ledger_history::watermark::terminal_boundary_watermark;
use sui_rpc_cursor::QueryType;
use sui_types::digests::TransactionDigest;
use tracing::Instrument;
use tracing::debug_span;
use tracing::info;

use crate::bigtable_client::BigTableClient;
use crate::config::PipelineStage;
use crate::object_cache::ObjectMap;
use crate::operation::QueryContext;
use crate::pipeline::InputOrderEmitter;
use crate::pipeline::ResolvedWatermarked;
use crate::pipeline::Watermarked;
use crate::pipeline::pipelined_chunks;
use crate::pipeline::resolve_watermarks;
use crate::pipeline::take_items;
use crate::render::transaction_to_response;
use crate::resolve;
use crate::resolve::compute_object_keys;
use crate::resolve::needs_object_types;
use crate::resolve::transaction_columns;
use crate::v2::get_transaction::validate_read_mask;

pub(crate) type ListTransactionsStream =
    BoxStream<'static, Result<ListTransactionsResponse, RpcError>>;
type TransactionWithObjectsStreamItem = (u64, u32, TransactionData, ObjectMap);

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
    let options = QueryOptions::from_proto(
        request.options.as_ref(),
        endpoint.default_limit_items,
        endpoint.max_limit_items,
        QueryType::Transactions,
    )?;
    let limit_items = options.limit_items;
    let ordering = options.ordering;
    let direction = options.scan_direction();

    let tx_range = resolve_tx_range(&client, checkpoint_range, &options)
        .instrument(debug_span!("resolve_tx_range"))
        .await?;
    let end_reason = tx_range.end_reason;
    let end_checkpoint = tx_range.end_checkpoint;
    let end_position = tx_range.end_position;
    let tx_range = tx_range.range;

    if tx_range.is_empty() {
        info!(
            filtered,
            limit_items,
            ?ordering,
            elapsed_ms = started.elapsed().as_millis(),
            "list_transactions: empty range"
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

    // Stage 1: discover tx_seq_digest rows for the requested response.
    // Filtered requests are sparse bitmap hits and still use chunked
    // multi_get lookups. Unfiltered requests scan the dense tx_seq_digest
    // keyspace directly, bounded by limit_items.
    let digest_stream: BoxStream<'static, Result<Watermarked<TxSeqDigestData>, anyhow::Error>> =
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
                    move |seqs| fetch_tx_seq_digests(client.clone(), seqs)
                },
            )
        } else {
            scan_tx_seq_digests(client.clone(), tx_range.clone(), limit_items, &options).await?
        };

    let render_transaction_contents = should_render_transaction_contents(&read_mask);
    if !render_transaction_contents {
        let digest_stream = resolve_watermarks(digest_stream, client.tx_wm_resolver(direction));
        return Ok(async_stream::try_stream! {
            futures::pin_mut!(digest_stream);
            let mut emitted = 0usize;
            let mut checkpoint_boundary: Option<u64> = None;
            let mut scan_limit_hit = false;
            while let Some(item) = digest_stream.next().await {
                match item {
                    Ok(ResolvedWatermarked::Item(row)) => {
                        checkpoint_boundary = advance_boundary_excluding_cp(checkpoint_boundary, row.checkpoint_number, &options);
                        let wm = item_watermark(&options, row.checkpoint_number, row.tx_sequence_number, checkpoint_boundary);
                        emitted += 1;
                        let yield_started = Instant::now();
                        yield transaction_response_from_tx_seq_digest(row, &read_mask, wm);
                        ctx.observe_stream_item_yield_wait(yield_started.elapsed());
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
                limit_items,
                ?ordering,
                emitted,
                ?reason,
                elapsed_ms = started.elapsed().as_millis(),
                "list_transactions: done (digest only)"
            );
        }
        .boxed());
    }

    let columns: Arc<[&'static str]> = transaction_columns(&read_mask).into();
    let needs_objects = needs_object_types(&read_mask);

    // Stage 3: Watermarked<TxSeqDigestData> -> Watermarked<(tx_seq, TransactionData)>.
    let tx_stream = pipelined_chunks(
        digest_stream,
        transactions_stage.chunk_size,
        transactions_stage.concurrency,
        {
            let client = client.clone();
            let columns = columns.clone();
            move |rows| fetch_transactions(client.clone(), columns.clone(), rows)
        },
    );

    // Stage 4: + ObjectMap. Object refs are precomputed per Item; Frontier
    // watermarks pass through pipelined_keyed_batches unchanged.
    let txn_with_objects_stream: BoxStream<
        'static,
        Result<Watermarked<TransactionWithObjectsStreamItem>, anyhow::Error>,
    > = resolve::with_object_maps(
        tx_stream,
        client.clone(),
        objects_stage,
        needs_objects,
        |(_, _, tx): &(u64, u32, TransactionData)| compute_object_keys(tx).into_iter().collect(),
    )
    .map_ok(|m| m.map_item(|((seq, offset, tx), objects)| (seq, offset, tx, objects)))
    .boxed();

    let txn_with_objects_stream =
        resolve_watermarks(txn_with_objects_stream, client.tx_wm_resolver(direction));

    Ok(async_stream::try_stream! {
        futures::pin_mut!(txn_with_objects_stream);

        let mut emitted = 0usize;
        let mut checkpoint_boundary: Option<u64> = None;
        let mut scan_limit_hit = false;
        while let Some(item) = txn_with_objects_stream.next().await {
            match item {
                Ok(ResolvedWatermarked::Item((tx_seq, tx_offset, tx_data, objects))) => {
                    checkpoint_boundary = advance_boundary_excluding_cp(checkpoint_boundary, tx_data.checkpoint_number, &options);
                    let wm = item_watermark(&options, tx_data.checkpoint_number, tx_seq, checkpoint_boundary);
                    let render_started = Instant::now();
                    let executed = transaction_to_response(tx_data, &read_mask, &objects, &resolver).await?;
                    ctx.observe_response_render(render_started.elapsed());
                    emitted += 1;
                    let yield_started = Instant::now();
                    yield transaction_item_response(wm, executed, tx_offset);
                    ctx.observe_stream_item_yield_wait(yield_started.elapsed());
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
            limit_items,
            ?ordering,
            emitted,
            ?reason,
            elapsed_ms = started.elapsed().as_millis(),
            "list_transactions: done"
        );
    }
    .boxed())
}

/// Wrap a constructed `Watermark` as a standalone wire frame.
fn watermark_response(watermark: Watermark) -> ListTransactionsResponse {
    let mut response = ListTransactionsResponse::default();
    response.response = Some(list_transactions_response::Response::Watermark(watermark));
    response
}

async fn scan_tx_seq_digests(
    client: BigTableClient,
    range: std::ops::Range<u64>,
    limit: usize,
    options: &QueryOptions,
) -> Result<BoxStream<'static, Result<Watermarked<TxSeqDigestData>, anyhow::Error>>, RpcError> {
    let rows = client
        .scan_tx_seq_digests_stream(range, options.scan_direction(), limit)
        .await?;
    Ok(rows.map_ok(Watermarked::Item).boxed())
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
    let paths = read_mask.to_field_mask().paths;
    paths.is_empty()
        || paths.len() > 2
        || paths.iter().any(|path| {
            path != ExecutedTransaction::DIGEST_FIELD.name
                && path != ExecutedTransaction::CHECKPOINT_FIELD.name
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

    transaction_item_response(watermark, transaction, row.tx_offset)
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
    transaction: ExecutedTransaction,
    tx_offset: u32,
) -> ListTransactionsResponse {
    let mut item = TransactionItem::default();
    item.transaction = Some(transaction);
    item.watermark = Some(watermark);
    item.transaction_offset = Some(tx_offset as u64);

    let mut response = ListTransactionsResponse::default();
    response.response = Some(list_transactions_response::Response::Item(item));
    response
}

fn end_response(reason: QueryEndReason) -> ListTransactionsResponse {
    let mut end = QueryEnd::default();
    end.reason = reason as i32;

    let mut response = ListTransactionsResponse::default();
    response.response = Some(list_transactions_response::Response::End(end));
    response
}

#[cfg(test)]
mod tests {
    use sui_rpc::field::FieldMask;
    use sui_rpc::field::FieldMaskUtil;
    use sui_types::digests::TransactionDigest;

    use super::*;

    fn read_mask(paths: &[&str]) -> FieldMaskTree {
        validate_read_mask(Some(FieldMask::from_paths(paths.iter().copied()))).unwrap()
    }

    fn unwrap_item(response: ListTransactionsResponse) -> TransactionItem {
        match response.response.expect("response frame") {
            list_transactions_response::Response::Item(item) => item,
            list_transactions_response::Response::End(_) => panic!("expected item frame"),
            _ => panic!("expected item frame"),
        }
    }

    fn options() -> QueryOptions {
        QueryOptions::from_proto(None, 100, 1_000, QueryType::Transactions).unwrap()
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
        let options = options();
        let wm = || {
            item_watermark(
                &options,
                row.checkpoint_number,
                row.tx_sequence_number,
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
            Some(&options.cursor_for_item(row.checkpoint_number, 42))
        );
        assert_eq!(digest_wm.checkpoint_hi, Some(8));
        assert_eq!(
            digest_wm.checkpoint_lo, None,
            "ascending scan must not set checkpoint_lo"
        );
        assert_eq!(
            digest_only.transaction_offset,
            Some(row.tx_offset as u64),
            "within-checkpoint offset should propagate to the item"
        );
        let transaction = digest_only.transaction.expect("executed transaction");
        assert_eq!(transaction.digest, Some(row.digest.to_string()));
        assert_eq!(transaction.checkpoint, None);

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
