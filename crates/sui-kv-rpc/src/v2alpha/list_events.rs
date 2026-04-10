// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::time::Instant;

use bytes::Bytes;
use futures::StreamExt;
use futures::TryStreamExt;
use sui_kvstore::BigTableClient;
use sui_kvstore::BitmapIndexSpec;
use sui_kvstore::KeyValueStoreReader;
use sui_kvstore::TransactionData;
use sui_kvstore::tables::event_bitmap_index;
use sui_kvstore::tables::transactions::col;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::Event as ProtoEvent;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_rpc_api::proto::google::rpc::bad_request::FieldViolation;
use sui_types::digests::TransactionDigest;
use tracing::info;

const CHUNK_MAX: usize = 64;
const STAGE_CONCURRENCY: usize = 4;

use super::filter::event_filter_to_query;
use crate::PackageResolver;
use crate::proto::sui::rpc::kv::v2alpha::EventResult;
use crate::proto::sui::rpc::kv::v2alpha::ListEventsRequest;
use crate::proto::sui::rpc::kv::v2alpha::ListEventsResponse;
use crate::v2::render_json;

const DEFAULT_PAGE_SIZE: u32 = 50;
const MAX_PAGE_SIZE: u32 = 1000;
const EVENT_READ_MASK_DEFAULT: &str = "event_type";

pub(crate) async fn list_events(
    mut client: BigTableClient,
    request: ListEventsRequest,
    resolver: &PackageResolver,
) -> Result<ListEventsResponse, RpcError> {
    let total_t0 = Instant::now();
    let filtered = request.filter.is_some();

    let read_mask = validate_event_read_mask(request.read_mask)?;
    let page_size = request
        .page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE) as usize;

    let cursor = request
        .page_token
        .as_ref()
        .map(decode_event_page_token)
        .transpose()?;

    let t0 = Instant::now();
    let event_range = resolve_event_range(
        &mut client,
        cursor,
        request.start_checkpoint,
        request.end_checkpoint,
    )
    .await?;
    let resolve_range_ms = t0.elapsed().as_millis();

    if event_range.is_empty() {
        info!(
            filtered,
            page_size,
            resolve_range_ms,
            total_ms = total_t0.elapsed().as_millis(),
            "list_events: empty range"
        );
        return Ok(ListEventsResponse::default());
    }

    let wants_json = read_mask.contains(ProtoEvent::JSON_FIELD.name);

    let t0 = Instant::now();
    // Step 1: produce a bounded list of event picks — (event_seq, tx_seq, event_idx).
    //
    // Filtered: bitmap scan in event-space yields packed event_seqs directly.
    // Unfiltered: walk `tx_seq_digest` and use `event_count` to enumerate real
    // event_seqs without touching tx rows. Walking the packed namespace
    // directly would be wasteful (real events occupy only 1/MAX_EVENTS_PER_TX
    // of the space).
    let mut picks: Vec<EventPick> = if let Some(filter) = &request.filter {
        let query = event_filter_to_query(filter)?;
        client
            .eval_bitmap_query_stream_with_spec(
                query,
                event_range.clone(),
                BitmapIndexSpec::event(),
            )
            .take(page_size + 1)
            .map_ok(|event_seq| {
                let (tx_seq, event_idx) = event_bitmap_index::decode_event_seq(event_seq);
                EventPick {
                    event_seq,
                    tx_seq,
                    event_idx,
                }
            })
            .try_collect()
            .await?
    } else {
        walk_tx_seq_digest_for_events(&mut client, event_range.clone(), page_size + 1).await?
    };
    let pick_ms = t0.elapsed().as_millis();
    let n_candidates = picks.len();
    // Both pick sources produce event_seqs in strictly ascending order:
    // the bitmap stream walks buckets in range order with RoaringBitmap's
    // ascending iter; the tx_seq_digest range scan walks tx_seqs forward
    // with event_idx 0..event_count per tx. No sort needed.
    let has_next = picks.len() > page_size;
    picks.truncate(page_size);

    // Step 2: fetch the `events` column for the unique contributing tx_seqs,
    // pipelined across two stages (tx_seq → digest → tx body). Rows stream
    // from each multi_get_stream as they arrive and chunks across stages run
    // concurrently via buffer_unordered + try_flatten_unordered.
    let mut unique_tx_seqs: Vec<u64> = picks.iter().map(|p| p.tx_seq).collect();
    unique_tx_seqs.sort_unstable();
    unique_tx_seqs.dedup();
    let n_unique_txs = unique_tx_seqs.len();
    let t0 = Instant::now();

    let column_filter = BigTableClient::column_filter(&[col::EVENTS]);

    // Stage 1: seq chunks → (seq, digest, cp_seq) rows.
    let digest_rows = futures::stream::iter(unique_tx_seqs.into_iter().map(Ok::<_, RpcError>))
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

    // Stage 2: digest chunks → (tx_seq, cp_seq, tx_data) rows.
    let tx_rows = digest_rows
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
        .try_flatten_unordered(None);

    let fetched: Vec<(u64, u64, TransactionData)> = tx_rows.try_collect().await?;
    let fetch_events_ms = t0.elapsed().as_millis();
    let by_tx_seq: HashMap<u64, (u64, TransactionData)> = fetched
        .into_iter()
        .map(|(seq, cp_seq, tx)| (seq, (cp_seq, tx)))
        .collect();

    // Step 3: join picks → events.
    let t0 = Instant::now();
    let mut events: Vec<EventResult> = Vec::with_capacity(picks.len());
    for pick in picks {
        let Some((checkpoint_seq, tx)) = by_tx_seq.get(&pick.tx_seq) else {
            continue;
        };
        let Some(tx_events) = tx.events.as_ref() else {
            continue;
        };
        let Some(event) = tx_events.data.get(pick.event_idx as usize) else {
            continue;
        };

        let mut proto_event = ProtoEvent::merge_from(event, &read_mask);
        if wants_json {
            proto_event.json = render_json(resolver, &event.type_, &event.contents)
                .await
                .map(Box::new);
        }

        events.push(EventResult {
            cursor: Some(encode_event_page_token(pick.event_seq)),
            checkpoint: Some(*checkpoint_seq),
            event_index: Some(pick.event_idx),
            transaction_digest: Some(tx.digest.to_string()),
            event: Some(proto_event),
            ..Default::default()
        });
    }

    let render_ms = t0.elapsed().as_millis();
    let n_events = events.len();

    let next_page_token = if has_next {
        events.last().and_then(|e| e.cursor.clone())
    } else {
        None
    };

    info!(
        filtered,
        wants_json,
        page_size,
        n_candidates,
        n_unique_txs,
        n_events,
        resolve_range_ms,
        pick_ms,
        fetch_events_ms,
        render_ms,
        total_ms = total_t0.elapsed().as_millis(),
        "list_events: done"
    );

    Ok(ListEventsResponse {
        events,
        next_page_token,
        ..Default::default()
    })
}

/// A single event picked out of the bitmap scan or tx_seq_digest walk,
/// carrying just enough to look up the concrete event after a bulk tx fetch.
struct EventPick {
    event_seq: u64,
    tx_seq: u64,
    event_idx: u32,
}

/// Stream-scan `tx_seq_digest` across the tx range covered by `event_range`,
/// using each row's `event_count` to enumerate real event_seqs per tx without
/// touching the tx body. Drops the underlying range-scan stream as soon as
/// `target` picks accumulate, so we never over-read.
async fn walk_tx_seq_digest_for_events(
    client: &mut BigTableClient,
    event_range: std::ops::Range<u64>,
    target: usize,
) -> Result<Vec<EventPick>, RpcError> {
    let start_tx = event_bitmap_index::decode_event_seq(event_range.start).0;
    let end_tx = event_bitmap_index::decode_event_seq(event_range.end).0;
    if start_tx >= end_tx {
        return Ok(Vec::new());
    }

    // Event_range.start may sit mid-tx (cursor resumes from inside a tx's
    // events); the event_seq lower-bound filter handles that uniformly.
    let lower_bound = event_range.start;
    let mut picked: Vec<EventPick> = Vec::with_capacity(target);
    let stream = client.scan_tx_seq_digest_stream(start_tx..end_tx).await?;
    futures::pin_mut!(stream);
    'outer: while let Some(row) = stream.next().await {
        let (tx_seq, _digest, _cp_seq, event_count) = row?;
        for event_idx in 0..event_count {
            let event_seq = event_bitmap_index::encode_event_seq(tx_seq, event_idx);
            if event_seq < lower_bound {
                continue;
            }
            picked.push(EventPick {
                event_seq,
                tx_seq,
                event_idx,
            });
            if picked.len() >= target {
                break 'outer;
            }
        }
    }
    Ok(picked)
}

fn validate_event_read_mask(read_mask: Option<FieldMask>) -> Result<FieldMaskTree, RpcError> {
    let read_mask = read_mask.unwrap_or_else(|| FieldMask::from_str(EVENT_READ_MASK_DEFAULT));
    read_mask.validate::<ProtoEvent>().map_err(|path| {
        FieldViolation::new("read_mask")
            .with_description(format!("invalid read_mask path: {path}"))
            .with_reason(ErrorReason::FieldInvalid)
    })?;
    Ok(FieldMaskTree::from(read_mask))
}

/// Resolve the packed-event_seq scan range from the request parameters.
///
/// On the first query the range is derived from the checkpoint range by
/// converting each bound to a tx_seq (via `checkpoint_to_tx_range`) and
/// shifting into the packed event-space. On subsequent queries the cursor
/// supplies the lower bound directly.
async fn resolve_event_range(
    client: &mut BigTableClient,
    cursor: Option<u64>,
    start_checkpoint: Option<u64>,
    end_checkpoint: Option<u64>,
) -> Result<std::ops::Range<u64>, RpcError> {
    let wm = client
        .get_watermark()
        .await?
        .ok_or_else(|| RpcError::new(tonic::Code::Unavailable, "no watermark available"))?;
    // `get_watermark_for_pipelines` returns `None` when `checkpoint_hi_inclusive` is
    // absent, so `get_watermark()` giving `Some(wm)` guarantees the inner value is set.
    let wm_hi_exclusive = wm
        .checkpoint_hi_inclusive
        .expect("get_watermark filters out rows with no observed checkpoint")
        + 1;

    let start_cp = start_checkpoint.unwrap_or(0).min(wm_hi_exclusive);
    let end_cp = end_checkpoint
        .unwrap_or(wm_hi_exclusive)
        .min(wm_hi_exclusive);
    if start_cp >= end_cp {
        return Ok(0..0);
    }

    if let Some(cursor_event_seq) = cursor {
        let end_tx = client.checkpoint_to_tx_range(0..end_cp).await?.end;
        let start_event_seq = cursor_event_seq + 1;
        let end_event_seq = event_bitmap_index::event_seq_lo(end_tx);
        Ok(start_event_seq..end_event_seq)
    } else {
        let tx_range = client.checkpoint_to_tx_range(start_cp..end_cp).await?;
        let start_event_seq = event_bitmap_index::event_seq_lo(tx_range.start);
        let end_event_seq = event_bitmap_index::event_seq_lo(tx_range.end);
        Ok(start_event_seq..end_event_seq)
    }
}

fn encode_event_page_token(event_seq: u64) -> Bytes {
    Bytes::from(event_seq.to_be_bytes().to_vec())
}

fn decode_event_page_token(token: &Bytes) -> Result<u64, RpcError> {
    let bytes: [u8; 8] = token.as_ref().try_into().map_err(|_| {
        FieldViolation::new("page_token")
            .with_description("invalid page_token: expected 8 bytes")
            .with_reason(ErrorReason::FieldInvalid)
    })?;
    Ok(u64::from_be_bytes(bytes))
}
