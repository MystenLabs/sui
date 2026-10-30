// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Events as a scan-then-live feed: each matching event is delivered as its own edge, ordered by
//! checkpoint, then by transaction position within the checkpoint, then by position within the
//! transaction. The cursor is a three-part position (checkpoint, transaction, event index), and
//! matching tests each event against the filter.

use std::future::Future;
use std::ops::RangeInclusive;
use std::sync::Arc;

use anyhow::Context;
use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use async_graphql::connection::EmptyFields;
use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcReader;
use sui_indexer_alt_reader::ledger_grpc_reader::StreamPage;
use sui_rpc::proto::sui::rpc::v2;
use tokio::sync::OnceCell;

use crate::api::types::event::CEvent;
use crate::api::types::event::Event;
use crate::api::types::event::EventToken;
use crate::api::types::event::event_from_stream_item;
use crate::api::types::event::filter::EventFilter;
use crate::error::RpcError;
use crate::pagination::Page;
use crate::scope::Scope;
use crate::task::streaming::ProcessedCheckpoint;
use crate::task::streaming::StreamedCaches;

use super::EventSubscriptionEdge;
use super::scan_then_live::Subscribable;

impl Subscribable for Event {
    type Item = Self;
    type Cursor = CEvent;
    type Filter = EventFilter;
    type ScanItem = v2::Event;
    type Output = EventSubscriptionEdge;

    fn subscription_type() -> &'static str {
        "events"
    }

    fn output(cursor: String, node: Option<Event>) -> EventSubscriptionEdge {
        EventSubscriptionEdge { cursor, node }
    }

    fn scan<'a>(
        reader: &'a LedgerGrpcReader,
        cp_bounds: RangeInclusive<u64>,
        page: &'a Page<CEvent>,
        filter: &'a EventFilter,
    ) -> impl Future<Output = Result<StreamPage<v2::Event>, RpcError>> + Send + 'a {
        Event::scan_grpc(reader, cp_bounds, page, page.limit(), filter)
    }

    fn build_node(
        caches: &Arc<StreamedCaches>,
        resolver_limits: &sui_package_resolver::Limits,
        payload: &v2::Event,
    ) -> Result<Self, RpcError> {
        let checkpoint = payload
            .checkpoint
            .context("ListEvents item missing checkpoint")?;
        let scope = Scope::for_backfill(caches.clone(), resolver_limits.clone(), checkpoint);
        event_from_stream_item(scope, payload)
    }

    fn matching_edges(
        checkpoint: &Arc<ProcessedCheckpoint>,
        caches: &Arc<StreamedCaches>,
        resolver_limits: &sui_package_resolver::Limits,
        filter: &EventFilter,
    ) -> Result<Vec<Edge<String, Self, EmptyFields>>, RpcError> {
        let timestamp_ms = Some(checkpoint.summary.timestamp_ms);
        let scope = Scope::for_streamed_checkpoint(
            caches.clone(),
            resolver_limits.clone(),
            checkpoint.clone(),
        );
        let mut edges = Vec::new();
        for tx in &checkpoint.transactions {
            let digest = tx
                .contents
                .digest()
                .expect("ExecutedTransaction digest is infallible");
            let events = tx.contents.events().unwrap_or_default();
            for (idx, native) in events.into_iter().enumerate() {
                if !filter.matches(&native) {
                    continue;
                }
                let cursor = EventToken::cursor(
                    checkpoint.summary.sequence_number,
                    tx.tx_sequence_number,
                    idx as u32,
                )
                .encode_cursor();
                edges.push(Edge::new(
                    cursor,
                    Event {
                        scope: scope.with_active_transaction_contents(digest, tx.contents.clone()),
                        native,
                        transaction_digest: digest,
                        sequence_number: idx as u64,
                        timestamp_ms: OnceCell::from(timestamp_ms),
                    },
                ));
            }
        }
        Ok(edges)
    }

    fn boundary_after(checkpoint: &ProcessedCheckpoint) -> Option<CEvent> {
        // `network_total_transactions` counts through this checkpoint, so it is the sequence number
        // of the next checkpoint's first transaction, whose first event is index 0.
        Some(EventToken::boundary(
            checkpoint.summary.sequence_number + 1,
            checkpoint.summary.network_total_transactions,
            0,
        ))
    }
}

#[cfg(test)]
mod tests {
    use sui_rpc_cursor::CursorKind;
    use sui_rpc_cursor::CursorToken;
    use sui_rpc_cursor::Position;

    use super::*;

    /// `boundary_after` marks the start of the next checkpoint: a `Boundary` at that checkpoint's
    /// first transaction and event index 0, so resuming after it begins at the first event of
    /// `checkpoint + 1`.
    #[test]
    fn boundary_after_marks_start_of_next_checkpoint() {
        let checkpoint = ProcessedCheckpoint::for_test(10, 42);
        let cursor = Event::boundary_after(&checkpoint).expect("event feed mints a boundary");
        let token = CursorToken::from(&cursor);
        assert_eq!(token.kind, CursorKind::Boundary);
        assert_eq!(
            token.position,
            Position::Events {
                checkpoint: 11,
                tx_seq: 42,
                event_index: 0,
            }
        );
    }
}
