// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Transaction subscription.
//!
//! Streams the transactions matching a filter, in checkpoint order, resuming from a cursor or a
//! checkpoint. Assembled in two phases that meet at a pinned `handoff` with no gap and no duplicate
//! ([`transactions_stream`]):
//!
//! 1. Backfill ([`scan_transactions`]): page the filtered scanning API (`list_transactions`, served
//!    from the bitmap index) from the resume point toward the tip.
//! 2. Live ([`live_transactions`]): follow the shared checkpoint broadcast, matching each
//!    checkpoint's transactions in memory.
//!
//! # Handoff, by example
//!
//! A client resumes after checkpoint 5, and the live tip is currently at 10:
//!
//! - Phase 1 scans the filter's matches forward: checkpoint 6, 7, 8, ... toward the tip.
//! - As it nears the tip, it pins the handoff: it subscribes to the live broadcast (which will
//!   deliver checkpoint 11 next) and records `handoff = 10`.
//! - Phase 1 finishes delivering matches through checkpoint 10, then stops.
//! - Phase 2 takes over from the live broadcast: checkpoint 11, 12, 13, ...
//!
//! The seam at 10 -> 11 has no gap and no duplicate. Subscribing *before* reading the tip is what
//! guarantees it: the live feed's first checkpoint is always `handoff + 1` or earlier (any overlap
//! is dropped), never past it. See [`live_transactions`] for the overlap skip and the gap check.
//!
//! # Empty pages
//!
//! Filters can be sparse: a scanned page may cover a range of checkpoints while matching no
//! transaction. Every page still reports how far it scanned (`checkpoint_hi`) as a coverage marker,
//! separate from any matches, so the handoff can still advance and Phase 1 can terminate across
//! stretches that matched nothing. Without it, a sparse subscription would never see its coverage
//! reach the handoff, and the backfill would never hand off to live.
//!
//! # Cursors
//!
//! Both phases mint the same opaque `CursorToken` (as a `CTransaction`), so a client can resume from
//! any delivered cursor: the backfill re-wraps the scan's server cursor, and live mints an equivalent
//! token from the checkpoint and transaction sequence numbers.
//!
//! # Anomalies
//!
//! A gap between backfill and live, or a subscriber that lags the broadcast buffer, disconnects with
//! `reconnect_error`; the client reconnects and resumes from its last cursor.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use async_graphql::connection::EmptyFields;
use async_stream::stream;
use backoff::ExponentialBackoff;
use bytes::Bytes;
use futures::Stream;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::AlphaLedgerGrpcReader;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::PageItem;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2alpha as proto;
use sui_rpc_cursor::CursorToken;
use sui_rpc_cursor::Position;
use tokio::sync::broadcast;
use tracing::warn;

use crate::api::scalars::cursor::OpaqueCursor;
use crate::api::types::transaction::CTransaction;
use crate::api::types::transaction::Transaction;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::config::SubscriptionConfig;
use crate::error::RpcError;
use crate::scope::Scope;
use crate::task::streaming::CheckpointBroadcaster;
use crate::task::streaming::ProcessedCheckpoint;
use crate::task::streaming::StreamingPackageStore;
use crate::task::streaming::SubscriptionBroadcast;
use crate::task::streaming::broadcast_error;
use crate::task::streaming::hydrate_executed_transaction;
use crate::task::streaming::reconnect_error;

/// Where a backfill resumes from.
pub(super) enum ResumeFrom {
    Cursor(Bytes),
    Checkpoint(u64),
}

/// A scan output: a matched edge, or a coverage-only advance (`edge: None`) reporting how far the
/// scan reached, so the handoff can pin through no-match stretches.
struct Scanned {
    checkpoint: u64,
    edge: Option<Edge<String, Transaction, EmptyFields>>,
}

/// Subscribe to transactions matching `filter`, backfilling from `resume` then following live.
///
/// Phase 1 drains the scan toward the tip; within half the broadcast buffer of it, it resubscribes
/// and pins `handoff`. Phase 2 follows the pinned receiver from `handoff + 1`.
pub(super) fn transactions_stream(
    reader: AlphaLedgerGrpcReader,
    broadcast: Arc<SubscriptionBroadcast>,
    config: SubscriptionConfig,
    package_store: Arc<StreamingPackageStore>,
    resolver_limits: sui_package_resolver::Limits,
    filter: TransactionFilter,
    resume: Option<ResumeFrom>,
) -> impl Stream<Item = Result<Edge<String, Transaction, EmptyFields>, RpcError>> {
    let handoff_threshold = config.broadcast_buffer as u64 / 2;
    let proto_filter = filter.to_bitmap_filter();

    stream! {
        let mut pending_receiver = None;
        let mut handoff: Option<u64> = None;
        let mut last_checkpoint: Option<u64> = None;

        // Phase 1: backfill toward the tip, pinning the live receiver near it.
        if let Some(resume) = resume {
            let scan = scan_transactions(
                reader,
                package_store.clone(),
                resolver_limits.clone(),
                proto_filter,
                resume,
            );
            for await scanned in scan {
                let Scanned { checkpoint, edge } = scanned?;

                // Resubscribe-first and pin once within threshold of the tip (match or coverage).
                if pending_receiver.is_none()
                    && broadcast.network_tip().saturating_sub(checkpoint) <= handoff_threshold
                {
                    pending_receiver = Some(broadcast.broadcaster().resubscribe());
                    handoff = Some(broadcast.network_tip());
                }

                match edge {
                    // A match at its own `checkpoint`: deliver it; a match past the handoff is live's.
                    Some(edge) => {
                        if handoff.is_some_and(|h| checkpoint > h) {
                            break;
                        }
                        yield Ok(edge);
                    }
                    // A coverage marker (page scanned, nothing to yield here): `checkpoint` is the
                    // fully-scanned frontier, so `>= handoff` means the handoff itself is covered.
                    None => {
                        if handoff.is_some_and(|h| checkpoint >= h) {
                            break;
                        }
                    }
                }
            }
            // Seam: we only break after pinning, so the scan has covered through the handoff.
            last_checkpoint = handoff;
        }

        // Phase 2: follow live from `handoff + 1` (a fresh receiver if there was no backfill).
        let receiver = pending_receiver.unwrap_or_else(|| broadcast.broadcaster().resubscribe());
        for await edge in live_transactions(receiver, last_checkpoint, package_store, resolver_limits, filter) {
            yield edge;
        }
    }
}

/// Page the scanning API from `resume` toward the tip, forever: emit each match and a per-page
/// coverage advance, then wait for new checkpoints once caught up. The caller stops consuming once it
/// has covered the handoff.
fn scan_transactions(
    reader: AlphaLedgerGrpcReader,
    package_store: Arc<StreamingPackageStore>,
    resolver_limits: sui_package_resolver::Limits,
    proto_filter: Option<proto::TransactionFilter>,
    resume: ResumeFrom,
) -> impl Stream<Item = Result<Scanned, RpcError>> {
    stream! {
        let mut position = resume;
        loop {
            let page = list_with_retry(scan_backoff(), || {
                reader.list_transactions(build_list_request(&position, &proto_filter))
            })
            .await?;
            let checkpoint_hi = page.checkpoint_hi();
            let has_more = page.has_more();
            let next_cursor = page.last_cursor().cloned();

            for item in page.items {
                let (checkpoint, edge) = build_scanned_edge(item, &package_store, &resolver_limits)?;
                yield Ok(Scanned { checkpoint, edge: Some(edge) });
            }
            // Emit a coverage marker so the handoff can advance through pages with no match.
            if let Some(checkpoint) = checkpoint_hi {
                yield Ok(Scanned { checkpoint, edge: None });
            }

            // Advance to the next page; a caught-up short-circuit has no cursor, so re-request as-is.
            if let Some(cursor) = next_cursor {
                position = ResumeFrom::Cursor(cursor);
            }
            // Caught up to the indexer tip; poll with a backoff until it advances.
            if !has_more {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

/// Follow the live broadcast from `last_checkpoint + 1`, delivering matching transactions. Drops the
/// one-checkpoint seam overlap (the resubscribe/tip race), gap-checks, and disconnects on anomalies.
fn live_transactions(
    mut receiver: CheckpointBroadcaster,
    mut last_checkpoint: Option<u64>,
    package_store: Arc<StreamingPackageStore>,
    resolver_limits: sui_package_resolver::Limits,
    filter: TransactionFilter,
) -> impl Stream<Item = Result<Edge<String, Transaction, EmptyFields>, RpcError>> {
    stream! {
        let mut delivered_live = false;
        loop {
            match receiver.recv().await {
                Ok(checkpoint) => {
                    let seq = checkpoint.summary.sequence_number;
                    if let Some(last) = last_checkpoint {
                        // Already covered by the scan (resubscribe/tip overlap): skip.
                        if seq <= last {
                            continue;
                        }
                        if seq > last + 1 {
                            warn!(
                                last_checkpoint = last,
                                received = seq,
                                "Unexpected gap between scan and live; disconnecting"
                            );
                            yield Err(reconnect_error());
                            return;
                        }
                    }
                    for edge in matching_edges(&checkpoint, &package_store, &resolver_limits, &filter)? {
                        yield Ok(edge);
                    }
                    last_checkpoint = Some(seq);
                    delivered_live = true;
                }
                // A lag before the first live checkpoint is catch-up overflow (likely kv-rpc lag).
                Err(broadcast::error::RecvError::Lagged(missed)) if !delivered_live => {
                    warn!(missed, "Subscriber fell behind during catch-up; disconnecting");
                    yield Err(reconnect_error());
                    return;
                }
                Err(e) => {
                    yield Err(broadcast_error(e));
                    return;
                }
            }
        }
    }
}

/// Build one `list_transactions` page request resuming from `position`, with the filter pushed
/// server-side.
fn build_list_request(
    position: &ResumeFrom,
    filter: &Option<proto::TransactionFilter>,
) -> proto::ListTransactionsRequest {
    // Whole `transaction`/`effects` (we cache the full proto), whole `balance_changes`/`objects`
    // (read whole), but only the `.bcs` bytes of `events`/`signatures` (all hydration reads of them).
    let read_mask = FieldMask::from_paths([
        "transaction",
        "effects",
        "events.bcs",
        "signatures.bcs",
        "balance_changes",
        "objects",
        "checkpoint",
        "timestamp",
    ]);

    let mut request = proto::ListTransactionsRequest::default().with_read_mask(read_mask);
    if let Some(filter) = filter {
        request = request.with_filter(filter.clone());
    }

    let mut options = proto::QueryOptions::default();
    match position {
        // A cursor pages mid-stream (and every page after the first).
        ResumeFrom::Cursor(cursor) => options.set_after(cursor.clone()),
        // A bare checkpoint seeds only the first page (`afterCheckpoint` starts at `cp + 1`).
        ResumeFrom::Checkpoint(cp) => request = request.with_start_checkpoint(cp + 1),
    }
    request.with_options(options)
}

/// Build the output edge for one backfilled transaction, hydrating its contents and carrying the
/// server's opaque gRPC cursor so clients can resume mid-stream.
fn build_scanned_edge(
    item: PageItem<ExecutedTransaction>,
    package_store: &Arc<StreamingPackageStore>,
    resolver_limits: &sui_package_resolver::Limits,
) -> Result<(u64, Edge<String, Transaction, EmptyFields>), RpcError> {
    let executed = item.payload;
    let checkpoint = executed
        .checkpoint
        .context("ExecutedTransaction missing checkpoint")?;
    let timestamp_ms = executed
        .timestamp
        .as_ref()
        .map(|t| t.seconds as u64 * 1000 + t.nanos as u64 / 1_000_000)
        .context("ExecutedTransaction missing timestamp")?;

    let contents = hydrate_executed_transaction(&executed, timestamp_ms, checkpoint)?;
    let scope =
        Scope::for_scanned_transaction(package_store.clone(), resolver_limits.clone(), &executed)?;
    let transaction = Transaction::with_contents(scope, Arc::new(contents))?;
    // The scan's opaque cursor is a `CursorToken`; re-wrap it so it resumes like a live-minted one.
    let token = CursorToken::decode(&item.cursor).context("Invalid scan cursor")?;
    let cursor = CTransaction::new(OpaqueCursor::new(token)).encode_cursor();
    Ok((checkpoint, Edge::new(cursor, transaction)))
}

/// The filter-matching transactions in one live checkpoint, in order.
fn matching_edges(
    checkpoint: &Arc<ProcessedCheckpoint>,
    package_store: &Arc<StreamingPackageStore>,
    resolver_limits: &sui_package_resolver::Limits,
    filter: &TransactionFilter,
) -> Result<Vec<Edge<String, Transaction, EmptyFields>>, RpcError> {
    let scope = Scope::for_streamed_checkpoint(
        package_store.clone(),
        resolver_limits.clone(),
        checkpoint.clone(),
    );
    let mut edges = Vec::new();
    for tx in &checkpoint.transactions {
        if !filter.matches(&tx.contents) {
            continue;
        }
        let transaction = Transaction::with_contents(scope.clone(), tx.contents.clone())?;
        let cursor = CTransaction::new(OpaqueCursor::new(CursorToken::item(
            Position::Transactions {
                checkpoint: checkpoint.summary.sequence_number,
                tx_seq: tx.tx_sequence_number,
            },
        )))
        .encode_cursor();
        edges.push(Edge::new(cursor, transaction));
    }
    Ok(edges)
}

/// The backfill scan's retry policy: jittered exponential backoff up to a bounded budget. 60s is the
/// top of the window where a failure is still plausibly a transient blip/deploy; beyond it we give
/// up rather than retry indefinitely (per industry retry-budget guidance).
fn scan_backoff() -> ExponentialBackoff {
    ExponentialBackoff {
        initial_interval: Duration::from_millis(100),
        max_interval: Duration::from_secs(5),
        max_elapsed_time: Some(Duration::from_secs(60)),
        ..Default::default()
    }
}

/// Run `fetch`, retrying transient failures under `policy`. A rolling indexer deploy (the common
/// transient outage) is absorbed invisibly; anything still failing once the budget is exhausted
/// propagates, ending the subscription so the client reconnects and resumes from its cursor.
async fn list_with_retry<T, F, Fut>(policy: ExponentialBackoff, fetch: F) -> anyhow::Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    backoff::future::retry(policy, || async {
        fetch().await.map_err(|e| {
            warn!("list_transactions failed, retrying: {e:#}");
            backoff::Error::transient(e)
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU32;
    use std::sync::atomic::Ordering::SeqCst;

    use super::*;

    /// Fast policy so the tests don't sleep on real backoff intervals.
    fn test_backoff(budget: Duration) -> ExponentialBackoff {
        ExponentialBackoff {
            initial_interval: Duration::from_millis(1),
            max_interval: Duration::from_millis(5),
            max_elapsed_time: Some(budget),
            ..Default::default()
        }
    }

    /// Transient failures within the budget are retried, and the operation eventually succeeds.
    #[tokio::test]
    async fn list_with_retry_recovers_from_transient_errors() {
        let attempts = AtomicU32::new(0);
        let got = list_with_retry(test_backoff(Duration::from_secs(5)), || async {
            let n = attempts.fetch_add(1, SeqCst);
            if n < 2 {
                Err(anyhow::anyhow!("transient outage"))
            } else {
                Ok(n)
            }
        })
        .await
        .expect("should recover within the retry budget");

        assert_eq!(got, 2);
        assert_eq!(attempts.load(SeqCst), 3, "failed twice, then succeeded");
    }

    /// A failure that never clears gives up once the budget is exhausted, rather than retrying
    /// forever.
    #[tokio::test]
    async fn list_with_retry_gives_up_after_budget() {
        let attempts = AtomicU32::new(0);
        let result: anyhow::Result<u32> =
            list_with_retry(test_backoff(Duration::from_millis(50)), || async {
                attempts.fetch_add(1, SeqCst);
                Err(anyhow::anyhow!("persistent failure"))
            })
            .await;

        assert!(
            result.is_err(),
            "must give up rather than retry indefinitely"
        );
        assert!(
            attempts.load(SeqCst) >= 2,
            "should have retried at least once before giving up",
        );
    }
}
