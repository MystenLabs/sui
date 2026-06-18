// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared BigTable resolution layer: turns streams (or bounded sets) of
//! checkpoint/transaction identifiers into fully-resolved entities — their
//! transactions and objects — using the chunked, concurrency-limited pipeline
//! primitives. Both the v2 point-get handlers and the v2alpha list handlers
//! build on these so request-size chunking, stage overlap, and object
//! deduplication are identical across them.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream;
use futures::stream::BoxStream;
use sui_kvstore::tables;
use sui_kvstore::{CheckpointData, TransactionData};
use sui_rpc::field::FieldMaskTree;
use sui_rpc::proto::sui::rpc::v2::{Checkpoint, ExecutedTransaction, TransactionEffects};
use sui_rpc_api::RpcError;
use sui_types::digests::TransactionDigest;
use sui_types::storage::ObjectKey;

use crate::bigtable_client::BigTableClient;
use crate::config::ResolvedStageConfig;
use crate::object_cache::{BigTableObjectFetcher, ObjectCache, ObjectMap};
use crate::pipeline::{InputOrderEmitter, Watermarked, pipelined_chunks, pipelined_keyed_batches};

pub(crate) type CpWithTxs = (u64, CheckpointData, Vec<TransactionData>);
pub(crate) type ResolvedCp = (u64, CheckpointData, Vec<TransactionData>, ObjectMap);

/// Attach a per-item `ObjectMap` to each item in `upstream`. When
/// `needs_objects` is false every item is paired with an empty map (no
/// BigTable read); otherwise object keys are derived per item via `keys_of`
/// and fetched through `pipelined_keyed_batches` — chunk-bounded, concurrent,
/// and deduplicated by a request-scoped `ObjectCache`, overlapping with the
/// still-arriving upstream. The cache is owned by the returned stream, so its
/// in-flight dispatches abort if the consumer drops the stream.
pub(crate) fn with_object_maps<I>(
    upstream: BoxStream<'static, Result<Watermarked<I>, anyhow::Error>>,
    client: BigTableClient,
    objects_stage: ResolvedStageConfig,
    needs_objects: bool,
    keys_of: impl Fn(&I) -> Vec<ObjectKey> + Send + 'static,
) -> BoxStream<'static, Result<Watermarked<(I, ObjectMap)>, anyhow::Error>>
where
    I: Send + 'static,
{
    if needs_objects {
        let object_cache = ObjectCache::new(Arc::new(BigTableObjectFetcher::new(client)));
        let with_keys = upstream
            .map_ok(move |m| {
                m.map_item(|item| {
                    let keys = keys_of(&item);
                    (item, keys)
                })
            })
            .boxed();
        pipelined_keyed_batches(
            with_keys,
            objects_stage.chunk_size,
            objects_stage.chunk_size,
            objects_stage.concurrency,
            move |keys| {
                let object_cache = object_cache.clone();
                async move {
                    object_cache
                        .get_many(keys)
                        .await
                        .map_err(anyhow::Error::new)
                }
            },
        )
        .boxed()
    } else {
        let empty: ObjectMap = Arc::new(HashMap::new());
        upstream
            .map_ok(move |m| {
                let empty = empty.clone();
                m.map_item(move |item| (item, empty.clone()))
            })
            .boxed()
    }
}

/// Resolve a stream of `(cp_seq, CheckpointData)` into
/// `(cp_seq, cp_data, txs, objects)`. Stage C fetches each chunk's
/// transactions; stage D attaches their objects. Shared by `get_checkpoint`
/// (single lookup) and the list-checkpoints handler (range scan).
pub(crate) fn resolve_checkpoints(
    client: BigTableClient,
    read_mask: &FieldMaskTree,
    transactions_stage: ResolvedStageConfig,
    objects_stage: ResolvedStageConfig,
    cp_data_stream: BoxStream<'static, Result<Watermarked<(u64, CheckpointData)>, anyhow::Error>>,
) -> BoxStream<'static, Result<Watermarked<ResolvedCp>, anyhow::Error>> {
    let tx_columns: Arc<[&'static str]> = list_transactions_columns(read_mask).into();
    let needs_objects = read_mask.contains(Checkpoint::OBJECTS_FIELD);

    // Stage C: (cp_seq, CheckpointData) -> + Vec<TransactionData>. Batched
    // across the chunk: gather ALL tx_digests across all cps in the chunk into
    // one multi_get, route results back per-cp, then emit in input cp_seq order
    // after the chunk drains.
    let cp_with_txs_stream = pipelined_chunks(
        cp_data_stream,
        transactions_stage.chunk_size,
        transactions_stage.concurrency,
        {
            let client = client.clone();
            let columns = tx_columns.clone();
            move |items| fetch_transactions_for_cps(client.clone(), columns.clone(), items)
        },
    );

    // Stage D: each cp must see only its own object keys, since
    // `render_full_checkpoint` folds the whole map into an `ObjectSet`.
    with_object_maps(
        cp_with_txs_stream,
        client,
        objects_stage,
        needs_objects,
        |(_, _, txs): &CpWithTxs| {
            txs.iter()
                .flat_map(compute_object_keys)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        },
    )
    .map_ok(|m| m.map_item(|((cp_seq, cp_data, txs), objects)| (cp_seq, cp_data, txs, objects)))
    .boxed()
}

/// Resolve a single already-fetched checkpoint (the point-get path): drive
/// `resolve_checkpoints` over a one-item stream and return the sole result.
/// `resolve_checkpoints` emits exactly one item per input checkpoint (empty
/// checkpoints are pre-emitted) or propagates a fetch error, so the stream is
/// never empty — hence no "missing result" failure mode.
pub(crate) async fn resolve_checkpoint(
    client: BigTableClient,
    read_mask: &FieldMaskTree,
    transactions_stage: ResolvedStageConfig,
    objects_stage: ResolvedStageConfig,
    cp_seq: u64,
    cp_data: CheckpointData,
) -> Result<ResolvedCp, RpcError> {
    let cp_stream =
        stream::once(async move { Ok::<_, anyhow::Error>(Watermarked::Item((cp_seq, cp_data))) })
            .boxed();
    let stream = resolve_checkpoints(
        client,
        read_mask,
        transactions_stage,
        objects_stage,
        cp_stream,
    );
    futures::pin_mut!(stream);
    while let Some(item) = stream.next().await {
        if let Watermarked::Item(resolved) = item.map_err(RpcError::from)? {
            return Ok(resolved);
        }
    }
    unreachable!("resolve_checkpoints emits exactly one item per input checkpoint")
}

/// Resolve `digests` into their transactions and (when the mask needs object
/// types) objects, keyed by digest for request-order reconstruction. Tx rows
/// stream in by chunk and their objects fetch concurrently, overlapping the tx
/// fetch. An empty `digests` issues no BigTable read.
pub(crate) async fn resolve_transactions(
    client: BigTableClient,
    digests: Vec<TransactionDigest>,
    read_mask: &FieldMaskTree,
    transactions_stage: ResolvedStageConfig,
    objects_stage: ResolvedStageConfig,
) -> Result<HashMap<TransactionDigest, (TransactionData, ObjectMap)>, RpcError> {
    if digests.is_empty() {
        return Ok(HashMap::new());
    }
    let columns: Arc<[&'static str]> = transaction_columns(read_mask).into();
    let needs_objects = needs_object_types(read_mask);

    let digest_stream = stream::iter(
        digests
            .into_iter()
            .map(|d| Ok::<_, anyhow::Error>(Watermarked::Item(d))),
    )
    .boxed();

    // Stage A: digests -> (digest, TransactionData), chunked & concurrent.
    let tx_stream = pipelined_chunks(
        digest_stream,
        transactions_stage.chunk_size,
        transactions_stage.concurrency,
        {
            let client = client.clone();
            let columns = columns.clone();
            move |chunk| fetch_transaction_rows(client.clone(), columns.clone(), chunk)
        },
    );

    // Stage B: attach each tx's own objects.
    let with_objects = with_object_maps(
        tx_stream,
        client,
        objects_stage,
        needs_objects,
        |(_, tx): &(TransactionDigest, TransactionData)| {
            compute_object_keys(tx).into_iter().collect()
        },
    );
    futures::pin_mut!(with_objects);

    let mut out = HashMap::new();
    while let Some(item) = with_objects.next().await {
        if let Watermarked::Item(((digest, tx), objects)) = item.map_err(RpcError::from)? {
            out.insert(digest, (tx, objects));
        }
    }
    Ok(out)
}

/// Stage-A/C fetch: fetch one chunk of transaction rows as a stream. An empty
/// chunk issues no read (see the empty-RowSet full-scan trap guarded in
/// `sui-kvstore`).
async fn fetch_transaction_rows(
    client: BigTableClient,
    columns: Arc<[&'static str]>,
    digests: Vec<TransactionDigest>,
) -> Result<
    BoxStream<'static, Result<(TransactionDigest, TransactionData), anyhow::Error>>,
    anyhow::Error,
> {
    if digests.is_empty() {
        return Ok(stream::empty().boxed());
    }
    let column_filter = BigTableClient::column_filter(&columns);
    Ok(client
        .get_transactions_stream(digests, Some(column_filter))
        .await?
        .boxed())
}

/// Fetch the transactions for a chunk of checkpoints in a single batched
/// multi_get, routing rows back per-cp and emitting in input cp_seq order once
/// each cp's full set has arrived.
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

    // CRITICAL: never call `get_transactions_stream` with an empty digest list
    // (an empty `RowSet` is read as a full transactions-table scan). When every
    // cp in this chunk is empty, fall back to an empty stream and let the
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
                "resolve_checkpoints: checkpoint transaction lookup",
            )? {
                yield v;
            }
        }
        futures::pin_mut!(tx_stream);
        while let Some(row) = tx_stream.next().await {
            let (digest, tx) = row?;
            let cp_seq = digest_to_cp.remove(&digest).ok_or_else(|| {
                anyhow::anyhow!("resolve_checkpoints: unexpected transaction body row {digest}")
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
                    "resolve_checkpoints: checkpoint transaction lookup",
                )? {
                    yield v;
                }
            }
        }
        // Defensive: if BigTable returned fewer rows than requested, surface
        // the missing digests as an internal error rather than emit a partial
        // checkpoint downstream.
        if !cp_data_by_seq.is_empty() {
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
                "resolve_checkpoints: BigTable returned fewer transactions than requested (cp_seq, got, expected)"
            );
            Err(RpcError::new(
                tonic::Code::Internal,
                format!(
                    "resolve_checkpoints: BigTable returned fewer transactions than requested for {} checkpoint(s)",
                    incomplete.len()
                ),
            ))?;
        }
        for v in emitter.finish("resolve_checkpoints: missing selected checkpoint transactions")? {
            yield v;
        }
    }
    .boxed())
}

// --- Read-mask -> column / fetch planning ---

/// Whether the checkpoint read mask requests transactions or objects (the heavy
/// path that needs `resolve_checkpoints`).
pub(crate) fn needs_transactions_or_objects(mask: &FieldMaskTree) -> bool {
    mask.contains(Checkpoint::TRANSACTIONS_FIELD) || mask.contains(Checkpoint::OBJECTS_FIELD)
}

/// Whether a transaction read mask needs object types resolved (i.e. requests
/// `effects.changed_objects` or `effects.unchanged_consensus_objects`).
pub(crate) fn needs_object_types(mask: &FieldMaskTree) -> bool {
    mask.subtree(ExecutedTransaction::EFFECTS_FIELD.name)
        .is_some_and(|submask| {
            submask.contains(TransactionEffects::CHANGED_OBJECTS_FIELD.name)
                || submask.contains(TransactionEffects::UNCHANGED_CONSENSUS_OBJECTS_FIELD.name)
        })
}

/// Object keys (id, version) referenced by a transaction's inputs/effects —
/// i.e. the objects to fetch to annotate its `changed_objects` types.
pub(crate) fn compute_object_keys(source: &TransactionData) -> BTreeSet<ObjectKey> {
    match (&source.transaction_data, &source.effects) {
        (Some(tx_data), Some(effects)) => sui_types::storage::get_transaction_object_set(
            tx_data,
            effects,
            &source.unchanged_loaded_runtime_objects,
        ),
        _ => BTreeSet::new(),
    }
}

/// Checkpoint-table columns needed for `mask`. Always includes `s` (summary);
/// adds `sg` (signatures) and `c` (contents) when requested.
pub(crate) fn checkpoint_columns(mask: &FieldMaskTree) -> Vec<&'static str> {
    use tables::checkpoints::col;
    let mut columns = vec![col::SUMMARY];
    if mask.contains(Checkpoint::SIGNATURE_FIELD) {
        columns.push(col::SIGNATURES);
    }
    if mask.contains(Checkpoint::CONTENTS_FIELD) {
        columns.push(col::CONTENTS);
    }
    columns
}

/// Checkpoint-table columns for the list/get checkpoint handlers. The heavy
/// (transactions/objects) path additionally needs `signatures` (to reconstruct
/// `CertifiedCheckpointSummary`) and `contents` (to enumerate tx digests).
pub(crate) fn list_checkpoint_columns(mask: &FieldMaskTree, needs_full: bool) -> Vec<&'static str> {
    use tables::checkpoints::col;
    let mut columns = checkpoint_columns(mask);
    if needs_full {
        if !columns.contains(&col::CONTENTS) {
            columns.push(col::CONTENTS);
        }
        if !columns.contains(&col::SIGNATURES) {
            columns.push(col::SIGNATURES);
        }
    }
    columns
}

/// Transaction-table columns needed for `mask`. Always includes `cn`/`ts`
/// (small metadata); adds `td`, `sg`, `ef`, `ev`, `bc`, `ul` as the mask
/// requires (and `td`/`ul` when object types must be resolved).
pub(crate) fn transaction_columns(mask: &FieldMaskTree) -> Vec<&'static str> {
    use tables::transactions::col;
    let mut columns = vec![col::CHECKPOINT_NUMBER, col::TIMESTAMP];

    if mask
        .subtree(ExecutedTransaction::TRANSACTION_FIELD.name)
        .is_some()
    {
        columns.push(col::DATA);
    }
    if mask
        .subtree(ExecutedTransaction::SIGNATURES_FIELD.name)
        .is_some()
    {
        columns.push(col::SIGNATURES);
    }
    if let Some(effects_submask) = mask.subtree(ExecutedTransaction::EFFECTS_FIELD.name) {
        columns.push(col::EFFECTS);
        let needs_objects = needs_object_types(mask);
        if effects_submask.contains(TransactionEffects::UNCHANGED_LOADED_RUNTIME_OBJECTS_FIELD.name)
            || needs_objects
        {
            columns.push(col::UNCHANGED_LOADED);
        }
        if needs_objects {
            columns.push(col::DATA);
        }
    }
    if mask
        .subtree(ExecutedTransaction::EVENTS_FIELD.name)
        .is_some()
    {
        columns.push(col::EVENTS);
    }
    if mask.contains(ExecutedTransaction::BALANCE_CHANGES_FIELD.name) {
        columns.push(col::BALANCE_CHANGES);
    }

    columns
}

/// Transaction-table columns needed to resolve a checkpoint's transactions. The
/// merge target (`full_checkpoint_content::ExecutedTransaction`) has non-Option
/// `transaction`/`effects`, so `td`+`ef` are always fetched; object resolution
/// also needs `ul`.
pub(crate) fn list_transactions_columns(mask: &FieldMaskTree) -> Vec<&'static str> {
    use tables::transactions::col;
    let mut columns = if let Some(submask) = mask.subtree(Checkpoint::TRANSACTIONS_FIELD.name) {
        transaction_columns(&submask)
    } else {
        // Baseline metadata columns even if the proto `transactions` field
        // isn't requested; we still need the rows to compute object keys.
        vec![col::CHECKPOINT_NUMBER, col::TIMESTAMP]
    };
    // Required to construct the merge target faithfully.
    for c in [col::DATA, col::EFFECTS] {
        if !columns.contains(&c) {
            columns.push(c);
        }
    }
    if mask.contains(Checkpoint::OBJECTS_FIELD) && !columns.contains(&col::UNCHANGED_LOADED) {
        columns.push(col::UNCHANGED_LOADED);
    }
    columns
}
