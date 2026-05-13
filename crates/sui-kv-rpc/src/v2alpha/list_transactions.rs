// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream::BoxStream;
use sui_kvstore::BitmapIndexSpec;
use sui_kvstore::TransactionData;
use sui_kvstore::TxSeqDigestData;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc_api::RpcError;
use sui_types::digests::TransactionDigest;
use sui_types::storage::ObjectKey;
use tracing::Instrument;
use tracing::debug_span;
use tracing::info;

use crate::bigtable_client::BigTableClient;
use crate::object_cache::BigTableObjectFetcher;
use crate::object_cache::ObjectCache;
use crate::object_cache::ObjectMap;
use crate::operation::QueryContext;
use crate::pipeline::InputOrderEmitter;
use crate::pipeline::pipelined_chunks;
use crate::pipeline::pipelined_keyed_batches;
use crate::query_options::CheckpointRange;
use crate::query_options::QueryOptions;
use crate::query_options::QueryType;
use crate::query_options::ResolvedRange;
use crate::v2::get_transaction::compute_object_keys;
use crate::v2::get_transaction::needs_object_types;
use crate::v2::get_transaction::transaction_columns;
use crate::v2::get_transaction::transaction_to_response;
use crate::v2::get_transaction::validate_read_mask;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEnd;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionItem;
use sui_rpc::proto::sui::rpc::v2alpha::list_transactions_response;

const DEFAULT_LIMIT_ITEMS: u32 = 50;
const MAX_LIMIT_ITEMS: u32 = 500;
const CHUNK_MAX: usize = 100;

pub(crate) type ListTransactionsStream =
    BoxStream<'static, Result<ListTransactionsResponse, RpcError>>;
type TransactionWithObjectsStreamItem = (u64, TransactionData, ObjectMap);

pub(crate) async fn list_transactions(
    ctx: QueryContext,
    request: ListTransactionsRequest,
) -> Result<ListTransactionsStream, RpcError> {
    let started = Instant::now();
    let filtered = request.filter.is_some();
    let client: BigTableClient = ctx.client().clone();
    let resolver: crate::PackageResolver = ctx.package_resolver().clone();
    let checkpoint_hi_exclusive = ctx.checkpoint_hi_exclusive();

    let checkpoint_range = CheckpointRange::from_request(
        request.start_checkpoint,
        request.end_checkpoint,
        checkpoint_hi_exclusive,
    )?;
    let read_mask = validate_read_mask(request.read_mask)?;
    let options = QueryOptions::from_proto(
        request.options.as_ref(),
        DEFAULT_LIMIT_ITEMS,
        MAX_LIMIT_ITEMS,
        QueryType::Transactions,
        request.filter.as_ref(),
    )?;
    let limit_items = options.limit_items;
    let ordering = options.ordering;

    let tx_range = resolve_tx_range(&client, checkpoint_range, &options)
        .instrument(debug_span!("resolve_tx_range"))
        .await?;
    let end_reason = tx_range.end_reason;
    let end_cursor = tx_range.end_cursor(&options);
    let tx_range = tx_range.range;

    if tx_range.is_empty() {
        info!(
            filtered,
            limit_items,
            ?ordering,
            elapsed_ms = started.elapsed().as_millis(),
            "list_transactions: empty range"
        );
        return Ok(
            futures::stream::once(async move { Ok(end_response(end_reason, end_cursor)) }).boxed(),
        );
    }

    // Stage 1: discover tx_seq values for the requested response.
    let seq_stream: BoxStream<'static, Result<u64, RpcError>> =
        if let Some(filter) = &request.filter {
            let query = ctx.transaction_filter_query(filter)?;
            client
                .eval_bitmap_query_stream(
                    query,
                    tx_range.clone(),
                    BitmapIndexSpec::tx(),
                    options.scan_direction(),
                )
                .map_err(RpcError::from)
                .boxed()
        } else {
            range_stream(tx_range.clone(), &options)
        };
    let seq_stream = seq_stream.take(limit_items).boxed();

    let request_bigtable_concurrency = ctx.request_bigtable_concurrency();

    // Stage 2: tx_seq -> tx_seq_digest rows. The pipeline drains each
    // chunk's BigTable stream before emitting rows to the next stage.
    let digest_stream = pipelined_chunks(seq_stream, CHUNK_MAX, request_bigtable_concurrency, {
        let client = client.clone();
        move |seqs| fetch_tx_seq_digests(client.clone(), seqs)
    });

    let render_transaction_contents = should_render_transaction_contents(&read_mask);
    if !render_transaction_contents {
        return Ok(async_stream::try_stream! {
            futures::pin_mut!(digest_stream);
            let mut emitted = 0usize;
            let mut last_cursor = None;
            while let Some(row) = digest_stream.try_next().await? {
                let cursor = options.cursor_for_item(row.checkpoint_number, row.tx_sequence_number);
                emitted += 1;
                last_cursor = Some(cursor.clone());
                let yield_started = Instant::now();
                yield transaction_response_from_tx_seq_digest(row, &read_mask, cursor);
                ctx.observe_stream_item_yield_wait(yield_started.elapsed());
            }
            let (reason, cursor) = query_end(emitted, limit_items, last_cursor, end_reason, end_cursor);
            yield end_response(reason, cursor);
            info!(
                filtered,
                limit_items,
                ?ordering,
                emitted,
                elapsed_ms = started.elapsed().as_millis(),
                "list_transactions: done (digest only)"
            );
        }
        .boxed());
    }

    let columns: Arc<[&'static str]> = transaction_columns(&read_mask).into();
    let needs_objects = needs_object_types(&read_mask);

    // Stage 3: tx_seq_digest rows -> (tx_seq, TransactionData). Rows are
    // ordered within each drained chunk.
    let tx_stream = pipelined_chunks(digest_stream, CHUNK_MAX, request_bigtable_concurrency, {
        let client = client.clone();
        let columns = columns.clone();
        move |rows| fetch_transactions(client.clone(), columns.clone(), rows)
    });

    // Stage 4: (tx_seq, TransactionData) -> (tx_seq, TransactionData, ObjectMap).
    // Object refs are precomputed per tx, then `pipelined_keyed_batches`
    // packs consecutive txs into batches whose deduped key union fits within
    // CHUNK_MAX (see comment on the constant — it serves both as upstream
    // chunk size and as the object-key request budget). Each packed batch
    // is one BigTable multiget; first-row latency is bounded by one fetch.
    let txn_with_objects_stream: BoxStream<
        'static,
        Result<TransactionWithObjectsStreamItem, RpcError>,
    > = if needs_objects {
        let object_cache = ObjectCache::new(Arc::new(BigTableObjectFetcher::new(client.clone())));
        let tx_with_keys = tx_stream
            .map_ok(|(seq, tx)| {
                let keys: Vec<ObjectKey> = compute_object_keys(&tx).into_iter().collect();
                ((seq, tx), keys)
            })
            .boxed();

        pipelined_keyed_batches(
            tx_with_keys,
            CHUNK_MAX,
            CHUNK_MAX,
            request_bigtable_concurrency,
            move |keys| {
                let object_cache = object_cache.clone();
                async move { object_cache.get_many(keys).await }
            },
        )
        .map_ok(|((seq, tx), objects)| (seq, tx, objects))
        .boxed()
    } else {
        // No object lookup needed — emit each tx with an empty ObjectMap;
        // preserves input order trivially via the upstream stream.
        tx_stream
            .map_ok(|(seq, tx)| (seq, tx, Arc::new(HashMap::new()) as ObjectMap))
            .boxed()
    };

    Ok(async_stream::try_stream! {
        futures::pin_mut!(txn_with_objects_stream);

        let mut emitted = 0usize;
        let mut last_cursor = None;
        while let Some((tx_seq, tx_data, objects)) = txn_with_objects_stream.try_next().await? {
            let cursor = options.cursor_for_item(tx_data.checkpoint_number, tx_seq);
            let render_started = Instant::now();
            let executed = transaction_to_response(tx_data, &read_mask, &objects, &resolver).await?;
            ctx.observe_response_render(render_started.elapsed());
            emitted += 1;
            last_cursor = Some(cursor.clone());
            let yield_started = Instant::now();
            yield transaction_item_response(cursor, executed);
            ctx.observe_stream_item_yield_wait(yield_started.elapsed());
        }
        let (reason, cursor) = query_end(emitted, limit_items, last_cursor, end_reason, end_cursor);
        yield end_response(reason, cursor);

        info!(
            filtered,
            limit_items,
            ?ordering,
            emitted,
            elapsed_ms = started.elapsed().as_millis(),
            "list_transactions: done"
        );
    }
    .boxed())
}

async fn fetch_tx_seq_digests(
    client: BigTableClient,
    seqs: Vec<u64>,
) -> Result<BoxStream<'static, Result<TxSeqDigestData, RpcError>>, RpcError> {
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
            let row = row.map_err(RpcError::from)?;
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
) -> Result<BoxStream<'static, Result<(u64, TransactionData), RpcError>>, RpcError> {
    if rows.is_empty() {
        return Ok(futures::stream::empty().boxed());
    }
    let column_filter = BigTableClient::column_filter(&columns);
    let digests: Vec<TransactionDigest> = rows.iter().map(|row| row.digest).collect();
    // Map each digest back to its tx_sequence_number so the output can carry
    // (seq, tx) pairs as the BigTable response arrives.
    let seq_by_digest: HashMap<TransactionDigest, u64> = rows
        .iter()
        .map(|row| (row.digest, row.tx_sequence_number))
        .collect();
    let tx_stream = client
        .get_transactions_stream(digests.clone(), Some(column_filter))
        .await?;
    Ok(async_stream::try_stream! {
        let mut emitter: InputOrderEmitter<TransactionDigest, (u64, TransactionData)> =
            InputOrderEmitter::new(digests);
        futures::pin_mut!(tx_stream);
        while let Some(row) = tx_stream.next().await {
            let (digest, tx) = row.map_err(RpcError::from)?;
            let seq = seq_by_digest.get(&digest).copied().ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("list_transactions: unexpected transaction body row {digest}"),
                )
            })?;
            for v in emitter.push(
                digest,
                (seq, tx),
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
    cursor: Bytes,
) -> ListTransactionsResponse {
    let mut transaction = ExecutedTransaction::default();
    if read_mask.contains(ExecutedTransaction::DIGEST_FIELD.name) {
        transaction.digest = Some(row.digest.to_string());
    }
    if read_mask.contains(ExecutedTransaction::CHECKPOINT_FIELD.name) {
        transaction.checkpoint = Some(row.checkpoint_number);
    }

    transaction_item_response(cursor, transaction)
}

/// Determine the tx_sequence_number scan window from the logical checkpoint
/// bounds. The checkpoint window is already clamped to indexed history and the
/// per-request scan width before this converts it into tx sequence space.
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
    cursor: Bytes,
    transaction: ExecutedTransaction,
) -> ListTransactionsResponse {
    let mut item = TransactionItem::default();
    item.cursor = Some(cursor);
    item.transaction = Some(transaction);

    let mut response = ListTransactionsResponse::default();
    response.response = Some(list_transactions_response::Response::Item(item));
    response
}

fn end_response(reason: QueryEndReason, cursor: Bytes) -> ListTransactionsResponse {
    let mut end = QueryEnd::default();
    end.cursor = Some(cursor);
    end.reason = reason as i32;

    let mut response = ListTransactionsResponse::default();
    response.response = Some(list_transactions_response::Response::End(end));
    response
}

fn query_end(
    emitted: usize,
    limit_items: usize,
    last_cursor: Option<Bytes>,
    end_reason: QueryEndReason,
    end_cursor: Bytes,
) -> (QueryEndReason, Bytes) {
    if emitted == limit_items {
        (
            QueryEndReason::ItemLimit,
            last_cursor.expect("item-limit responses have a last cursor"),
        )
    } else {
        (end_reason, end_cursor)
    }
}

fn range_stream(
    range: std::ops::Range<u64>,
    options: &QueryOptions,
) -> BoxStream<'static, Result<u64, RpcError>> {
    if options.is_ascending() {
        futures::stream::iter(range.map(Ok::<_, RpcError>)).boxed()
    } else {
        futures::stream::iter(range.rev().map(Ok::<_, RpcError>)).boxed()
    }
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
        QueryOptions::from_proto(
            None,
            100,
            1_000,
            QueryType::Transactions,
            Option::<&sui_rpc::proto::sui::rpc::v2alpha::TransactionFilter>::None,
        )
        .unwrap()
    }

    #[test]
    fn renders_transaction_response_from_tx_seq_digest() {
        let row = TxSeqDigestData {
            tx_sequence_number: 42,
            digest: TransactionDigest::new([7; 32]),
            event_count: 3,
            checkpoint_number: 9,
        };
        let options = options();

        let digest_only = unwrap_item(transaction_response_from_tx_seq_digest(
            row,
            &read_mask(&["digest"]),
            options.cursor_for_item(row.checkpoint_number, row.tx_sequence_number),
        ));
        assert_eq!(
            digest_only.cursor,
            Some(options.cursor_for_item(row.checkpoint_number, 42))
        );
        let transaction = digest_only.transaction.expect("executed transaction");
        assert_eq!(transaction.digest, Some(row.digest.to_string()));
        assert_eq!(transaction.checkpoint, None);

        let checkpoint_only = unwrap_item(transaction_response_from_tx_seq_digest(
            row,
            &read_mask(&["checkpoint"]),
            options.cursor_for_item(row.checkpoint_number, row.tx_sequence_number),
        ));
        let transaction = checkpoint_only.transaction.expect("executed transaction");
        assert_eq!(transaction.digest, None);
        assert_eq!(transaction.checkpoint, Some(9));

        let both = unwrap_item(transaction_response_from_tx_seq_digest(
            row,
            &read_mask(&["digest", "checkpoint"]),
            options.cursor_for_item(row.checkpoint_number, row.tx_sequence_number),
        ));
        let transaction = both.transaction.expect("executed transaction");
        assert_eq!(transaction.digest, Some(row.digest.to_string()));
        assert_eq!(transaction.checkpoint, Some(9));
    }
}
