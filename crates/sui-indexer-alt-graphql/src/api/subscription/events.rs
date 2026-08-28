// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Events as a scan-then-live feed: each matching event is delivered as its own edge, ordered by
//! checkpoint, then by transaction position within the checkpoint, then by position within the
//! transaction. The cursor is a three-part position (checkpoint, transaction, event index), and
//! matching tests each event against the filter.

use std::future::Future;
use std::ops::RangeInclusive;
use std::sync::Arc;

use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use async_graphql::connection::EmptyFields;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::AlphaLedgerGrpcReader;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::StreamPage;
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

use super::scan_then_live::Subscribable;

impl Subscribable for Event {
    type Item = Self;
    type Cursor = CEvent;
    type Filter = EventFilter;
    type ScanItem = v2::Event;

    fn subscription_type() -> &'static str {
        "events"
    }

    fn scan<'a>(
        reader: &'a AlphaLedgerGrpcReader,
        cp_bounds: RangeInclusive<u64>,
        page: &'a Page<CEvent>,
        filter: &'a EventFilter,
    ) -> impl Future<Output = Result<StreamPage<v2::Event>, RpcError>> + Send + 'a {
        Event::scan_grpc(reader, cp_bounds, page, filter)
    }

    fn build_node(
        caches: &Arc<StreamedCaches>,
        resolver_limits: &sui_package_resolver::Limits,
        payload: &v2::Event,
    ) -> Result<Self, RpcError> {
        let scope = Scope::for_indexed(caches.clone(), resolver_limits.clone());
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
}
