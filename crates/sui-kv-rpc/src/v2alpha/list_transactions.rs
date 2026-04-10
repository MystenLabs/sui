// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::time::Instant;

use bytes::Bytes;
use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream::BoxStream;
use sui_kvstore::BigTableClient;
use sui_kvstore::KeyValueStoreReader;
use sui_kvstore::TransactionData;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_rpc_api::proto::google::rpc::bad_request::FieldViolation;
use sui_types::digests::TransactionDigest;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use tracing::info;

use super::filter::transaction_filter_to_query;
use crate::PackageResolver;
use crate::proto::sui::rpc::kv::v2alpha::ListTransactionsRequest;
use crate::proto::sui::rpc::kv::v2alpha::ListTransactionsResponse;
use crate::proto::sui::rpc::kv::v2alpha::TransactionResult;
use crate::v2::get_transaction::compute_object_keys;
use crate::v2::get_transaction::needs_object_types;
use crate::v2::get_transaction::transaction_columns;
use crate::v2::get_transaction::transaction_to_response;
use crate::v2::get_transaction::validate_read_mask;

const DEFAULT_PAGE_SIZE: u32 = 50;
const MAX_PAGE_SIZE: u32 = 1000;
/// Max rows per batched multi_get at any pipeline stage.
const CHUNK_MAX: usize = 64;
/// Max concurrent multi_gets in flight per pipeline stage.
const STAGE_CONCURRENCY: usize = 4;

pub(crate) async fn list_transactions(
    client: BigTableClient,
    request: ListTransactionsRequest,
    resolver: &PackageResolver,
) -> Result<ListTransactionsResponse, RpcError> {
    let total_t0 = Instant::now();
    let filtered = request.filter.is_some();

    let read_mask = validate_read_mask(request.read_mask)?;
    let page_size = request
        .page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE) as usize;

    let cursor = request
        .page_token
        .as_ref()
        .map(decode_tx_page_token)
        .transpose()?;

    let t0 = Instant::now();
    let tx_range = resolve_tx_range(
        &client,
        cursor,
        request.start_checkpoint,
        request.end_checkpoint,
    )
    .await?;
    let resolve_range_ms = t0.elapsed().as_millis();

    if tx_range.is_empty() {
        info!(
            filtered,
            page_size,
            resolve_range_ms,
            total_ms = total_t0.elapsed().as_millis(),
            "list_transactions: empty range"
        );
        return Ok(ListTransactionsResponse::default());
    }

    let columns = transaction_columns(&read_mask);
    let column_filter = BigTableClient::column_filter(&columns);
    let needs_objects = needs_object_types(&read_mask);

    let pipeline_t0 = Instant::now();

    // Stage 1: tx_seq stream (filtered or unfiltered).
    let seq_stream: BoxStream<'static, Result<u64, RpcError>> =
        if let Some(filter) = &request.filter {
            let query = transaction_filter_to_query(filter)?;
            client
                .eval_bitmap_query_stream(query, tx_range.clone())
                .take(page_size + 1)
                .map_err(RpcError::from)
                .boxed()
        } else {
            futures::stream::iter(tx_range.take(page_size + 1).map(Ok::<_, RpcError>)).boxed()
        };

    // Stage 2: seq chunks → streaming digest rows.
    // ready_chunks batches whatever is currently available (no added wait);
    // each chunk fires one multi_get_stream over tx_seq_digest, whose rows
    // flatten into a single stream via try_flatten_unordered so downstream
    // stages can start work before the full batch completes.
    let digest_rows: BoxStream<'static, Result<(u64, TransactionDigest, u64, u32), RpcError>> =
        seq_stream
            .ready_chunks(CHUNK_MAX)
            .map({
                let client = client.clone();
                move |chunk| {
                    let mut client = client.clone();
                    async move {
                        let seqs: Vec<u64> = chunk.into_iter().collect::<Result<_, _>>()?;
                        if seqs.is_empty() {
                            return Ok::<_, RpcError>(
                                futures::stream::empty::<
                                    Result<(u64, TransactionDigest, u64, u32), RpcError>,
                                >()
                                .boxed(),
                            );
                        }
                        let inner = client.resolve_tx_digests_stream(seqs).await?;
                        Ok(inner.map_err(RpcError::from).boxed())
                    }
                }
            })
            .buffer_unordered(STAGE_CONCURRENCY)
            .try_flatten_unordered(None)
            .boxed();

    // Stage 3: digest chunks → streaming tx body rows, re-zipped with (seq, cp_seq).
    let tx_rows: BoxStream<'static, Result<(u64, u64, TransactionData), RpcError>> = digest_rows
        .ready_chunks(CHUNK_MAX)
        .map({
            let client = client.clone();
            let column_filter = column_filter.clone();
            move |chunk| {
                let mut client = client.clone();
                let column_filter = column_filter.clone();
                async move {
                    let rows: Vec<(u64, TransactionDigest, u64, u32)> =
                        chunk.into_iter().collect::<Result<_, _>>()?;
                    if rows.is_empty() {
                        return Ok::<_, RpcError>(
                            futures::stream::empty::<
                                Result<(u64, u64, TransactionData), RpcError>,
                            >()
                            .boxed(),
                        );
                    }
                    let digests: Vec<TransactionDigest> =
                        rows.iter().map(|(_, d, _, _)| *d).collect();
                    let seq_map: HashMap<TransactionDigest, (u64, u64)> = rows
                        .into_iter()
                        .map(|(seq, d, cp, _)| (d, (seq, cp)))
                        .collect();
                    let inner = client
                        .get_transactions_stream(digests, Some(column_filter))
                        .await?;
                    Ok(inner
                        .map_err(RpcError::from)
                        .and_then(move |(digest, tx)| {
                            let entry = seq_map.get(&digest).copied();
                            async move {
                                let (seq, cp) = entry.ok_or_else(|| {
                                    RpcError::new(tonic::Code::Internal, "digest not in seq_map")
                                })?;
                                Ok((seq, cp, tx))
                            }
                        })
                        .boxed())
                }
            }
        })
        .buffer_unordered(STAGE_CONCURRENCY)
        .try_flatten_unordered(None)
        .boxed();

    // Stage 4: per-chunk object fetch. Each chunk accumulates its tx bodies'
    // object-key set and fires one streaming multi_get over `objects`. Dedup
    // is within-chunk only — at page_size ≤ 50 that's almost always one
    // chunk, so cross-chunk duplicates don't happen in practice.
    let rendered: Vec<(u64, u64, TransactionData, HashMap<ObjectKey, Object>)> = if needs_objects {
        tx_rows
            .ready_chunks(CHUNK_MAX)
            .map({
                let client = client.clone();
                move |chunk| {
                    let mut client = client.clone();
                    async move {
                        let txs: Vec<(u64, u64, TransactionData)> =
                            chunk.into_iter().collect::<Result<_, _>>()?;
                        if txs.is_empty() {
                            return Ok::<_, RpcError>(Vec::new());
                        }
                        let keys: Vec<ObjectKey> = txs
                            .iter()
                            .flat_map(|(_, _, tx)| compute_object_keys(tx))
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect();
                        let mut obj_map: HashMap<ObjectKey, Object> = HashMap::new();
                        if !keys.is_empty() {
                            let stream = client.get_objects_stream(keys).await?;
                            futures::pin_mut!(stream);
                            while let Some(obj) = stream.next().await {
                                let obj = obj.map_err(RpcError::from)?;
                                obj_map.insert(ObjectKey(obj.id(), obj.version()), obj);
                            }
                        }
                        Ok(txs
                            .into_iter()
                            .map(|(seq, cp, tx)| (seq, cp, tx, obj_map.clone()))
                            .collect::<Vec<_>>())
                    }
                }
            })
            .buffer_unordered(STAGE_CONCURRENCY)
            .try_fold(Vec::new(), |mut acc, v| async move {
                acc.extend(v);
                Ok(acc)
            })
            .await?
    } else {
        tx_rows
            .map_ok(|(seq, cp, tx)| (seq, cp, tx, HashMap::new()))
            .try_collect()
            .await?
    };

    let pipeline_ms = pipeline_t0.elapsed().as_millis();

    // Order restoration: pipeline produced items in completion order.
    let mut rendered = rendered;
    rendered.sort_by_key(|(seq, _, _, _)| *seq);
    let n_candidates = rendered.len();
    let has_next = rendered.len() > page_size;
    rendered.truncate(page_size);

    if rendered.is_empty() {
        info!(
            filtered,
            page_size,
            resolve_range_ms,
            pipeline_ms,
            total_ms = total_t0.elapsed().as_millis(),
            "list_transactions: empty page"
        );
        return Ok(ListTransactionsResponse::default());
    }

    let last_tx_seq = rendered.last().map(|(seq, _, _, _)| *seq);
    let n_page = rendered.len();
    let t0 = Instant::now();
    let mut transactions = Vec::with_capacity(rendered.len());
    for (tx_seq, checkpoint_seq, tx_data, objects) in rendered {
        let executed = transaction_to_response(tx_data, &read_mask, &objects, resolver).await?;
        transactions.push(TransactionResult {
            cursor: Some(encode_tx_page_token(tx_seq)),
            checkpoint: Some(checkpoint_seq),
            transaction: Some(executed),
            ..Default::default()
        });
    }
    let render_ms = t0.elapsed().as_millis();

    let next_page_token = if has_next {
        last_tx_seq.map(encode_tx_page_token)
    } else {
        None
    };

    info!(
        filtered,
        page_size,
        n_candidates,
        n_page,
        resolve_range_ms,
        pipeline_ms,
        render_ms,
        total_ms = total_t0.elapsed().as_millis(),
        "list_transactions: done"
    );

    Ok(ListTransactionsResponse {
        transactions,
        next_page_token,
        ..Default::default()
    })
}

/// Determine the tx_sequence_number range from request parameters.
///
/// Clamps `start_checkpoint` / `end_checkpoint` against the indexed watermark
/// before resolving so that out-of-range bounds produce an empty result rather
/// than an error on a missing checkpoint summary.
async fn resolve_tx_range(
    client: &BigTableClient,
    cursor: Option<u64>,
    start_checkpoint: Option<u64>,
    end_checkpoint: Option<u64>,
) -> Result<std::ops::Range<u64>, RpcError> {
    let mut wm_client = client.clone();
    let wm = wm_client
        .get_watermark()
        .await?
        .ok_or_else(|| RpcError::new(tonic::Code::Unavailable, "no watermark available"))?;
    let wm_hi_exclusive = wm.checkpoint_hi_inclusive + 1;

    let start_cp = start_checkpoint.unwrap_or(0).min(wm_hi_exclusive);
    let end_cp = end_checkpoint
        .unwrap_or(wm_hi_exclusive)
        .min(wm_hi_exclusive);
    if start_cp >= end_cp {
        return Ok(0..0);
    }

    let start_fut = {
        let mut client = client.clone();
        async move {
            if let Some(seq) = cursor {
                return Ok::<u64, RpcError>(seq + 1);
            }
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
        let mut client = client.clone();
        async move { Ok::<u64, RpcError>(client.checkpoint_to_tx_range(0..end_cp).await?.end) }
    };

    let (start_tx, end_tx) = tokio::try_join!(start_fut, end_fut)?;
    if start_tx >= end_tx {
        return Ok(0..0);
    }
    Ok(start_tx..end_tx)
}

fn encode_tx_page_token(tx_seq: u64) -> Bytes {
    Bytes::from(tx_seq.to_be_bytes().to_vec())
}

fn decode_tx_page_token(token: &Bytes) -> Result<u64, RpcError> {
    let bytes: [u8; 8] = token.as_ref().try_into().map_err(|_| {
        FieldViolation::new("page_token")
            .with_description("invalid page_token: expected 8 bytes")
            .with_reason(ErrorReason::FieldInvalid)
    })?;
    Ok(u64::from_be_bytes(bytes))
}
