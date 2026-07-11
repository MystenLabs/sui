// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::time::Instant;

use futures::StreamExt;
use futures::stream::BoxStream;
use prost_types::FieldMask;
use sui_inverted_index::BitmapQuery;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEnd;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc_cursor::Position;
use sui_sdk_types::Digest;
use sui_types::storage::LedgerTxSeqDigest;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use crate::grpc::v2::ledger_service::get_transaction::render_executed_transaction;
use crate::ledger_history::filter::transaction_filter_to_query;
use crate::ledger_history::query_options::CheckpointRange;
use crate::ledger_history::query_options::QueryOptions;
use crate::ledger_history::query_options::ResolvedRange;
use crate::ledger_history::watermark::advance_covered_bound_before_checkpoint;
use crate::ledger_history::watermark::boundary_watermark;
use crate::ledger_history::watermark::item_watermark;
use crate::ledger_history::watermark::scan_frontier_cursor_cp;

use super::bitmap_scan::LedgerBitmapKind;
use super::bitmap_scan::PendingBitmapBucket;
use super::bitmap_scan::TX_BITMAP_BUCKET_SIZE;
use super::bitmap_scan::drain_bitmap_hits_with_budget;
use super::chunked_scan::ChunkArgs;
use super::chunked_scan::ChunkTerminal;
use super::chunked_scan::ChunkedScan;
use super::chunked_scan::ScanChunkDone;
use super::chunked_scan::cancelled;
use super::ledger_read::apply_tx_seq_floor;
use super::ledger_read::checkpoint_hi_exclusive;
use super::ledger_read::checkpoint_to_tx_boundary;
use super::ledger_read::checkpoint_to_tx_range;
use super::ledger_read::get_tx_seq_digest_multi;
use super::ledger_read::get_tx_seq_digest_rows;
use super::ledger_read::lowest_available_tx_seq;
use super::ledger_read::remaining_range_after;
use super::ledger_read::sequence_frontier_checkpoint;
use super::ledger_read::validate_checkpoint_bounds;
use super::query_end::effective_terminal_reason;

const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::TRANSACTION;

pub(crate) type ListTransactionsStream =
    BoxStream<'static, Result<ListTransactionsResponse, RpcError>>;

pub(crate) async fn list_transactions(
    service: RpcService,
    request: ListTransactionsRequest,
) -> Result<ListTransactionsStream, RpcError> {
    let started = Instant::now();
    let start_checkpoint = request.start_checkpoint;
    let end_checkpoint = request.end_checkpoint;
    let filter = request.filter;
    let request_options = request.options;
    let filtered = filter.is_some();
    validate_checkpoint_bounds(start_checkpoint, end_checkpoint)?;
    let read_mask = validate_read_mask(request.read_mask)?;
    let ledger_history = service.config.ledger_history();
    let endpoint = ledger_history.list_transactions();
    let bitmap_bucket_scan_budget = ledger_history.bitmap_bucket_scan_budget();
    let chunk_bucket_scan_budget = ledger_history.chunk_bucket_scan_budget();
    let max_bitmap_filter_literals = ledger_history.max_bitmap_filter_literals();
    let options = QueryOptions::transactions_from_proto(
        request_options.as_ref(),
        endpoint.default_limit_items,
        endpoint.max_limit_items,
    )?;
    let limit_items = options.limit_items;
    let ordering = options.ordering;
    let filter_query = filter
        .as_ref()
        .map(|filter| transaction_filter_to_query(filter, max_bitmap_filter_literals))
        .transpose()?;

    let initial_state = TransactionScanState::Init {
        start_checkpoint,
        end_checkpoint,
        filter_query,
    };

    let terminal_options = options.clone();
    Ok(async_stream::try_stream! {
        let render_contents = should_render_transaction_contents(&read_mask);
        let scan = ChunkedScan::new(
            initial_state,
            limit_items,
            endpoint.chunk_max,
            bitmap_bucket_scan_budget,
            move |state, args: ChunkArgs| {
                spawn_transaction_chunk(
                    service.clone(),
                    state,
                    read_mask.clone(),
                    options.clone(),
                    args.scan_budget,
                    chunk_bucket_scan_budget,
                    args.chunk_item_limit,
                    args.remaining_request_item_limit,
                    render_contents,
                    args.cancel,
                )
            },
        );

        let mut responses = transaction_response_stream(
            scan,
            terminal_options,
            limit_items,
            started,
            filtered,
            ordering,
        );
        while let Some(response) = responses.next().await {
            yield response?;
        }
    }
    .boxed())
}

fn transaction_response_stream<State, Spawn>(
    mut scan: ChunkedScan<State, ListTransactionsResponse, Spawn>,
    terminal_options: QueryOptions,
    limit_items: usize,
    started: Instant,
    filtered: bool,
    ordering: crate::ledger_history::query_options::Ordering,
) -> ListTransactionsStream
where
    State: Send + 'static,
    Spawn: FnMut(
            State,
            ChunkArgs,
        )
            -> JoinHandle<Result<ScanChunkDone<State, ListTransactionsResponse>, RpcError>>
        + Send
        + 'static,
{
    async_stream::try_stream! {
        let mut covered_checkpoint_bound = None;
        while let Some(mut response) = scan.next_item().await? {
            if let Some(checkpoint) = response
                .watermark
                .as_ref()
                .and_then(|watermark| watermark.checkpoint)
            {
                covered_checkpoint_bound = Some(checkpoint);
            }
            if response.transaction.is_some()
                && scan.produced() == limit_items
                && scan.exhausted()
            {
                let mut end = QueryEnd::default();
                end.reason = Some(QueryEndReason::ItemLimit as i32);
                response.end = Some(end);
            }
            yield response;
        }

        let produced = scan.produced();
        let chunk_terminal = scan.into_terminal().expect("query emits terminal state");
        let terminal_reason =
            effective_terminal_reason(produced, limit_items, chunk_terminal.reason());
        if terminal_reason != QueryEndReason::ItemLimit {
            let terminal_watermark =
                chunk_terminal.into_watermark(&terminal_options, covered_checkpoint_bound);
            yield end_response(terminal_watermark, terminal_reason);
        }
        info!(
            filtered,
            limit_items,
            ?ordering,
            emitted = produced,
            ?terminal_reason,
            elapsed_ms = started.elapsed().as_millis(),
            "list_transactions: done"
        );
    }
    .boxed()
}

fn spawn_transaction_chunk(
    service: RpcService,
    state: TransactionScanState,
    read_mask: FieldMaskTree,
    options: QueryOptions,
    scan_budget: usize,
    chunk_scan_budget: usize,
    chunk_item_limit: usize,
    remaining_request_item_limit: usize,
    render_contents: bool,
    cancel: CancellationToken,
) -> JoinHandle<Result<TransactionChunkDone, RpcError>> {
    tokio::task::spawn_blocking(move || {
        next_transaction_chunk(
            service,
            state,
            read_mask,
            options,
            render_contents,
            scan_budget,
            chunk_scan_budget,
            chunk_item_limit,
            remaining_request_item_limit,
            &cancel,
        )
    })
}

#[derive(Clone)]
enum TransactionScanState {
    Init {
        start_checkpoint: Option<u64>,
        end_checkpoint: Option<u64>,
        filter_query: Option<BitmapQuery>,
    },
    Unfiltered {
        range: Range<u64>,
        range_exhaustion_reason: QueryEndReason,
        end_checkpoint: u64,
        end_position: u64,
    },
    Filtered {
        query: BitmapQuery,
        range: Option<Range<u64>>,
        pending_bucket: Option<PendingBitmapBucket>,
        range_exhaustion_reason: QueryEndReason,
        end_checkpoint: u64,
        end_position: u64,
    },
}

type TransactionChunkDone = ScanChunkDone<TransactionScanState, ListTransactionsResponse>;

fn next_transaction_chunk(
    service: RpcService,
    mut state: TransactionScanState,
    read_mask: FieldMaskTree,
    options: QueryOptions,
    render_contents: bool,
    scan_budget: usize,
    chunk_scan_budget: usize,
    chunk_item_limit: usize,
    remaining_request_item_limit: usize,
    cancel: &CancellationToken,
) -> Result<TransactionChunkDone, RpcError> {
    let ascending = options.is_ascending();
    let mut remaining_scan_budget = scan_budget;
    let (rows, next_state, terminal, scan_watermark) = loop {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        match state {
            TransactionScanState::Init {
                start_checkpoint,
                end_checkpoint,
                filter_query,
            } => {
                let checkpoint_range = CheckpointRange::from_request(
                    start_checkpoint,
                    end_checkpoint,
                    checkpoint_hi_exclusive(&service)?,
                )?;
                let tx_range =
                    resolve_tx_range(&service, start_checkpoint, checkpoint_range, &options)?;
                let terminal = ChunkTerminal::boundary(
                    tx_range.end_reason,
                    Position::Transactions {
                        checkpoint: tx_range.end_checkpoint,
                        tx_seq: tx_range.end_position,
                    },
                    None,
                );
                let range = tx_range.range;
                if range.is_empty() {
                    return Ok(TransactionChunkDone {
                        items: Vec::new(),
                        produced: 0,
                        next_state: None,
                        terminal,
                        remaining_scan_budget,
                    });
                }
                state = match filter_query {
                    Some(query) => TransactionScanState::Filtered {
                        query,
                        range: Some(range),
                        pending_bucket: None,
                        range_exhaustion_reason: tx_range.end_reason,
                        end_checkpoint: tx_range.end_checkpoint,
                        end_position: tx_range.end_position,
                    },
                    None => TransactionScanState::Unfiltered {
                        range,
                        range_exhaustion_reason: tx_range.end_reason,
                        end_checkpoint: tx_range.end_checkpoint,
                        end_position: tx_range.end_position,
                    },
                };
                continue;
            }
            TransactionScanState::Unfiltered {
                range,
                range_exhaustion_reason,
                end_checkpoint,
                end_position,
            } => {
                let rows =
                    get_tx_seq_digest_rows(&service, range.clone(), !ascending, chunk_item_limit)?;
                let next_state = rows
                    .last()
                    .and_then(|row| remaining_range_after(range, row.tx_sequence_number, ascending))
                    .map(|range| TransactionScanState::Unfiltered {
                        range,
                        range_exhaustion_reason,
                        end_checkpoint,
                        end_position,
                    });
                let terminal = ChunkTerminal::boundary(
                    range_exhaustion_reason,
                    Position::Transactions {
                        checkpoint: end_checkpoint,
                        tx_seq: end_position,
                    },
                    None,
                );
                break (rows, next_state, terminal, None);
            }
            TransactionScanState::Filtered {
                query,
                range,
                pending_bucket,
                range_exhaustion_reason,
                end_checkpoint,
                end_position,
            } => {
                let hit_limit = chunk_item_limit.min(remaining_request_item_limit);
                let chunk_scan_budget = remaining_scan_budget.min(chunk_scan_budget);
                let hits = drain_bitmap_hits_with_budget(
                    service.clone(),
                    LedgerBitmapKind::Transaction,
                    TX_BITMAP_BUCKET_SIZE,
                    query.clone(),
                    pending_bucket,
                    range,
                    options.scan_direction(),
                    hit_limit,
                    chunk_scan_budget,
                    cancel,
                )?;
                remaining_scan_budget -= hits.buckets_scanned;
                if cancel.is_cancelled() {
                    return Err(cancelled());
                }
                let chunk_scan_limit_reached = hits.chunk_scan_limit_reached;
                let coalesced_frontier = hits.coalesced_frontier;
                // A chunk scan-limit only ends the request when the request
                // budget is also exhausted, or when there is no continuation.
                let request_scan_limit_reached = chunk_scan_limit_reached
                    && (remaining_scan_budget == 0
                        || (hits.next_range.is_none() && hits.pending_bucket.is_none()));
                let rows = get_tx_seq_digest_multi(&service, &hits.items)?;
                let next_state = if request_scan_limit_reached {
                    None
                } else {
                    (hits.pending_bucket.is_some() || hits.next_range.is_some()).then_some(
                        TransactionScanState::Filtered {
                            query,
                            range: hits.next_range,
                            pending_bucket: hits.pending_bucket,
                            range_exhaustion_reason,
                            end_checkpoint,
                            end_position,
                        },
                    )
                };
                let scan_end_reason = if request_scan_limit_reached {
                    QueryEndReason::ScanLimit
                } else {
                    range_exhaustion_reason
                };
                let coalesced_frontier = if chunk_scan_limit_reached {
                    Some(coalesced_frontier.ok_or_else(|| {
                        RpcError::new(
                            tonic::Code::Internal,
                            "transaction scan limit missing authoritative frontier",
                        )
                    })?)
                } else {
                    None
                };
                let frontier_watermark = if request_scan_limit_reached
                    || (chunk_scan_limit_reached && rows.is_empty())
                {
                    Some(scan_transaction_watermark(
                        &service,
                        &options,
                        coalesced_frontier.expect("checked for scan-limit chunk"),
                        ascending,
                    )?)
                } else {
                    None
                };
                let scan_watermark = if !request_scan_limit_reached && rows.is_empty() {
                    frontier_watermark.clone().map(watermark_response)
                } else {
                    None
                };
                let terminal_position = Position::Transactions {
                    checkpoint: end_checkpoint,
                    tx_seq: end_position,
                };
                let terminal = if request_scan_limit_reached {
                    ChunkTerminal::scan_limit(
                        terminal_position,
                        frontier_watermark
                            .expect("request scan limit constructs frontier watermark"),
                    )
                } else {
                    ChunkTerminal::boundary(scan_end_reason, terminal_position, None)
                };
                break (rows, next_state, terminal, scan_watermark);
            }
        }
    };

    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let mut items = render_transaction_rows(
        &service,
        rows,
        &read_mask,
        &options,
        render_contents,
        cancel,
    )?;
    let produced = items.len();
    if let Some(watermark) = scan_watermark {
        items.push(watermark);
    }
    Ok(TransactionChunkDone {
        items,
        produced,
        next_state,
        terminal,
        remaining_scan_budget,
    })
}

/// Scan watermark for a filtered chunk whose scan budget ran out mid-gap.
/// A transaction's member id is its own `tx_sequence_number`, so the frontier
/// decodes to itself. Checkpoint coverage is independent: at the ascending
/// genesis frontier there is no completed checkpoint, but `(0, 0)` is still
/// the authoritative safe resume cursor.
fn scan_transaction_watermark(
    service: &RpcService,
    options: &QueryOptions,
    frontier: u64,
    ascending: bool,
) -> Result<Watermark, RpcError> {
    transaction_frontier_watermark(
        options,
        frontier,
        sequence_frontier_checkpoint(service, frontier, ascending)?,
    )
}

fn transaction_frontier_watermark(
    options: &QueryOptions,
    frontier: u64,
    checkpoint: Option<u64>,
) -> Result<Watermark, RpcError> {
    let boundary =
        checkpoint.and_then(|cp| advance_covered_bound_before_checkpoint(None, cp, options));
    let cursor_cp = scan_frontier_cursor_cp(checkpoint, frontier, options.scan_direction())
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                format!("transaction scan frontier {frontier} has no checkpoint mapping"),
            )
        })?;
    Ok(boundary_watermark(
        Position::Transactions {
            checkpoint: cursor_cp,
            tx_seq: frontier,
        },
        boundary,
    ))
}

fn render_transaction_rows(
    service: &RpcService,
    rows: Vec<LedgerTxSeqDigest>,
    read_mask: &FieldMaskTree,
    options: &QueryOptions,
    render_contents: bool,
    cancel: &CancellationToken,
) -> Result<Vec<ListTransactionsResponse>, RpcError> {
    let mut transaction_reads = if render_contents {
        let digests = rows
            .iter()
            .map(|row| {
                let digest: Digest = row.digest.into();
                digest
            })
            .collect::<Vec<_>>();
        service
            .reader
            .multi_get_transaction_reads(&digests)?
            .into_iter()
    } else {
        Vec::new().into_iter()
    };

    let mut items = Vec::with_capacity(rows.len());
    // Per-chunk running boundary; monotonic across chunks because rows are
    // emitted in scan-checkpoint order.
    let mut checkpoint_boundary: Option<u64> = None;
    for row in rows {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        checkpoint_boundary = advance_covered_bound_before_checkpoint(
            checkpoint_boundary,
            row.checkpoint_number,
            options,
        );
        let watermark = item_watermark(
            Position::Transactions {
                checkpoint: row.checkpoint_number,
                tx_seq: row.tx_sequence_number,
            },
            checkpoint_boundary,
        );
        let response = if render_contents {
            let transaction_read = transaction_reads
                .next()
                .expect("transaction reads match tx_seq rows");
            let transaction = render_executed_transaction(
                service,
                transaction_read,
                row.checkpoint_number,
                read_mask,
            )?;
            transaction_item_response(watermark, transaction, row.tx_offset, read_mask)
        } else {
            let mut transaction = ExecutedTransaction::default();
            if read_mask.contains(ExecutedTransaction::DIGEST_FIELD.name) {
                transaction.digest = Some(row.digest.to_string());
            }
            if read_mask.contains(ExecutedTransaction::CHECKPOINT_FIELD.name) {
                transaction.checkpoint = Some(row.checkpoint_number);
            }
            transaction_item_response(watermark, transaction, row.tx_offset, read_mask)
        };
        items.push(response);
    }
    Ok(items)
}

pub(crate) fn validate_read_mask(read_mask: Option<FieldMask>) -> Result<FieldMaskTree, RpcError> {
    let read_mask = read_mask.unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
    read_mask
        .validate::<ExecutedTransaction>()
        .map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
    Ok(FieldMaskTree::from(read_mask))
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

fn resolve_tx_range(
    service: &RpcService,
    start_checkpoint: Option<u64>,
    checkpoint_range: CheckpointRange,
    options: &QueryOptions,
) -> Result<ResolvedRange, RpcError> {
    let cp_range = checkpoint_range.resolve(options);
    if cp_range.is_empty() {
        let tx_boundary =
            checkpoint_to_tx_boundary(service, cp_range.terminal_checkpoint(options.ordering))?;
        return Ok(cp_range.with_range(tx_boundary..tx_boundary, options.ordering));
    }

    let tx_range = checkpoint_to_tx_range(service, cp_range.range.clone())?;
    let mut resolved = options.apply_cursor_bounds(cp_range.with_range(tx_range, options.ordering));
    if !resolved.range.is_empty() {
        let explicit_lower = start_checkpoint.is_some() || options.has_after_cursor();
        let floor = lowest_available_tx_seq(service)?;
        resolved.range.start = apply_tx_seq_floor(resolved.range.start, explicit_lower, floor)?;
    }
    Ok(resolved)
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

fn watermark_response(watermark: Watermark) -> ListTransactionsResponse {
    let mut response = ListTransactionsResponse::default();
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
    use super::*;
    use sui_rpc::proto::sui::rpc::v2alpha::Ordering;
    use sui_rpc::proto::sui::rpc::v2alpha::QueryOptions as ProtoQueryOptions;
    use sui_rpc_cursor::CursorToken;

    fn options(ascending: bool) -> QueryOptions {
        let mut proto = ProtoQueryOptions::default();
        if !ascending {
            proto.ordering = Some(Ordering::Descending as i32);
        }
        QueryOptions::transactions_from_proto(Some(&proto), 100, 100).unwrap()
    }

    #[test]
    fn scan_limit_terminal_frames_are_directional_transaction_cursors() {
        for (ascending, frontier, checkpoint, expected_position, expected_proof) in [
            (
                true,
                0,
                None,
                Position::Transactions {
                    checkpoint: 0,
                    tx_seq: 0,
                },
                None,
            ),
            (
                true,
                41,
                Some(7),
                Position::Transactions {
                    checkpoint: 7,
                    tx_seq: 41,
                },
                Some(6),
            ),
            (
                true,
                42,
                Some(9),
                Position::Transactions {
                    checkpoint: 9,
                    tx_seq: 42,
                },
                Some(8),
            ),
            (
                false,
                u64::MAX,
                None,
                Position::Transactions {
                    checkpoint: u64::MAX,
                    tx_seq: u64::MAX,
                },
                None,
            ),
            (
                false,
                19,
                Some(7),
                Position::Transactions {
                    checkpoint: 8,
                    tx_seq: 19,
                },
                Some(8),
            ),
            (
                false,
                18,
                Some(5),
                Position::Transactions {
                    checkpoint: 6,
                    tx_seq: 18,
                },
                Some(6),
            ),
        ] {
            let options = options(ascending);
            let watermark = transaction_frontier_watermark(&options, frontier, checkpoint).unwrap();
            assert_eq!(
                CursorToken::decode(
                    watermark
                        .cursor
                        .as_ref()
                        .expect("transaction frontier cursor")
                )
                .unwrap(),
                CursorToken::boundary(expected_position)
            );
            assert_eq!(watermark.checkpoint, expected_proof);
            let terminal = ChunkTerminal::scan_limit(expected_position, watermark);
            let response = end_response(
                terminal.into_watermark(&options, Some(123)),
                QueryEndReason::ScanLimit,
            );
            assert!(response.transaction.is_none());
            assert_eq!(
                response.watermark.as_ref().and_then(|wm| wm.checkpoint),
                expected_proof
            );
            assert_eq!(
                response.end.as_ref().map(|end| end.reason()),
                Some(QueryEndReason::ScanLimit)
            );
        }
    }

    async fn assert_error_terminates_response_driver(
        expected_code: tonic::Code,
        expected_message: &'static str,
    ) {
        let scan = ChunkedScan::new(0usize, 5, 1, 10, move |state, args: ChunkArgs| {
            tokio::task::spawn(async move {
                if state == 0 {
                    let mut transaction = ExecutedTransaction::default();
                    transaction.digest = Some("successful-transaction".into());
                    let mut watermark = Watermark::default();
                    watermark.checkpoint = Some(7);
                    let mut response = ListTransactionsResponse::default();
                    response.transaction = Some(transaction);
                    response.watermark = Some(watermark);
                    Ok(ScanChunkDone {
                        items: vec![response],
                        produced: 1,
                        next_state: Some(1),
                        terminal: ChunkTerminal::RangeEnd {
                            reason: QueryEndReason::CheckpointBound,
                            position: Position::Transactions {
                                checkpoint: 8,
                                tx_seq: 42,
                            },
                        },
                        remaining_scan_budget: args.scan_budget,
                    })
                } else if expected_code == tonic::Code::Cancelled {
                    Err(cancelled())
                } else {
                    Err(RpcError::new(expected_code, expected_message))
                }
            })
        });
        let options = options(true);
        let ordering = options.ordering;
        let mut responses =
            transaction_response_stream(scan, options, 5, Instant::now(), false, ordering);

        let response = responses
            .next()
            .await
            .expect("successful response precedes worker error")
            .expect("first response is successful");
        assert_eq!(
            response
                .transaction
                .as_ref()
                .and_then(|transaction| transaction.digest.as_deref()),
            Some("successful-transaction")
        );
        assert_eq!(
            response
                .watermark
                .as_ref()
                .and_then(|watermark| watermark.checkpoint),
            Some(7)
        );
        assert!(
            response.end.is_none(),
            "the endpoint driver must not attach a clean end before a worker error"
        );

        let error = responses
            .next()
            .await
            .expect("worker error is the next stream result")
            .expect_err("worker error must not become a QueryEnd response");
        let status = tonic::Status::from(error);
        assert_eq!(status.code(), expected_code);
        assert_eq!(status.message(), expected_message);
        assert!(
            responses.next().await.is_none(),
            "endpoint terminal construction must be unreachable after the error"
        );
    }

    #[tokio::test]
    async fn response_driver_ends_with_internal_status_after_successful_frame() {
        assert_error_terminates_response_driver(tonic::Code::Internal, "injected scan fault").await;
    }

    #[tokio::test]
    async fn response_driver_ends_with_cancelled_status_after_successful_frame() {
        assert_error_terminates_response_driver(tonic::Code::Cancelled, "request cancelled").await;
    }
}
