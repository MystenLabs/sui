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
use crate::pipeline::AbortOnDrop;
use crate::pipeline::InputOrderEmitter;
use crate::pipeline::ResolvedWatermarked;
use crate::pipeline::Watermarked;
use crate::pipeline::pipelined_chunks;
use crate::pipeline::pipelined_keyed_batches;
use crate::pipeline::resolve_watermarks;
use crate::pipeline::take_items;
use crate::query_options::CheckpointRange;
use crate::query_options::QueryOptions;
use crate::query_options::QueryType;
use crate::query_options::ResolvedRange;
use crate::v2::get_transaction::compute_object_keys;
use crate::v2::get_transaction::needs_object_types;
use crate::v2::get_transaction::transaction_columns;
use crate::v2::get_transaction::transaction_to_response_observed;
use crate::v2::get_transaction::validate_read_mask;
use sui_inverted_index::BitmapScanLimitExceeded;
use sui_inverted_index::error_contains;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEnd;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionItem;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc::proto::sui::rpc::v2alpha::list_transactions_response;

const DEFAULT_LIMIT_ITEMS: u32 = 50;
const MAX_LIMIT_ITEMS: u32 = 500;
const CHUNK_MAX: usize = 100;

pub(crate) type ListTransactionsStream =
    BoxStream<'static, Result<ListTransactionsResponse, RpcError>>;
type TransactionWithObjectsStreamItem = (u64, u32, TransactionData, ObjectMap);

struct RenderedTransaction {
    tx_sequence_number: u64,
    tx_offset: u32,
    checkpoint_number: u64,
    transaction: ExecutedTransaction,
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

    let checkpoint_range = CheckpointRange::from_request(
        request.start_checkpoint,
        request.end_checkpoint,
        checkpoint_hi_exclusive,
    )?;
    let read_mask = Arc::new(validate_read_mask(request.read_mask)?);
    let options = QueryOptions::from_proto(
        request.options.as_ref(),
        DEFAULT_LIMIT_ITEMS,
        MAX_LIMIT_ITEMS,
        QueryType::Transactions,
        request.filter.as_ref(),
    )?;
    let limit_items = options.limit_items;
    let ordering = options.ordering;
    let direction = options.scan_direction();

    let resolve_started = Instant::now();
    let tx_range = resolve_tx_range(&client, checkpoint_range, &options)
        .instrument(debug_span!("resolve_tx_range"))
        .await?;
    ctx.observe_stage("transaction.resolve_range", resolve_started.elapsed());
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

    let request_bigtable_concurrency = ctx.request_bigtable_concurrency();

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
            pipelined_chunks(seq_stream, CHUNK_MAX, request_bigtable_concurrency, {
                let client = client.clone();
                let ctx = ctx.clone();
                move |seqs| fetch_tx_seq_digests(ctx.clone(), client.clone(), seqs)
            })
        } else {
            scan_tx_seq_digests(
                ctx.clone(),
                client.clone(),
                tx_range.clone(),
                limit_items,
                &options,
            )
            .await?
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
                        checkpoint_boundary = advance_checkpoint_boundary(checkpoint_boundary, row.checkpoint_number, &options);
                        let wm = item_watermark(&options, row.checkpoint_number, row.tx_sequence_number, checkpoint_boundary);
                        emitted += 1;
                        let yield_started = Instant::now();
                        yield transaction_response_from_tx_seq_digest(row, &read_mask, wm);
                        ctx.observe_stream_item_yield_wait(yield_started.elapsed());
                    }
                    Ok(ResolvedWatermarked::Watermark { position, cp }) => {
                        checkpoint_boundary = advance_checkpoint_boundary(checkpoint_boundary, cp, &options);
                        let wm = boundary_watermark(&options, cp, position, checkpoint_boundary, direction);
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
    let tx_stream = pipelined_chunks(digest_stream, CHUNK_MAX, request_bigtable_concurrency, {
        let client = client.clone();
        let columns = columns.clone();
        let ctx = ctx.clone();
        move |rows| fetch_transactions(ctx.clone(), client.clone(), columns.clone(), rows)
    });

    // Stage 4: + ObjectMap. Object refs are precomputed per Item; Frontier
    // watermarks pass through pipelined_keyed_batches unchanged.
    let txn_with_objects_stream: BoxStream<
        'static,
        Result<Watermarked<TransactionWithObjectsStreamItem>, anyhow::Error>,
    > = if needs_objects {
        let object_cache = ObjectCache::new(Arc::new(BigTableObjectFetcher::new(client.clone())));
        let object_ctx = ctx.clone();
        let tx_with_keys = tx_stream
            .map_ok(|m| {
                m.map_item(|(seq, offset, tx)| {
                    let keys: Vec<ObjectKey> = compute_object_keys(&tx).into_iter().collect();
                    ((seq, offset, tx), keys)
                })
            })
            .boxed();

        pipelined_keyed_batches(
            tx_with_keys,
            CHUNK_MAX,
            CHUNK_MAX,
            request_bigtable_concurrency,
            move |keys| {
                let object_cache = object_cache.clone();
                let ctx = object_ctx.clone();
                async move {
                    let fetch_started = Instant::now();
                    let key_count = keys.len();
                    let objects = object_cache
                        .get_many(keys)
                        .await
                        .map_err(anyhow::Error::new)?;
                    ctx.observe_stage("transaction.fetch_objects", fetch_started.elapsed());
                    info!(
                        key_count,
                        object_count = objects.len(),
                        elapsed_ms = fetch_started.elapsed().as_millis(),
                        "list_transactions: fetch_objects done"
                    );
                    Ok(objects)
                }
            },
        )
        .map_ok(|m| m.map_item(|((seq, offset, tx), objects)| (seq, offset, tx, objects)))
        .boxed()
    } else {
        // No object lookup needed — emit each tx with an empty ObjectMap.
        tx_stream
            .map_ok(|m| {
                m.map_item(|(seq, offset, tx)| {
                    (seq, offset, tx, Arc::new(HashMap::new()) as ObjectMap)
                })
            })
            .boxed()
    };

    let render_ctx = ctx.clone();
    let render_concurrency = ctx.response_render_concurrency();
    let rendered_txn_stream = txn_with_objects_stream
        .map(move |item| {
            let read_mask = read_mask.clone();
            let resolver = resolver.clone();
            let ctx = render_ctx.clone();
            async move {
                match item? {
                    Watermarked::Item((tx_sequence_number, tx_offset, tx_data, objects)) => {
                        let checkpoint_number = tx_data.checkpoint_number;
                        let render_task = AbortOnDrop::new(tokio::spawn(async move {
                            let render_started = Instant::now();
                            let observe_ctx = ctx.clone();
                            let transaction = transaction_to_response_observed(
                                tx_data,
                                &read_mask,
                                &objects,
                                &resolver,
                                move |stage, elapsed| observe_ctx.observe_stage(stage, elapsed),
                            )
                            .await?;
                            ctx.observe_response_render(render_started.elapsed());
                            Ok::<_, anyhow::Error>(RenderedTransaction {
                                tx_sequence_number,
                                tx_offset,
                                checkpoint_number,
                                transaction,
                            })
                        }));
                        let rendered = render_task.await.map_err(|e| {
                            anyhow::anyhow!("list_transactions: render task failed: {e}")
                        })??;
                        Ok::<Watermarked<RenderedTransaction>, anyhow::Error>(Watermarked::Item(
                            rendered,
                        ))
                    }
                    Watermarked::Watermark(position) => Ok(Watermarked::Watermark(position)),
                }
            }
        })
        .buffered(render_concurrency)
        .boxed();

    let rendered_txn_stream =
        resolve_watermarks(rendered_txn_stream, client.tx_wm_resolver(direction));

    Ok(async_stream::try_stream! {
        futures::pin_mut!(rendered_txn_stream);

        let mut emitted = 0usize;
        let mut checkpoint_boundary: Option<u64> = None;
        let mut scan_limit_hit = false;
        while let Some(item) = rendered_txn_stream.next().await {
            match item {
                Ok(ResolvedWatermarked::Item(rendered)) => {
                    checkpoint_boundary = advance_checkpoint_boundary(checkpoint_boundary, rendered.checkpoint_number, &options);
                    let wm = item_watermark(&options, rendered.checkpoint_number, rendered.tx_sequence_number, checkpoint_boundary);
                    emitted += 1;
                    let yield_started = Instant::now();
                    yield transaction_item_response(wm, rendered.transaction, rendered.tx_offset);
                    ctx.observe_stream_item_yield_wait(yield_started.elapsed());
                }
                Ok(ResolvedWatermarked::Watermark { position, cp }) => {
                    checkpoint_boundary = advance_checkpoint_boundary(checkpoint_boundary, cp, &options);
                    let wm = boundary_watermark(&options, cp, position, checkpoint_boundary, direction);
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

/// For ListTransactions, the scan-direction completion boundary is
/// `item_cp ± 1` because the item's own cp may still have unscanned
/// matching transactions at higher (asc) or lower (desc) tx_seqs.
/// Monotonic in scan direction: stored as `checkpoint_hi` (ascending) or
/// `checkpoint_lo` (descending) by the Watermark builders below.
///
/// When the direction-adjusted candidate would overflow (`item_cp == 0`
/// ascending or `u64::MAX` descending), the previously accumulated
/// boundary is preserved rather than collapsed back to `None`.
fn advance_checkpoint_boundary(
    prev: Option<u64>,
    item_cp: u64,
    options: &QueryOptions,
) -> Option<u64> {
    let candidate = if options.is_ascending() {
        item_cp.checked_sub(1)
    } else {
        item_cp.checked_add(1)
    };
    match (prev, candidate) {
        (p, None) => p,
        (None, Some(c)) => Some(c),
        (Some(p), Some(c)) if options.is_ascending() => Some(p.max(c)),
        (Some(p), Some(c)) => Some(p.min(c)),
    }
}

/// Populate the direction-matching field of a `Watermark` from the
/// per-scan boundary value. Exactly one of `checkpoint_hi` /
/// `checkpoint_lo` is set, never both.
fn set_checkpoint_bound(wm: &mut Watermark, options: &QueryOptions, boundary: Option<u64>) {
    if options.is_ascending() {
        wm.checkpoint_hi = boundary;
    } else {
        wm.checkpoint_lo = boundary;
    }
}

/// Build the embedded `Watermark` for an item: cursor encodes this item's
/// position (so the next request's `after`/`before` resumes past it) plus
/// the current direction-matching checkpoint boundary.
fn item_watermark(
    options: &QueryOptions,
    cp: u64,
    position: u64,
    checkpoint_boundary: Option<u64>,
) -> Watermark {
    let mut wm = Watermark::default();
    wm.cursor = Some(options.cursor_for_item(cp, position));
    set_checkpoint_bound(&mut wm, options, checkpoint_boundary);
    wm
}

/// Build a standalone `Watermark` frame for the scan frontier between
/// items. The watermark's resolved `cp` is `cp_of(F-1)` ascending or
/// `cp_of(F)` descending — the last cp the bitmap might still emit
/// items in. The CURSOR encoding is asymmetric: ascending uses `cp`
/// directly (Boundary `after` advances the cp range start), but
/// descending needs `cp + 1` because Boundary `before` treats the cp
/// coordinate as an EXCLUSIVE upper bound (and we want `cp_of(F)`
/// included on resume).
fn boundary_watermark(
    options: &QueryOptions,
    cp: u64,
    position: u64,
    checkpoint_boundary: Option<u64>,
    direction: sui_inverted_index::ScanDirection,
) -> Watermark {
    let cursor_cp = if direction.is_ascending() {
        cp
    } else {
        cp.saturating_add(1)
    };
    let mut wm = Watermark::default();
    wm.cursor = Some(options.cursor_for_boundary(cursor_cp, position));
    set_checkpoint_bound(&mut wm, options, checkpoint_boundary);
    wm
}

/// Wrap a constructed `Watermark` as a standalone wire frame.
fn watermark_response(watermark: Watermark) -> ListTransactionsResponse {
    let mut response = ListTransactionsResponse::default();
    response.response = Some(list_transactions_response::Response::Watermark(watermark));
    response
}

/// Whether the scan reached the natural end of the requested range (the ledger
/// tip or a requested `end_checkpoint`) rather than being truncated by an item
/// or scan limit, or bounded by a client cursor. Only natural completion proves
/// the range's final checkpoint complete.
fn reached_range_end(reason: QueryEndReason) -> bool {
    matches!(
        reason,
        QueryEndReason::LedgerTip | QueryEndReason::CheckpointBound
    )
}

/// Boundary watermark emitted once the scan has drained the entire resolved
/// range under natural completion. Unlike per-item watermarks it can claim the
/// range's final checkpoint complete — `end_checkpoint - 1` ascending (the
/// exclusive cp upper) or `end_checkpoint` descending (the inclusive cp lower) —
/// because no further transactions exist in it within the requested range. The
/// `(end_checkpoint, end_position)` cursor resumes exactly past the scanned
/// range.
fn terminal_boundary_watermark(
    options: &QueryOptions,
    end_checkpoint: u64,
    end_position: u64,
) -> Watermark {
    let boundary = if options.is_ascending() {
        end_checkpoint.checked_sub(1)
    } else {
        Some(end_checkpoint)
    };
    let mut wm = Watermark::default();
    wm.cursor = Some(options.cursor_for_boundary(end_checkpoint, end_position));
    set_checkpoint_bound(&mut wm, options, boundary);
    wm
}

async fn scan_tx_seq_digests(
    ctx: QueryContext,
    client: BigTableClient,
    range: std::ops::Range<u64>,
    limit: usize,
    options: &QueryOptions,
) -> Result<BoxStream<'static, Result<Watermarked<TxSeqDigestData>, anyhow::Error>>, RpcError> {
    let open_started = Instant::now();
    let rows = client
        .scan_tx_seq_digests_stream(range, options.scan_direction(), limit)
        .await?;
    ctx.observe_stage(
        "transaction.scan_tx_seq_digests_open",
        open_started.elapsed(),
    );
    Ok(rows.map_ok(Watermarked::Item).boxed())
}

async fn fetch_tx_seq_digests(
    ctx: QueryContext,
    client: BigTableClient,
    seqs: Vec<u64>,
) -> Result<BoxStream<'static, Result<TxSeqDigestData, anyhow::Error>>, anyhow::Error> {
    if seqs.is_empty() {
        return Ok(futures::stream::empty().boxed());
    }
    let stage_started = Instant::now();
    let open_started = Instant::now();
    // The permit lives inside the stream returned by `BigTableClient::
    // resolve_tx_digests_stream` and drops with that stream — propagated
    // through to the stream we return below.
    let seq_count = seqs.len();
    let digest_stream = client.resolve_tx_digests_stream(seqs.clone()).await?;
    let open_elapsed = open_started.elapsed();
    ctx.observe_stage("transaction.fetch_tx_seq_digests_open", open_elapsed);
    Ok(async_stream::try_stream! {
        let mut emitter: InputOrderEmitter<u64, TxSeqDigestData> =
            InputOrderEmitter::new(seqs);
        futures::pin_mut!(digest_stream);
        let mut row_count = 0usize;
        let mut emitted = 0usize;
        while let Some(row) = digest_stream.next().await {
            let row = row?;
            row_count += 1;
            for v in emitter.push(
                row.tx_sequence_number,
                row,
                "list_transactions: transaction digest lookup",
            )? {
                emitted += 1;
                yield v;
            }
        }
        for v in emitter.finish("list_transactions: missing selected transaction digest")? {
            emitted += 1;
            yield v;
        }
        ctx.observe_stage("transaction.fetch_tx_seq_digests_drain", stage_started.elapsed());
        info!(
            seq_count,
            row_count,
            emitted,
            open_ms = open_elapsed.as_millis(),
            elapsed_ms = stage_started.elapsed().as_millis(),
            "list_transactions: fetch_tx_seq_digests done"
        );
    }
    .boxed())
}

async fn fetch_transactions(
    ctx: QueryContext,
    client: BigTableClient,
    columns: Arc<[&'static str]>,
    rows: Vec<TxSeqDigestData>,
) -> Result<BoxStream<'static, Result<(u64, u32, TransactionData), anyhow::Error>>, anyhow::Error> {
    if rows.is_empty() {
        return Ok(futures::stream::empty().boxed());
    }
    let stage_started = Instant::now();
    let column_filter = BigTableClient::column_filter(&columns);
    let digests: Vec<TransactionDigest> = rows.iter().map(|row| row.digest).collect();
    let digest_count = digests.len();
    // Map each digest back to its (tx_sequence_number, within-checkpoint offset)
    // so the output can carry them alongside the tx body as it arrives.
    let meta_by_digest: HashMap<TransactionDigest, (u64, u32)> = rows
        .iter()
        .map(|row| (row.digest, (row.tx_sequence_number, row.tx_offset)))
        .collect();
    let open_started = Instant::now();
    let tx_stream = client
        .get_transactions_stream(digests.clone(), Some(column_filter))
        .await?;
    let open_elapsed = open_started.elapsed();
    ctx.observe_stage("transaction.fetch_transactions_open", open_elapsed);
    Ok(async_stream::try_stream! {
        let mut emitter: InputOrderEmitter<TransactionDigest, (u64, u32, TransactionData)> =
            InputOrderEmitter::new(digests);
        futures::pin_mut!(tx_stream);
        let mut row_count = 0usize;
        let mut emitted = 0usize;
        while let Some(row) = tx_stream.next().await {
            let (digest, tx) = row?;
            row_count += 1;
            let (seq, offset) = meta_by_digest.get(&digest).copied().ok_or_else(|| {
                anyhow::anyhow!("list_transactions: unexpected transaction body row {digest}")
            })?;
            for v in emitter.push(
                digest,
                (seq, offset, tx),
                "list_transactions: transaction body lookup",
            )? {
                emitted += 1;
                yield v;
            }
        }
        for v in emitter.finish("list_transactions: missing selected transaction body")? {
            emitted += 1;
            yield v;
        }
        ctx.observe_stage("transaction.fetch_transactions_drain", stage_started.elapsed());
        info!(
            digest_count,
            row_count,
            emitted,
            open_ms = open_elapsed.as_millis(),
            elapsed_ms = stage_started.elapsed().as_millis(),
            "list_transactions: fetch_transactions done"
        );
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
        QueryOptions::from_proto(
            None,
            100,
            1_000,
            QueryType::Transactions,
            Option::<&sui_rpc::proto::sui::rpc::v2alpha::TransactionFilter>::None,
        )
        .unwrap()
    }

    fn descending_options() -> QueryOptions {
        let mut proto = sui_rpc::proto::sui::rpc::v2alpha::QueryOptions::default();
        proto.ordering = sui_rpc::proto::sui::rpc::v2alpha::Ordering::Descending as i32;
        QueryOptions::from_proto(
            Some(&proto),
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

    /// Descending scans set `checkpoint_lo` instead of `checkpoint_hi`,
    /// so a client can read the direction-correct boundary from the
    /// wire frame without knowing the request's ordering.
    #[test]
    fn descending_item_watermark_sets_checkpoint_lo_not_hi() {
        let options = descending_options();
        let wm = item_watermark(&options, 9, 42, Some(10));
        assert_eq!(
            wm.checkpoint_hi, None,
            "descending scan must not set checkpoint_hi"
        );
        assert_eq!(
            wm.checkpoint_lo,
            Some(10),
            "descending scan stores the boundary in checkpoint_lo"
        );
    }

    /// On natural completion the terminal frame claims the range's final
    /// checkpoint complete: ascending uses `end_checkpoint - 1` (exclusive cp
    /// upper) and resumes from `(end_checkpoint, end_position)`.
    #[test]
    fn terminal_boundary_watermark_ascending_claims_end_minus_one() {
        let options = options();
        let wm = terminal_boundary_watermark(&options, 10, 100);
        assert_eq!(wm.checkpoint_hi, Some(9));
        assert_eq!(wm.checkpoint_lo, None);
        assert_eq!(
            wm.cursor.as_ref(),
            Some(&options.cursor_for_boundary(10, 100))
        );
    }

    /// Descending stores the range's lowest checkpoint (inclusive) in
    /// `checkpoint_lo`.
    #[test]
    fn terminal_boundary_watermark_descending_claims_end_checkpoint() {
        let options = descending_options();
        let wm = terminal_boundary_watermark(&options, 10, 100);
        assert_eq!(wm.checkpoint_lo, Some(10));
        assert_eq!(wm.checkpoint_hi, None);
        assert_eq!(
            wm.cursor.as_ref(),
            Some(&options.cursor_for_boundary(10, 100))
        );
    }
}
