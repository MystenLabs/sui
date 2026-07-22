// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared BigTable resolution layer: turns streams (or bounded sets) of
//! checkpoint/transaction identifiers into fully-resolved entities — their
//! transactions and objects — using the chunked, concurrency-limited pipeline
//! primitives. Both the v2 point-get handlers and the list handlers
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
use crate::pipeline::{Watermarked, pipelined_chunks, pipelined_keyed_batches};

pub(crate) type CpWithTxs = (u64, CheckpointData, Vec<TransactionData>);
pub(crate) type ResolvedCp = (u64, CheckpointData, Vec<TransactionData>, ObjectMap);
/// Stage-C keyed-batch output: a checkpoint paired with a map of just its own
/// transaction rows (the `pipelined_keyed_batches` per-item map).
type CpTxMap = (
    (u64, CheckpointData),
    Arc<HashMap<TransactionDigest, TransactionData>>,
);

/// Attach a per-item `ObjectMap` to each item in `upstream`. When
/// `needs_objects` is false every item is paired with an empty map (no
/// BigTable read); otherwise object keys are derived per item via `keys_of`
/// and fetched through `pipelined_keyed_batches` — chunk-bounded, concurrent,
/// and deduplicated by a request-scoped `ObjectCache`, overlapping with the
/// still-arriving upstream. The cache is owned by the returned stream, so its
/// in-flight dispatches abort if the consumer drops the stream.
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
                        .map_err(|e| E::from(anyhow::Error::new(e)))
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
    let tx_columns: Arc<[&'static str]> = list_transactions_columns(read_mask).into();
    let needs_objects = read_mask.contains(Checkpoint::OBJECTS_FIELD);

    // Stage C: (cp_seq, CheckpointData) -> + Vec<TransactionData>. Derive each
    // checkpoint's transaction digests and fetch them through
    // `pipelined_keyed_batches` — chunk-bounded and concurrent, batching at
    // `chunk_size` keys per BigTable request.
    let with_keys = cp_data_stream
        .map(|res: Result<Watermarked<(u64, CheckpointData)>, E>| {
            let m = res?;
            Ok::<_, E>(match m {
                Watermarked::Item((cp_seq, cp_data)) => {
                    let keys = cp_data
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
                    Watermarked::Item(((cp_seq, cp_data), keys))
                }
                Watermarked::Watermark(p) => Watermarked::Watermark(p),
            })
        })
        .boxed();

    let cp_with_map_stream = pipelined_keyed_batches(
        with_keys,
        transactions_stage.chunk_size,
        transactions_stage.chunk_size,
        transactions_stage.concurrency,
        {
            let client = client.clone();
            let columns = tx_columns.clone();
            move |keys| {
                let client = client.clone();
                let columns = columns.clone();
                async move {
                    let requested = keys.len();
                    let stream = fetch_transaction_rows(client, columns, keys)
                        .await
                        .map_err(E::from)?;
                    futures::pin_mut!(stream);
                    let mut map: HashMap<TransactionDigest, TransactionData> = HashMap::new();
                    while let Some(row) = stream.next().await {
                        let (digest, tx) = row.map_err(E::from)?;
                        map.insert(digest, tx);
                    }
                    if map.len() != requested {
                        return Err(E::from(anyhow::anyhow!(
                            "resolve_checkpoints: BigTable returned fewer transactions than \
                             requested ({} of {} rows)",
                            map.len(),
                            requested
                        )));
                    }
                    Ok(map)
                }
            }
        },
    );

    let cp_with_txs_stream = cp_with_map_stream
        .map(|res: Result<Watermarked<CpTxMap>, E>| {
            let m = res?;
            Ok::<_, E>(match m {
                Watermarked::Item(((cp_seq, cp_data), tx_map)) => {
                    let contents = cp_data.contents.as_ref().ok_or_else(|| {
                        E::from(anyhow::anyhow!(
                            "checkpoint {cp_seq} contents column missing"
                        ))
                    })?;
                    // The per-item map is uniquely owned here — the reassembler
                    // just built it and handed it over — so move each body out
                    // in checkpoint-contents order rather than deep-cloning it.
                    // The `unwrap_or_else` clone is a correctness fallback for
                    // the (currently impossible) case of a shared map.
                    let mut tx_map = Arc::try_unwrap(tx_map).unwrap_or_else(|arc| (*arc).clone());
                    let mut txs: Vec<TransactionData> = Vec::with_capacity(contents.size());
                    for d in contents.iter() {
                        let digest = d.transaction;
                        let tx = tx_map.remove(&digest).ok_or_else(|| {
                            E::from(anyhow::anyhow!(
                                "resolve_checkpoints: BigTable returned fewer transactions \
                                 than requested (checkpoint {cp_seq} missing transaction {digest})"
                            ))
                        })?;
                        txs.push(tx);
                    }
                    Watermarked::Item((cp_seq, cp_data, txs))
                }
                Watermarked::Watermark(p) => Watermarked::Watermark(p),
            })
        })
        .boxed();

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

#[cfg(test)]
mod tests {
    use super::*;

    use bytes::Bytes;
    use sui_kvstore::BigTableClient as InnerBigTableClient;
    use sui_kvstore::testing::{MockBigtableServer, ReadRowsResponseOrder};
    use sui_rpc::field::{FieldMask, FieldMaskUtil};
    use sui_types::base_types::ExecutionDigests;
    use sui_types::digests::TransactionEffectsDigest;
    use sui_types::messages_checkpoint::CheckpointContents;

    use crate::bigtable_client::Metrics;

    /// Transactions-stage chunk size used by the resolver tests. Mirrors the
    /// production default (`DEFAULT_STAGE_CHUNK_SIZE`) and is the per-request
    /// batch size the pipeline hands to BigTable — deliberately far below the
    /// backend `MAX_TX_DIGESTS_PER_REQUEST` clamp.
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

    /// Drive the real `resolve_checkpoints` stream over `cps` and collect the
    /// emitted checkpoint items (dropping watermarks).
    async fn run_resolver(
        client: BigTableClient,
        read_mask: &FieldMaskTree,
        cps: Vec<(u64, CheckpointData)>,
    ) -> Result<Vec<ResolvedCp>, RpcError> {
        let tx_stage = ResolvedStageConfig {
            chunk_size: TX_CHUNK_SIZE,
            concurrency: 4,
        };
        let obj_stage = ResolvedStageConfig {
            chunk_size: 100,
            concurrency: 1,
        };
        let input = stream::iter(
            cps.into_iter()
                .map(|cp| Ok::<_, RpcError>(Watermarked::Item(cp))),
        )
        .boxed();
        let stream = resolve_checkpoints(client, read_mask, tx_stage, obj_stage, input);
        futures::pin_mut!(stream);
        let mut out = Vec::new();
        while let Some(item) = stream.next().await {
            if let Watermarked::Item(resolved) = item? {
                out.push(resolved);
            }
        }
        Ok(out)
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
        // One checkpoint with more transactions than the stage `chunk_size`, so
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
                "ReadRows call exceeded chunk_size: {} keys",
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
        assert!(
            status.message().contains(
                "resolve_checkpoints: BigTable returned fewer transactions than requested"
            ),
            "unexpected message: {}",
            status.message()
        );
    }
}
