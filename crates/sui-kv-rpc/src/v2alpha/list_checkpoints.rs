// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream;
use futures::stream::BoxStream;
use sui_kvstore::BitmapIndexSpec;
use sui_kvstore::CheckpointData;
use sui_kvstore::TransactionData;
use sui_kvstore::tables;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::Checkpoint;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_rpc_api::proto::google::rpc::bad_request::FieldViolation;
use sui_types::digests::TransactionDigest;
use sui_types::full_checkpoint_content::Checkpoint as FullCheckpoint;
use sui_types::full_checkpoint_content::ExecutedTransaction as FullExecutedTransaction;
use sui_types::full_checkpoint_content::ObjectSet;
use sui_types::messages_checkpoint::CertifiedCheckpointSummary;
use sui_types::storage::ObjectKey;
use tracing::Instrument;
use tracing::debug_span;
use tracing::info;

use crate::bigtable_client::BigTableClient;
use crate::bigtable_client::stage;
use crate::object_cache::BigTableObjectFetcher;
use crate::object_cache::ObjectCache;
use crate::object_cache::ObjectMap;
use crate::operation::QueryContext;
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
use crate::v2::get_checkpoint::checkpoint_columns;
use crate::v2::get_transaction::compute_object_keys;
use crate::v2::get_transaction::transaction_columns;
use sui_inverted_index::BitmapScanLimitExceeded;
use sui_inverted_index::ScanDirection;
use sui_inverted_index::error_contains;

use sui_rpc::proto::sui::rpc::v2alpha::CheckpointItem;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEnd;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc::proto::sui::rpc::v2alpha::list_checkpoints_response;

const DEFAULT_LIMIT_ITEMS: u32 = 10;
const MAX_LIMIT_ITEMS: u32 = 100;
const CHUNK_MAX: usize = 100;
const READ_MASK_DEFAULT: &str = "sequence_number,digest";

type CpWithTxs = (u64, CheckpointData, Vec<TransactionData>);
type ResolvedCp = (u64, CheckpointData, Vec<TransactionData>, ObjectMap);

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
        DEFAULT_LIMIT_ITEMS,
        MAX_LIMIT_ITEMS,
        QueryType::Checkpoints,
        request.filter.as_ref(),
    )?;
    let limit_items = options.limit_items;
    let ordering = options.ordering;
    let direction = options.scan_direction();

    let cp_range = async { Ok::<_, RpcError>(resolve_cp_range(checkpoint_range, &options)) }
        .instrument(debug_span!("resolve_cp_range"))
        .await?;
    let end_reason = cp_range.end_reason;
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
        return Ok(stream::once(async move { Ok(end_response(end_reason)) }).boxed());
    }

    let needs_full = needs_transactions_or_objects(&read_mask);
    let cp_columns: Arc<[&'static str]> = list_checkpoint_columns(&read_mask, needs_full).into();

    let request_bigtable_concurrency = ctx.request_bigtable_concurrency();
    let scan_budget = ctx.scan_budget(BitmapIndexSpec::tx());

    // Stage A: discover cp_seq values for the requested response. Filtered
    // requests use bitmap-eval; the tx_seq watermarks it emits are
    // translated to cp_seq watermarks by `filtered_checkpoint_seq_stream`
    // before they reach the handler's loop. Unfiltered requests produce the
    // cp range directly (cheap, no IO); every cp_seq becomes an item, so the
    // last item watermark is the final resume cursor.
    let seq_stream: BoxStream<'static, Result<Watermarked<u64>, anyhow::Error>> =
        if let Some(filter) = &request.filter {
            let tx_range = client.checkpoint_to_tx_range(cp_range.clone()).await?;
            filtered_checkpoint_seq_stream(
                &ctx,
                filter,
                tx_range,
                limit_items,
                options.clone(),
                scan_budget,
            )
            .await?
        } else {
            // Unfiltered: items cover the resolved cp range densely;
            // the last item's cursor is a sufficient resume point.
            range_stream(cp_range.clone(), &options)
                .map(|r| r.map(Watermarked::Item).map_err(anyhow::Error::new))
                .boxed()
        };
    let seq_stream = take_items(seq_stream, limit_items);

    // Stage B: Watermarked<cp_seq> -> Watermarked<(cp_seq, CheckpointData)>. One
    // multi_get per chunk against the checkpoints table.
    let cp_data_stream = pipelined_chunks(seq_stream, CHUNK_MAX, request_bigtable_concurrency, {
        let client = client.clone();
        let columns = cp_columns.clone();
        move |seqs| fetch_checkpoint_data(client.clone(), columns.clone(), seqs)
    });

    // Fast path: read_mask doesn't request transactions or objects → render
    // directly from CheckpointData via the existing `checkpoint_to_response`
    // (with `checkpoint_bucket = None`, the GCS branch is a no-op).
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
                        let wm = item_watermark(&options, cp_seq, checkpoint_boundary);
                        emitted += 1;
                        let message =
                            crate::v2::get_checkpoint::checkpoint_to_response(cp_data, &read_mask, None)
                                .await?;
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
                        let wm = boundary_watermark(&options, cp_frontier, checkpoint_boundary);
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
    let tx_columns: Arc<[&'static str]> = list_transactions_columns(&read_mask).into();
    let needs_objects = read_mask.contains(Checkpoint::OBJECTS_FIELD);

    // Stage C: (cp_seq, CheckpointData) -> + Vec<TransactionData>. Batched
    // across the chunk: gather ALL tx_digests across all cps in the chunk
    // into one multi_get, route results back per-cp, then emit in input
    // cp_seq order after the chunk drains.
    let cp_with_txs_stream =
        pipelined_chunks(cp_data_stream, CHUNK_MAX, request_bigtable_concurrency, {
            let client = client.clone();
            let columns = tx_columns.clone();
            move |items| fetch_transactions_for_cps(client.clone(), columns.clone(), items)
        });

    // Stage D: + ObjectMap. Per-cp object refs are precomputed, then
    // `pipelined_keyed_batches` packs consecutive cps into batches whose
    // deduped key union fits within CHUNK_MAX (see comment on the
    // constant). The helper splits the per-batch fetch result back out
    // per cp — `render_full_checkpoint` builds an `ObjectSet` by
    // iterating the whole map, so each cp must see only its own keys.
    let object_cache = ObjectCache::new(Arc::new(BigTableObjectFetcher::new(client.clone())));
    let cp_full_stream: BoxStream<'static, Result<Watermarked<ResolvedCp>, anyhow::Error>> =
        if needs_objects {
            let cp_with_keys = cp_with_txs_stream
                .map_ok(|m: Watermarked<CpWithTxs>| {
                    m.map_item(|(cp_seq, cp_data, txs)| {
                        let keys: Vec<ObjectKey> = txs
                            .iter()
                            .flat_map(compute_object_keys)
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect();
                        ((cp_seq, cp_data, txs), keys)
                    })
                })
                .boxed();
            pipelined_keyed_batches(
                cp_with_keys,
                CHUNK_MAX,
                CHUNK_MAX,
                request_bigtable_concurrency,
                {
                    let object_cache = object_cache.clone();
                    move |keys| {
                        let object_cache = object_cache.clone();
                        async move {
                            object_cache
                                .get_many(keys)
                                .await
                                .map_err(anyhow::Error::new)
                        }
                    }
                },
            )
            .map_ok(|m| {
                m.map_item(|((cp_seq, cp_data, txs), objects)| (cp_seq, cp_data, txs, objects))
            })
            .boxed()
        } else {
            let empty: ObjectMap = Arc::new(HashMap::new());
            cp_with_txs_stream
                .map_ok(move |m: Watermarked<CpWithTxs>| {
                    let empty = empty.clone();
                    m.map_item(move |(cp_seq, cp_data, txs)| (cp_seq, cp_data, txs, empty.clone()))
                })
                .boxed()
        };

    let cp_full_stream = resolve_watermarks(cp_full_stream, client.tx_wm_resolver(direction));

    // Stage E: sync render — build full_checkpoint_content::Checkpoint and
    // merge into the proto Checkpoint (CPU-only, no further IO).
    Ok(async_stream::try_stream! {
        // Hold the cache for the lifetime of the response stream so in-flight
        // object dispatches are aborted if the consumer drops the stream.
        let _object_cache = object_cache;
        futures::pin_mut!(cp_full_stream);
        let mut emitted = 0usize;
        let mut checkpoint_boundary: Option<u64> = None;
        let mut scan_limit_hit = false;
        while let Some(item) = cp_full_stream.next().await {
            match item {
                Ok(ResolvedWatermarked::Item((cp_seq, cp_data, txs, objects))) => {
                    checkpoint_boundary = advance_checkpoint_boundary(checkpoint_boundary, cp_seq, &options);
                    let wm = item_watermark(&options, cp_seq, checkpoint_boundary);
                    let response = render_full_checkpoint(
                        (cp_seq, cp_data, txs, objects),
                        &read_mask,
                        wm,
                    )?;
                    emitted += 1;
                    yield response;
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
                    let wm = boundary_watermark(&options, cp_frontier, checkpoint_boundary);
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

/// For ListCheckpoints, the scan-direction completion boundary is the
/// item's cp directly: `filtered_checkpoint_seq_stream` dedupes so a cp
/// is never re-emitted, meaning "cp X emitted" is equivalent to "cp X
/// fully processed for the purposes of this scan." (Compare to
/// list_transactions/list_events, where the boundary is `item_cp ± 1`
/// because the item's own cp may still have more matches at other
/// tx_seqs/event_seqs.) Stored as `checkpoint_hi` (ascending) or
/// `checkpoint_lo` (descending) by the Watermark builders below.
///
/// Also called via `frontier_to_boundary_candidate` to fold standalone
/// `Watermark` frames into the same boundary so sparse filtered scans
/// surface real cp progress even across long match-less gaps.
fn advance_checkpoint_boundary(
    prev: Option<u64>,
    candidate: u64,
    options: &QueryOptions,
) -> Option<u64> {
    Some(match prev {
        None => candidate,
        Some(p) if options.is_ascending() => p.max(candidate),
        Some(p) => p.min(candidate),
    })
}

/// Convert a cp-space scan frontier from `filtered_checkpoint_seq_stream`
/// into a checkpoint-boundary candidate for `advance_checkpoint_boundary`.
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

/// Build the embedded `Watermark` for a ListCheckpoints item. cp and
/// position are the same cp_seq (item cursor encoding).
fn item_watermark(
    options: &QueryOptions,
    cp_seq: u64,
    checkpoint_boundary: Option<u64>,
) -> Watermark {
    let mut wm = Watermark::default();
    wm.cursor = Some(options.cursor_for_item(cp_seq, cp_seq));
    set_checkpoint_bound(&mut wm, options, checkpoint_boundary);
    wm
}

/// Build a standalone scan-frontier `Watermark`. The cursor uses
/// `Boundary` kind. `filtered_checkpoint_seq_stream` translates tx_seq
/// watermarks into cp_seq watermarks before they reach the handler, and
/// clamps them past any cp already delivered as an Item — so the
/// emitted boundary cursor always resumes past every delivered
/// checkpoint while still including any cp not yet delivered (its first
/// matching tx may sit later in the scan).
fn boundary_watermark(
    options: &QueryOptions,
    cp_seq: u64,
    checkpoint_boundary: Option<u64>,
) -> Watermark {
    let mut wm = Watermark::default();
    wm.cursor = Some(options.cursor_for_boundary(cp_seq, cp_seq));
    set_checkpoint_bound(&mut wm, options, checkpoint_boundary);
    wm
}

fn watermark_response(watermark: Watermark) -> ListCheckpointsResponse {
    let mut response = ListCheckpointsResponse::default();
    response.response = Some(list_checkpoints_response::Response::Watermark(watermark));
    response
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

async fn fetch_transactions_for_cps(
    client: BigTableClient,
    columns: Arc<[&'static str]>,
    items: Vec<(u64, CheckpointData)>,
) -> Result<BoxStream<'static, Result<CpWithTxs, anyhow::Error>>, anyhow::Error> {
    if items.is_empty() {
        return Ok(stream::empty().boxed());
    }

    let mut input_order: Vec<u64> = Vec::with_capacity(items.len());
    let mut cp_data_by_seq: HashMap<u64, CheckpointData> = HashMap::with_capacity(items.len());
    let mut expected_count: HashMap<u64, usize> = HashMap::with_capacity(items.len());
    let mut digest_to_cp: HashMap<TransactionDigest, u64> = HashMap::new();
    let mut txs_by_seq: HashMap<u64, Vec<TransactionData>> = HashMap::with_capacity(items.len());
    let mut flat_digests: Vec<TransactionDigest> = Vec::new();

    for (cp_seq, cp_data) in items {
        let contents = cp_data
            .contents
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("checkpoint {cp_seq} contents column missing"))?;
        let cp_digests: Vec<_> = contents.iter().map(|d| d.transaction).collect();
        expected_count.insert(cp_seq, cp_digests.len());
        txs_by_seq.insert(cp_seq, Vec::with_capacity(cp_digests.len()));
        for digest in &cp_digests {
            digest_to_cp.insert(*digest, cp_seq);
        }
        flat_digests.extend(cp_digests);
        input_order.push(cp_seq);
        cp_data_by_seq.insert(cp_seq, cp_data);
    }

    // Empty checkpoints (zero transactions) generate no BigTable rows, so
    // pre-collect them and release through the emitter before draining the
    // tx stream — otherwise they'd never get emitted.
    let empty_cps: Vec<u64> = input_order
        .iter()
        .copied()
        .filter(|cp_seq| expected_count[cp_seq] == 0)
        .collect();

    // CRITICAL: never call `get_transactions_stream` with an empty digest
    // list. `multi_get_stream` builds a ReadRowsRequest with
    // `rows_limit = keys.len()`, and BigTable interprets `rows_limit = 0`
    // as "no limit" — i.e. a full transactions-table scan. When every cp
    // in this chunk is empty, fall back to an empty stream and let the
    // pre-emit loop alone drive emission.
    let tx_stream: BoxStream<'static, Result<(TransactionDigest, TransactionData), anyhow::Error>> =
        if flat_digests.is_empty() {
            stream::empty().boxed()
        } else {
            let column_filter = BigTableClient::column_filter(&columns);
            client
                .get_transactions_stream(flat_digests, Some(column_filter))
                .await?
                .boxed()
        };

    Ok(async_stream::try_stream! {
        let mut emitter: InputOrderEmitter<u64, CpWithTxs> = InputOrderEmitter::new(input_order);
        for cp_seq in empty_cps {
            let cp_data = cp_data_by_seq.remove(&cp_seq).expect("cp_data entry present");
            for v in emitter.push(
                cp_seq,
                (cp_seq, cp_data, Vec::new()),
                "list_checkpoints: checkpoint transaction lookup",
            )? {
                yield v;
            }
        }
        futures::pin_mut!(tx_stream);
        while let Some(row) = tx_stream.next().await {
            let (digest, tx) = row?;
            let cp_seq = digest_to_cp.remove(&digest).ok_or_else(|| {
                anyhow::anyhow!("list_checkpoints: unexpected transaction body row {digest}")
            })?;
            let cp_txs = txs_by_seq
                .get_mut(&cp_seq)
                .expect("txs_by_seq entry present");
            cp_txs.push(tx);
            if cp_txs.len() == expected_count[&cp_seq] {
                let txs = txs_by_seq.remove(&cp_seq).expect("txs_by_seq entry");
                let cp_data = cp_data_by_seq
                    .remove(&cp_seq)
                    .expect("cp_data entry present");
                for v in emitter.push(
                    cp_seq,
                    (cp_seq, cp_data, txs),
                    "list_checkpoints: checkpoint transaction lookup",
                )? {
                    yield v;
                }
            }
        }
        // Defensive: if BigTable returned fewer rows than requested, surface
        // the missing digests as an internal error rather than emit a
        // partial checkpoint downstream. cp_data_by_seq still containing
        // entries means at least one cp's tx set was incomplete.
        if !cp_data_by_seq.is_empty() {
            // Build a per-cp report (got vs expected) to make production
            // triage tractable — without it the error message alone gives
            // no signal about which cps or how big the gap was.
            let mut incomplete: Vec<(u64, usize, usize)> = cp_data_by_seq
                .keys()
                .map(|cp_seq| {
                    let got = txs_by_seq.get(cp_seq).map(|v| v.len()).unwrap_or(0);
                    let expected = expected_count.get(cp_seq).copied().unwrap_or(0);
                    (*cp_seq, got, expected)
                })
                .collect();
            incomplete.sort_unstable();
            tracing::warn!(
                incomplete_count = incomplete.len(),
                ?incomplete,
                "list_checkpoints: BigTable returned fewer transactions than requested (cp_seq, got, expected)"
            );
            Err(RpcError::new(
                tonic::Code::Internal,
                format!(
                    "list_checkpoints: BigTable returned fewer transactions than requested for {} checkpoint(s)",
                    incomplete.len()
                ),
            ))?;
        }
        for v in emitter.finish("list_checkpoints: missing selected checkpoint transactions")? {
            yield v;
        }
    }
    .boxed())
}

fn render_full_checkpoint(
    item: ResolvedCp,
    read_mask: &FieldMaskTree,
    watermark: Watermark,
) -> Result<ListCheckpointsResponse, RpcError> {
    let (cp_seq, cp_data, txs, objects) = item;

    let summary = cp_data.summary.ok_or_else(|| {
        RpcError::new(
            tonic::Code::Internal,
            format!("checkpoint {cp_seq} summary column missing"),
        )
    })?;
    let signatures = cp_data.signatures.ok_or_else(|| {
        RpcError::new(
            tonic::Code::Internal,
            format!("checkpoint {cp_seq} signatures column missing"),
        )
    })?;
    let contents = cp_data.contents.ok_or_else(|| {
        RpcError::new(
            tonic::Code::Internal,
            format!("checkpoint {cp_seq} contents column missing"),
        )
    })?;

    let executed_transactions = txs
        .into_iter()
        .map(|tx| {
            let transaction = tx.transaction_data.ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("transaction {} data column missing", tx.digest),
                )
            })?;
            let effects = tx.effects.ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("transaction {} effects column missing", tx.digest),
                )
            })?;
            Ok::<_, RpcError>(FullExecutedTransaction {
                transaction,
                signatures: tx.signatures.unwrap_or_default(),
                effects,
                events: tx.events,
                unchanged_loaded_runtime_objects: tx.unchanged_loaded_runtime_objects,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut object_set = ObjectSet::default();
    for (_, obj) in objects.iter() {
        object_set.insert(obj.clone());
    }

    let full_checkpoint = FullCheckpoint {
        summary: CertifiedCheckpointSummary::new_from_data_and_sig(summary, signatures),
        contents,
        transactions: executed_transactions,
        object_set,
    };

    let mut message = Checkpoint::default();
    message.merge(&full_checkpoint, read_mask);

    Ok(response_for(watermark, message))
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

fn needs_transactions_or_objects(mask: &FieldMaskTree) -> bool {
    mask.contains(Checkpoint::TRANSACTIONS_FIELD) || mask.contains(Checkpoint::OBJECTS_FIELD)
}

/// Columns to fetch from the checkpoints table. When transactions or
/// objects are in the read mask we additionally need `signatures` (to
/// reconstruct `CertifiedCheckpointSummary`) and `contents` (to enumerate
/// each checkpoint's transaction digests).
fn list_checkpoint_columns(mask: &FieldMaskTree, needs_full: bool) -> Vec<&'static str> {
    let mut columns = checkpoint_columns(mask);
    if needs_full {
        if !columns.contains(&tables::checkpoints::col::CONTENTS) {
            columns.push(tables::checkpoints::col::CONTENTS);
        }
        if !columns.contains(&tables::checkpoints::col::SIGNATURES) {
            columns.push(tables::checkpoints::col::SIGNATURES);
        }
    }
    columns
}

/// Columns to fetch from the transactions table for the heavy path. The
/// merge target is `full_checkpoint_content::ExecutedTransaction`, whose
/// `transaction: TransactionData` and `effects: TransactionEffects` fields
/// are non-`Option` — even when the read mask only asks for
/// `transactions.digest`, the merge reads `source.transaction.digest()` to
/// produce the response, so we must always have data + effects available.
/// Object resolution likewise needs data + effects + unchanged_loaded to
/// derive `compute_object_keys`. Optional source fields (signatures,
/// events, unchanged_loaded) are gated by the mask.
fn list_transactions_columns(mask: &FieldMaskTree) -> Vec<&'static str> {
    let mut columns = if let Some(submask) = mask.subtree(Checkpoint::TRANSACTIONS_FIELD.name) {
        transaction_columns(&submask)
    } else {
        // Baseline metadata columns even if the proto `transactions` field
        // isn't requested; we still need the rows to compute object keys.
        vec![
            tables::transactions::col::CHECKPOINT_NUMBER,
            tables::transactions::col::TIMESTAMP,
        ]
    };
    // Required to construct the merge target faithfully — see fn doc.
    for col in [
        tables::transactions::col::DATA,
        tables::transactions::col::EFFECTS,
    ] {
        if !columns.contains(&col) {
            columns.push(col);
        }
    }
    if mask.contains(Checkpoint::OBJECTS_FIELD)
        && !columns.contains(&tables::transactions::col::UNCHANGED_LOADED)
    {
        columns.push(tables::transactions::col::UNCHANGED_LOADED);
    }
    columns
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
        let mut tx_seq_chunk: Vec<u64> = Vec::with_capacity(CHUNK_MAX);
        let mut last_cp_seq: Option<u64> = None;
        let mut emitted = 0usize;

        loop {
            // Read until we have a full chunk of tx_seq Items, OR a Frontier
            // marker arrives (forcing flush), OR the upstream ends.
            let mut pending_watermark: Option<u64> = None;
            while tx_seq_chunk.len() < CHUNK_MAX && pending_watermark.is_none() {
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

fn range_stream(
    range: std::ops::Range<u64>,
    options: &QueryOptions,
) -> BoxStream<'static, Result<u64, RpcError>> {
    if options.is_ascending() {
        stream::iter(range.map(Ok::<_, RpcError>)).boxed()
    } else {
        stream::iter(range.rev().map(Ok::<_, RpcError>)).boxed()
    }
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
