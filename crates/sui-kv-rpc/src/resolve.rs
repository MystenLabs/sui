// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared BigTable resolution layer: turns streams (or bounded sets) of
//! checkpoint/transaction identifiers into fully-resolved entities — their
//! transactions and objects — using the burst-planned, concurrency-limited
//! pipeline primitives. Both the v2 point-get handlers and the list handlers
//! build on these so read sizing, stage overlap, and object deduplication are
//! identical across them.

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
use sui_types::object::Object;
use sui_types::storage::ObjectKey;

use crate::bigtable_client::BigTableClient;
use crate::config::ResolvedStageConfig;
use crate::object_cache::{BigTableObjectFetcher, ObjectMap};
use crate::pipeline::{
    InputOrderEmitter, SeekUnit, Watermarked, pipelined_chunks, pipelined_keyed_batches,
};
use crate::request_cache::CacheFetcher;

pub(crate) type ResolvedCp = (u64, CheckpointData, Vec<TransactionData>, ObjectMap);

/// One transaction of a checkpoint, as `resolve_checkpoints` reads it: each
/// checkpoint is flattened into one item per transaction digest, in contents
/// order.
struct CheckpointTransaction {
    cp_seq: u64,
    /// `None` only for the single item of a checkpoint without transactions.
    digest: Option<TransactionDigest>,
    /// The checkpoint itself, carried by its last item only, so that item
    /// closes the checkpoint when the items are regrouped.
    checkpoint: Option<CheckpointData>,
}

/// A [`CheckpointTransaction`] with its `transactions` row (`None` exactly
/// when the item has no digest).
type CheckpointTransactionRow = (CheckpointTransaction, Option<TransactionData>);

/// Context of the error raised when a read of `resolve_checkpoints` returns
/// fewer `transactions` rows than it requested.
const MISSING_CHECKPOINT_TRANSACTIONS: &str =
    "resolve_checkpoints: BigTable returned fewer transactions than requested";

/// Context of the errors raised when a read of `resolve_checkpoints` returns a
/// `transactions` row it did not request, or one row twice.
const CHECKPOINT_TRANSACTION_ROW: &str = "resolve_checkpoints: transaction row";

/// Attach a per-item `ObjectMap` to each item in `upstream`. When
/// `needs_objects` is false every item is paired with an empty map (no
/// BigTable read); otherwise object keys are derived per item via `keys_of`
/// and read through `pipelined_keyed_batches`: each object key still to read
/// in a burst of ready items is one seek, and the burst is planned into reads
/// of at most `max_seeks_per_read` keys, split over the stage's free
/// `concurrency` slots when `min_seeks_per_split_read` is set, so one item's
/// objects may be divided between reads. Reads overlap with
/// the still-arriving upstream, and an object the request has already read, or
/// is reading, is not read again. The request-scoped cache is owned by the
/// returned stream, so its in-flight reads abort if the consumer drops the
/// stream.
pub(crate) fn with_object_maps<I, E>(
    upstream: BoxStream<'static, Result<Watermarked<I>, E>>,
    client: BigTableClient,
    objects_stage: ResolvedStageConfig,
    needs_objects: bool,
    keys_of: impl Fn(&I) -> Vec<ObjectKey> + Send + 'static,
) -> BoxStream<'static, Result<Watermarked<(I, ObjectMap)>, E>>
where
    I: Send + 'static,
    E: From<anyhow::Error> + Send + 'static,
{
    if needs_objects {
        let with_keys = upstream
            .map_ok(move |m| {
                m.map_item(|item| {
                    let keys = keys_of(&item);
                    (item, keys)
                })
            })
            .boxed();
        let fetcher: Arc<dyn CacheFetcher<ObjectKey, Object>> =
            Arc::new(BigTableObjectFetcher::new(client));
        pipelined_keyed_batches(
            with_keys,
            objects_stage.max_seeks_per_read,
            objects_stage.concurrency,
            objects_stage.min_seeks_per_split_read,
            fetcher,
        )
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
/// `(cp_seq, cp_data, txs, objects)`, exactly one item per input checkpoint,
/// in input order, with watermarks in place. Shared by `get_checkpoint` and
/// `list_checkpoints`.
///
/// Reads work per transaction; only rendering needs the whole checkpoint.
/// Each checkpoint is flattened into one item per transaction digest, so
/// stage C plans a checkpoint's digests like any `transactions` read
/// (`BatchGetTransactions`): split over the stage's free `concurrency` slots,
/// at most `max_seeks_per_read` per read. Stage C emits transactions in input order,
/// and stage D attaches each transaction's objects, so a transaction's object
/// read can start once its row and every earlier transaction's row have
/// arrived, without waiting for the rest of its checkpoint. The transactions
/// are then regrouped per checkpoint.
pub(crate) fn resolve_checkpoints<E>(
    client: BigTableClient,
    read_mask: &FieldMaskTree,
    transactions_stage: ResolvedStageConfig,
    objects_stage: ResolvedStageConfig,
    cp_data_stream: BoxStream<'static, Result<Watermarked<(u64, CheckpointData)>, E>>,
) -> BoxStream<'static, Result<Watermarked<ResolvedCp>, E>>
where
    E: From<anyhow::Error> + Send + 'static,
{
    let columns: Arc<[&'static str]> = list_transactions_columns(read_mask).into();
    let needs_objects = read_mask.contains(Checkpoint::OBJECTS_FIELD);

    // Stage C: one item per transaction -> + its `transactions` row, read in
    // chunks of digests and emitted in input order.
    let transaction_stream = pipelined_chunks(
        flatten_checkpoints(cp_data_stream),
        transactions_stage.max_seeks_per_read,
        transactions_stage.concurrency,
        transactions_stage.min_seeks_per_split_read,
        SeekUnit::Item,
        {
            let client = client.clone();
            move |items: Vec<CheckpointTransaction>| {
                let fetch = fetch_checkpoint_transactions(client.clone(), columns.clone(), items);
                async move {
                    let rows = fetch.await.map_err(E::from)?;
                    Ok::<_, E>(rows.map(|row| row.map_err(E::from)).boxed())
                }
            }
        },
    );

    // Stage D: each transaction's own objects.
    let with_objects = with_object_maps(
        transaction_stream,
        client,
        objects_stage,
        needs_objects,
        |(_, transaction): &CheckpointTransactionRow| {
            transaction.as_ref().map_or_else(Vec::new, |transaction| {
                compute_object_keys(transaction).into_iter().collect()
            })
        },
    );

    regroup_checkpoints(with_objects)
}

/// Flatten each checkpoint into one [`CheckpointTransaction`] per transaction
/// digest of its contents, in contents order; the last item carries the
/// checkpoint. A checkpoint without transactions becomes one item with no
/// digest. Watermarks pass through unchanged.
fn flatten_checkpoints<E>(
    checkpoints: BoxStream<'static, Result<Watermarked<(u64, CheckpointData)>, E>>,
) -> BoxStream<'static, Result<Watermarked<CheckpointTransaction>, E>>
where
    E: From<anyhow::Error> + Send + 'static,
{
    async_stream::try_stream! {
        let mut checkpoints = checkpoints;
        while let Some(frame) = checkpoints.next().await {
            match frame? {
                Watermarked::Item((cp_seq, cp_data)) => {
                    let mut digests = cp_data
                        .contents
                        .as_ref()
                        .ok_or_else(|| {
                            E::from(anyhow::anyhow!(
                                "checkpoint {cp_seq} contents column missing"
                            ))
                        })?
                        .iter()
                        .map(|d| d.transaction)
                        .collect::<Vec<TransactionDigest>>();
                    let last = digests.pop();
                    for digest in digests {
                        yield Watermarked::Item(CheckpointTransaction {
                            cp_seq,
                            digest: Some(digest),
                            checkpoint: None,
                        });
                    }
                    yield Watermarked::Item(CheckpointTransaction {
                        cp_seq,
                        digest: last,
                        checkpoint: Some(cp_data),
                    });
                }
                Watermarked::Watermark(p) => yield Watermarked::Watermark(p),
            }
        }
    }
    .boxed()
}

/// Stage-C fetch of `resolve_checkpoints`: read the `transactions` rows of
/// the items that have a digest and emit every item with its row in input
/// order. Rows arrive in BigTable key order, so they are reordered by item
/// index; items without a digest need no row. A row missing from the read is
/// an error.
async fn fetch_checkpoint_transactions(
    client: BigTableClient,
    columns: Arc<[&'static str]>,
    items: Vec<CheckpointTransaction>,
) -> Result<BoxStream<'static, Result<CheckpointTransactionRow, anyhow::Error>>, anyhow::Error> {
    let item_count = items.len();
    let mut digests = Vec::with_capacity(item_count);
    let mut pending_by_digest: HashMap<TransactionDigest, (usize, CheckpointTransaction)> =
        HashMap::with_capacity(item_count);
    let mut without_row = Vec::new();
    for (index, item) in items.into_iter().enumerate() {
        match item.digest {
            Some(digest) => {
                digests.push(digest);
                pending_by_digest.insert(digest, (index, item));
            }
            None => without_row.push((index, item)),
        }
    }
    let rows = fetch_transaction_rows(client, columns, digests).await?;
    Ok(async_stream::try_stream! {
        // Keyed by item index so the emitter releases items in input order.
        let mut emitter: InputOrderEmitter<usize, CheckpointTransactionRow> =
            InputOrderEmitter::new((0..item_count).collect());
        for (index, item) in without_row {
            for ready in emitter.push(index, (item, None), CHECKPOINT_TRANSACTION_ROW)? {
                yield ready;
            }
        }
        let mut rows = rows;
        while let Some(row) = rows.next().await {
            let (digest, transaction) = row?;
            let (index, item) = pending_by_digest.remove(&digest).ok_or_else(|| {
                anyhow::anyhow!("resolve_checkpoints: unexpected transaction row {digest}")
            })?;
            for ready in emitter.push(
                index,
                (item, Some(transaction)),
                CHECKPOINT_TRANSACTION_ROW,
            )? {
                yield ready;
            }
        }
        if !pending_by_digest.is_empty() {
            Err(missing_checkpoint_transactions(&pending_by_digest))?;
        }
        for ready in emitter.finish(MISSING_CHECKPOINT_TRANSACTIONS)? {
            yield ready;
        }
    }
    .boxed())
}

/// The error for a `resolve_checkpoints` read that returned fewer `transactions` rows than it
/// requested. Names the checkpoints whose rows are missing and up to four of the digests, so the
/// failing checkpoint can be found from the message alone.
fn missing_checkpoint_transactions(
    missing: &HashMap<TransactionDigest, (usize, CheckpointTransaction)>,
) -> anyhow::Error {
    let mut checkpoints: Vec<u64> = missing.values().map(|(_, item)| item.cp_seq).collect();
    checkpoints.sort_unstable();
    checkpoints.dedup();
    let digests: Vec<String> = missing.keys().take(4).map(ToString::to_string).collect();
    anyhow::anyhow!(
        "{MISSING_CHECKPOINT_TRANSACTIONS}: missing {} row(s) of checkpoint(s) {checkpoints:?}, \
         including {}",
        missing.len(),
        digests.join(", "),
    )
}

/// A checkpoint whose transactions [`regroup_checkpoints`] is gathering.
struct GatheredCheckpoint {
    cp_seq: u64,
    transactions: Vec<TransactionData>,
    objects: HashMap<ObjectKey, Object>,
}

/// Fold consecutive transaction items of one checkpoint back into
/// `(cp_seq, cp_data, txs, objects)`, emitted when the item carrying the
/// checkpoint arrives. Each checkpoint's map holds only its own transactions'
/// objects, since `render_full_checkpoint` folds the whole map into an
/// `ObjectSet`.
///
/// A watermark, another checkpoint's item, or the end of input while a
/// checkpoint is partly gathered means its flattening was broken: that is an
/// internal error and the checkpoint is never emitted.
fn regroup_checkpoints<E>(
    transactions: BoxStream<'static, Result<Watermarked<(CheckpointTransactionRow, ObjectMap)>, E>>,
) -> BoxStream<'static, Result<Watermarked<ResolvedCp>, E>>
where
    E: From<anyhow::Error> + Send + 'static,
{
    async_stream::try_stream! {
        let mut transactions = transactions;
        let mut gathering: Option<GatheredCheckpoint> = None;
        while let Some(frame) = transactions.next().await {
            match frame? {
                Watermarked::Item(((item, transaction), objects)) => {
                    let CheckpointTransaction { cp_seq, checkpoint, .. } = item;
                    let mut gathered = gathering.take().unwrap_or_else(|| GatheredCheckpoint {
                        cp_seq,
                        transactions: Vec::new(),
                        objects: HashMap::new(),
                    });
                    if gathered.cp_seq != cp_seq {
                        Err::<(), E>(partly_gathered_checkpoint(
                            gathered.cp_seq,
                            format!("a transaction of checkpoint {cp_seq} arrived"),
                        ))?;
                    }
                    if let Some(transaction) = transaction {
                        gathered.transactions.push(transaction);
                    }
                    // The per-transaction map is uniquely owned unless it is
                    // the shared empty map, so objects are moved, not cloned.
                    let objects =
                        Arc::try_unwrap(objects).unwrap_or_else(|shared| (*shared).clone());
                    if gathered.objects.is_empty() {
                        gathered.objects = objects;
                    } else {
                        gathered.objects.extend(objects);
                    }
                    match checkpoint {
                        Some(cp_data) => {
                            yield Watermarked::Item((
                                cp_seq,
                                cp_data,
                                gathered.transactions,
                                Arc::new(gathered.objects),
                            ));
                        }
                        None => gathering = Some(gathered),
                    }
                }
                Watermarked::Watermark(p) => {
                    if let Some(gathered) = &gathering {
                        Err::<(), E>(partly_gathered_checkpoint(
                            gathered.cp_seq,
                            "a watermark arrived",
                        ))?;
                    }
                    yield Watermarked::Watermark(p);
                }
            }
        }
        if let Some(gathered) = &gathering {
            Err::<(), E>(partly_gathered_checkpoint(gathered.cp_seq, "the stream ended"))?;
        }
    }
    .boxed()
}

/// The internal error of [`regroup_checkpoints`] when `cause` interrupts
/// checkpoint `cp_seq` before its last transaction.
fn partly_gathered_checkpoint<E: From<anyhow::Error>>(
    cp_seq: u64,
    cause: impl std::fmt::Display,
) -> E {
    E::from(anyhow::anyhow!(
        "resolve_checkpoints: {cause} while checkpoint {cp_seq} was partly gathered"
    ))
}

/// Resolve a single already-fetched checkpoint (the point-get path): drive
/// `resolve_checkpoints` over a one-item stream and return the sole result.
/// `resolve_checkpoints` emits exactly one item per input checkpoint (a
/// checkpoint without transactions reads none) or propagates a fetch error, so
/// the stream is never empty — hence no "missing result" failure mode.
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

/// Resolve `digests` into transactions and the object data needed to render
/// complete `ExecutedTransaction.objects` sets or annotate effects object
/// types, keyed by digest for request-order reconstruction. Transaction rows
/// stream in by chunk and their object reads overlap the transaction fetch. An
/// empty `digests` issues no BigTable read.
///
/// A transaction whose checkpoint is above `checkpoint_hi_inclusive` is left out
/// of the result before its objects are read: its `transactions` row can be
/// written before the `objects` rows it references, so reading them could fail.
pub(crate) async fn resolve_transactions(
    client: BigTableClient,
    digests: Vec<TransactionDigest>,
    checkpoint_hi_inclusive: u64,
    read_mask: &FieldMaskTree,
    transactions_stage: ResolvedStageConfig,
    objects_stage: ResolvedStageConfig,
) -> Result<HashMap<TransactionDigest, (TransactionData, ObjectMap)>, RpcError> {
    if digests.is_empty() {
        return Ok(HashMap::new());
    }
    let columns: Arc<[&'static str]> = transaction_columns(read_mask).into();
    let needs_objects = needs_transaction_objects(read_mask);

    let digest_stream = stream::iter(
        digests
            .into_iter()
            .map(|d| Ok::<_, anyhow::Error>(Watermarked::Item(d))),
    )
    .boxed();

    // Stage A: digests -> (digest, TransactionData), chunked & concurrent.
    let tx_stream = pipelined_chunks(
        digest_stream,
        transactions_stage.max_seeks_per_read,
        transactions_stage.concurrency,
        transactions_stage.min_seeks_per_split_read,
        SeekUnit::Item,
        {
            let client = client.clone();
            let columns = columns.clone();
            move |chunk| fetch_transaction_rows(client.clone(), columns.clone(), chunk)
        },
    )
    .try_filter(move |item| {
        std::future::ready(!matches!(
            item,
            Watermarked::Item((_, tx)) if tx.checkpoint_number > checkpoint_hi_inclusive
        ))
    })
    .boxed();

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

/// Fetch one chunk of transaction rows as a stream, in arrival order.
/// Used by stage A of `resolve_transactions` and stage C of
/// `resolve_checkpoints`. An empty chunk issues no read (see the empty-RowSet
/// full-scan trap guarded in `sui-kvstore`).
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

/// Whether a transaction read mask needs its canonical object set or object
/// types embedded in effects.
pub(crate) fn needs_transaction_objects(mask: &FieldMaskTree) -> bool {
    mask.contains(ExecutedTransaction::OBJECTS_FIELD.name) || needs_object_types(mask)
}

/// Canonical object keys referenced by a transaction's inputs and effects.
/// Callers use these objects for complete transaction object sets and effects
/// type annotation.
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
/// requires, including all object-data columns when complete transaction
/// objects or effects object types must be resolved.
pub(crate) fn transaction_columns(mask: &FieldMaskTree) -> Vec<&'static str> {
    transaction_columns_for_mask(mask, needs_transaction_objects(mask))
}

fn transaction_columns_for_mask(mask: &FieldMaskTree, needs_objects: bool) -> Vec<&'static str> {
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
        if effects_submask.contains(TransactionEffects::UNCHANGED_LOADED_RUNTIME_OBJECTS_FIELD.name)
        {
            columns.push(col::UNCHANGED_LOADED);
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
    if needs_objects {
        for column in [col::DATA, col::EFFECTS, col::UNCHANGED_LOADED] {
            if !columns.contains(&column) {
                columns.push(column);
            }
        }
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
        transaction_columns_for_mask(&submask, needs_object_types(&submask))
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

#[cfg(test)]
mod tests {
    use super::*;

    use bytes::Bytes;
    use sui_kvstore::BigTableClient as InnerBigTableClient;
    use sui_kvstore::testing::{MockBigtableServer, ReadRowsResponseOrder, insert_checkpoint_rows};
    use sui_rpc::field::{FieldMask, FieldMaskUtil};
    use sui_types::base_types::ExecutionDigests;
    use sui_types::digests::TransactionEffectsDigest;
    use sui_types::messages_checkpoint::CheckpointContents;

    use crate::bigtable_client::Metrics;
    use crate::v2::test_utils::{
        canonical_transaction_object_keys, two_transaction_object_checkpoint,
    };

    /// Transactions-stage `max_seeks_per_read` used by the resolver tests:
    /// the per-request batch size the pipeline hands to BigTable, deliberately
    /// far below the backend `MAX_TX_DIGESTS_PER_REQUEST` clamp.
    const TX_CHUNK_SIZE: usize = 100;

    /// Deterministic, unique 32-byte digest: `i` big-endian in the low 8 bytes.
    fn digest(i: u64) -> TransactionDigest {
        let mut bytes = [0u8; 32];
        bytes[24..32].copy_from_slice(&i.to_be_bytes());
        TransactionDigest::new(bytes)
    }

    /// A checkpoint whose `contents` enumerate `digests` in order. `summary`
    /// and `signatures` stay `None` — the resolver tests don't render.
    fn checkpoint_data(digests: &[TransactionDigest]) -> CheckpointData {
        let contents = CheckpointContents::new_with_digests_only_for_tests(
            digests
                .iter()
                .map(|d| ExecutionDigests::new(*d, TransactionEffectsDigest::ZERO)),
        );
        CheckpointData {
            summary: None,
            contents: Some(contents),
            signatures: None,
        }
    }

    /// Insert a minimal transaction row (just the small metadata columns) so
    /// `tables::transactions::decode` succeeds for `digest`.
    async fn insert_tx_row(mock: &MockBigtableServer, digest: TransactionDigest) {
        mock.insert_row(
            tables::transactions::NAME,
            tables::transactions::encode_key(&digest),
            [
                (
                    tables::transactions::col::CHECKPOINT_NUMBER,
                    Bytes::from(bcs::to_bytes(&0u64).unwrap()),
                ),
                (
                    tables::transactions::col::TIMESTAMP,
                    Bytes::from(bcs::to_bytes(&0u64).unwrap()),
                ),
            ],
        )
        .await;
    }

    /// Spawn a mock BigTable server and a request-scoped wrapper client wired
    /// to it. The returned `JoinHandle` keeps the server task owned by the test.
    async fn setup() -> (
        MockBigtableServer,
        BigTableClient,
        tokio::task::JoinHandle<()>,
    ) {
        let mock = MockBigtableServer::new();
        let (addr, handle) = mock.start().await.unwrap();
        let inner = InnerBigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .unwrap();
        let client = BigTableClient::new(
            inner,
            16,
            Metrics::for_testing().0,
            "test_resolve_checkpoints",
        );
        (mock, client, handle)
    }

    /// Transactions-stage config of the resolver tests: reads of at most
    /// `TX_CHUNK_SIZE` digests over `concurrency` slots.
    fn transactions_stage(
        concurrency: usize,
        min_seeks_per_split_read: Option<usize>,
    ) -> ResolvedStageConfig {
        ResolvedStageConfig {
            max_seeks_per_read: TX_CHUNK_SIZE,
            concurrency,
            min_seeks_per_split_read,
            max_keys_per_read: None,
        }
    }

    /// Drive the real `resolve_checkpoints` stream over `cps` and collect the
    /// emitted checkpoint items (dropping watermarks).
    async fn run_resolver(
        client: BigTableClient,
        read_mask: &FieldMaskTree,
        cps: Vec<(u64, CheckpointData)>,
    ) -> Result<Vec<ResolvedCp>, RpcError> {
        let frames = cps.into_iter().map(Watermarked::Item).collect();
        let out =
            run_resolver_frames(client, read_mask, transactions_stage(4, None), frames).await?;
        Ok(out
            .into_iter()
            .filter_map(|frame| match frame {
                Watermarked::Item(resolved) => Some(resolved),
                Watermarked::Watermark(_) => None,
            })
            .collect())
    }

    /// Drive the real `resolve_checkpoints` stream over `frames` with
    /// `tx_stage` and collect every emitted frame, watermarks included.
    async fn run_resolver_frames(
        client: BigTableClient,
        read_mask: &FieldMaskTree,
        tx_stage: ResolvedStageConfig,
        frames: Vec<Watermarked<(u64, CheckpointData)>>,
    ) -> Result<Vec<Watermarked<ResolvedCp>>, RpcError> {
        let obj_stage = ResolvedStageConfig {
            max_seeks_per_read: 100,
            concurrency: 1,
            min_seeks_per_split_read: None,
            max_keys_per_read: None,
        };
        let input = stream::iter(frames.into_iter().map(Ok::<_, RpcError>)).boxed();
        resolve_checkpoints(client, read_mask, tx_stage, obj_stage, input)
            .try_collect()
            .await
    }

    /// Recorded `ReadRows` calls scoped to the transactions table.
    async fn tx_read_calls(mock: &MockBigtableServer) -> Vec<sui_kvstore::testing::ReadRowsCall> {
        mock.read_rows_calls()
            .await
            .into_iter()
            .filter(|c| c.table == tables::transactions::NAME)
            .collect()
    }

    #[tokio::test]
    async fn resolve_checkpoints_splits_fat_checkpoint_transaction_reads() {
        let (mock, client, _handle) = setup().await;
        // One checkpoint with more transactions than the stage's `max_seeks_per_read`, so
        // the pipeline must split its digests across multiple capped BigTable
        // requests (the objects stage batches the same way). The backend
        // `MAX_TX_DIGESTS_PER_REQUEST` clamp is a separate safety net covered by
        // the `sui-kvstore` `get_transactions_stream_*` tests.
        let n = TX_CHUNK_SIZE * 2 + 1;
        let digests: Vec<_> = (0..n as u64).map(digest).collect();
        for d in &digests {
            insert_tx_row(&mock, *d).await;
        }
        let read_mask = FieldMaskTree::from(FieldMask::from_paths(["transactions.digest"]));
        let out = run_resolver(client, &read_mask, vec![(0, checkpoint_data(&digests))])
            .await
            .unwrap();

        assert_eq!(out.len(), 1, "exactly one checkpoint item");
        let (_, _, txs, _) = &out[0];
        assert_eq!(txs.len(), n, "all transactions returned");

        let calls = tx_read_calls(&mock).await;
        assert!(
            calls.len() >= 2,
            "fat checkpoint should split into >=2 ReadRows calls, got {}",
            calls.len()
        );
        for c in &calls {
            assert!(
                c.row_keys.len() <= TX_CHUNK_SIZE,
                "ReadRows call exceeded max_seeks_per_read: {} keys",
                c.row_keys.len()
            );
        }
        let total: usize = calls.iter().map(|c| c.row_keys.len()).sum();
        assert_eq!(total, n, "every digest fetched exactly once");
    }

    #[tokio::test]
    async fn resolve_checkpoints_reconstructs_transactions_in_contents_order() {
        let (mock, client, _handle) = setup().await;
        let digests: Vec<_> = (0..3).map(digest).collect();
        for d in &digests {
            insert_tx_row(&mock, *d).await;
        }
        // Backend arrival order is deliberately the reverse of the request,
        // so a caller that emits in arrival order would fail this assertion.
        mock.set_read_rows_response_order(ReadRowsResponseOrder::ReverseRequestOrder)
            .await;
        let read_mask = FieldMaskTree::from(FieldMask::from_paths(["transactions.digest"]));
        let out = run_resolver(client, &read_mask, vec![(10, checkpoint_data(&digests))])
            .await
            .unwrap();

        assert_eq!(out.len(), 1);
        let (_, _, txs, _) = &out[0];
        let got: Vec<_> = txs.iter().map(|tx| tx.digest).collect();
        assert_eq!(
            got, digests,
            "transactions must follow checkpoint contents order"
        );
    }

    #[tokio::test]
    async fn resolve_checkpoints_empty_checkpoint_skips_transaction_read() {
        let (mock, client, _handle) = setup().await;
        let read_mask = FieldMaskTree::from(FieldMask::from_paths(["transactions.digest"]));
        let out = run_resolver(client, &read_mask, vec![(5, checkpoint_data(&[]))])
            .await
            .unwrap();

        assert_eq!(out.len(), 1, "empty checkpoint still emits one item");
        let (_, _, txs, _) = &out[0];
        assert!(txs.is_empty(), "empty checkpoint has no transactions");
        assert!(
            tx_read_calls(&mock).await.is_empty(),
            "empty checkpoint must not issue a transactions ReadRows"
        );
    }

    #[tokio::test]
    async fn resolve_checkpoints_missing_transaction_row_errors_internal() {
        let (mock, client, _handle) = setup().await;
        let digests: Vec<_> = (0..2).map(digest).collect();
        insert_tx_row(&mock, digests[0]).await; // omit digests[1]
        let read_mask = FieldMaskTree::from(FieldMask::from_paths(["transactions.digest"]));
        let err = run_resolver(client, &read_mask, vec![(7, checkpoint_data(&digests))])
            .await
            .unwrap_err();

        let status = tonic::Status::from(err);
        assert_eq!(status.code(), tonic::Code::Internal);
        let message = status.message();
        assert!(
            message.contains(&format!(
                "resolve_checkpoints: BigTable returned fewer transactions than requested: \
                 missing 1 row(s) of checkpoint(s) [7], including {}",
                digests[1]
            )),
            "unexpected message: {message}"
        );
    }

    #[tokio::test]
    async fn resolve_checkpoints_spreads_checkpoint_transactions_over_free_slots() {
        let (mock, client, _handle) = setup().await;
        // A checkpoint's digests are planned like any `transactions` read: with
        // four free slots and `min_seeks_per_split_read` 2, its eight digests go out as
        // four parallel reads of two.
        let digests: Vec<_> = (0..8).map(digest).collect();
        for d in &digests {
            insert_tx_row(&mock, *d).await;
        }
        let read_mask = FieldMaskTree::from(FieldMask::from_paths(["transactions.digest"]));
        let out = run_resolver_frames(
            client,
            &read_mask,
            transactions_stage(4, Some(2)),
            vec![Watermarked::Item((0, checkpoint_data(&digests)))],
        )
        .await
        .unwrap();

        let [Watermarked::Item((_, _, txs, _))] = out.as_slice() else {
            panic!(
                "expected exactly one checkpoint item, got {} frames",
                out.len()
            );
        };
        let got: Vec<_> = txs.iter().map(|tx| tx.digest).collect();
        assert_eq!(
            got, digests,
            "transactions must follow checkpoint contents order"
        );
        let read_sizes: Vec<usize> = tx_read_calls(&mock)
            .await
            .iter()
            .map(|c| c.row_keys.len())
            .collect();
        assert_eq!(read_sizes, vec![2; 4], "one read of two digests per slot");
    }

    #[tokio::test]
    async fn resolve_checkpoints_emits_one_item_per_checkpoint_around_watermarks() {
        let (mock, client, _handle) = setup().await;
        let digests: Vec<_> = (0..5).map(digest).collect();
        for d in &digests {
            insert_tx_row(&mock, *d).await;
        }
        mock.set_read_rows_response_order(ReadRowsResponseOrder::ReverseRequestOrder)
            .await;
        let read_mask = FieldMaskTree::from(FieldMask::from_paths(["transactions.digest"]));
        // `min_seeks_per_split_read` 1 splits each checkpoint's transactions over
        // separate reads, so every checkpoint is regrouped from several reads.
        let frames = vec![
            Watermarked::Item((1, checkpoint_data(&digests[..2]))),
            Watermarked::Watermark(1),
            Watermarked::Item((2, checkpoint_data(&[]))),
            Watermarked::Item((3, checkpoint_data(&digests[2..]))),
            Watermarked::Watermark(3),
        ];
        let out = run_resolver_frames(client, &read_mask, transactions_stage(4, Some(1)), frames)
            .await
            .unwrap();

        let got: Vec<Watermarked<(u64, Vec<TransactionDigest>)>> = out
            .into_iter()
            .map(|frame| {
                frame.map_item(|(cp_seq, _, txs, _)| {
                    (cp_seq, txs.iter().map(|tx| tx.digest).collect())
                })
            })
            .collect();
        assert_eq!(
            got,
            vec![
                Watermarked::Item((1, digests[..2].to_vec())),
                Watermarked::Watermark(1),
                Watermarked::Item((2, Vec::new())),
                Watermarked::Item((3, digests[2..].to_vec())),
                Watermarked::Watermark(3),
            ]
        );
    }

    #[tokio::test]
    async fn resolve_checkpoints_merges_each_transactions_objects_per_checkpoint() {
        let (mock, client, _handle) = setup().await;
        let checkpoint = two_transaction_object_checkpoint();
        insert_checkpoint_rows(&mock, &checkpoint).await;
        let cp_data = CheckpointData {
            summary: None,
            contents: Some(checkpoint.contents.clone()),
            signatures: None,
        };
        let read_mask = FieldMaskTree::from(FieldMask::from_paths(["objects"]));
        let out = run_resolver(client, &read_mask, vec![(0, cp_data)])
            .await
            .unwrap();

        assert_eq!(out.len(), 1, "exactly one checkpoint item");
        let (_, _, txs, objects) = &out[0];
        assert_eq!(txs.len(), 2);
        let first: BTreeSet<ObjectKey> = canonical_transaction_object_keys(&checkpoint, 0)
            .into_iter()
            .collect();
        let second: BTreeSet<ObjectKey> = canonical_transaction_object_keys(&checkpoint, 1)
            .into_iter()
            .collect();
        assert!(
            !first.is_subset(&second) && !second.is_subset(&first),
            "each transaction must reference objects the other does not"
        );
        let expected: BTreeSet<ObjectKey> = first.union(&second).copied().collect();
        let got: BTreeSet<ObjectKey> = objects.keys().copied().collect();
        assert_eq!(
            got, expected,
            "the checkpoint's map holds both transactions' objects and nothing else"
        );
    }

    /// A synthetic stage-D frame for `regroup_checkpoints`: a transaction of
    /// checkpoint `cp_seq` without row or objects; `last` makes it carry the
    /// checkpoint.
    fn transaction_frame(
        cp_seq: u64,
        digest_index: u64,
        last: bool,
    ) -> Result<Watermarked<(CheckpointTransactionRow, ObjectMap)>, RpcError> {
        let item = CheckpointTransaction {
            cp_seq,
            digest: Some(digest(digest_index)),
            checkpoint: last.then(|| checkpoint_data(&[])),
        };
        Ok(Watermarked::Item(((item, None), Arc::new(HashMap::new()))))
    }

    #[tokio::test]
    async fn regroup_checkpoints_rejects_partly_gathered_checkpoint() {
        // Checkpoint 4's first transaction arrives, then something other than
        // the rest of checkpoint 4.
        let interruptions = [
            ("watermark", Some(Ok(Watermarked::Watermark(4)))),
            ("other checkpoint", Some(transaction_frame(5, 1, true))),
            ("end of input", None),
        ];
        for (case, interruption) in interruptions {
            let frames = std::iter::once(transaction_frame(4, 0, false)).chain(interruption);
            let mut out: Vec<_> = regroup_checkpoints(stream::iter(frames).boxed())
                .collect()
                .await;

            assert_eq!(out.len(), 1, "{case}: only the error is emitted");
            let Some(Err(err)) = out.pop() else {
                panic!("{case}: expected an error");
            };
            let status = tonic::Status::from(err);
            assert_eq!(status.code(), tonic::Code::Internal, "{case}");
            assert!(
                status
                    .message()
                    .contains("checkpoint 4 was partly gathered"),
                "{case}: unexpected message: {}",
                status.message()
            );
        }
    }
}
